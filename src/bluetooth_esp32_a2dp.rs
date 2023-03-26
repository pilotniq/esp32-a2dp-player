use anyhow::Result;

use futures::executor::block_on;
use lazy_static::lazy_static;

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

use crate::bluetooth_gap_hal::ScannedDevice;
use crate::bluetooth_hal::{AsyncCall, BDAddr, Stream};

pub struct ESP32A2DP {}

#[derive(Clone, Copy)]
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
    pub static ref CONNECTION_STATE: std::sync::Mutex<ConnectionState> =
        std::sync::Mutex::new(ConnectionState::default());
    pub static ref CONNECTION_STATE_EVENT: event_listener::Event = event_listener::Event::new();
    pub static ref DISCOVERY_CHANNEL: (
        async_broadcast::Sender<ScannedDevice>,
        async_broadcast::Receiver<ScannedDevice>
    ) = async_broadcast::broadcast(2);
    pub static ref A2DP: std::sync::Mutex<ESP32A2DP> = std::sync::Mutex::new(ESP32A2DP::new());
    pub static ref SRC_READY_CALL: AsyncCall::<Result<()>> = AsyncCall::<Result<()>>::new();
    static ref PLAY_STATE: futures_locks::Mutex<PlayState> =
        futures_locks::Mutex::new(PlayState { stream: None });
}

impl ESP32A2DP {
    fn new() -> Self {
        ESP32A2DP {}
    }

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
        // tood: have the FnOnce be able to return error
        log::info!("A2DP Calling connect");
        let addr_copy = *addr;

        let mut listener = CONNECTION_STATE_EVENT.listen();
        let addr3 = addr_copy;
        unsafe {
            esp!(esp_a2d_source_connect(addr3.as_ptr() as *mut u8))?;
        };

        loop {
            log::info!("connect: waiting for event");
            listener.await;
            log::info!("connect: got event");

            let guard = CONNECTION_STATE.lock().unwrap();
            log::info!("event is {}", guard.state);

            if guard.state == esp_a2d_connection_state_t_ESP_A2D_CONNECTION_STATE_CONNECTING {
                log::info!("Got connecting");
                break;
            }
            listener = CONNECTION_STATE_EVENT.listen();
        }

        let mut result_opt: Option<ConnectionState> = None;

        log::info!("Connecting...");
        loop {
            let listener = CONNECTION_STATE_EVENT.listen();

            listener.await;

            let guard = CONNECTION_STATE.lock().unwrap();
            if guard.state == esp_a2d_connection_state_t_ESP_A2D_CONNECTION_STATE_DISCONNECTED
                || guard.state == esp_a2d_connection_state_t_ESP_A2D_CONNECTION_STATE_CONNECTED
            {
                result_opt.replace(*guard);
                break;
            }
        }

        log::info!("A2DP connect: After state wait");

        let result = result_opt.unwrap();

        #[allow(non_upper_case_globals)]
        match result.state {
            esp_a2d_connection_state_t_ESP_A2D_CONNECTION_STATE_CONNECTED => {
                log::info!("A2DP connect Returning Ok");
                Ok(())
            }
            esp_a2d_connection_state_t_ESP_A2D_CONNECTION_STATE_DISCONNECTED => {
                log::info!("A2DP connect Returning Error");
                Err(anyhow::anyhow!(result.disconnect_reason))
            }

            _ => panic!("Invalid state"),
        }
    }

    pub async fn play(stream: Box<dyn Stream<i16>>) -> Result<()> {
        // Setup playback
        let mut play_state = PLAY_STATE.lock().await;
        play_state.stream = Some(stream);

        drop(play_state);

        log::info!("play: before source ready do and wait");

        SRC_READY_CALL
            .do_and_wait(|| unsafe {
                esp!(esp_a2d_media_ctrl(
                    esp_a2d_media_ctrl_t_ESP_A2D_MEDIA_CTRL_CHECK_SRC_RDY
                ))
                .expect("CHECK_SRC_READY failed");
            })
            .await?;

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
            _ => panic!("Unknown A2DP callback event {event}"),
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
                    state.state = conn_state.state;
                    CONNECTION_STATE_EVENT.notify(usize::MAX);
                }
                esp_a2d_connection_state_t_ESP_A2D_CONNECTION_STATE_DISCONNECTED
                | esp_a2d_connection_state_t_ESP_A2D_CONNECTION_STATE_CONNECTED => {
                    state.state = conn_state.state;
                    state.disconnect_reason = conn_state.disc_rsn;

                    CONNECTION_STATE_EVENT.notify(usize::MAX);
                }
                esp_a2d_connection_state_t_ESP_A2D_CONNECTION_STATE_DISCONNECTING => {
                    log::info!("A2DP: Disconnecting");
                    state.state = conn_state.state;
                    CONNECTION_STATE_EVENT.notify(usize::MAX);
                }
                _ => panic!("Invalid connection state: {}", conn_state.state),
            }
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
