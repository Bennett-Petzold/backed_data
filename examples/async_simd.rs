#![cfg(all(feature = "simd_json", feature = "async"))]

/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::{collections::HashMap, env::temp_dir};

use backed_data::{
    entry::{
        adapters::{DecodeBg, EncodeBg, SyncCoderAsAsync},
        disks::{Plainfile, ReadDisk, WriteDisk},
        formats::{AsyncDecoder, AsyncEncoder, SimdJsonCoder},
    },
    utils::blocking::tokio_blocking,
};
use tokio::fs::remove_dir_all;

#[tokio::main]
async fn main() {
    let backing_file = temp_dir().join("backed_array_async_simd");
    let data: HashMap<usize, usize> = (0..10_000).map(|x| (x, x * 2)).collect();

    let disk = Plainfile::new(backing_file.clone());

    // Create an adapter to run encoding and decoding in tokio's blocking
    // threadpool.
    let coder = SyncCoderAsAsync::<_, Plainfile, _, _, _, _>::new(
        SimdJsonCoder::default(),
        |x: DecodeBg<_, _>| unsafe { tokio_blocking(x) },
        |x: EncodeBg<_, _>| unsafe { tokio_blocking(x) },
    );

    // Encode asynchronously via a background thread
    coder
        .encode(&data, disk.write_disk().unwrap())
        .await
        .unwrap();

    // Decode asynchronously via a background thread
    let decoded = coder.decode(disk.read_disk().unwrap()).await.unwrap();
    assert_eq!(data, decoded);

    let _ = remove_dir_all(backing_file).await;
}
