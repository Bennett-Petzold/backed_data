/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/*!
Backbone traits and structs for the library implementation.
*/

#[cfg(feature = "async")]
pub mod blocking;

#[cfg(any(feature = "unsafe_array", feature = "encrypted", feature = "mmap"))]
mod extender;
#[cfg(any(feature = "unsafe_array", feature = "encrypted", feature = "mmap"))]
pub use extender::*;

#[cfg(feature = "async")]
pub mod sync;

#[cfg(feature = "network")]
pub mod output_boxer;

mod protected;
pub use protected::*;

mod refs;
pub use refs::*;

mod once;
pub use once::*;

mod async_compat_cursor;
pub use async_compat_cursor::*;
