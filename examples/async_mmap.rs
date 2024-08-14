#![cfg(all(feature = "mmap", feature = "async"))]

#[cfg(target_os = "windows")]
fn main() {}

#[cfg(not(target_os = "windows"))]
#[tokio::main]
async fn main() {
    not_windows::example().await
}

#[cfg(not(target_os = "windows"))]
mod not_windows {
    use std::{
        array,
        cmp::max,
        env::temp_dir,
        hint::black_box,
        time::{Duration, SystemTime},
    };

    use backed_data::{
        entry::{
            adapters::{SyncAsAsyncRead, SyncAsAsyncReadBg, SyncAsAsyncWriteBg},
            disks::{mmap::ReadMmapCursor, AsyncReadDisk, AsyncWriteDisk, Mmap},
            SyncAsAsync,
        },
        utils::blocking::BlockingFn,
    };
    use futures::{AsyncReadExt, AsyncWriteExt};
    use tokio::{join, task::spawn_blocking, time::sleep};

    const ELEMENTS_PER: usize = 256;
    const NUM_COPIES: usize = 1_000;
    const SIZE: usize = ELEMENTS_PER * NUM_COPIES;

    pub async fn example() {
        let backing_file = temp_dir().join("backed_array_async_mmap");

        let disk = SyncAsAsync::new(
            Mmap::try_from(backing_file.clone()).unwrap(),
            |x: SyncAsAsyncReadBg<ReadMmapCursor>| {
                spawn_blocking(|| x.call());
            },
            |x: SyncAsAsyncWriteBg<_>| {
                spawn_blocking(|| x.call());
            },
            None,
            None,
        );

        {
            // Data can be written in asynchronously.
            let input_data_single: [u8; ELEMENTS_PER] = array::from_fn(|x| x as u8);
            let input_data = [input_data_single; NUM_COPIES].concat();
            let mut write_disk = disk.async_write_disk().await.unwrap();

            let start_time = SystemTime::now();

            write_disk.write_all(&input_data).await.unwrap();
            write_disk.close().await.unwrap();

            let elapsed_time = SystemTime::now().duration_since(start_time).unwrap();
            println!("Time to write: {:#?}", elapsed_time);
        }

        let read_fn = |mut disk: SyncAsAsyncRead| async move {
            let mut buf = Vec::with_capacity(SIZE);

            let start_time = SystemTime::now();
            disk.read_to_end(&mut buf).await.unwrap();
            let elapsed_time = SystemTime::now().duration_since(start_time).unwrap();

            // Prevent this read from being optimized away.
            black_box(buf);

            elapsed_time
        };

        // Wait for write disk to drop.
        while disk.async_read_disk().await.is_err() {
            sleep(Duration::from_millis(1)).await;
        }

        {
            let read_disk = disk.async_read_disk().await.unwrap();
            let time = read_fn(read_disk).await;
            println!("\nTime to read a single: {:#?}", time);
        }

        {
            let read_disk = disk.async_read_disk().await.unwrap();
            let time = read_fn(read_disk).await;
            println!("\nTime for second single read: {:#?}", time);
        }

        {
            let read_disk_1 = disk.async_read_disk().await.unwrap();
            let read_disk_2 = disk.async_read_disk().await.unwrap();
            let (time_1, time_2) = join!(read_fn(read_disk_1), read_fn(read_disk_2));

            println!("\nConcurrent read time is similar to single read time.",);
            println!("Time to read two concurrently: {:#?}", max(time_1, time_2));
        }
    }
}
