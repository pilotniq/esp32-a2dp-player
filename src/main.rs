#![feature(stmt_expr_attributes)]
use anyhow::Result;
use bluetooth_gap_hal::ScannedDevice;
use esp_idf_sys as _;
use futures::executor::block_on;
use std::time;
use sysinfo::{RefreshKind, System, SystemExt};

use crate::{
    bluetooth_esp32::ESP32Bluetooth, bluetooth_hal::Bluetooth, uuids::Bluetooth16bitUUIDEnum,
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

    block_on(start(&mut bluetooth))?;

    println!("Hello, world!");

    std::thread::sleep(ONE_SECOND);

    bluetooth.deinit()?;

    Ok(())
}

async fn start<'a>(bluetooth: &mut ESP32Bluetooth<'a>) -> Result<()> {
    let mut discovery = bluetooth.gap_start_discovery()?;
    let mut found: Option<ScannedDevice> = None;

    while found.is_none() && !discovery.is_closed() {
        log::info!("Waiting for discovery");
        match discovery.recv().await {
            Ok(device) => {
                log::info!("Device is {:?}", device);

                if device.has_16bit_uuid(Bluetooth16bitUUIDEnum::AdvancedAudioDistribution as u16) {
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
            bluetooth.a2dp_connect(&dev.address).await?;

            log::info!("Connected!");

            audio::playback_task(bluetooth).await?;
        }
        None => log::info!("Bluetooth search timed out"),
    };
    Ok(())
}
