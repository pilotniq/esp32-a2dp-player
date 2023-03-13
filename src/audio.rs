use crate::bluetooth_hal::Bluetooth;
use crate::bluetooth_hal::Stream;
use anyhow::Result;

use std::{
    cmp::min,
    collections::VecDeque,
    fs::File,
    io::Read,
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::Instant,
};
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

struct OggBluetoothStream {
    filename: String,
    thread: Option<JoinHandle<Result<()>>>,
    buffer_condvar: Arc<Condvar>,
    buffer: Arc<Mutex<VecDeque<i16>>>,
    end_of_file: Arc<Mutex<bool>>,
}

impl OggBluetoothStream {
    pub fn new(filename: &str) -> Result<Self> {
        Ok(OggBluetoothStream {
            filename: filename.to_string(),
            thread: None,
            buffer_condvar: Arc::new(Condvar::new()),
            buffer: Arc::new(Mutex::new(VecDeque::new())),
            end_of_file: Arc::new(Mutex::new(false)),
        })
    }

    pub fn start(&mut self) -> Result<()> {
        // start thread decoding file to buffer
        let filename = self.filename.clone();
        let buffer = self.buffer.clone();
        let condvar = self.buffer_condvar.clone();
        let eos = self.end_of_file.clone();

        self.thread = Some(
            thread::Builder::new()
                .name("decoding_thread".to_owned())
                .stack_size(28000) // 4096 as too small a stack. May need 14000-28000 which was main
                .spawn(move || {
                    OggBluetoothStream::decoding_thread(filename, buffer, condvar, &eos)
                })?,
        );
        Ok(())
    }

    fn decoding_thread(
        filename: String,
        buffer_mutex: Arc<Mutex<VecDeque<i16>>>,
        condvar: Arc<Condvar>,
        eos_mutex: &Mutex<bool>,
    ) -> Result<()> {
        // this task
        let file = File::open(filename)?;
        log::info!("Opened file, creating StreamReader");

        let mut decoder = librespot_tremor::Decoder::new(file)?;
        let packets = decoder.packets();

        let mut end_of_stream = false;

        for packet_result in packets {
            let mut packet = match packet_result {
                Ok(packet) => packet,
                Err(e) => return Err(anyhow::anyhow!("Vorbis error")),
            };
            let mut buffer = buffer_mutex.lock().expect("Failed to lock"); // not sure why ? doesn't work here

            while buffer.len() >= 88200 {
                buffer = condvar.wait(buffer).expect("Condvar wait failed");
            }

            drop(buffer); // = Release lock

            end_of_stream =
                OggBluetoothStream::buffer_packet(&mut packet, &buffer_mutex, &condvar, eos_mutex)?;
        }
        Ok(())
    }

    // returns true on end of stream
    fn buffer_packet(
        // &mut self,
        packet: &mut librespot_tremor::Packet,
        buffer_mutex: &Mutex<VecDeque<i16>>,
        condvar: &Arc<Condvar>,
        eos_mutex: &Mutex<bool>,
    ) -> Result<bool> {
        // Performance:
        // Packets are 2048 samples = 1024 frames. Representing 1024 / 44100 seconds = ca 23 ms of audio
        let mut buffer = buffer_mutex.lock().expect("xyzyz");
        let t3 = Instant::now();
        // this seems very inefficient, but optmize later
        for sample in &packet.data {
            buffer.push_back(*sample);
        }
        condvar.notify_all();
        let len = buffer.len();
        drop(buffer);

        Ok(false)
    }
}

impl Stream<i16> for OggBluetoothStream {
    // Bluetooth reads 256 sample buffers representing 128 frames = 128 / 44100 s = 2.9 ms
    fn read(&mut self, buf: &mut [i16]) -> Result<usize> {
        // Start by copying from our buffer to the result buffer
        let mut copy_count = 0;

        while copy_count < buf.len() {
            let t0 = Instant::now();

            let mut end_of_stream = *self.end_of_file.lock().expect("Failed to lock eos");
            let mut buffer = self.buffer.lock().expect("Failed to lock");

            while buffer.len() == 0 && !end_of_stream {
                buffer = self.buffer_condvar.wait(buffer).expect("Wait 17");
                end_of_stream = *self.end_of_file.lock().expect("Failde to lock eos");
            }

            if buffer.len() == 0 && end_of_stream {
                return Ok(0); // not sure what we return at EOS. TODO
            }

            let copy_len = min(buf.len() - copy_count, buffer.len());
            assert!(copy_len > 0);

            let samples: Vec<i16> = buffer.drain(..copy_len).collect();
            let len = buffer.len();
            drop(buffer);

            buf[copy_count..(copy_count + copy_len)].clone_from_slice(&samples);
            copy_count += copy_len;

            self.buffer_condvar.notify_all();
        }

        Ok(copy_count)
    }
}

pub async fn playback_task<'a>(bluetooth: &mut dyn Bluetooth<'a>) -> Result<()> {
    // Open audio file
    log::info!("Creating OggBluetoothStream");
    let mut stream = OggBluetoothStream::new("/sdcard/sun.ogg")?;
    stream.start()?;
    log::info!("Created ogg Bluetooth stream");

    bluetooth.a2dp_play(Box::new(stream)).await
}
