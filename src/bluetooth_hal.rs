use crate::bluetooth_gap_hal::ScannedDevice;
use anyhow::Result;
use async_trait::async_trait;

pub type BDAddr = [u8; 6];

#[async_trait]
pub trait Bluetooth<'a> {
    fn init(&mut self, device_name: &str) -> Result<()>;
    fn deinit(&mut self) -> Result<()>;

    fn gap_start_discovery(&self) -> Result<async_broadcast::Receiver<ScannedDevice>>;
    fn gap_cancel_discovery(&self) -> Result<()>;

    async fn a2dp_connect(&mut self, addr: &BDAddr) -> Result<()>;
    async fn a2dp_play(&mut self, stream: Box<dyn Stream<i16>>) -> Result<()>;
}

pub trait Stream<T>: Send {
    fn read(&mut self, buf: &mut [T]) -> Result<usize>;
}

pub struct AsyncCall<T> {
    event: event_listener::Event,
    result: std::sync::Mutex<Option<T>>,
}

impl<T> Default for AsyncCall<T> {
    fn default() -> Self {
        Self {
            event: event_listener::Event::new(),
            result: std::sync::Mutex::new(None),
        }
    }
}

impl<T> AsyncCall<T> {
    pub fn new() -> Self {
        AsyncCall::default()
    }

    #[allow(dead_code)]
    pub async fn wait(&self) -> T {
        self.event.listen().await;

        self.result.lock().unwrap().take().unwrap()
    }
    pub async fn do_and_wait<F>(&self, f: F) -> T
    where
        F: FnOnce(),
    {
        let listener = self.event.listen();

        f();

        listener.await;
        self.result.lock().unwrap().take().unwrap()
    }

    pub fn complete(&self, result: T) {
        self.result.lock().unwrap().replace(result);
        self.event.notify(usize::MAX);
    }
}

pub fn bdaddr_to_string(addr: BDAddr) -> String {
    let mut result = String::new();

    for byte in addr {
        result.push_str(&format!("{byte:02X}"));
    }

    result
}
