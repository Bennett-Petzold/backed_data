#![cfg_attr(
    all(not(miri), not(target_os = "windows"), feature = "mmap",),
    doc = "```"
)]
#![cfg_attr(
    any(miri, target_os = "windows", not(feature = "mmap")),
    doc = "```ignore"
)]
#![doc = include_str!("../../examples/mmap.rs")]
//! ```
