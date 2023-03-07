/*
 * The GAP layer of the Bluetooth low energy protocol stack is responsible for connection functionality.
 * This layer handles the access modes and procedures of the device including device discovery,
 * link establishment, link termination, initiation of security features, and device configuration.
 */

use anyhow::anyhow;
use esp_idf_sys::{
    esp, esp_bt_gap_cb_event_t, esp_bt_gap_cb_event_t_ESP_BT_GAP_ACL_CONN_CMPL_STAT_EVT,
    esp_bt_gap_cb_event_t_ESP_BT_GAP_AUTH_CMPL_EVT, esp_bt_gap_cb_event_t_ESP_BT_GAP_DISC_RES_EVT,
    esp_bt_gap_cb_event_t_ESP_BT_GAP_DISC_STATE_CHANGED_EVT,
    esp_bt_gap_cb_event_t_ESP_BT_GAP_MODE_CHG_EVT, esp_bt_gap_cb_event_t_ESP_BT_GAP_PIN_REQ_EVT,
    esp_bt_gap_cb_event_t_ESP_BT_GAP_RMT_SRVCS_EVT,
    esp_bt_gap_cb_event_t_ESP_BT_GAP_RMT_SRVC_REC_EVT, esp_bt_gap_cb_param_t,
    esp_bt_gap_dev_prop_t, esp_bt_gap_dev_prop_type_t,
    esp_bt_gap_dev_prop_type_t_ESP_BT_GAP_DEV_PROP_BDNAME,
    esp_bt_gap_dev_prop_type_t_ESP_BT_GAP_DEV_PROP_COD,
    esp_bt_gap_dev_prop_type_t_ESP_BT_GAP_DEV_PROP_EIR,
    esp_bt_gap_dev_prop_type_t_ESP_BT_GAP_DEV_PROP_RSSI,
    esp_bt_gap_discovery_state_t_ESP_BT_GAP_DISCOVERY_STARTED,
    esp_bt_gap_discovery_state_t_ESP_BT_GAP_DISCOVERY_STOPPED, esp_bt_gap_pin_reply,
    esp_bt_pin_code_t, esp_bt_status_t_ESP_BT_STATUS_HCI_SUCCESS,
    esp_bt_status_t_ESP_BT_STATUS_SUCCESS,
};
// use num_derive::FromPrimitive;
// use num_traits::FromPrimitive;

use crate::bluetooth_esp32::ESP32_BLUETOOTH_GLOBALS;
use crate::bluetooth_gap_hal::{ClassOfDevice, DeviceProperty, ScannedDevice};
use crate::bluetooth_hal::AsyncCallState;

impl From<*const esp_bt_gap_dev_prop_t> for DeviceProperty {
    fn from(property: *const esp_bt_gap_dev_prop_t) -> Self {
        unsafe {
            let t: esp_bt_gap_dev_prop_type_t = (*property).type_;
            #[allow(non_upper_case_globals)]
            match t {
                esp_bt_gap_dev_prop_type_t_ESP_BT_GAP_DEV_PROP_BDNAME => {
                    let slice = std::slice::from_raw_parts(
                        (*property).val as *const u8,
                        (*property).len as usize,
                    );
                    let mut name = String::new();
                    for byte in slice {
                        name.push_str(&format!("{:02x}", *byte));
                    }

                    log::info!("name: {}", name);
                    // let name = from_utf8(slice).unwrap();
                    DeviceProperty::Name(name)
                }
                esp_bt_gap_dev_prop_type_t_ESP_BT_GAP_DEV_PROP_COD => {
                    DeviceProperty::Class(ClassOfDevice {
                        value: *((*property).val as *const u32),
                    })
                }
                esp_bt_gap_dev_prop_type_t_ESP_BT_GAP_DEV_PROP_RSSI => {
                    DeviceProperty::Rssi(*((*property).val as *const i8))
                }
                esp_bt_gap_dev_prop_type_t_ESP_BT_GAP_DEV_PROP_EIR => {
                    let slice = std::slice::from_raw_parts(
                        (*property).val as *const u8,
                        (*property).len as usize,
                    );
                    DeviceProperty::Eir(slice.to_vec())
                }
                _ => panic!(),
            }
        }
    }
}

// see https://github.com/nopnop2002/esp-idf-a2dp-source/blob/4af86a2f1e7009f9ea5d522658eece9c4f63c6f9/main/main.c#L257
pub extern "C" fn bt_app_gap_cb(event: esp_bt_gap_cb_event_t, param: *mut esp_bt_gap_cb_param_t) {
    #[allow(non_upper_case_globals)]
    match event {
        esp_bt_gap_cb_event_t_ESP_BT_GAP_DISC_RES_EVT => unsafe {
            log::info!("Discovery result");

            let result = &(*param).disc_res;
            let mut scanned_device = ScannedDevice {
                address: result.bda,
                ..Default::default()
            };

            for i in 0..(result.num_prop) {
                scanned_device.add_property(DeviceProperty::from(
                    result.prop.add(i as usize) as *const esp_bt_gap_dev_prop_t
                ));
            }

            // let g = ESP32_BLUETOOTH_GLOBALS.discovery.clone(); // lock().unwrap();
            // log::info!("Discovery result: Globals locked");
            // let (lock, cvar) = &*g;
            let discovery = &ESP32_BLUETOOTH_GLOBALS.discovery;
            let mut result = discovery.result.lock().unwrap();
            log::info!("Discovery result: List locked");
            match &mut result.result {
                None => result.result = Some(vec![scanned_device]),
                Some(list) => list.push(scanned_device),
            }

            log::info!("Discovery result: Notifying cvar");
            discovery.condvar.notify_all();
        }, // filter_inquiry_scan_result(param),
        esp_bt_gap_cb_event_t_ESP_BT_GAP_DISC_STATE_CHANGED_EVT => {
            let state = unsafe { (*param).disc_st_chg.state };
            match state {
                esp_bt_gap_discovery_state_t_ESP_BT_GAP_DISCOVERY_STOPPED => {
                    log::info!("Discovery stopped");

                    let discovery = &ESP32_BLUETOOTH_GLOBALS.discovery;
                    log::info!("Got discovery from globals");
                    let mut result = discovery.result.lock().unwrap();
                    log::info!("Locked discovery result");

                    result.state = AsyncCallState::Finished;
                    discovery.condvar.notify_all();
                    log::info!("Notified discovery cancelled");
                }
                esp_bt_gap_discovery_state_t_ESP_BT_GAP_DISCOVERY_STARTED => {
                    log::info!("GAP Discovery started.");
                }
                _ => panic!(),
            }
        }
        esp_bt_gap_cb_event_t_ESP_BT_GAP_ACL_CONN_CMPL_STAT_EVT => {
            // ACL connection complete status event
            unsafe {
                let status = (*param).acl_conn_cmpl_stat;
                let connection = &ESP32_BLUETOOTH_GLOBALS.connection;
                let result = &mut connection.result.lock().unwrap();

                result.result = Some(
                    #[allow(non_upper_case_globals)]
                    match status.stat {
                        esp_bt_status_t_ESP_BT_STATUS_SUCCESS
                        | esp_bt_status_t_ESP_BT_STATUS_HCI_SUCCESS => Ok(()),
                        _ => Err(anyhow!(format!("Status is {}", status.stat))),
                    },
                );
                result.state = AsyncCallState::Finished;
                connection.condvar.notify_all();
                log::info!(
                    "Connection status = {}, handle={}",
                    status.stat,
                    status.handle
                );
            }
        }
        esp_bt_gap_cb_event_t_ESP_BT_GAP_RMT_SRVCS_EVT
        | esp_bt_gap_cb_event_t_ESP_BT_GAP_RMT_SRVC_REC_EVT => todo!(),

        esp_bt_gap_cb_event_t_ESP_BT_GAP_AUTH_CMPL_EVT => todo!(),

        esp_bt_gap_cb_event_t_ESP_BT_GAP_PIN_REQ_EVT => unsafe {
            let mut param = (*param).pin_req;
            log::info!(
                "ESP_BT_GAP_PIN_REQ_EVT min_16_digit: {}",
                param.min_16_digit
            );
            if param.min_16_digit {
                log::info!("Input pin code: 0000 0000 0000 0000");
                let mut pin_code: esp_bt_pin_code_t = [0; 16];
                esp!(esp_bt_gap_pin_reply(
                    param.bda.as_mut_ptr(),
                    true,
                    16,
                    pin_code.as_mut_ptr()
                ))
                .unwrap();
            } else {
                log::info!("Input pin code: 1234");
                #[allow(clippy::char_lit_as_u8)]
                let mut pin_code: esp_bt_pin_code_t = [
                    '1' as u8, '2' as u8, '3' as u8, '4' as u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                ];
                esp!(esp_bt_gap_pin_reply(
                    param.bda.as_mut_ptr(),
                    true,
                    4,
                    pin_code.as_mut_ptr()
                ))
                .unwrap();
            }
        },
        esp_bt_gap_cb_event_t_ESP_BT_GAP_MODE_CHG_EVT => unsafe {
            log::info!("ESP_BT_GAP_MODE_CHG_EVT mode: {}", (*param).mode_chg.mode);
        },
        _ => log::info!("GAP event {}", event),
    }
}
