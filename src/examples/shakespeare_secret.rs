#![cfg_attr(
    all(not(miri), feature = "bincode", feature = "encrypted"),
    doc = "```"
)]
#![cfg_attr(
    any(miri, not(feature = "bincode"), not(feature = "encrypted")),
    doc = "```no_run"
)]
#![doc = include_str!("../../examples/shakespeare_secret.rs")]
//! ```
