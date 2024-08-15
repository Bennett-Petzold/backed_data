//! Example usage re-exports.
//!
//! All examples are re-exported from `$CRATE_ROOT/examples`, and can be
//! executed via `cargo run --example NAME` with the necessary feature flags.

pub mod async_mmap;
pub mod async_simd;
pub mod large_reencode;
pub mod mmap;
pub mod remote_loading;
pub mod remote_simd;
pub mod shakespeare_secret;
