// #![feature(async_fn_in_trait)]
#![feature(stmt_expr_attributes)]
use anyhow::Result;
use futures::executor::block_on;
use std::time;
use sysinfo::{RefreshKind, System, SystemExt};
// use esp_idf_hal::prelude::*;
use esp_idf_sys as _;

use crate::{
    bluetooth_esp32::ESP32Bluetooth,
    bluetooth_hal::{AsyncCallState, Bluetooth},
    uuids::Bluetooth16bitUUIDEnum,
}; // If using the `binstart` feature of `esp-idf-sys`, always keep this module imported
   // use log::info;

mod audio;
mod bluetooth_esp32;
mod bluetooth_esp32_a2dp;
mod bluetooth_gap_esp32;
mod bluetooth_gap_hal;
mod bluetooth_hal;
mod esp32;
mod sd_card;
mod uuids;

/* Pins used in on LoLin D32 Pro */
const PIN_SDCARD_CS: i32 = 4;
const PIN_SDCARD_SCLK: i32 = 18;
const PIN_SDCARD_MOSI: i32 = 23;
const PIN_SDCARD_MISO: i32 = 19;

const ONE_SECOND: time::Duration = time::Duration::from_millis(1000);

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

    let mut esp32 = esp32::Esp32::new();
    esp32.init();

    // let kind = ;
    let kind2 = RefreshKind::new().with_memory();
    let mut system = System::new_with_specifics(kind2);

    print_memory(&mut system);

    log::info!("Initializing SD card");
    sd_card::init(
        PIN_SDCARD_CS,
        PIN_SDCARD_SCLK,
        PIN_SDCARD_MISO,
        PIN_SDCARD_MOSI,
    )?;

    // let mut wifi = WifiDriver::new()
    let mut bluetooth = ESP32Bluetooth::new(&mut esp32, true, true);

    log::info!("Bluetooth created 1");
    print_memory(&mut system);

    bluetooth.init("Piccolo")?;

    log::info!("Starting scanning");
    print_memory(&mut system);
    /*
       let pair = Arc::new((Mutex::new(vec![]), Condvar::new()));
       let pair2 = Arc::clone(&pair);
    */
    let discovery = &*bluetooth.gap_start_discovery().unwrap();

    let mut result = discovery.result.lock().unwrap();
    log::info!("Discovery list locked");
    let mut found = false;
    let mut device = None;

    while !found && result.state == AsyncCallState::InProgress {
        log::info!("Waiting for discovery");
        result = discovery.condvar.wait(result).unwrap();
        log::info!("Got discovery");
        if let Some(list) = &mut result.result {
            device = list.pop()
        }
        log::info!("Device is {:?}", device);
        // device = result.result.unwrap().pop();
        // let d2 = device.take().unwrap();

        if device
            .as_ref()
            .unwrap()
            .has_16bit_uuid(Bluetooth16bitUUIDEnum::AdvancedAudioDistribution as u16)
        {
            found = true;
            log::info!("Found A2DP device")
        } else {
            log::info!("Ignoring device" /* device.unwrap().name.take().unwrap() */);
        }
    }
    drop(result); // must unlock list here

    match device {
        Some(dev) => {
            // async {
            log::info!("Device is {}", &dev.name.unwrap());
            log::info!("Cancelling discovery");
            block_on(bluetooth.gap_cancel_discovery())?;
            print_memory(&mut system);

            log::info!("Discovery cancelled, connecting...");
            block_on(bluetooth.a2dp_connect(&dev.address))?;

            log::info!("Connected!");
            print_memory(&mut system);

            block_on(audio::playback_task(&mut bluetooth))?;

            // Ok(())
            // };
        }
        None => log::info!("Bluetooth search timed out"),
    };

    /*
    drop(bluetooth);

    let _playback_thread = thread::Builder::new()
        .name("playback_thread".to_owned())
        .stack_size(4096)
        .spawn(move || {
            if let Err(e) = playback_task(state, &bluetooth_mutex) {
                log::warn!("Error running playback thread {e:?}");
            }
        })?;
        */
    println!("Hello, world!");

    std::thread::sleep(ONE_SECOND);

    bluetooth.deinit()?;
    // drop(bluetooth);

    Ok(())
}
