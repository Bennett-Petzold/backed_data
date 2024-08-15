#![cfg_attr(
    all(
        not(miri),
        feature = "simd_json",
        feature = "async",
        feature = "network"
    ),
    doc = "```"
)]
#![cfg_attr(
    any(
        miri,
        not(feature = "simd_json"),
        not(feature = "async"),
        not(feature = "network")
    ),
    doc = "```ignore"
)]
#![doc = include_str!("../../examples/remote_simd.rs")]
//! ```
