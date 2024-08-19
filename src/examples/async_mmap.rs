/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

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
