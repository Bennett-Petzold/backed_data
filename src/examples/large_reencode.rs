#![cfg_attr(
    any(
        not(feature = "async_zstd"),
        not(feature = "async_bincode"),
        not(feature = "csv")
    ),
    doc = "```ignore"
)]
#![cfg_attr(
    any(feature = "async_zstd", feature = "async_bincode", feature = "csv"),
    doc = "```no_run"
)]
#![doc = include_str!("../../examples/large_reencode.rs")]
//! ```
