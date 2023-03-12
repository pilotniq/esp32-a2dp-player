use crate::bluetooth_hal::Bluetooth;
use crate::bluetooth_hal::Stream;
use anyhow::Result;
use lewton::inside_ogg::OggStreamReader;
use std::{
    cmp::min,
    collections::VecDeque,
    fs::File,
    io::Read,
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::Instant,
};
// use tremor;
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
/*
impl FileStream {
    pub fn new(filename: &str) -> Result<Self> {
        Ok(FileStream {
            file: File::open(filename)?,
        })
    }
}
 */
struct OggBluetoothStream {
    // file: File,
    filename: String,
    // ogg_stream_reader: OggStreamReader<File>,
    thread: Option<JoinHandle<Result<()>>>,
    buffer_condvar: Arc<Condvar>,
    buffer: Arc<Mutex<VecDeque<i16>>>,
    end_of_file: Arc<Mutex<bool>>,
}

impl OggBluetoothStream {
    pub fn new(filename: &str) -> Result<Self> {
        Ok(OggBluetoothStream {
            // file,
            filename: filename.to_string(), // ogg_stream_reader,
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

        // let decoder = tremor::Decoder::new(file)?;

        let mut ogg_stream_reader = OggStreamReader::new(file)?;
        let mut end_of_stream = false;
        /*
               log::info!(
                   "{} channels, sample rate = {} Hz",
                   ogg_stream_reader.ident_hdr.audio_channels,
                   ogg_stream_reader.ident_hdr.audio_sample_rate
               );
        */
        while !end_of_stream {
            let mut buffer = buffer_mutex.lock().expect("Failed to lock"); // not sure why ? doesn't work here

            while buffer.len() >= 88200 {
                buffer = condvar.wait(buffer).expect("Condvar wait failed");
            }
            // log::info!("decoding_thread: buffer.len={}", buffer.len());
            drop(buffer);

            end_of_stream = OggBluetoothStream::buffer_packet(
                &mut ogg_stream_reader,
                &buffer_mutex,
                &condvar,
                eos_mutex,
            )?;
            // let len = buffer.len();
            // drop(buffer);
            // log::info!("Buffer.len={}", len);
        }
        Ok(())
        /*
        if let Err(e) = playback_task(state, &bluetooth_mutex) {
            log::warn!("Error running playback thread {e:?}");
        }
         */
    }

    // returns true on end of stream
    fn buffer_packet(
        // &mut self,
        reader: &mut OggStreamReader<File>,
        buffer_mutex: &Mutex<VecDeque<i16>>,
        condvar: &Arc<Condvar>,
        eos_mutex: &Mutex<bool>,
    ) -> Result<bool> {
        // Performance:
        // Packets are 2048 samples = 1024 frames. Representing 1024 / 44100 seconds = ca 23 ms of audio
        // Time to read & decode is 5.4 - 37 ms. Time to buffer/copy is 1 to 18 ms.
        // Total times are ca 8 - 50 ms.
        let t0 = Instant::now();
        let packet_option = reader.read_dec_packet_itl()?;
        let t1 = Instant::now();
        if let Some(packet) = packet_option {
            // is this inefficient?
            /*
            log::info!(
                "OggBluetoothStream.buffer_packet: got packet, len={}",
                packet.len()
            );
             */
            // let mut new_dq = VecDeque::with_capacity(packet.len());
            let mut new_dq = VecDeque::from(packet);
            let t2 = Instant::now();
            // new_dq.append(packet);
            let mut buffer = buffer_mutex.lock().expect("xyzyz");
            let t3 = Instant::now();
            buffer.append(&mut new_dq);
            let t4 = Instant::now();
            condvar.notify_all();
            let t5 = Instant::now();
            let len = buffer.len();
            drop(buffer);

            let t6 = Instant::now();
            log::info!(
                "buffer_packet: read={} us, dequeue={} us, lock={} us, append={} us, notify={} us, drop={} us, total={} us, size={}",
                t1.duration_since(t0).as_micros(),
                t2.duration_since(t1).as_micros(),
                t3.duration_since(t2).as_micros(),
                t4.duration_since(t3).as_micros(),
                t5.duration_since(t4).as_micros(),
                t6.duration_since(t5).as_micros(),
                t6.duration_since(t0).as_micros(),
                len
            );
            Ok(false)
        } else {
            log::info!("OggBluetoothStream.buffer_packet: end_of_stream");
            // todo: somehow signal end of stream?
            let mut eos = eos_mutex.lock().expect("Failed to lock eos");
            *eos = true;
            condvar.notify_all();
            Ok(true)
        }
    }
}

impl Stream<i16> for OggBluetoothStream {
    // Bluetooth reads 256 sample buffers representing 128 frames = 128 / 44100 s = 2.9 ms
    fn read(&mut self, buf: &mut [i16]) -> Result<usize> {
        // Start by copying from our buffer to the result buffer
        let mut copy_count = 0;
        // log::info!("OggBluetoothStream.read(), buf length={}", buf.len());

        while copy_count < buf.len() {
            let t0 = Instant::now();

            let mut end_of_stream = *self.end_of_file.lock().expect("Failed to lock eos");
            let mut buffer = self.buffer.lock().expect("Failed to lock");

            let t1 = Instant::now();
            while buffer.len() == 0 && !end_of_stream {
                // log::info!("read: waiting for data");
                buffer = self.buffer_condvar.wait(buffer).expect("Wait 17");
                // log::info!("read: after wait, have {}", buffer.len());
                end_of_stream = *self.end_of_file.lock().expect("Failde to lock eos");
            }

            let t2 = Instant::now();
            if buffer.len() == 0 && end_of_stream {
                return Ok(0); // not sure what we return at EOS. TODO
            }

            let copy_len = min(buf.len() - copy_count, buffer.len());
            /*
            log::info!(
                "copy_count={}, copy_len={} buffer.len={}, buf.len={}",
                copy_count,
                copy_len,
                self.buffer.len(),
                buf.len()
            );
             */
            assert!(copy_len > 0);
            // if copy_len > 0 {
            // read is only implemented for VecDeque<u8>, not for i16
            // self.buffer.read_exact();
            // make it inefficient for now
            // log::info!("Before drain, len={}", buffer.len());
            let t3 = Instant::now();
            let samples: Vec<i16> = buffer.drain(..copy_len).collect();
            let t4 = Instant::now();
            let len = buffer.len();
            drop(buffer);
            let t5 = Instant::now();
            /*
            log::info!(
                "After drain, len={}, samples.len={}",
                buffer.len(),
                samples.len()
            ); */
            buf[copy_count..(copy_count + copy_len)].clone_from_slice(&samples);
            let t6 = Instant::now();
            copy_count += copy_len;
            // if buffer.len() < 2048 {
            self.buffer_condvar.notify_all();
            let t7 = Instant::now();
            /*
            log::info!("read: locking {} us, waiting {} us, nothing: {} us, drain: {} us, drop: {} us, clone: {} us, notify {} us, total {} us, len={}",
                t1.duration_since(t0).as_micros(),
                t2.duration_since(t1).as_micros(),
                t3.duration_since(t2).as_micros(),
                t4.duration_since(t3).as_micros(),
                t5.duration_since(t4).as_micros(),
                t6.duration_since(t5).as_micros(),
                t7.duration_since(t6).as_micros(),
                t7.duration_since(t0).as_micros(),
                len);
                 */
            // }
        }

        // log::info!("OggBluetoothStream.read(), returning {}", copy_count);

        Ok(copy_count)
    }
}

pub async fn playback_task<'a>(bluetooth: &mut dyn Bluetooth<'a>) -> Result<()> {
    // Open audio file
    // let file_stream = FileStream::new("/sdcard/SUN.RAW")?;

    // let bluetoothR = bt_mutex.lock();
    // let bluetooth = &(bluetoothR?);
    log::info!("Creating OggBluetoothStream");
    let mut stream = OggBluetoothStream::new("/sdcard/sun.ogg")?;
    stream.start()?;
    log::info!("Created ogg Bluetooth stream");

    bluetooth.a2dp_play(Box::new(stream)).await
    // drop(bluetooth);

    // block_on(future);
    // std::thread::sleep(ONE_MINUTE);

    // Ok(())
}
