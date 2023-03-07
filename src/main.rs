// #![feature(async_fn_in_trait)]
#![feature(stmt_expr_attributes)]
use anyhow::Result;
use bluetooth_hal::Stream;
use core::time;
use futures::executor::block_on;
use std::{fs::File, io::Read};
// use esp_idf_hal::prelude::*;
use esp_idf_sys as _;

use crate::{
    bluetooth_esp32::ESP32Bluetooth,
    bluetooth_hal::{AsyncCallState, Bluetooth},
    uuids::Bluetooth16bitUUIDEnum,
}; // If using the `binstart` feature of `esp-idf-sys`, always keep this module imported
   // use log::info;

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

struct FileStream {
    file: File,
}

impl Stream<i16> for FileStream {
    fn read(&mut self, buf: &mut [i16]) -> Result<usize> {
        // Todo: What happens with endianness here? Got it from https://play.rust-lang.org/?version=stable&mode=debug&edition=2018&gist=94c22f162a45e25652fa1f0ba9404078

        let buffer_view_u8 =
            unsafe { std::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, buf.len() * 2) };

        let bytes_read = self
            .file
            .read(buffer_view_u8)
            .map_err(anyhow::Error::from)?;
        Ok(bytes_read / 2)
    }
}

impl FileStream {
    pub fn new(filename: &str) -> Result<Self> {
        Ok(FileStream {
            file: File::open(filename)?,
        })
    }
}

async fn playback_task<'a>(
    /*_state: Arc<Mutex<Cell<bool>>>, */ bluetooth: &mut dyn Bluetooth<'a>,
) -> Result<()> {
    log::info!("Initializing SD card");
    /* if let Err(err) = */
    sd_card::init(
        PIN_SDCARD_CS,
        PIN_SDCARD_SCLK,
        PIN_SDCARD_MISO,
        PIN_SDCARD_MOSI,
    )?;

    // Open audio file
    let file_stream = FileStream::new("/sdcard/SUN.RAW")?;

    // let bluetoothR = bt_mutex.lock();
    // let bluetooth = &(bluetoothR?);
    bluetooth.a2dp_play(Box::new(file_stream)).await
    // drop(bluetooth);

    // block_on(future);
    // std::thread::sleep(ONE_MINUTE);

    // Ok(())
}

fn main() -> Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71

    let mut esp32 = esp32::Esp32::new();
    esp32.init();

    log::info!("ESP Initialized");

    let mut bluetooth = ESP32Bluetooth::new(&mut esp32, true, true);
    // let bluetooth = bluetooth_mutex.lock();
    // bluetooth.configure(&mut esp32, true, true);

    // let mut bluetooth = ESP32Bluetooth::new(&mut esp32, true, true);

    log::info!("Bluetooth created");

    bluetooth.init("Piccolo")?;

    log::info!("Starting scanning");
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
        // log::info!("Waiting for discovery");
        result = discovery.condvar.wait(result).unwrap();
        // log::info!("Got discovery");
        if let Some(list) = &mut result.result {
            device = list.pop()
        }
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

            log::info!("Discovery cancelled, connecting...");
            block_on(bluetooth.a2dp_connect(&dev.address))?;

            log::info!("Connected!");

            // Ok(())
            // };
        }
        None => log::info!("Bluetooth search timd out"),
    };
    /*
    &|scanned_device| {
        if scanned_device.has_16bit_uuid(Bluetooth16bitUUIDEnum::AdvancedAudioDistribution as u16) {
            let (lock, cvar) = &*pair2;
            let mut found = lock.lock().unwrap();
            *found = Some(scanned_device);
            cvar.notify_one();

            log::info!(
                "Found A2DP Bluetooth device",
                // &scanned_device.name.unwrap()
            );
            // bluetooth.gap_cancel_discovery();
        } else {
            log::info!("Discovered device {:?}", &scanned_device); // bdaddr_to_string(addr)
                                                                   /*
                                                                   for property in properties {
                                                                       log::info!("Property: {:?}", property);
                                                                   }
                                                                       */
        }
    })?;
    */
    log::info!("Started scanning");
    // let peripherals = Peripherals::take().unwrap();
    // let state = Arc::new(Mutex::new(Cell::new(false)));

    block_on(playback_task(&mut bluetooth))?;
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
