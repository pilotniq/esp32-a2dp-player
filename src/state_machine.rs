//   New approach - there is only ever one ConcreteState in existence, it is the state of the system

use std::sync::Arc;

use crate::{
    boot_state::Boot, esp32::Esp32, playback_state::Playback, wifi_connect_state::WifiConnect,
};
use async_trait::async_trait;
use esp_idf_hal::modem::Modem;
use esp_idf_svc::{
    eventloop::{EspEventLoop, System},
    nvs::{EspNvsPartition, NvsDefault},
};
use futures::lock::Mutex;

pub enum StateEnum<'a> {
    Boot(ConcreteState<'a, Boot>),
    WifiConnect(ConcreteState<'a, WifiConnect>),
    Playback(ConcreteState<'a, Playback>),
}

#[async_trait]
pub trait StateExecutor<'a> {
    async fn execute(self, machine: &mut StateMachine<'a>) -> StateEnum<'a>;
}

pub struct StateMachine<'a> {
    pub state: Option<StateEnum<'a>>,
    pub modem: Option<Modem>,
    pub eventloop: Option<EspEventLoop<System>>,
    pub nvs_partition: Option<EspNvsPartition<NvsDefault>>, // peripherals: Peripherals
    pub esp32: Esp32,
}

impl<'a> StateMachine<'a> {
    pub fn new(
        modem: Modem,
        eventloop: EspEventLoop<System>,
        nvs_partition: EspNvsPartition<NvsDefault>,
        esp32: Esp32,
    ) -> Arc<Mutex<StateMachine<'a>>> {
        let me = StateMachine {
            modem: Some(modem),
            eventloop: Some(eventloop),
            nvs_partition: Some(nvs_partition),
            state: None,
            esp32,
        };
        Arc::new(Mutex::new(me))
    }

    pub async fn execute(&mut self) {
        let state = self.state.take();
        self.state = match state.unwrap() {
            StateEnum::Boot(state) => Some(state.execute(self).await),
            StateEnum::WifiConnect(state) => Some(state.execute(self).await),
            StateEnum::Playback(state) => Some(state.execute(self).await),
        };
    }
    pub fn set_state(&mut self, new_state: StateEnum<'a>) {
        self.state = Some(new_state);
    }
}

pub struct ConcreteState<'a, S> {
    pub state: S,
    pub machine: Arc<Mutex<StateMachine<'a>>>, // Mutex instead of RefCell to make it Send
}

impl<'a> ConcreteState<'a, Boot> {
    #[allow(clippy::new_ret_no_self)]
    pub async fn new(machine: Arc<Mutex<StateMachine<'a>>>) {
        let s = ConcreteState {
            state: Boot {},
            machine: machine.clone(),
        };

        let s2 = StateEnum::Boot(s);

        machine.lock().await.set_state(s2);
    }
}
