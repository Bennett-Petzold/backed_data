#![cfg(all(feature = "async_zstd", feature = "async_bincode", feature = "csv"))]

use std::{
    env::temp_dir,
    fs::read_dir,
    future::ready,
    io::{Cursor, ErrorKind},
    path::PathBuf,
    pin::pin,
    str::FromStr,
    time::SystemTime,
};

use async_zip::base::read::stream::ZipFileReader;
use backed_data::{
    directory::AsyncZstdDirBackedArray,
    entry::formats::{AsyncBincodeCoder, AsyncDecoder},
};
use csv::Reader;
use fs_extra::dir::get_size;
use futures::{stream, Stream, StreamExt, TryStreamExt};
use get_size::GetSize;
use humansize::{format_size, BINARY};
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressFinish, ProgressStyle};
use serde::{Deserialize, Serialize};
use tokio::{fs::remove_dir_all, join, sync::mpsc, task::spawn_blocking};
use tokio_stream::wrappers::UnboundedReceiverStream;

const BLS_DATA: &str = "https://data.bls.gov/cew/data/files/2023/csv/2023_qtrly_by_industry.zip";
const NUM_ITEMS: usize = 2159; // Manually counted
const COMPRESSION_LEVEL: u8 = 11; // Middle of range

const ITEMS_PER_ZSTD_FILE: usize = NUM_ITEMS.div_ceil(100);
const NUM_ZSTD_FILES: usize = NUM_ITEMS.div_ceil(ITEMS_PER_ZSTD_FILE);

#[tokio::main]
async fn main() {
    // Manually counted stats
    println!("ZIP archive size: ~ 481 MiB");
    println!("Uncompressed size: ~ 4.4 GiB");

    println!("Concurrent re-encoding buffer: {NUM_ZSTD_FILES} items at once.");

    let target_dir = temp_dir().join("backed_data_large_reencode");
    let _ = remove_dir_all(&target_dir).await;
    println!("Placing files in: {:#?}, deleting at end.", target_dir);

    // Live terminal progress bars
    let all_progress = MultiProgress::new();
    let (download_pb, unzip_pb, schedule_pb, decode_csv_pb, encrypt_pb, unify_pb) =
        create_progress_bars(all_progress.clone());

    // Generate a stream from the remote CSV file.
    let bls_stream = bls_setup(download_pb, unzip_pb);

    // Create a stream of backed arrays
    let mut bls_backings = pin!(bls_stream
        .map(|raw_data| {
            let decode_csv_pb = decode_csv_pb.clone();
            async move {
                // Use a blocking thread to run csv conversion
                let data: Vec<QecwData> = spawn_blocking(|| {
                    Reader::from_reader(Cursor::new(raw_data))
                        .deserialize()
                        .collect::<Result<Vec<_>, _>>()
                        .unwrap()
                })
                .await
                .unwrap();
                decode_csv_pb.inc(1);

                data
            }
        })
        // Collect many CSV files for one ZSTD backing
        .chunks(ITEMS_PER_ZSTD_FILE)
        .map(|data| {
            schedule_pb.inc(1);

            let target_dir = target_dir.clone();
            let encrypt_pb = encrypt_pb.clone();
            async move {
                // Spawn each encoding to execute concurrently
                tokio::spawn(async move {
                    // Collect the CSV conversion futures, once the blocking
                    // thread is done.
                    let data: Vec<_> = stream::iter(data)
                        .then(|data| data)
                        .collect::<Vec<_>>()
                        .await
                        .into_iter()
                        .flatten()
                        .collect();

                    // Create a random UUID file in `target_dir`
                    let mut backing = AsyncZstdDirBackedArray::<
                        COMPRESSION_LEVEL,
                        QecwData,
                        AsyncBincodeCoder<_>,
                    >::a_new(target_dir)
                    .await
                    .unwrap();

                    // Re-encode the data as zstd-compressed bincode, removing
                    // it from memory and flushing to disk.
                    backing.a_append(data).await.unwrap();

                    encrypt_pb.inc(1);

                    backing
                })
                .await
                .unwrap()
            }
        })
        // Wait for the spawned futures in no particular order.
        .buffer_unordered(NUM_ZSTD_FILES));

    // Combine all the files in this directory into one representation, as soon
    // as they are complete.
    let mut backing_array = bls_backings.next().await.unwrap();
    unify_pb.inc(1);
    while let Some(backing) = bls_backings.next().await {
        backing_array.a_append_dir(backing).await.unwrap();
        unify_pb.inc(1);
    }

    all_progress.set_draw_target(ProgressDrawTarget::hidden());

    // Collect and print some stats about backed array
    show_load_data(backing_array, &target_dir).await;

    remove_dir_all(&target_dir).await.unwrap();
}

// ---------- Code beyond this point is not key to the example ---------- //

/// Set up a stream that downloads and unzips bls data.
fn bls_setup(download_pb: ProgressBar, unzip_pb: ProgressBar) -> impl Stream<Item = Vec<u8>> {
    let (tx, bls_data_rx) = mpsc::unbounded_channel();

    // Pull BLS data from remote.
    tokio::spawn(async move {
        let mut bls_response = reqwest::get(reqwest::Url::from_str(BLS_DATA).unwrap())
            .await
            .unwrap();
        download_pb.set_length(bls_response.content_length().unwrap());
        loop {
            match bls_response.chunk().await {
                Err(x) => tx
                    .send(Err(std::io::Error::new(ErrorKind::Other, x)))
                    .unwrap(),
                Ok(None) => break,
                Ok(Some(x)) => {
                    download_pb.inc(x.len() as u64);
                    tx.send(Ok(x)).unwrap()
                }
            }
        }
    });

    // Mutate the BLS rx into a zip file reader
    let bls_data_stream = UnboundedReceiverStream::new(bls_data_rx).into_async_read();
    let bls_unzip = ZipFileReader::new(bls_data_stream);

    // Future stream that produces each bls entry
    stream::unfold(bls_unzip, move |bls_unzip| {
        let unzip_pb = unzip_pb.clone();
        async move {
            let mut bls_unzip = bls_unzip.next_with_entry().await.unwrap()?;
            let mut sink = Vec::new();
            bls_unzip
                .reader_mut()
                .read_to_end_checked(&mut sink)
                .await
                .unwrap();
            unzip_pb.inc(1);
            Some((sink, bls_unzip.done().await.unwrap()))
        }
    })
}

// ---------- Code beyond this point is unrelated to the processing pipeline ---------- //

/// Collect and print some stats about backed array.
async fn show_load_data<const ZSTD_LEVEL: u8, T, Coder>(
    mut backing_array: AsyncZstdDirBackedArray<'_, ZSTD_LEVEL, T, Coder>,
    target_dir: &PathBuf,
) where
    T: Send + Sync + for<'de> Deserialize<'de> + GetSize,
    Coder: AsyncDecoder<
        async_compression::futures::bufread::ZstdDecoder<
            futures::io::BufReader<backed_data::entry::disks::AsyncFile>,
        >,
        T = Box<[T]>,
    >,
{
    println!("# Loaded entries: {}", backing_array.loaded_len(),);
    println!(
        "Memory size: ~ {}",
        format_size(mem_size(&mut backing_array).await, BINARY)
    );
    println!(
        "Directory size: ~ {}",
        format_size(get_size(target_dir).unwrap(), BINARY)
    );

    let mut files: Vec<_> = read_dir(target_dir)
        .unwrap()
        .map(|e| get_size(e.unwrap().path()).unwrap())
        .collect();
    files.sort_unstable();
    println!("Smallest file size: ~ {}", format_size(files[0], BINARY));
    println!(
        "Largest file size: ~ {}",
        format_size(files[files.len() - 1], BINARY)
    );
    println!(
        "Mean file size: ~ {}",
        format_size((files.iter().sum::<u64>() as usize) / files.len(), BINARY)
    );

    let prior_to_first_load = SystemTime::now();
    let _ = backing_array.a_get(1_500_000).await;

    println!(
        "\nTime to load get(1_500_000): {:#?}",
        SystemTime::now()
            .duration_since(prior_to_first_load)
            .unwrap()
    );
    println!(
        "# Loaded entries after get(0): ~ {}",
        backing_array.loaded_len()
    );
    println!(
        "Memory size after get(1_500_000): ~ {}",
        format_size(mem_size(&mut backing_array).await, BINARY)
    );

    let prior_to_second_load = SystemTime::now();
    let _ = join!(
        backing_array.a_get(1_500_000 * 2),
        backing_array.a_get(1_500_000 * 3)
    );

    println!(
        "\nTime to load (1_500_000 * [2, 3]) concurrently: {:#?}",
        SystemTime::now()
            .duration_since(prior_to_second_load)
            .unwrap()
    );
    println!(
        "# Loaded entries after (1_500_000 * [2, 3]) concurrently: ~ {}",
        backing_array.loaded_len()
    );
    println!(
        "Memory size after after (1_500_000 * [2, 3]) concurrently: ~ {}",
        format_size(mem_size(&mut backing_array).await, BINARY)
    );

    let prior_to_third_load = SystemTime::now();
    for x in [1_500_000; 3]
        .into_iter()
        .enumerate()
        .map(|(idx, x)| (x * (idx + 1)) + 1)
    {
        let _ = backing_array.a_get(x).await;
    }
    println!(
        "\nTime to load 3 gets from cached memory: {:#?}",
        SystemTime::now()
            .duration_since(prior_to_third_load)
            .unwrap()
    );
}

#[derive(Debug, Default, PartialEq, PartialOrd, Clone, Serialize, Deserialize, GetSize)]
pub struct QecwData {
    area_fips: String,
    own_code: u8,
    industry_code: String,
    agglvl_code: String,
    size_code: u8,
    year: String,
    qtr: u8,
    disclosure_code: Option<char>,
    area_title: String,
    own_title: String,
    industry_title: String,
    agglvl_title: String,
    size_title: String,
    qtrly_estabs_count: f32,
    month1_emplvl: f32,
    month2_emplvl: f32,
    month3_emplvl: f32,
    total_qtrly_wages: f32,
    taxable_qtrly_wages: f32,
    qtrly_contributions: f32,
    avg_wkly_wage: f32,
    lq_disclosure_code: Option<char>,
    lq_qtrly_estabs_count: f32,
    lq_month1_emplvl: f32,
    lq_month2_emplvl: f32,
    lq_month3_emplvl: f32,
    lq_total_qtrly_wages: f32,
    lq_qtrly_contributions: f32,
    lq_avg_wkly_wage: f32,
    oty_disclosure_code: Option<char>,
    oty_qtrly_estabs_count_chg: f32,
    oty_qtrly_estabs_count_pct_chg: f32,
    oty_month1_emplvl_chg: f32,
    oty_month1_emplvl_pct_chg: f32,
    oty_month2_emplvl_chg: f32,
    oty_month2_emplvl_pct_chg: f32,
    oty_month3_emplvl_chg: f32,
    oty_month3_emplvl_pct_chg: f32,
    oty_total_qtrly_wages_chg: f32,
    oty_total_qtrly_wages_pct_chg: f32,
    oty_taxable_qtrly_wages_chg: f32,
    oty_taxable_qtrly_wages_pct_chg: f32,
    oty_qtrly_contributions_chg: f32,
    oty_qtrly_contributions_pct_chg: f32,
    oty_avg_wkly_wage_chg: f32,
    oty_avg_wkly_wage_pct_chg: f32,
}

/// Get memory footprint of the backed array.
async fn mem_size<const ZSTD_LEVEL: u8, T, Coder>(
    backing_array: &mut AsyncZstdDirBackedArray<'_, ZSTD_LEVEL, T, Coder>,
) -> usize
where
    T: Send + Sync + for<'de> Deserialize<'de> + GetSize,
    Coder: AsyncDecoder<
        async_compression::futures::bufread::ZstdDecoder<
            futures::io::BufReader<backed_data::entry::disks::AsyncFile>,
        >,
        T = Box<[T]>,
    >,
{
    stream::iter(backing_array.raw_chunks())
        .then(|mut chunk| async move {
            let chunk = chunk.as_mut();
            if chunk.is_loaded() {
                if let Ok(chunk) = chunk.a_load().await {
                    chunk.iter().map(|item| item.get_size()).sum()
                } else {
                    panic!("Error calculating mem_size")
                }
            } else {
                0
            }
        })
        .fold(0, |acc, x| ready(acc + x))
        .await
        + backing_array.key_starts().get_size()
        + backing_array.key_ends().get_size()
}

/// 100% aesthetics.
fn create_progress_bars(
    all_progress: MultiProgress,
) -> (
    ProgressBar,
    ProgressBar,
    ProgressBar,
    ProgressBar,
    ProgressBar,
    ProgressBar,
) {
    let download_pb = all_progress.add(ProgressBar::new(0));
    download_pb.set_style(
        ProgressStyle::with_template(
            "Download:    [{elapsed_precise}] [{bar:<80.cyan/blue}] {percent}% ({eta_precise})",
        )
        .unwrap()
        .progress_chars("#>-"),
    );
    download_pb.clone().with_finish(ProgressFinish::Abandon);

    let unzip_pb = all_progress.add(ProgressBar::new(NUM_ITEMS as u64));
    unzip_pb.set_style(
        ProgressStyle::with_template(
            "Unzip:       [{elapsed_precise}] [{bar:<80.green/22}] {pos}/{len} ({eta_precise})",
        )
        .unwrap()
        .progress_chars("#>-"),
    );
    unzip_pb.clone().with_finish(ProgressFinish::Abandon);

    let decode_csv_pb = all_progress.add(ProgressBar::new(NUM_ITEMS as u64));
    decode_csv_pb.set_style(
        ProgressStyle::with_template(
            "Decoding:    [{elapsed_precise}] [{bar:<80.cyan/blue}] {pos}/{len} ({eta_precise})",
        )
        .unwrap()
        .progress_chars("#>-"),
    );
    decode_csv_pb.clone().with_finish(ProgressFinish::Abandon);

    let schedule_pb = all_progress.add(ProgressBar::new(NUM_ZSTD_FILES as u64));
    schedule_pb.set_style(
        ProgressStyle::with_template(
            "Scheduling:  [{elapsed_precise}] [{bar:<80.green/22}] {pos}/{len} ({eta_precise})",
        )
        .unwrap()
        .progress_chars("#>-"),
    );
    schedule_pb.clone().with_finish(ProgressFinish::Abandon);

    let encrypt_pb = all_progress.add(ProgressBar::new(NUM_ZSTD_FILES as u64));
    encrypt_pb.set_style(
        ProgressStyle::with_template(
            "Encryption:  [{elapsed_precise}] [{bar:<80.cyan/blue}] {pos}/{len} ({eta_precise})",
        )
        .unwrap()
        .progress_chars("#>-"),
    );
    encrypt_pb.clone().with_finish(ProgressFinish::Abandon);

    let unify_pb = all_progress.add(ProgressBar::new(NUM_ZSTD_FILES as u64));
    unify_pb.set_style(
        ProgressStyle::with_template(
            "Unification: [{elapsed_precise}] [{bar:<80.green/22}] {pos}/{len} ({eta_precise})",
        )
        .unwrap()
        .progress_chars("#>-"),
    );
    unify_pb.clone().with_finish(ProgressFinish::Abandon);

    all_progress.set_move_cursor(true);

    (
        download_pb,
        unzip_pb,
        schedule_pb,
        decode_csv_pb,
        encrypt_pb,
        unify_pb,
    )
}
