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
    any(
        miri,
        target_os = "windows",
        not(feature = "mmap"),
        not(feature = "async")
    ),
    doc = "```ignore"
)]
#![doc = include_str!("../../examples/async_mmap.rs")]
//! ```
