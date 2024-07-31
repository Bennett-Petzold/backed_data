#[cfg(feature = "async-zstd")]
pub mod async_impl;
pub mod sync_impl;

#[cfg(any(feature = "zstdmt", feature = "async-zstdmt"))]
use {lazy_static::lazy_static, std::sync::Mutex};

#[cfg(any(feature = "zstdmt", feature = "async-zstdmt"))]
lazy_static! {
    static ref ZSTD_MULTITHREAD: Mutex<u32> = Mutex::new(1);
}

#[cfg(any(feature = "zstdmt", feature = "async-zstdmt"))]
/// Set the level of zstdmt multithreading
///
/// Default is 1 (one extra thread).
/// Values greater than 0 run compression in N background threads.
pub fn set_zstd_multithread(value: u32) {
    *ZSTD_MULTITHREAD.lock().unwrap() = value;
}
