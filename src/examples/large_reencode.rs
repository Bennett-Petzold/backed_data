/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

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
