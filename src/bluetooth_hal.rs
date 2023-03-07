use crate::bluetooth_gap_hal::ScannedDevice;
use async_trait::async_trait;
use std::sync::{Arc, Condvar, Mutex};

use anyhow::Result;

pub type BDAddr = [u8; 6];

#[async_trait]
pub trait Bluetooth<'a> {
    fn init(&mut self, device_name: &str) -> Result<()>;
    fn deinit(&mut self) -> Result<()>;

    fn gap_start_discovery(
        &self, /* cb: &'static dyn Fn(ScannedDevice), */
    ) -> Result<Arc<AsyncCall<Vec<ScannedDevice>>>>;
    async fn gap_cancel_discovery(&self) -> Result<()>;

    async fn a2dp_connect(&mut self, addr: &BDAddr) -> Result<()>;
    async fn a2dp_play(&mut self, stream: Box<dyn Stream<i16>>) -> Result<()>;
}

pub trait Stream<T>: Send {
    fn read(&mut self, buf: &mut [T]) -> Result<usize>;
}

#[derive(PartialEq, Eq)]
pub enum AsyncCallState {
    Invalid,
    InProgress,
    Finished,
}

pub struct AsyncResult<T> {
    pub result: Option<T>,
    pub state: AsyncCallState,
}

pub struct AsyncCall<T> {
    pub result: Mutex<AsyncResult<T>>,
    pub condvar: Condvar,
}

impl<T> Default for AsyncCall<T> {
    fn default() -> Self {
        Self {
            result: Mutex::new(AsyncResult::<T> {
                result: None,
                state: AsyncCallState::Invalid,
            }),
            condvar: Condvar::new(),
        }
    }
}

impl<T> AsyncCall<T> {
    pub fn wait(&self) -> T {
        let mut result = self.result.lock().unwrap();
        while result.state != AsyncCallState::Finished {
            result = self.condvar.wait(result).unwrap();
        }
        result.result.take().unwrap()
    }
    pub fn complete(&self, result: T) {
        let mut result_mutex = self.result.lock().unwrap();
        result_mutex.result = Some(result);
        result_mutex.state = AsyncCallState::Finished;
        self.condvar.notify_all();
    }
}

pub fn bdaddr_to_string(addr: BDAddr) -> String {
    let mut result = String::new();

    for byte in addr {
        result.push_str(&format!("{:02X}", byte));
    }

    result
}
