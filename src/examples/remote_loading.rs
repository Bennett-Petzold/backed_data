#![cfg_attr(
    all(not(miri), feature = "async_csv", feature = "network"),
    doc = "```"
)]
#![cfg_attr(
    any(miri, not(feature = "async_csv"), not(feature = "network")),
    doc = "```ignore"
)]
#![doc = include_str!("../../examples/remote_loading.rs")]
//! ```
