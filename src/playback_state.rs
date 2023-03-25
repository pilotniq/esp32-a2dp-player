use async_trait::async_trait;

use crate::{
    audio,
    bluetooth_esp32::ESP32Bluetooth,
    bluetooth_gap_hal::ScannedDevice,
    bluetooth_hal::Bluetooth,
    boot_state::Boot,
    sd_card,
    state_machine::{ConcreteState, StateEnum, StateExecutor, StateMachine},
    uuids::Bluetooth16bitUUIDEnum,
    wifi_connect_state::WifiConnect,
};

/* TODO: The pins used should go in a struct describing the board / hardware used */
/* Pins used in on LoLin D32 Pro */
const PIN_SDCARD_CS: i32 = 4;
const PIN_SDCARD_SCLK: i32 = 18;
const PIN_SDCARD_MOSI: i32 = 23;
const PIN_SDCARD_MISO: i32 = 19;

pub struct Playback {}

impl<'a> From<ConcreteState<'a, WifiConnect>> for ConcreteState<'a, Playback> {
    fn from(value: ConcreteState<'a, WifiConnect>) -> Self {
        ConcreteState::<Playback> {
            machine: value.machine.clone(),
            state: Playback {},
        }
    }
}

#[async_trait]
impl<'a> StateExecutor<'a> for ConcreteState<'a, Playback> {
    async fn execute(self, machine: &mut StateMachine) -> StateEnum<'a> {
        log::info!("Initializing SD card");
        sd_card::init(
            PIN_SDCARD_CS,
            PIN_SDCARD_SCLK,
            PIN_SDCARD_MISO,
            PIN_SDCARD_MOSI,
        )
        .unwrap(); // TODO: Handle errors

        let mut bluetooth = ESP32Bluetooth::new(true, true);

        log::info!("Bluetooth created 1");

        bluetooth
            .pre_init(&mut machine.esp32)
            .expect("Bluetooth preinit failed");

        bluetooth.init("Piccolo").unwrap(); // TODO: Error handling

        log::info!("Starting scanning");

        let mut discovery = bluetooth
            .gap_start_discovery()
            .expect("Bluetooth start discovery failed");
        let mut found: Option<ScannedDevice> = None;

        while found.is_none() && !discovery.is_closed() {
            log::info!("Waiting for discovery");
            match discovery.recv().await {
                Ok(device) => {
                    log::info!("Device is {:?}", device);

                    if device
                        .has_16bit_uuid(Bluetooth16bitUUIDEnum::AdvancedAudioDistribution as u16)
                    {
                        found.replace(device);
                        log::info!("Found A2DP device")
                    } else {
                        log::info!("Ignoring device");
                    }
                }
                Err(e) => {
                    if !discovery.is_closed() {
                        panic!("Error waiting for discovery: {e}");
                    }
                }
            } // end of while not found loop
        }

        match found {
            Some(dev) => {
                // async {
                log::info!("Device is {}", &dev.name.unwrap());
                log::info!("Cancelling discovery");
                let cancel_result = bluetooth.gap_cancel_discovery();
                if let Err(e) = cancel_result {
                    log::info!("Ignoring cancel discovery error {e}");
                }

                log::info!("Discovery cancelled, connecting...");
                bluetooth
                    .a2dp_connect(&dev.address)
                    .await
                    .expect("a2dp_connect failed");

                log::info!("Connected!");

                audio::playback_task(&mut bluetooth)
                    .await
                    .expect("Playback failed");
            }
            None => log::info!("Bluetooth search timed out"),
        };

        bluetooth.deinit().unwrap(); // TODO: Error handling

        let mut machine = self.machine.lock().await;
        bluetooth
            .post_deinit(&mut machine.esp32)
            .expect("Bluetooth post de_init failed");
        drop(machine);

        log::info!("Playback state completed, going to boot state");

        StateEnum::Boot(ConcreteState::<Boot>::from(self))
    }
}
