#![cfg_attr(all(not(miri), feature = "simd_json", feature = "async"), doc = "```")]
#![cfg_attr(
    any(miri, not(feature = "simd_json"), not(feature = "async")),
    doc = "```ignore"
)]
#![doc = include_str!("../../examples/async_simd.rs")]
//! ```
