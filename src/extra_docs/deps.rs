/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/*!
Dependencies are broken down by feature category, then feature.

# Library Functionality
* `async`
    * [`futures`] - The most popular library for core [`future`][`std::future`] functionality.
    * [`cfg_if`] - Prevents copy-pasting for complex feature definitions.
* `unsafe_array`
    * [`stable_deref_trait`] - [`StableDeref`][`stable_deref_trait::StableDeref`] Guarantees that a container variable can be moved without
        invalidating element references.
* `directory`
    * [`uuid`] - [`Uuid::new_v4`][`uuid::Uuid::new_v4`] is used for random file names.

# Async Executors

* `tokio`
    * [`tokio`]
    * [`tokio_util`] - Convert from tokio I/O traits to [`futures`] I/O traits.
    * [`async_scoped`] - Provides a scoped async block not available in base [`futures`], so blocking
    * thread handles can have less than `'static` lifetimes.
* `smol`
    * [`smol`]

# Disk Types
* `all_disks`
    * All dependencies below.
* `encrypted`
    * [`secrets`] - Provides protected memory via [`libsodium`](https://doc.libsodium.org/) to make
        memory snooping difficult.
    * [`aes_gcm`] - Provides an encryption algorithm to protect data written to disks.
    * [`stable_deref_trait`] - [`StableDeref`][`stable_deref_trait::StableDeref`] Guarantees that the secret container can be moved without
        invalidating element references.
* `mmap`
    * [`memmap2`] - Cross-OS memory mapping wrapper.
    * [`stable_deref_trait`] - [`StableDeref`][`stable_deref_trait::StableDeref`] Guarantees that the mmap array is valid, even when the handle is moved.
* `network`
    * Dependencies from `async`
    * [`reqwest`] - Backend for remote communications.
    * [`bytes`], [`http_serde`], [`url`] - (De)serialization definitions for [`reqwest`].
* `zstd`
    * [`zstd`]
    * [`num_traits`] - Generic for unsigned numbers.
* `async_zstd`
    * `async` dependencies.
    * [`num_traits`] - Generic for unsigned numbers.
    * [`async_compression`] - Async zstd implementation.
* `zstdmt`/`async_zstdmt`
    * `zstd`/`async_zstd` dependencies.

# Predefined Formats
* `all_formats`
    * All dependencies below

#### Synchronous Formats
* `bincode`
    * [`bincode`]
* `serde_json`
    * [`serde_json`]
* `simd_json`
    * [`simd_json`]
* `csv`
    * [`csv`]

#### Async Formats
* `async_bincode`
    * [`async_bincode`]
* `async_csv`
    * [`csv_async`]
*/
