#![feature(stmt_expr_attributes)]
use std::sync::Arc;

use anyhow::Result;
use boot_state::Boot;
use esp_idf_hal::prelude::Peripherals;
use esp_idf_svc::{eventloop::EspSystemEventLoop, nvs::EspDefaultNvsPartition};
use esp_idf_sys as _;
use futures::executor::block_on;
use state_machine::ConcreteState;
use sysinfo::{RefreshKind, System, SystemExt};

use crate::state_machine::StateMachine;
// If using the `binstart` feature of `esp-idf-sys`, always keep this module imported
// use log::info;

mod audio;
mod bluetooth_esp32;
mod bluetooth_esp32_a2dp;
mod bluetooth_gap_esp32;
mod bluetooth_gap_hal;
mod bluetooth_hal;
mod boot_state;
mod esp32;
mod playback_state;
mod sd_card;
mod state_machine;
mod uuids;
mod wifi_connect_state;

fn print_memory(system: &mut System) {
    system.refresh_system();
    log::info!(
        "RAM: total={}, available={}, free={}",
        system.total_memory(),
        system.available_memory(),
        system.free_memory()
    );
}
fn main() -> Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71

    let esp32 = esp32::Esp32::new();
    esp32.init();

    let kind2 = RefreshKind::new().with_memory();
    let mut system = System::new_with_specifics(kind2);

    print_memory(&mut system);

    let peripherals = Peripherals::take().unwrap();
    let sys_loop = EspSystemEventLoop::take().unwrap();
    let nvs = EspDefaultNvsPartition::take().unwrap();

    log::info!("Before creating state machine");
    let state_machine = StateMachine::new(peripherals.modem, sys_loop, nvs, esp32);
    log::info!("After creating state machine");

    block_on(ConcreteState::<Boot>::new(state_machine.clone()));

    log::info!("After creating Boot state");

    loop {
        // TODO: Implement async executor so we don't use block_on.
        log::info!("Before executing state machine");
        block_on(run_state_machine(&state_machine));
        log::info!("After executing state machine");
    }
}

async fn run_state_machine<'a>(state_machine: &Arc<futures::lock::Mutex<StateMachine<'a>>>) {
    let mut sm = state_machine.lock().await;
    sm.execute().await;
}
