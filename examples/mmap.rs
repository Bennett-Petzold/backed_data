#![cfg(feature = "mmap")]

/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

#[cfg(target_os = "windows")]
fn main() {}

#[cfg(not(target_os = "windows"))]
fn main() {
    not_windows::example()
}

#[cfg(not(target_os = "windows"))]
mod not_windows {
    use std::{
        array,
        env::temp_dir,
        fs::remove_dir_all,
        hint::black_box,
        io::{Read, Write},
        iter::repeat_with,
        sync::{
            atomic::{self, AtomicUsize},
            Arc, Condvar, Mutex,
        },
        thread::{available_parallelism, spawn},
        time::SystemTime,
    };

    use backed_data::entry::disks::{Mmap, ReadDisk, WriteDisk};

    const ELEMENTS_PER: usize = 256;
    const NUM_COPIES: usize = 1_000;
    const SIZE: usize = ELEMENTS_PER * NUM_COPIES;

    pub fn example() {
        println!("All measurements can significantly change based on the environment.",);

        let backing_file = temp_dir().join("backed_array_mmap");

        let mut disk = Mmap::try_from(backing_file.clone()).unwrap();

        let mut write_disk = disk.write_disk().unwrap();

        // Cannot get another mmap instance while one is live.
        assert!(disk.write_disk().is_err());
        assert!(disk.read_disk().is_err());

        {
            let input_data_single: [u8; ELEMENTS_PER] = array::from_fn(|x| x as u8);
            let input_data: Vec<_> = [input_data_single; NUM_COPIES].concat();

            let prior_to_first_write = SystemTime::now();
            write_disk.write_all(&input_data).unwrap();

            let elapsed_time = SystemTime::now()
                .duration_since(prior_to_first_write)
                .unwrap();

            println!("\nTime for write: {:#?}", elapsed_time);
        }

        drop(write_disk);
        let read_disk = disk.read_disk().unwrap();

        // Cannot get a mutable mmap instance while reading
        assert!(disk.write_disk().is_err());
        // Can get another immutable mmap instance while reading
        assert!(disk.read_disk().is_ok());

        drop(read_disk);

        {
            let mut buf = Vec::with_capacity(SIZE);
            let mut read_disk = disk.read_disk().unwrap();

            let prior_to_first_read = SystemTime::now();
            read_disk.read_to_end(&mut buf).unwrap();

            let elapsed_time = SystemTime::now()
                .duration_since(prior_to_first_read)
                .unwrap();

            println!("\nReads reuse the memory mapping from writes.",);
            println!("(This is usually higher than write time).",);
            println!("Time for read: {:#?}", elapsed_time,);

            // Prevent read from being optimized away.
            black_box(buf);
        };

        let read_elapsed_time = {
            let mut buf = Vec::with_capacity(SIZE);
            let mut read_disk = disk.read_disk().unwrap();

            let prior_to_second_read = SystemTime::now();
            read_disk.read_to_end(&mut buf).unwrap();

            let elapsed_time = SystemTime::now()
                .duration_since(prior_to_second_read)
                .unwrap();

            println!("\nReads reuse the memory mapping from each other.",);
            println!("(This is usually significantly lower than initial read time).",);
            println!("Time for second read: {:#?}", elapsed_time,);

            // Prevent read from being optimized away.
            black_box(buf);

            elapsed_time
        };

        {
            let num_threads = available_parallelism().map(|x| x.into()).unwrap_or(1) / 2;
            if num_threads > 1 {
                let trigger = Arc::new((
                    Condvar::new(),
                    AtomicUsize::new(num_threads),
                    Mutex::new(false),
                ));
                let disk = Arc::new(disk);

                let futures: Vec<_> = repeat_with(|| {
                    let trigger = trigger.clone();
                    let disk = disk.clone();
                    spawn(move || {
                        let mut buf = Vec::with_capacity(SIZE);
                        trigger.1.fetch_sub(1, atomic::Ordering::Relaxed);
                        while !*trigger.0.wait(trigger.2.lock().unwrap()).unwrap() {}
                        disk.read_disk().unwrap().read_to_end(&mut buf).unwrap();
                        buf
                    })
                })
                .take(num_threads)
                .collect();
                while trigger.1.load(atomic::Ordering::Relaxed) > 0 {}

                let prior_to_second_read = SystemTime::now();
                *trigger.2.lock().unwrap() = true;
                trigger.0.notify_all();

                let all_bufs: Vec<_> = futures.into_iter().map(|x| x.join().unwrap()).collect();

                let elapsed_time = SystemTime::now()
                    .duration_since(prior_to_second_read)
                    .unwrap();

                println!(
                    "\nTime for {num_threads} parallel reads: {:#?}",
                    elapsed_time,
                );
                println!("(This is usually slower than in sequence, due to coordination cost).",);
                println!(
                    "Approximate time this would take in in sequence: {:#?}",
                    read_elapsed_time * (num_threads as u32),
                );

                // Prevent read from being optimized away.
                black_box(all_bufs);
            }
        }

        let disk = Mmap::try_from(backing_file.clone()).unwrap();
        {
            let mut read_disk = disk.read_disk().unwrap();
            let mut buf = Vec::with_capacity((256 / 2) * 10_000);

            let prior_to_unbuffered_read = SystemTime::now();
            read_disk.read_to_end(&mut buf).unwrap();

            let elapsed_time = SystemTime::now()
                .duration_since(prior_to_unbuffered_read)
                .unwrap();

            println!("\nRead without memory buffer: {:#?}", elapsed_time,);
            println!("(This benefits from the previous mapping, but still has to read back in).",);

            // Prevent read from being optimized away.
            black_box(buf);
        }

        {
            let mut read_disk = disk.read_disk().unwrap();
            let mut buf = Vec::with_capacity((256 / 2) * 10_000);
            let prior_to_unbuffered_read = SystemTime::now();
            read_disk.read_to_end(&mut buf).unwrap();

            let elapsed_time = SystemTime::now()
                .duration_since(prior_to_unbuffered_read)
                .unwrap();

            println!("\nRead with reused memory buffer: {:#?}", elapsed_time,);
            println!("(This is usually lower than non-buffer read time).",);

            // Prevent read from being optimized away.
            black_box(buf);
        }

        let _ = remove_dir_all(backing_file);
    }
}
