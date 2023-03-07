use anyhow::Result;

use futures::executor::block_on;
// use futures::executor::block_on;
use lazy_static::lazy_static;
use std::sync::{Condvar, Mutex};

use esp_idf_sys::{
    esp, esp_a2d_cb_event_t, esp_a2d_cb_event_t_ESP_A2D_AUDIO_CFG_EVT,
    esp_a2d_cb_event_t_ESP_A2D_AUDIO_STATE_EVT, esp_a2d_cb_event_t_ESP_A2D_CONNECTION_STATE_EVT,
    esp_a2d_cb_event_t_ESP_A2D_MEDIA_CTRL_ACK_EVT, esp_a2d_cb_event_t_ESP_A2D_PROF_STATE_EVT,
    /* esp_a2d_cb_event_t_ESP_A2D_SNK_PSC_CFG_EVT,
    esp_a2d_cb_event_t_ESP_A2D_SNK_GET_DELAY_VALUE_EVT,
    esp_a2d_cb_event_t_ESP_A2D_SNK_SET_DELAY_VALUE_EVT */
    esp_a2d_cb_param_t, esp_a2d_connection_state_t,
    esp_a2d_connection_state_t_ESP_A2D_CONNECTION_STATE_CONNECTED,
    esp_a2d_connection_state_t_ESP_A2D_CONNECTION_STATE_CONNECTING,
    esp_a2d_connection_state_t_ESP_A2D_CONNECTION_STATE_DISCONNECTED,
    esp_a2d_connection_state_t_ESP_A2D_CONNECTION_STATE_DISCONNECTING, esp_a2d_disc_rsn_t,
    esp_a2d_disc_rsn_t_ESP_A2D_DISC_RSN_NORMAL, esp_a2d_media_ctrl,
    esp_a2d_media_ctrl_ack_t_ESP_A2D_MEDIA_CTRL_ACK_SUCCESS,
    esp_a2d_media_ctrl_t_ESP_A2D_MEDIA_CTRL_CHECK_SRC_RDY,
    esp_a2d_media_ctrl_t_ESP_A2D_MEDIA_CTRL_START, esp_a2d_register_callback,
    /* esp_a2d_cb_event_t, */ esp_a2d_source_connect, esp_a2d_source_init,
    esp_a2d_source_register_data_callback,
};

use crate::bluetooth_esp32::ESP32_BLUETOOTH_GLOBALS;
use crate::bluetooth_hal::{AsyncCall, BDAddr, Stream};

pub struct ESP32A2DP {}

pub struct ConnectionState {
    state: esp_a2d_connection_state_t,
    disconnect_reason: esp_a2d_disc_rsn_t,
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self {
            state: esp_a2d_connection_state_t_ESP_A2D_CONNECTION_STATE_DISCONNECTED,
            disconnect_reason: esp_a2d_disc_rsn_t_ESP_A2D_DISC_RSN_NORMAL,
        }
    }
}

struct PlayState {
    stream: Option<Box<dyn Stream<i16>>>,
    // src_ready_call: AsyncCall<Result<()>>,
}

lazy_static! {
    pub static ref CONNECTION_CALL: AsyncCall<Result<()>> = AsyncCall::<Result<()>>::default();
    pub static ref CONNECTION_STATE_CONDVAR: Condvar = Condvar::new();
    pub static ref CONNECTION_STATE: Mutex<ConnectionState> = Mutex::new(Default::default());
    pub static ref A2DP: Mutex<ESP32A2DP> = Mutex::new(ESP32A2DP::new());
    pub static ref SRC_READY_CALL: AsyncCall::<Result<()>> = AsyncCall::<Result<()>>::default();
    static ref PLAY_STATE: futures_locks::Mutex<PlayState> =
        futures_locks::Mutex::new(PlayState { stream: None });
}

impl ESP32A2DP {
    fn new() -> Self {
        ESP32A2DP {}
    }
    /*
       fn get() -> MutexGuard<ESP32A2DP> {
           A2DP.lock()
       }
    */
    pub fn init(&self) -> Result<()> {
        unsafe {
            esp!(esp_a2d_source_init())?;
            esp!(esp_a2d_register_callback(Some(ESP32A2DP::bt_app_a2d_cb)))?;
            esp!(esp_a2d_source_register_data_callback(Some(
                ESP32A2DP::bt_app_a2d_data_cb
            )))?;
        }
        Ok(())
    }

    pub async fn connect(addr: &BDAddr) -> Result<()> {
        log::info!("A2DP Calling connect");
        unsafe {
            esp!(esp_a2d_source_connect(addr.as_ptr() as *mut u8))?;
        };
        // wait until connecting
        /*
            wait_until_condvar(CONNECTION_STATE_CONDVAR, CONNECTION_STATE,
                |s: MutexGuard<ConnectionState>| s.state == esp_a2d_connection_state_t_ESP_A2D_CONNECTION_STATE_CONNECTING);
        */
        log::info!("A2DP Connect: waiting for connection");
        ESP32_BLUETOOTH_GLOBALS.connection.wait()?;

        log::info!("A2DP Connect: locking connection state");
        let mut state = CONNECTION_STATE.lock().unwrap();
        log::info!("A2DP Connect: state={}", state.state);

        /*
        loop {
            if state.state == esp_a2d_connection_state_t_ESP_A2D_CONNECTION_STATE_CONNECTING {
                break;
            } else {
                let result = CONNECTION_STATE_CONDVAR.wait(state);
                state = result.expect("Waiting for connecting failed");
            }
        }
         */
        // Doesn't work, borrow issues in while loop
        while state.state != esp_a2d_connection_state_t_ESP_A2D_CONNECTION_STATE_CONNECTING
            && state.state != esp_a2d_connection_state_t_ESP_A2D_CONNECTION_STATE_CONNECTED
        {
            state = CONNECTION_STATE_CONDVAR
                .wait(state)
                .expect("Wait for connecting failed");
        }

        log::info!("A2DP Connecting X2");

        log::info!("A2DP connect before while loop");
        while state.state != esp_a2d_connection_state_t_ESP_A2D_CONNECTION_STATE_CONNECTED
            && state.state != esp_a2d_connection_state_t_ESP_A2D_CONNECTION_STATE_DISCONNECTED
        {
            log::info!("A2DP connect, state={}", state.state);
            state = CONNECTION_STATE_CONDVAR
                .wait(state)
                .expect("Wait for connection result failed");
        }

        log::info!("A2DP connect: After state wait");

        #[allow(non_upper_case_globals)]
        match state.state {
            esp_a2d_connection_state_t_ESP_A2D_CONNECTION_STATE_CONNECTED => {
                log::info!("A2DP connect Returning Ok");
                Ok(())
            }
            esp_a2d_connection_state_t_ESP_A2D_CONNECTION_STATE_DISCONNECTED => {
                log::info!("A2DP connect Returning Error");
                Err(anyhow::anyhow!(state.disconnect_reason))
            }

            _ => panic!("Invalid state"),
        }
    }

    pub async fn play(stream: Box<dyn Stream<i16>>) -> Result<()> {
        // Setup playback
        let mut play_state = PLAY_STATE.lock().await;
        play_state.stream = Some(stream);
        // let src_ready_call = &play_state.src_ready_call;
        drop(play_state);

        unsafe {
            esp!(esp_a2d_media_ctrl(
                esp_a2d_media_ctrl_t_ESP_A2D_MEDIA_CTRL_CHECK_SRC_RDY
            ))?;
        }

        log::info!("Waiting for source ready");
        // let src_ready_call = SRC_READY_CALL.
        SRC_READY_CALL.wait()?;

        log::info!("Source is ready. Starting media.");

        unsafe {
            esp!(esp_a2d_media_ctrl(
                esp_a2d_media_ctrl_t_ESP_A2D_MEDIA_CTRL_START
            ))?;
        }

        // somehow wait until playback is finished
        Ok(())
    }
    extern "C" fn bt_app_a2d_cb(event: esp_a2d_cb_event_t, param: *mut esp_a2d_cb_param_t) {
        #[allow(non_upper_case_globals)]
        match event {
            // connection state changed event
            esp_a2d_cb_event_t_ESP_A2D_CONNECTION_STATE_EVT => {
                let me = A2DP.lock().unwrap();
                me.process_connection_state_evt(param)
            }
            esp_a2d_cb_event_t_ESP_A2D_AUDIO_STATE_EVT => unsafe {
                log::info!(
                    "Got ESP_A2D_AUDIO_STATE_EVT, state={}",
                    &(*param).audio_stat.state
                );
            }, // audio stream transmission state changed event
            esp_a2d_cb_event_t_ESP_A2D_AUDIO_CFG_EVT => todo!(), // used only for sink
            esp_a2d_cb_event_t_ESP_A2D_MEDIA_CTRL_ACK_EVT => {
                log::info!("Got ESP_A2D_MEDIA_CTRL_ACK_EVT");
                // let play_state = block_on(PLAY_STATE.lock());
                // log::info!("bt_app_a2d_cb MEDIA_CTRL_ACK locked PLAY_STATE");
                unsafe {
                    let stat = &(*param).media_ctrl_stat;
                    match stat.cmd {
                        esp_a2d_media_ctrl_t_ESP_A2D_MEDIA_CTRL_CHECK_SRC_RDY => {
                            #[allow(non_upper_case_globals)]
                            let result = match stat.status {
                                esp_a2d_media_ctrl_ack_t_ESP_A2D_MEDIA_CTRL_ACK_SUCCESS => Ok(()),
                                _ => Err(anyhow::anyhow!(format!("Media error {}", stat.status))),
                            };
                            log::info!("Got ESP_A2D_MEDIA_CTRL_ACK_EVT: calling complete");
                            SRC_READY_CALL.complete(result);
                            log::info!("Got ESP_A2D_MEDIA_CTRL_ACK_EVT: after complete");
                        }
                        esp_a2d_media_ctrl_t_ESP_A2D_MEDIA_CTRL_START => {
                            log::info!("Media start ACK, status = {}", stat.status);
                        }
                        _ => todo!(),
                    }
                }
            } // acknowledge event in response to media control commands
            esp_a2d_cb_event_t_ESP_A2D_PROF_STATE_EVT => todo!(), // indicate a2dp init&deinit complete
            // esp_a2d_cb_event_t_ESP_A2D_SNK_PSC_CFG_EVT => todo!(), // protocol service capabilities configured，only used for A2DP SINK
            // esp_a2d_cb_event_t_ESP_A2D_SNK_SET_DELAY_VALUE_EVT => todo!(), // indicate a2dp sink set delay report value complete, only used for A2DP SINK
            // esp_a2d_cb_event_t_ESP_A2D_SNK_GET_DELAY_VALUE_EVT => todo!(), // indicate a2dp sink get delay report value complete, only used for A2DP SINK
            // esp_a2d_cb_event_t_ESP_A2D_REPORT_SNK_DELAY_VALUE_EVT => todo!(), // report delay value, only used for A2DP SRC
            _ => panic!("Unknown A2DP callback event {}", event),
        }
    }

    fn process_connection_state_evt(&self, param: *mut esp_a2d_cb_param_t) {
        unsafe {
            let conn_state = &(*param).conn_stat;

            log::info!(
                "A2DP connection state: {}, disconnect reason {}",
                conn_state.state,
                conn_state.disc_rsn
            );

            let mut state = CONNECTION_STATE
                .lock()
                .expect("Failed to lock connection state");

            #[allow(non_upper_case_globals)]
            match conn_state.state {
                esp_a2d_connection_state_t_ESP_A2D_CONNECTION_STATE_CONNECTING => {
                    log::info!("A2DP: Connecting");
                }
                esp_a2d_connection_state_t_ESP_A2D_CONNECTION_STATE_DISCONNECTED
                | esp_a2d_connection_state_t_ESP_A2D_CONNECTION_STATE_CONNECTED => {
                    state.state = conn_state.state;
                    state.disconnect_reason = conn_state.disc_rsn;

                    CONNECTION_STATE_CONDVAR.notify_all();
                }
                esp_a2d_connection_state_t_ESP_A2D_CONNECTION_STATE_DISCONNECTING => {
                    log::info!("A2DP: Disconnecting");
                }
                _ => panic!("Invalid connection state: {}", conn_state.state),
            }

            /*
                    a2d = (esp_a2d_cb_param_t *)(param);
            if (a2d->conn_stat.state == ESP_A2D_CONNECTION_STATE_CONNECTED) {
                ESP_LOGI(BT_AV_TAG, "a2dp connected");
                s_a2d_state =  APP_AV_STATE_CONNECTED;
                s_media_state = APP_AV_MEDIA_STATE_IDLE;
                esp_bt_gap_set_scan_mode(ESP_BT_NON_CONNECTABLE, ESP_BT_NON_DISCOVERABLE);
            } else if (a2d->conn_stat.state == ESP_A2D_CONNECTION_STATE_DISCONNECTED) {
                s_a2d_state =  APP_AV_STATE_UNCONNECTED;
            }
             */
        }
    }

    extern "C" fn bt_app_a2d_data_cb(buf: *mut u8, len: i32) -> i32 {
        if len == -1 {
            log::info!("bt_app_a2d_data_cb: got negative len {}, returning 0", len);
            return 0;
        } else if len < 0 {
            return len; // just a hack, I don't know why I'm getting -512 as length.
        }

        let mut play_state = block_on(PLAY_STATE.lock());
        match &mut play_state.stream {
            Some(stream) => {
                // ignore byte ordering for now, and hope for the best
                let buffer_view_i16 = unsafe {
                    // log::info!("bt_app_a2d_data_cb: len={}", len);
                    let slice_len = (len / 2) as usize;
                    std::slice::from_raw_parts_mut(buf as *mut i16, slice_len)
                };

                match stream.read(buffer_view_i16) {
                    Ok(count) => (count * 2) as i32,
                    Err(err) => {
                        log::error!("bt_app_a2d_data_cb: error reading from stream: {}", err);
                        0
                    }
                }
            }
            None => {
                log::error!("bt_app_a2d_data_cb: no stream to read from!");
                0
            }
        }
    }
}
