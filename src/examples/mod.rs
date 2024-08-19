/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Example usage re-exports.
//!
//! All examples are re-exported from `$CRATE_ROOT/examples`, and can be
//! executed via `cargo run --example NAME` with the necessary feature flags.

pub mod async_mmap;
pub mod async_simd;
pub mod large_reencode;
pub mod mmap;
pub mod remote_loading;
pub mod remote_simd;
pub mod shakespeare_secret;
