#![cfg_attr(
    all(
        not(miri),
        not(target_os = "windows"),
        feature = "mmap",
        feature = "async"
    ),
    doc = "```"
)]
#![cfg_attr(
    all(
        not(target_os = "windows"),
        any(miri, not(feature = "mmap"), not(feature = "async")),
    ),
    doc = "```no_run"
)]
#![cfg_attr(target_os = "windows", doc = "```ignore")]
#![doc = include_str!("../../examples/async_mmap.rs")]
//! ```
