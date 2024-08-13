#![cfg_attr(
    all(not(miri), not(target_os = "windows"), feature = "mmap",),
    doc = "```"
)]
#![cfg_attr(
    all(not(target_os = "windows"), any(miri, not(feature = "mmap")),),
    doc = "```no_run"
)]
#![cfg_attr(target_os = "windows", doc = "```ignore")]
#![doc = include_str!("../../examples/mmap.rs")]
//! ```
