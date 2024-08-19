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
#[cfg(all(feature = "bincode", feature = "directory"))] {
    use std::{
        array::from_fn,
        env::temp_dir,
    };

    use serde::{Serialize, Deserialize};

    use backed_data::{
        entry::formats::BincodeCoder,
        directory::StdDirBackedArray,
    };

    # const BACKING_PATH: &str = "backed_data_motivating_example";
    #[derive(PartialEq, Eq, Serialize, Deserialize)]
    struct LargeStruct {
    # /*
        ...
    # */
    };

    impl LargeStruct {
        fn new_random() -> Self {
            # /*
              ...
            # */
            # Self {}
        }
    }

    const NUM_ELEMENTS: usize = 100_000;

    // This application only needs to find three random elements.
    let query = [0, 10_000, 50_000];

    // Do not use a temporary directory in real code.
    // Use some location that actually guarantees memory leaves RAM.
    let backing_dir = temp_dir().join(BACKING_PATH);

    // Define an array over `backing_dir` that automatically creates
    // new files when new data is appended.
    let mut backing = StdDirBackedArray
        ::<_, BincodeCoder<_>>::
        new(backing_dir.clone())
        .unwrap();

    let mut generator = (0..NUM_ELEMENTS)
        .into_iter()
        .map(|_| LargeStruct::new_random())
        .peekable();

    // Build the indices and backing store in 1,000 item chunks.
    while generator.peek().is_some() {
        let chunk_data: Vec<_> = generator.by_ref().take(1_000).collect();

        // Add a new bincode-encoded file that stores 1,000 elements.
        // After this operation, the elements are on disk only.
        backing.append(chunk_data);
    }

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
pub mod utils;

#[cfg(feature = "array")]
pub mod array;
#[cfg(feature = "directory")]
pub mod directory;

#[cfg(any(test, feature = "test"))]
pub mod test_utils;

pub mod extra_docs;

pub use entry::{BackedEntryArr, BackedEntryArrLock, BackedEntryCell, BackedEntryLock};

#[cfg(feature = "async")]
pub use entry::BackedEntryAsync;
