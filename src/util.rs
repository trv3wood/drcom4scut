use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use bytes::{Buf, BufMut, Bytes, BytesMut};
use chrono::{Local, NaiveTime};
use pnet::datalink::MacAddr;
use rand::random;

const MILLI_SEC: Duration = Duration::from_millis(10);
const SEC: Duration = Duration::from_secs(1);

#[derive(Clone, Default)]
pub struct ShutdownSignal {
    inner: Arc<AtomicBool>,
}

impl ShutdownSignal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn request_shutdown(&self) {
        self.inner.store(true, Ordering::Release);
    }

    pub fn is_shutdown(&self) -> bool {
        self.inner.load(Ordering::Acquire)
    }
}

#[inline]
pub fn sleep() {
    std::thread::sleep(MILLI_SEC);
}

#[inline]
pub fn ip_to_vec(ip: &IpAddr) -> Vec<u8> {
    match ip {
        IpAddr::V4(ip) => ip.octets().to_vec(),
        IpAddr::V6(ip) => ip.octets().to_vec(),
    }
}

#[inline]
pub fn put_mac(data: &mut BytesMut, mac: &MacAddr) {
    data.put_u8(mac.0);
    data.put_u8(mac.1);
    data.put_u8(mac.2);
    data.put_u8(mac.3);
    data.put_u8(mac.4);
    data.put_u8(mac.5);
}

#[inline]
pub fn get_mac(data: &mut Bytes) -> MacAddr {
    let mut mac = MacAddr::zero();
    mac.0 = data.get_u8();
    mac.1 = data.get_u8();
    mac.2 = data.get_u8();
    mac.3 = data.get_u8();
    mac.4 = data.get_u8();
    mac.5 = data.get_u8();
    mac
}

#[inline]
#[allow(dead_code)]
pub fn sleep_at(time: NaiveTime) -> Option<()> {
    let mut dt = Local::now().date_naive().and_time(time);
    while dt < Local::now().naive_local() {
        dt += chrono::Duration::days(1);
    }
    while dt > Local::now().naive_local() {
        std::thread::sleep(SEC);
    }
    Some(())
}

#[inline]
pub fn sleep_at_with_shutdown(time: NaiveTime, shutdown: &ShutdownSignal) -> Option<()> {
    let mut dt = Local::now().date_naive().and_time(time);
    while dt < Local::now().naive_local() {
        dt += chrono::Duration::days(1);
    }
    while dt > Local::now().naive_local() {
        if shutdown.is_shutdown() {
            return None;
        }
        std::thread::sleep(SEC);
    }
    Some(())
}

#[inline]
pub fn sleep_with_shutdown(duration: Duration, shutdown: &ShutdownSignal) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < duration {
        if shutdown.is_shutdown() {
            return false;
        }
        let remaining = duration.saturating_sub(start.elapsed());
        std::thread::sleep(std::cmp::min(remaining, Duration::from_millis(200)));
    }
    true
}

#[inline]
pub fn random_vec(n: usize) -> Vec<u8> {
    let mut v = Vec::with_capacity(n);
    for _ in 0..n {
        v.push(random::<u8>());
    }
    v
}

/// enum of ChannelData.state
///
/// # State
pub enum State {
    Success,
    Stop,
    Sleep,
    Quit,
}

pub struct ChannelData {
    pub state: State,
    pub data: Vec<u8>,
}
