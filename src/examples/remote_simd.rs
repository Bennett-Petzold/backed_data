/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

#![cfg_attr(
    all(
        not(miri),
        feature = "simd_json",
        feature = "async",
        feature = "network"
    ),
    doc = "```"
)]
#![cfg_attr(
    any(
        miri,
        not(feature = "simd_json"),
        not(feature = "async"),
        not(feature = "network")
    ),
    doc = "```ignore"
)]
#![doc = include_str!("../../examples/remote_simd.rs")]
//! ```
