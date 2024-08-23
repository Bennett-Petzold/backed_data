/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![cfg_attr(all(not(test), not(unsafe_feature)), forbid(unsafe_code))]

/*!
Cache data outside memory, loading in when referenced.

You may want a more standard data storage solution! See
[`alternatives`][`extra_docs::alternatives`] to make sure another approach
doesn't fit your case better.

This crate uses some `unsafe` code on certain features (not the default
features). See [`unsafe_usage`][`extra_docs::unsafe_usage`] for the listing and explanations.
Dependencies are listed and explained in [`deps`][`extra_docs::deps`].
[`features`][`extra_docs::features`] runs down selecting which of the (many) features to use.

# Motivating Example

More examples that demonstrate more complex uses are in [`examples`].

Assume that you have some `LargeStruct` that takes up significant storage and
can be reduced to a smaller representation for searching. If it is stored in a
collection where only a small number of elements are used, keeping them all
loaded in memory wastes system resources. A
[vector store](https://en.wikipedia.org/wiki/Vector_database) is one such
structure. This example assumes a large, hashable type with only a few entries
accessed.

```rust
# #[cfg(not(miri))] {
#[cfg(all(feature = "bincode", feature = "array"))] {
    use std::{
        env::temp_dir,
        iter::from_fn,
    };

    use serde::{Serialize, Deserialize};

    use backed_data::{
        entry::{
            disks::Plainfile,
            formats::BincodeCoder,
        },
        array::VecBackedArray,
    };

    # const BACKING_PATH: &str = "backed_data_motivating_example";
    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct LargeStruct {
        val: u8,
    # /*
        ...
    # */
    };

    impl LargeStruct {
        fn new_random() -> Self {
            # /*
              ...
            # */
            # Self { val: 1 }
        }
    }

    const NUM_BACKINGS: usize = 1_000;
    const ELEMENTS_PER_BACKING: usize = 1_000;

    // This application only needs to find three random elements.
    let query = [0, 10_000, 50_000];

    // Do not use a temporary directory in real code.
    // Use some location that actually guarantees memory leaves RAM.
    let backing_dir = temp_dir().join(BACKING_PATH);
    std::fs::create_dir_all(&backing_dir).unwrap();

    // Define a backed array using Vec.
    let mut backing = VecBackedArray
        ::<LargeStruct, Plainfile, BincodeCoder<_>>::
        new();

    // Build the indices and backing store in 1,000 item chunks.
    for _ in 0..NUM_BACKINGS {
        let chunk_data: Vec<_> = from_fn(|| Some(LargeStruct::new_random()))
            .take(ELEMENTS_PER_BACKING)
            .collect();

        // This is handled automatically by `DirectoryBackedArray` types.
        let target_file = backing_dir.join(uuid::Uuid::new_v4().to_string()).into();

        // Add a new bincode-encoded file that stores 1,000 elements.
        // After this operation, the elements are on disk only (chunk_data
        // is dropped by scope rules).
        backing.append(chunk_data, target_file, BincodeCoder::default()).unwrap();
    }
    # assert!(backing.len() > 50_000);

    // Query for three elements. At most 3,000 elements are loaded, because
    // the data is split into 1,000 element chunks. Only 2,997 useless
    // elements are kept in memory, instead of 99,997.
    let results: Vec<_> = query
        .iter()
        .map(|q| backing.get(*q))
        .collect();
    # std::fs::remove_dir_all(backing_dir);
}
# }
```

# Usage

The core structure is [`entry::BackedEntry`], which is wrapped by [`mod@array`]
and [`directory`]. It should be pointed at external data to load when used.
That data will remain in memory until unloaded (so subsequent reads avoid
the cost of decoding). Try to only unload in one of the following scenarios:
* The data will not be read again.
* The program's heap footprint needs to shrink.
* The external store was modified by another process.

Each entry needs a [format][`entry::formats`] and (potentially layered)
[disks][`entry::disks`] to use. The [`mod@array`] wrapper also needs choices of
[containers][`array::Container`] to hold its array of keys and array of backed
entries.

# Licensing and Contributing

All code is licensed under MPL 2.0. See the [FAQ](https://www.mozilla.org/en-US/MPL/2.0/FAQ/)
for license questions. The license non-viral copyleft and does not block this library from
being used in closed-source codebases. If you are using this library for a commercial purpose,
consider reaching out to `dansecob.dev@gmail.com` to make a financial contribution.

Contributions are welcome at
<https://github.com/Bennett-Petzold/backed_data>. Please open an issue or
PR if:
* Some dependency is extraneous, unsafe, or has a versioning issue.
* Any unsafe code is insufficiently explained or tested.
* There is any other issue or missing feature.
*/

pub mod entry;
pub mod examples;
pub mod extra_docs;
pub mod utils;

#[cfg(feature = "array")]
pub mod array;
#[cfg(feature = "directory")]
pub mod directory;

#[cfg(any(test, feature = "test"))]
pub mod test_utils;

pub use entry::{BackedEntryArr, BackedEntryArrLock, BackedEntryCell, BackedEntryLock};

#[cfg(feature = "async")]
pub use entry::BackedEntryAsync;

#[cfg(feature = "array")]
pub use array::VecBackedArray;

#[cfg(feature = "directory")]
pub use directory::StdDirBackedArray;

#[cfg(all(feature = "directory", runtime, feature = "zstd"))]
pub use directory::ZstdDirBackedArray;

#[cfg(all(feature = "directory", runtime, feature = "async_zstd"))]
pub use directory::AsyncZstdDirBackedArray;
