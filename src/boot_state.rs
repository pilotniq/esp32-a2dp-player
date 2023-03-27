use async_trait::async_trait;

use crate::{
    playback_state::Playback,
    state_machine::{ConcreteState, StateEnum, StateExecutor, StateMachine},
    wifi_connect_state::WifiConnect,
};

pub struct Boot {}

#[async_trait]
impl<'a> StateExecutor<'a> for ConcreteState<'a, Boot> {
    async fn execute(self, _machine: &mut StateMachine) -> StateEnum<'a> {
        log::info!("Executing Boot state, transitioning to WifiConnect");
        StateEnum::WifiConnect(ConcreteState::<WifiConnect>::from(self))
    }
}

#[async_trait]
impl<'a> From<ConcreteState<'a, Playback>> for ConcreteState<'a, Boot> {
    fn from(value: ConcreteState<'a, Playback>) -> Self {
        ConcreteState::<Boot> {
            machine: value.machine.clone(),
            state: Boot {},
        }
    }
}
