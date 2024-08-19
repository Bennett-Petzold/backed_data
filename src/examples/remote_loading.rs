/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

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
