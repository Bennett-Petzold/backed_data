/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/*!
Features are broken into four category headers. The default features are
`bincode`, `directory`, and `zstd`. With no features available, synchronous
[`BackedEntry`][`crate::entry::BackedEntry`] is available with plainfile disks,
but the library needs at least one `coder` feature or custom coder implementation
to provide the entry.

`all_disks` enables all disk types, and `all_formats` enables all coder formats.

# Library Functionality
* `async`
    * Provide asynchronous variants library-wide, usually methods prepended with `a_`.
        Enabled by any [executor feature](#async-executors).
* `array`
    * Provide [`BackedArray`][`crate::array::BackedArray`] to split one array
        into multiple smaller [`BackedEntry`][`crate::entry::BackedEntry`] arrays in storage,
        but represent as a single array to the user.
* `unsafe_array`
    * Enable features for `array` that requires `unsafe` code. Needed for mutable iterators
        and access on containers that return handles instead of direct references (e.g.
        [`SecretVecWrapper`][`crate::entry::disks::encrypted::SecretVecWrapper`]).
* `directory`
    * Provide [`DirectoryBackedArray`][`crate::directory::DirectoryBackedArray`] to wrap
        [`BackedArray`][`crate::array::BackedArray`]. This simplifies appending data to
        the backing arrays and saving/reloading arrays.

# Async Executors
None of these features are necessary to use an async executor, but they provide
a correct [`AsyncFile`][`crate::entry::disks::AsyncFile`] for the `Plainfile`
disks to use. They also give convenience handles in
[`blocking`][`crate::utils::blocking`].

* `tokio`
* `smol`

# Disk Types
`Plainfile` variant disks are available with no features. Some
[executor feature](#async-executors) needs to be chosen in order for those base
types to have asynchronous file access.
Custom disks can be defined by implementing
[`WriteDisk`][`crate::entry::disks::WriteDisk`] and [`ReadDisk`][`crate::entry::disks::ReadDisk`]
(and/or the async versions).
They can also be defined using [`crate::entry::disks::custom`] types.

* `all_disks`
    * Enable all disk types.
* `encrypted` - [`Encrypted`][`crate::entry::disks::Encrypted`]
    * Uses [`secrets`](https://github.com/stouset/secrets) and
        [`aes-gcm`](https://docs.rs/aes-gcm/latest/aes_gcm/) to encrypt all
        data stored to disk, and minimize the risk of memory-peeking in the
        encryption/decryption process.
* `mmap` - [`Mmap`][`crate::entry::disks::Mmap`]
    * Enables the use of [`mmap`](https://en.wikipedia.org/wiki/Mmap) calls on
        UNIX systems (guarded by `mmap_impl`) to replace regular file I/O.
        Reuses mapped memory when possible to reduce reading from disk costs.
* `network` - [`Network`][`crate::entry::disks::Network`]
    * Uses [`reqwest`](https://github.com/seanmonstar/reqwest) to read from a
        remote resource.
* `zstd` - [`ZstdDisk`][`crate::entry::disks::ZstdDisk`]
    * [`zstd-rs`](https://github.com/gyscos/zstd-rs) compression and decompression.
* `async_zstd` - [`ZstdDisk`][`crate::entry::disks::ZstdDisk`]
    * `zstd`, but asynchronous.
* `zstdmt`/`async_zstdmt` - [`ZstdDisk`][`crate::entry::disks::ZstdDisk`]
    * Enable `zstd` with multithreaded compression support. May degrade performance;
        test that the data compression speed actually benefits from this feature. This
        is not necessary to compress multiple files with `zstd` at once, only to use
        multiple threads for the compression of a single file.

# Predefined Formats
Formats can be custom-defined by implementing
[`Encoder`][`crate::entry::formats::Encoder`] and
[`Decoder`][`crate::entry::formats::Decoder`] (and/or the async versions).
These features give ready to use defaults.

(All formats use Serde, but the Serde traits do not implement over
[`Read`][`std::io::Read`] and [`Write`][`std::io::Write`]).

* `all_formats`
    * Enable all formats.

#### Synchronous Formats
* `bincode` - [`BincodeCoder`][`crate::entry::formats::BincodeCoder`]
    * Binary encoding format.
* `serde_json` - [`SerdeJsonCoder`][`crate::entry::formats::SerdeJsonCoder`]
    * Serde's flagship (de)serializer.
* `simd_json` - [`SimdJsonCoder`][`crate::entry::formats::SimdJsonCoder`]
    * Significantly faster than `serde_json`.
* `csv` - [`CsvCoder`][`crate::entry::formats::CsvCoder`]
    * Allows for reading from/writing to csv files.

#### Async Formats
* `async_bincode` - [`AsyncBincodeCoder`][`crate::entry::formats::AsyncBincodeCoder`]
    * Binary format. NOT compatible with `bincode`, due to a size header.
* `async_csv` - [`AsyncCsvCoder`][`crate::entry::formats::AsyncCsvCoder`]
    * Compatible with `csv`.
*/
