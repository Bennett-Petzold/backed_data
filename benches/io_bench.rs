#![cfg(all(feature = "bincode", feature = "directory"))]

use std::{
    env::temp_dir,
    fs::{create_dir, read_dir, read_to_string, remove_dir_all, File},
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::Duration,
};

use backed_data::{directory::StdDirBackedArray, entry::formats::BincodeCoder};
use chrono::Local;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use fs_extra::dir::get_size;
use humansize::{format_size, BINARY};
use tokio::runtime;

#[cfg(feature = "zstd")]
use backed_data::directory::ZstdDirBackedArray;

#[cfg(feature = "async_zstd")]
use backed_data::directory::AsyncZstdDirBackedArray;

#[cfg(feature = "async_bincode")]
use backed_data::{directory::AsyncStdDirBackedArray, entry::formats::AsyncBincodeCoder};

#[cfg(feature = "async")]
use futures::StreamExt;

#[cfg(feature = "async-zstd")]
use backed_array::zstd::async_impl::ZstdDirBackedArray as AsyncZstdBacked;

fn logfile() -> &'static Mutex<File> {
    static LOGFILE: OnceLock<Mutex<File>> = OnceLock::new();
    LOGFILE.get_or_init(|| {
        let _ = create_dir("bench_logs");
        Mutex::new(
            File::options()
                .create(true)
                .read(true)
                .write(true)
                .truncate(true)
                .open(
                    Path::new("bench_logs")
                        .join(Local::now().format("%Y-%m-%dT%H:%M:%S").to_string() + ".log"),
                )
                .unwrap(),
        )
    })
}

fn complete_works() -> &'static Vec<String> {
    static LOGFILE: OnceLock<Vec<String>> = OnceLock::new();
    LOGFILE.get_or_init(|| {
        read_dir("shakespeare-dataset/text")
            .unwrap()
            .map(|text| read_to_string(text.unwrap().path()).unwrap())
            .collect::<Vec<String>>()
    })
}

fn sync_all_dir(path: PathBuf) {
    read_dir(path).unwrap().for_each(|file_path| {
        File::open(file_path.unwrap().path())
            .unwrap()
            .sync_all()
            .unwrap()
    })
}

fn create_plainfiles(path: PathBuf, data: &[String]) -> StdDirBackedArray<u8, BincodeCoder> {
    let mut arr = StdDirBackedArray::<u8, BincodeCoder>::new(path).unwrap();
    for inner_data in data {
        arr.append(inner_data.as_ref()).unwrap();
    }
    arr
}

#[cfg(feature = "zstd")]
fn create_zstdfiles<'a, const LEVEL: u8>(
    path: PathBuf,
    data: &[String],
) -> ZstdDirBackedArray<'a, LEVEL, u8, BincodeCoder> {
    let mut arr: ZstdDirBackedArray<'a, LEVEL, u8, BincodeCoder> =
        ZstdDirBackedArray::new(path).unwrap();
    for inner_data in data {
        arr.append(inner_data.as_ref()).unwrap();
    }
    arr
}

#[cfg(feature = "async_bincode")]
async fn create_plainfiles_async(
    path: PathBuf,
    data: &[String],
) -> AsyncStdDirBackedArray<u8, AsyncBincodeCoder> {
    let mut arr: AsyncStdDirBackedArray<u8, AsyncBincodeCoder> =
        AsyncStdDirBackedArray::new(path).unwrap();
    for inner_data in data {
        arr.a_append(inner_data.as_ref()).await.unwrap();
    }
    arr
}

#[cfg(feature = "async_bincode")]
async fn create_plainfiles_async_parallel(
    path: PathBuf,
    data: &[String],
) -> AsyncStdDirBackedArray<u8, AsyncBincodeCoder> {
    use std::iter::repeat;

    let mut handles = data
        .iter()
        .map(|data| data.clone().into_bytes())
        .zip(repeat(path.clone()))
        .map(|(point, path)| {
            tokio::spawn(async move {
                let mut arr: AsyncStdDirBackedArray<u8, AsyncBincodeCoder> =
                    AsyncStdDirBackedArray::new(path).unwrap();
                arr.a_append(point).await.unwrap();
                arr
            })
        });

    let mut combined = handles.next().unwrap().await.unwrap();
    for next in handles {
        combined.a_append_dir(next.await.unwrap()).await.unwrap();
    }
    combined
}

#[cfg(all(feature = "async_zstd", feature = "async_bincode"))]
async fn create_zstdfiles_async<const LEVEL: u8>(
    path: PathBuf,
    data: &[String],
) -> AsyncZstdDirBackedArray<LEVEL, u8, AsyncBincodeCoder> {
    let mut arr: AsyncZstdDirBackedArray<LEVEL, u8, AsyncBincodeCoder> =
        AsyncZstdDirBackedArray::new(path).unwrap();
    for inner_data in data {
        arr.a_append(inner_data.as_ref()).await.unwrap();
    }
    arr
}

#[cfg(all(feature = "async_zstd", feature = "async_bincode"))]
async fn create_zstdfiles_async_parallel<const LEVEL: u8>(
    path: PathBuf,
    data: &[String],
) -> AsyncZstdDirBackedArray<LEVEL, u8, AsyncBincodeCoder> {
    use std::iter::repeat;

    let mut handles = data
        .iter()
        .map(|data| data.clone().into_bytes())
        .zip(repeat(path.clone()))
        .map(|(point, path)| {
            tokio::spawn(async move {
                let mut arr: AsyncZstdDirBackedArray<LEVEL, u8, AsyncBincodeCoder> =
                    AsyncZstdDirBackedArray::new(path).unwrap();
                arr.a_append(point).await.unwrap();
                arr
            })
        });

    let mut combined = handles.next().unwrap().await.unwrap();
    for next in handles {
        combined.a_append_dir(next.await.unwrap()).await.unwrap();
    }
    combined
}

fn file_creation_bench(c: &mut Criterion) {
    let data = complete_works();

    let path = temp_dir().join(uuid::Uuid::new_v4().to_string());
    let _ = create_dir(path.clone());

    let mut group = c.benchmark_group("file_creation_benches");
    group.bench_function("create_plainfiles", |b| {
        let _ = remove_dir_all(path.clone());
        create_dir(path.clone()).unwrap();
        b.iter(|| create_plainfiles(black_box(path.clone()), black_box(data)))
    });

    sync_all_dir(path.clone());
    println!(
        "Plainfiles size: {}",
        format_size(get_size(path.clone()).unwrap(), BINARY)
    );
    writeln!(
        logfile().lock().unwrap(),
        "Plainfiles size: {}",
        format_size(get_size(path.clone()).unwrap(), BINARY)
    )
    .unwrap();

    #[cfg(feature = "zstd")]
    group.bench_function("create_zstdfiles", |b| {
        let _ = remove_dir_all(path.clone());
        create_dir(path.clone()).unwrap();
        b.iter(|| create_zstdfiles::<0>(black_box(path.clone()), black_box(data)))
    });

    sync_all_dir(path.clone());
    println!(
        "Zstdfiles size: {}",
        format_size(get_size(path.clone()).unwrap(), BINARY)
    );
    writeln!(
        logfile().lock().unwrap(),
        "Zstdfiles size: {}",
        format_size(get_size(path.clone()).unwrap(), BINARY)
    )
    .unwrap();

    #[cfg(feature = "async")]
    {
        let rt = runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        group.bench_function("async_create_plainfiles", |b| {
            let _ = remove_dir_all(path.clone());
            create_dir(path.clone()).unwrap();
            b.to_async(&rt)
                .iter(|| create_plainfiles_async(black_box(path.clone()), black_box(data)))
        });

        sync_all_dir(path.clone());
        println!(
            "Async plainfiles size: {}",
            format_size(get_size(path.clone()).unwrap(), BINARY)
        );
        writeln!(
            logfile().lock().unwrap(),
            "Async plainfiles size: {}",
            format_size(get_size(path.clone()).unwrap(), BINARY)
        )
        .unwrap();

        group.bench_function("async_create_plainfiles_parallel", |b| {
            let _ = remove_dir_all(path.clone());
            create_dir(path.clone()).unwrap();
            b.to_async(&rt)
                .iter(|| create_plainfiles_async_parallel(black_box(path.clone()), black_box(data)))
        });

        sync_all_dir(path.clone());
        println!(
            "Async parallel plainfiles size: {}",
            format_size(get_size(path.clone()).unwrap(), BINARY)
        );
        writeln!(
            logfile().lock().unwrap(),
            "Async parallel plainfiles size: {}",
            format_size(get_size(path.clone()).unwrap(), BINARY)
        )
        .unwrap();

        #[cfg(feature = "async_zstd")]
        {
            group.bench_function("async_create_zstdfiles", |b| {
                let _ = remove_dir_all(path.clone());
                create_dir(path.clone()).unwrap();
                b.to_async(&rt)
                    .iter(|| create_zstdfiles_async::<0>(black_box(path.clone()), black_box(data)))
            });

            sync_all_dir(path.clone());
            println!(
                "Async zstdfiles size: {}",
                format_size(get_size(path.clone()).unwrap(), BINARY)
            );
            writeln!(
                logfile().lock().unwrap(),
                "Async zstdfiles size: {}",
                format_size(get_size(path.clone()).unwrap(), BINARY)
            )
            .unwrap();

            group.bench_function("async_create_zstdfiles_parallel", |b| {
                let _ = remove_dir_all(path.clone());
                create_dir(path.clone()).unwrap();
                b.to_async(&rt).iter(|| async {
                    create_zstdfiles_async_parallel::<0>(black_box(path.clone()), black_box(data))
                        .await
                })
            });

            sync_all_dir(path.clone());
            println!(
                "Async zstdfiles parallel size: {}",
                format_size(get_size(path.clone()).unwrap(), BINARY)
            );
            writeln!(
                logfile().lock().unwrap(),
                "Async zstdfiles parallel size: {}",
                format_size(get_size(path.clone()).unwrap(), BINARY)
            )
            .unwrap();
        }

        rt.shutdown_background();
    }

    remove_dir_all(path.clone()).unwrap();
}

fn read_plainfiles(f: PathBuf) -> usize {
    let arr: StdDirBackedArray<u8, BincodeCoder> = StdDirBackedArray::load(f).unwrap();
    arr.iter().collect::<Result<Vec<_>, _>>().unwrap().len()
}

#[cfg(feature = "zstd")]
fn read_zstdfiles(f: PathBuf) -> usize {
    let arr: ZstdDirBackedArray<0, u8, BincodeCoder> = ZstdDirBackedArray::load(f).unwrap();
    arr.iter().collect::<Result<Vec<_>, _>>().unwrap().len()
}

#[cfg(feature = "async_bincode")]
async fn read_plainfiles_async(path: PathBuf) -> usize {
    let arr: AsyncStdDirBackedArray<u8, AsyncBincodeCoder> =
        AsyncStdDirBackedArray::a_load(path).await.unwrap();
    arr.stream().collect::<Vec<_>>().await.len()
}

#[cfg(all(feature = "async_zstd", feature = "async_bincode"))]
async fn read_zstdfiles_async(path: PathBuf) -> usize {
    let arr: AsyncZstdDirBackedArray<0, u8, AsyncBincodeCoder> =
        AsyncZstdDirBackedArray::a_load(path).await.unwrap();
    arr.stream().collect::<Vec<_>>().await.len()
}

fn file_load_bench(c: &mut Criterion) {
    let data = complete_works();

    let path = temp_dir().join(uuid::Uuid::new_v4().to_string());
    let _ = remove_dir_all(path.clone());
    create_dir(path.clone()).unwrap();

    let mut group = c.benchmark_group("file_load_benches");

    let arr = create_plainfiles(path.clone(), data);
    arr.save().unwrap();
    sync_all_dir(path.clone());

    group.bench_function("load_plainfiles", |b| {
        b.iter(|| black_box(read_plainfiles(path.clone())))
    });

    #[cfg(feature = "zstd")]
    {
        let _ = remove_dir_all(path.clone());
        create_dir(path.clone()).unwrap();

        let arr = create_zstdfiles::<0>(path.clone(), data);
        arr.save().unwrap();
        sync_all_dir(path.clone());

        group.bench_function("load_zstdfiles", |b| {
            b.iter(|| black_box(read_zstdfiles(path.clone())))
        });
    }

    #[cfg(feature = "async_bincode")]
    {
        let rt = runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        let _ = remove_dir_all(path.clone());
        create_dir(path.clone()).unwrap();

        rt.block_on(async {
            let arr = create_plainfiles_async(path.clone(), data).await;
            arr.a_save().await.unwrap();
        });
        sync_all_dir(path.clone());

        group.bench_function("async_load_plainfiles", |b| {
            b.to_async(&rt)
                .iter(|| black_box(read_plainfiles_async(path.clone())))
        });

        #[cfg(feature = "async_zstd")]
        {
            let _ = remove_dir_all(path.clone());
            create_dir(path.clone()).unwrap();

            rt.block_on(async {
                let arr = create_zstdfiles_async::<0>(path.clone(), data).await;
                arr.a_save().await.unwrap();
            });
            sync_all_dir(path.clone());

            group.bench_function("async_load_zstdfiles", |b| {
                b.to_async(&rt)
                    .iter(|| black_box(read_zstdfiles_async(path.clone())))
            });
        }

        rt.shutdown_background()
    }

    remove_dir_all(path.clone()).unwrap();
}

#[cfg(feature = "zstd")]
fn zstd_setting_benches(c: &mut Criterion) {
    #[cfg(any(feature = "zstdmt", feature = "async_zstdmt"))]
    use backed_data::entry::disks::ZSTD_MULTITHREAD;

    use criterion::BenchmarkId;

    let data = complete_works();

    let path = temp_dir().join(uuid::Uuid::new_v4().to_string());

    #[cfg(feature = "async_zstd")]
    let rt = runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut group = c.benchmark_group("zstd_setting_benches");
    seq_macro::seq!(ZSTD_LEVEL in 0..22 {
        #[cfg(any(feature = "zstdmt", feature = "async_zstdmt"))]
        {
            *ZSTD_MULTITHREAD.lock().unwrap() = 0;
        }

        let _ = create_dir(path.clone());

        group.bench_with_input(
            BenchmarkId::new("zstd_write", format!("Compression Level: {}", ZSTD_LEVEL)),
            &ZSTD_LEVEL,
            |b, _| {
                let _ = remove_dir_all(path.clone());
                create_dir(path.clone()).unwrap();
                b.iter(|| create_zstdfiles::<ZSTD_LEVEL>(black_box(path.clone()), black_box(data)))
            },
        );

        sync_all_dir(path.clone());
        println!(
            "Zstdfiles size with compression {}: {}",
            ZSTD_LEVEL,
            format_size(get_size(path.clone()).unwrap(), BINARY)
        );
        writeln!(
            logfile().lock().unwrap(),
            "Zstdfiles size with compression {}: {}",
            ZSTD_LEVEL,
            format_size(get_size(path.clone()).unwrap(), BINARY)
        )
        .unwrap();

        create_zstdfiles::<ZSTD_LEVEL>(path.clone(), data)
            .save()
            .unwrap();
        group.bench_with_input(
            BenchmarkId::new("zstd_read", format!("Compression Level: {}", ZSTD_LEVEL)),
            &(),
            |b, _| b.iter(|| black_box(read_zstdfiles(path.clone()))),
        );

        #[cfg(all(feature = "async_zstd", feature = "async_bincode"))]
        {
            group.bench_with_input(
                BenchmarkId::new(
                    "zstd_write_async",
                    format!("Compression Level: {}", ZSTD_LEVEL),
                ),
                &ZSTD_LEVEL,
                |b, _| {
                    let _ = remove_dir_all(path.clone());
                    create_dir(path.clone()).unwrap();
                    b.to_async(&rt).iter(|| {
                        create_zstdfiles_async::<ZSTD_LEVEL>(
                            black_box(path.clone()),
                            black_box(data),
                        )
                    })
                },
            );

            sync_all_dir(path.clone());
            println!(
                "Zstdfiles async size with compression {}: {}",
                ZSTD_LEVEL,
                format_size(get_size(path.clone()).unwrap(), BINARY)
            );
            writeln!(
                logfile().lock().unwrap(),
                "Zstdfiles async size with compression {}: {}",
                ZSTD_LEVEL,
                format_size(get_size(path.clone()).unwrap(), BINARY)
            )
            .unwrap();

            let _ = remove_dir_all(path.clone());
            create_dir(path.clone()).unwrap();

            let arr = rt.block_on(create_zstdfiles_async::<ZSTD_LEVEL>(path.clone(), data));
            rt.block_on(arr.a_save()).unwrap();
            sync_all_dir(path.clone());

            group.bench_with_input(
                BenchmarkId::new(
                    "zstd_read_async",
                    format!("Compression Level: {}", ZSTD_LEVEL),
                ),
                &(),
                |b, _| {
                    b.to_async(&rt)
                        .iter(|| black_box(read_zstdfiles_async(path.clone())))
                },
            );
        }

        #[cfg(any(feature = "zstdmt", feature = "async-zstdmt"))]
        for t_count in 0..5 {
            *ZSTD_MULTITHREAD.lock().unwrap() = t_count;
            #[cfg(feature = "zstdmt")]
            group.bench_with_input(
                BenchmarkId::new(
                    "zstd_write_mt",
                    format!("Compression Level: {}, Thread Count: {t_count}", ZSTD_LEVEL),
                ),
                &ZSTD_LEVEL,
                |b, _| {
                    let _ = remove_dir_all(path.clone());
                    create_dir(path.clone()).unwrap();
                    b.iter(|| {
                        create_zstdfiles::<ZSTD_LEVEL>(
                            black_box(path.clone()),
                            black_box(data),
                        )
                    })
                },
            );

            #[cfg(feature = "async-zstdmt")]
            group.bench_with_input(
                BenchmarkId::new(
                    "zstd_write_async_mt",
                    format!("Compression Level: {}, Thread Count: {t_count}", ZSTD_LEVEL),
                ),
                &ZSTD_LEVEL,
                |b, _| {
                    let _ = remove_dir_all(path.clone());
                    create_dir(path.clone()).unwrap();
                    b.to_async(&rt).iter(|| {
                        create_zstdfiles_async::<ZSTD_LEVEL>(
                            black_box(path.clone()),
                            black_box(data),
                        )
                    })
                },
            );
        }
    });
    group.finish();

    #[cfg(feature = "async_zstd")]
    rt.shutdown_background();

    remove_dir_all(path.clone()).unwrap();
}

criterion_group! {
    name = io_benches;
    config = Criterion::default().sample_size(10).measurement_time(Duration::from_secs(10));
    targets = file_creation_bench,
    file_load_bench,
    zstd_setting_benches
}
criterion_main!(io_benches);
