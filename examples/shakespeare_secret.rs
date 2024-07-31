#![cfg(all(feature = "bincode", feature = "encrypted"))]

use std::{
    cell::OnceCell,
    env::temp_dir,
    fs::{remove_dir_all, File},
    io::{BufRead, BufReader},
    str::{from_utf8, from_utf8_mut},
};

use backed_data::{
    directory::DirectoryBackedArray,
    entry::{
        disks::{Encrypted, Plainfile, SecretVecWrapper},
        formats::BincodeCoder,
        BackedEntry,
    },
};

fn main() {
    let dataset_dir = temp_dir().join("shakespeare_secret");

    // Pull the complete works of Shakespeare from a disk.
    let shakespeare = BufReader::new(
        File::open("shakespeare-dataset/text/king-john_TXT_FolgerShakespeare.txt").unwrap(),
    )
    .lines()
    .map(|line| line.unwrap());

    // Initialize a directory backed array that
    // * Keeps the data in each entry guarded
    // * Keeps the length of each entry guarded
    // * Uses OnceCell for lower performance cost, but !Send
    // * Saves/loads from disk with Aes256Gcm encryption
    #[allow(clippy::type_complexity)]
    let mut shakespeare_encoded: DirectoryBackedArray<
        SecretVecWrapper<usize>,
        Vec<BackedEntry<OnceCell<SecretVecWrapper<u8>>, Encrypted<Plainfile>, BincodeCoder<_>>>,
    > = DirectoryBackedArray::new(dataset_dir.clone()).unwrap();

    // Add all the lines from the work to the directory as encrypted data.
    // Use `append_memory` instead of `append` to keep it memory.
    // Note that Encrypted's default implementation over paths will generate
    // random encryption keys/nonces that are not accessible.
    for work in shakespeare {
        shakespeare_encoded.append_memory(work).unwrap();
    }

    let num_you: usize = shakespeare_encoded
        .chunk_iter()
        .map(|chunk| {
            let chunk = chunk.unwrap();
            let chunk_ref = chunk.borrow();
            from_utf8(&chunk_ref).unwrap().matches("you").count()
        })
        .sum();
    println!("# \"you\" prior to modification: {}", num_you);

    shakespeare_encoded.chunk_mut_iter().for_each(|x| {
        let mut x = x.unwrap();

        {
            // Open the inner value, holding the guards in the local stack.
            let mut opened_x = x.borrow_mut();

            // View the data as &mut str, but keep it in guarded memory.
            let var = from_utf8_mut(&mut opened_x).unwrap();

            // Replace all instances of "thou" with "you", keeping all data (minus
            // some stack variables) within the protected memory.
            while let Some(idx) = var.rfind("thou") {
                let thou = "thou".as_bytes();
                let you = "you".as_bytes();

                let var = unsafe { var.as_bytes_mut() };

                var[idx..(idx + you.len())].copy_from_slice(you);
                for trailing_idx in (idx + you.len())..(var.len() - (thou.len() - you.len())) {
                    var[trailing_idx] = var[trailing_idx + 1];
                }

                for dangling_idx in var.len() - (thou.len() - you.len())..var.len() {
                    var[dangling_idx] = b'0';
                }
            }
        }
        // Now that the borrow of `x` is dropped, memory accesses will SIGSEGV.

        // Open access to x temporarily to update encrypted disk.
        x.flush().unwrap();
    });

    let num_you: usize = shakespeare_encoded
        .chunk_iter()
        .map(|chunk| {
            let chunk = chunk.unwrap();
            let chunk_ref = chunk.borrow();
            from_utf8(&chunk_ref).unwrap().matches("you").count()
        })
        .sum();
    println!("# \"you\" after modification: {}", num_you);

    // Drop the secret "thou" -> "you" translation, clearing out memory and
    // making it impossible to decode the directory.
    drop(shakespeare_encoded);

    // Example cleanup.
    let _ = remove_dir_all(dataset_dir);
}
