use std::ffi::CString;
use std::sync::Arc;

use async_trait::async_trait;
use esp_idf_sys::{
    esp, esp_bluedroid_enable, esp_bluedroid_init, esp_bt_connection_mode_t_ESP_BT_CONNECTABLE,
    esp_bt_controller_config_t, esp_bt_controller_enable, esp_bt_controller_init,
    esp_bt_controller_mem_release, esp_bt_dev_set_device_name,
    esp_bt_discovery_mode_t_ESP_BT_GENERAL_DISCOVERABLE, esp_bt_gap_cancel_discovery,
    esp_bt_gap_register_callback, esp_bt_gap_set_scan_mode, esp_bt_gap_start_discovery,
    esp_bt_inq_mode_t_ESP_BT_INQ_MODE_GENERAL_INQUIRY, esp_bt_mode_t,
    esp_bt_mode_t_ESP_BT_MODE_BLE, esp_bt_mode_t_ESP_BT_MODE_BTDM,
    esp_bt_mode_t_ESP_BT_MODE_CLASSIC_BT, BTDM_CTRL_AUTO_LATENCY_EFF, BTDM_CTRL_HLI,
    BTDM_CTRL_LEGACY_AUTH_VENDOR_EVT_EFF, BT_HCI_UART_BAUDRATE_DEFAULT, BT_HCI_UART_NO_DEFAULT,
    CONFIG_BTDM_BLE_SLEEP_CLOCK_ACCURACY_INDEX_EFF, CONFIG_BTDM_CTRL_BLE_MAX_CONN_EFF,
    CONFIG_BTDM_CTRL_BR_EDR_MAX_ACL_CONN_EFF, CONFIG_BTDM_CTRL_BR_EDR_MAX_SYNC_CONN_EFF,
    CONFIG_BTDM_CTRL_BR_EDR_SCO_DATA_PATH_EFF, CONFIG_BTDM_CTRL_PCM_POLAR_EFF,
    CONFIG_BTDM_CTRL_PCM_ROLE_EFF, CONTROLLER_ADV_LOST_DEBUG_BIT,
    ESP_BT_CONTROLLER_CONFIG_MAGIC_VAL, ESP_TASK_BT_CONTROLLER_PRIO, ESP_TASK_BT_CONTROLLER_STACK,
    MESH_DUPLICATE_SCAN_CACHE_SIZE, NORMAL_SCAN_DUPLICATE_CACHE_SIZE, SCAN_DUPLICATE_MODE,
    SCAN_DUPLICATE_TYPE_VALUE, SCAN_DUPL_CACHE_REFRESH_PERIOD, SCAN_SEND_ADV_RESERVED_SIZE,
};
use lazy_static::lazy_static;

use anyhow::Result;

use crate::bluetooth_esp32_a2dp::ESP32A2DP;
use crate::bluetooth_gap_esp32::bt_app_gap_cb;
use crate::bluetooth_gap_hal::ScannedDevice;
use crate::bluetooth_hal::AsyncCall;
use crate::bluetooth_hal::*;
use crate::esp32::Esp32;

pub struct Esp32BluetoothGlobals {
    pub discovery: std::sync::Mutex<
        Option<(
            async_broadcast::Sender<ScannedDevice>,
            async_broadcast::Receiver<ScannedDevice>,
        )>,
    >,
    pub connection: Arc<AsyncCall<Result<()>>>,
}

lazy_static! {
    pub static ref ESP32_BLUETOOTH_GLOBALS: Esp32BluetoothGlobals = Esp32BluetoothGlobals {
        discovery: std::sync::Mutex::new(None),
        connection: Arc::new(AsyncCall::<Result<()>>::new())
    };
}

pub struct ESP32Bluetooth {
    classic: bool,
    low_energy: bool,
}

#[cfg(config_btdm_ctrl_mode_ble_only)]
const BTDM_CONTROLLER_MODE_EFF: esp_bt_mode_t = esp_bt_mode_t_ESP_BT_MODE_BLE;
#[cfg(config_btdm_ctrl_mode_br_edronly)]
const BTDM_CONTROLLER_MODE_EFF: esp_bt_mode_t = esp_bt_mode_t_ESP_BT_MODE_CLASSIC_BT;
#[cfg(not(any(config_btdm_ctrl_mode_ble_only, config_btdm_ctrl_mode_br_edronly)))]
const BTDM_CONTROLLER_MODE_EFF: esp_bt_mode_t = esp_bt_mode_t_ESP_BT_MODE_BTDM;

// from https://github.com/espressif/esp-idf/blob/master/components/bt/include/esp32/include/esp_bt.h
const BT_CONTROLLER_INIT_CONFIG_DEFAULT: esp_bt_controller_config_t = esp_bt_controller_config_t {
    controller_task_stack_size: ESP_TASK_BT_CONTROLLER_STACK as u16,
    controller_task_prio: ESP_TASK_BT_CONTROLLER_PRIO as u8,
    hci_uart_no: BT_HCI_UART_NO_DEFAULT as u8,
    hci_uart_baudrate: BT_HCI_UART_BAUDRATE_DEFAULT,
    scan_duplicate_mode: SCAN_DUPLICATE_MODE as u8,
    scan_duplicate_type: SCAN_DUPLICATE_TYPE_VALUE as u8,
    normal_adv_size: NORMAL_SCAN_DUPLICATE_CACHE_SIZE as u16,
    mesh_adv_size: MESH_DUPLICATE_SCAN_CACHE_SIZE as u16,
    send_adv_reserved_size: SCAN_SEND_ADV_RESERVED_SIZE as u16,
    controller_debug_flag: CONTROLLER_ADV_LOST_DEBUG_BIT,
    mode: BTDM_CONTROLLER_MODE_EFF as u8,
    ble_max_conn: CONFIG_BTDM_CTRL_BLE_MAX_CONN_EFF as u8,
    bt_max_acl_conn: CONFIG_BTDM_CTRL_BR_EDR_MAX_ACL_CONN_EFF as u8,
    bt_sco_datapath: CONFIG_BTDM_CTRL_BR_EDR_SCO_DATA_PATH_EFF as u8,
    auto_latency: BTDM_CTRL_AUTO_LATENCY_EFF != 0,
    bt_legacy_auth_vs_evt: BTDM_CTRL_LEGACY_AUTH_VENDOR_EVT_EFF != 0,
    bt_max_sync_conn: CONFIG_BTDM_CTRL_BR_EDR_MAX_SYNC_CONN_EFF as u8,
    ble_sca: CONFIG_BTDM_BLE_SLEEP_CLOCK_ACCURACY_INDEX_EFF as u8,
    pcm_role: CONFIG_BTDM_CTRL_PCM_ROLE_EFF as u8,
    pcm_polar: CONFIG_BTDM_CTRL_PCM_POLAR_EFF as u8,
    hli: BTDM_CTRL_HLI != 0,
    dup_list_refresh_period: SCAN_DUPL_CACHE_REFRESH_PERIOD as u16,
    magic: ESP_BT_CONTROLLER_CONFIG_MAGIC_VAL,
};

#[async_trait]
impl<'a> Bluetooth<'a> for ESP32Bluetooth {
    // Requires nvs_flash_init to have been performed first
    fn init(&mut self, device_name: &str) -> Result<()> {
        let unused_mode = if !self.classic {
            Some(esp_bt_mode_t_ESP_BT_MODE_CLASSIC_BT)
        } else if !self.low_energy {
            Some(esp_bt_mode_t_ESP_BT_MODE_BLE)
        } else {
            None
        };

        if let Some(u_m) = unused_mode {
            unsafe {
                esp!(esp_bt_controller_mem_release(u_m))?;
            };
        }

        log::info!("After bt mem release");

        let mut bt_cfg = BT_CONTROLLER_INIT_CONFIG_DEFAULT;

        unsafe {
            esp!(esp_bt_controller_init(
                &mut bt_cfg as *mut esp_bt_controller_config_t
            ))?;

            log::info!("After bt controller init");

            if self.classic {
                esp!(esp_bt_controller_enable(
                    BTDM_CONTROLLER_MODE_EFF // mode needs to match that in bt_cfg
                ))?;
            }

            log::info!("After bt controller enable");

            esp!(esp_bluedroid_init())?;

            log::info!("After bluedroid init");

            esp!(esp_bluedroid_enable())?;
            log::info!("After bluedroid enable");

            let dev_name = CString::new(device_name)?;

            esp!(esp_bt_dev_set_device_name(dev_name.as_ptr()))?;

            /* register GAP callback function */
            esp!(esp_bt_gap_register_callback(Some(bt_app_gap_cb)))?;

            /* set discoverable and connectable mode (make this configurable later */
            esp!(esp_bt_gap_set_scan_mode(
                esp_bt_connection_mode_t_ESP_BT_CONNECTABLE,
                esp_bt_discovery_mode_t_ESP_BT_GENERAL_DISCOVERABLE
            ))?;
        };

        let a2dp = crate::bluetooth_esp32_a2dp::A2DP.lock().unwrap();

        a2dp.init()?; // should await / block on?

        log::info!("Bluetooth initialized");

        Ok(())
    }

    fn gap_start_discovery(&self) -> Result<async_broadcast::Receiver<ScannedDevice>> {
        let mut discovery_lock = ESP32_BLUETOOTH_GLOBALS
            .discovery
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock: {e}"))?;

        if discovery_lock.is_some() {
            return Err(anyhow::anyhow!("Already scanning"));
        }
        let new_channel = async_broadcast::broadcast::<ScannedDevice>(2);
        unsafe {
            esp!(esp_bt_gap_start_discovery(
                esp_bt_inq_mode_t_ESP_BT_INQ_MODE_GENERAL_INQUIRY,
                10, /* Duration of discovery, 10 = 10 * 1.28 seconds = 13 seconds */
                0   /* Number of responses that can be received */
            ))?;
        }

        let result_receiver = new_channel.1.clone();
        log::info!("gap_start_discovery: setting discovery to broadcast channel");
        discovery_lock.replace(new_channel);
        drop(discovery_lock);

        Ok(result_receiver)
    }

    fn gap_cancel_discovery(&self) -> Result<()> {
        let discovery_lock = ESP32_BLUETOOTH_GLOBALS.discovery.lock().unwrap();

        if discovery_lock.is_some() {
            unsafe {
                esp!(esp_bt_gap_cancel_discovery())?;
            }

            // channel will be closed by callback event indicating end of discovery

            Ok(())
        } else {
            Err(anyhow::anyhow!("No Discovery in progress"))
        }
    }

    async fn a2dp_connect(&mut self, addr: &BDAddr) -> Result<()> {
        ESP32A2DP::connect(addr).await
    }

    async fn a2dp_play(&mut self, stream: Box<dyn Stream<i16>>) -> Result<()> {
        ESP32A2DP::play(stream).await
    }

    fn deinit(&mut self) -> Result<()> {
        Ok(())
    }
}

impl ESP32Bluetooth {
    pub fn new(classic: bool, low_energy: bool) -> Self {
        ESP32Bluetooth {
            classic,
            low_energy,
        }
    }

    pub fn pre_init(&self, esp32: &mut Esp32) -> Result<()> {
        // Initialize NVS.
        // All examples do this, but I have yet to see the docs that say that this needs to be done

        esp32.nvs_init()?;

        log::info!("NVS Initialized");
        Ok(())
    }

    pub fn post_deinit(&self, esp32: &mut Esp32) -> Result<()> {
        esp32.nvs_deinit()?;
        Ok(())
    }
}
