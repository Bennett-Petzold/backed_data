/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

#![cfg_attr(all(not(miri), feature = "simd_json", feature = "tokio"), doc = "```")]
#![cfg_attr(
    any(miri, not(feature = "simd_json"), not(feature = "tokio")),
    doc = "```ignore"
)]
#![doc = include_str!("../../examples/async_simd.rs")]
//! ```
