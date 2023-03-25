use std::{thread, time::Duration};

use async_trait::async_trait;
use embedded_svc::wifi::{ClientConfiguration, Configuration};
use esp_idf_svc::wifi::WifiDriver;

use crate::{
    boot_state::Boot,
    playback_state::Playback,
    state_machine::{ConcreteState, StateEnum, StateExecutor, StateMachine},
};

pub struct WifiConnect {}

impl<'a> From<ConcreteState<'a, Boot>> for ConcreteState<'a, WifiConnect> {
    fn from(val: ConcreteState<Boot>) -> ConcreteState<WifiConnect> {
        // start trying to connect
        // ... Logic prior to transition
        ConcreteState::<WifiConnect> {
            machine: val.machine.clone(),
            // ... attr: val.attr
            state: WifiConnect { /* wifi_driver */ },
        }
    }
}

#[async_trait]
impl<'a> StateExecutor<'a> for ConcreteState<'a, WifiConnect> {
    async fn execute(self, machine: &mut StateMachine) -> StateEnum<'a> {
        // start trying to connect
        let modem = machine.modem.take().unwrap();
        let event_loop = machine.eventloop.take().unwrap();

        let mut wifi_driver = WifiDriver::new(modem, event_loop, None).unwrap();

        wifi_driver
            .set_configuration(&Configuration::Client(ClientConfiguration {
                ssid: "<SSID>>".into(),
                password: "<Password>".into(),
                ..Default::default()
            }))
            .unwrap();

        // Unsure if we need to change power management
        // Probably DTIM configuration parameter needs to be set for default power management to work.
        // unsafe { esp!(esp_wifi_set_ps(wifi_ps_type_t_WIFI_PS_NONE)) }
        //     .expect("Failed to disable Wifi power management");

        wifi_driver.start().unwrap();

        // todo: replace with async wifi driver
        wifi_driver.connect().unwrap();

        let sleep_duration = Duration::from_millis(100);

        log::info!("Waiting for Wifi to connect");
        while !wifi_driver.is_connected().unwrap() {
            thread::sleep(sleep_duration);
        }
        log::info!("Wifi connected");

        log::info!("Transitioning to playback");

        // todo: go to some other state here
        StateEnum::Playback(ConcreteState::<Playback>::from(self))
    }
}
