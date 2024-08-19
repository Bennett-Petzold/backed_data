/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/*!
Unsafe code information.

Some features need capabilities only unlocked by `unsafe` code.
Unsafe code is tested by Miri where possible. It cannot execute all valid
Rust code, so some tests need to be left uncovered. Please open an issue or
PR for any unsound use of `unsafe`. The isolation of unsafe code to only
the explicitly unsafe features is guarded by a conditional
`forbid(unsafe_code)` in the crate root.

# Safe Features
* `array`
* `directory`
* `async`
* `bincode`
* `serde_json`
* `simd_json`
* `csv`
* `async_bincode`
* `async_csv`
* `zstd`
* `zstdmt`
* `async_zstd`
* `async_zstdmt`
* `network`

# Unsafe Features
See documentation and comments in the feature function/structs for
finer-grained justifications.

### `unsafe_array`
The `generic` methods are premised on the guarantees of
[`StableDeref`](stable_deref_trait::StableDeref). Mainly, we can borrow
from the storage pointed to by a container even if the container itself is
moved on stack. `unsafe` is needed to keep the container handles alive as
long as their referenced data is considered valid, but also allow the
handles to move.

The mutable iterators also need `unsafe`. The nature of a one-way iterator
is that the accesses to each element in the container are unique, but this
borrowing is outside of the slice API. So unsafe pointers are needed to
give out the exclusive mutable element access.

The complex iterators that save storage or flush on drop also `unsafe`, so
that the last instance of the handle can perform mutable cleanup. The
shared mutable element is not modified besides creation and cleanup.

### `tokio` & `smol`
Due to [`std::mem::forget`], scoped async blocks are inherently unsafe. The
runtime execution options provided for runtime executors rely on scoped
async blocks, so need to expose an `unsafe` function for them.

### `mmap`
All mmap APIs are unsafe, due to the possibility of an external process
modifying the file. `unsafe` is also used for mutable mmaps, so that it can
be preserved after the write handle is dropped but enforce mutability rules
at runtime. Otherwise the mmap would be pointlessly read again from disk
when the write handle is dropped and read handles are opened.
*/
