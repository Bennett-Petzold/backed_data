use std::{
    env::temp_dir,
    fs::{create_dir, read_dir, read_to_string, remove_dir_all, File},
    io::{Seek, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::Duration,
};

use backed_array::{
    directory::sync_impl::DirectoryBackedArray, meta::sync_impl::BackedArrayWrapper,
    zstd::sync_impl::ZstdDirBackedArray,
};
use chrono::Local;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use fs_extra::dir::get_size;
use humansize::{format_size, BINARY};

#[cfg(feature = "async")]
use {
    backed_array::{
        directory::async_impl::DirectoryBackedArray as AsyncDirBacked,
        meta::async_impl::BackedArrayWrapper as AsyncWrapper,
    },
    futures::{stream, StreamExt},
    std::pin::pin,
    tokio::{fs::File as AsyncFile, runtime::Runtime},
};

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

fn create_plainfiles(path: PathBuf, data: &[String]) -> DirectoryBackedArray<u8> {
    let mut arr: DirectoryBackedArray<u8> = DirectoryBackedArray::new(path).unwrap();
    for inner_data in data {
        arr.append(inner_data.as_ref()).unwrap();
    }
    arr
}

#[cfg(feature = "zstd")]
fn create_zstdfiles(path: PathBuf, data: &[String], level: Option<i32>) -> ZstdDirBackedArray<u8> {
    let mut arr: ZstdDirBackedArray<u8> = ZstdDirBackedArray::new(path, level).unwrap();
    for inner_data in data {
        arr.append(inner_data.as_ref()).unwrap();
    }
    arr
}

#[cfg(feature = "async")]
async fn create_plainfiles_async(path: PathBuf, data: &[String]) -> AsyncDirBacked<u8> {
    let mut arr: AsyncDirBacked<u8> = AsyncDirBacked::new(path).await.unwrap();
    for inner_data in data {
        arr.append(inner_data.as_ref()).await.unwrap();
    }
    arr
}

#[cfg(feature = "async")]
async fn create_plainfiles_async_parallel(path: PathBuf, data: &[String]) -> AsyncDirBacked<u8> {
    use std::iter::repeat;

    let gen = stream::iter(data.iter().zip(repeat(path.clone()))).map(|(point, path)| async move {
        let mut arr: AsyncDirBacked<u8> = AsyncDirBacked::new(path).await.unwrap();
        arr.append(point.as_ref()).await.unwrap();
        arr
    });
    let mut gen = gen.buffer_unordered(data.len());

    let combined = gen.next().await.unwrap();
    gen.fold(combined, |mut combined, next| async move {
        combined.append_array(next).await.unwrap();
        combined
    })
    .await
}

#[cfg(feature = "async")]
async fn create_zstdfiles_async(
    path: PathBuf,
    data: &[String],
    level: Option<i32>,
) -> AsyncZstdBacked<u8> {
    let mut arr: AsyncZstdBacked<u8> = AsyncZstdBacked::new(path, level).await.unwrap();
    for inner_data in data {
        arr.append(inner_data.as_ref()).await.unwrap();
    }
    arr
}

#[cfg(feature = "async-zstd")]
async fn create_zstdfiles_async_parallel(
    path: PathBuf,
    data: &[String],
    level: Option<i32>,
) -> AsyncZstdBacked<u8> {
    use std::iter::repeat;

    let gen = stream::iter(data.iter().zip(repeat(path.clone()))).map(|(point, path)| async move {
        let mut arr: AsyncZstdBacked<u8> = AsyncZstdBacked::new(path, level).await.unwrap();
        arr.append(point.as_ref()).await.unwrap();
        arr
    });
    let mut gen = gen.buffer_unordered(data.len());

    let combined = gen.next().await.unwrap();
    gen.fold(combined, |mut combined, next| async move {
        combined.append_array(next).await.unwrap();
        combined
    })
    .await
}

fn file_creation_bench(c: &mut Criterion) {
    let data = complete_works();

    let path = temp_dir().join("file_creation_bench");
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
        b.iter(|| create_zstdfiles(black_box(path.clone()), black_box(data), None))
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
        let rt = Runtime::new().unwrap();

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

        #[cfg(feature = "async-zstd")]
        {
            group.bench_function("async_create_zstdfiles", |b| {
                let _ = remove_dir_all(path.clone());
                create_dir(path.clone()).unwrap();
                b.to_async(&rt)
                    .iter(|| create_zstdfiles_async(black_box(path.clone()), black_box(data), None))
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
                b.to_async(&rt).iter(|| {
                    create_zstdfiles_async_parallel(black_box(path.clone()), black_box(data), None)
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
    }

    remove_dir_all(path.clone()).unwrap();
}

fn read_plainfiles(f: &mut File) -> usize {
    f.rewind().unwrap();
    let mut arr: DirectoryBackedArray<u8> = DirectoryBackedArray::load(f).unwrap();
    arr.item_iter(0)
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .len()
}

#[cfg(feature = "zstd")]
fn read_zstdfiles(f: &mut File) -> usize {
    f.rewind().unwrap();
    let mut arr: ZstdDirBackedArray<u8> = ZstdDirBackedArray::load(f).unwrap();
    arr.item_iter(0)
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .len()
}

#[cfg(feature = "async")]
async fn read_plainfiles_async(path: PathBuf) -> usize {
    let mut file = AsyncFile::open(path).await.unwrap();
    let mut arr: AsyncDirBacked<u8> = AsyncDirBacked::load(&mut file).await.unwrap();
    let mut collect = Vec::new();

    let mut stream = pin!(arr.item_stream(0));
    while let Some(entry) = stream.next().await {
        collect.push(entry.unwrap());
    }
    collect.len()
}

#[cfg(feature = "async-zstd")]
async fn read_zstdfiles_async(path: PathBuf) -> usize {
    let mut file = AsyncFile::open(path).await.unwrap();
    let mut arr: AsyncZstdBacked<u8> = AsyncZstdBacked::load(&mut file).await.unwrap();
    let mut collect = Vec::new();

    let mut stream = pin!(arr.item_stream(0));
    while let Some(entry) = stream.next().await {
        collect.push(entry.unwrap());
    }
    collect.len()
}

fn file_load_bench(c: &mut Criterion) {
    let data = complete_works();

    let path = temp_dir().join("file_load_bench");
    let _ = remove_dir_all(path.clone());
    create_dir(path.clone()).unwrap();

    let mut group = c.benchmark_group("file_load_benches");

    let mut file = File::options()
        .create(true)
        .read(true)
        .write(true)
        .truncate(true)
        .open(path.join("CONFIG"))
        .unwrap();

    let mut arr = create_plainfiles(path.clone(), data);
    arr.save_to_disk(file.try_clone().unwrap()).unwrap();
    sync_all_dir(path.clone());

    group.bench_function("load_plainfiles", |b| {
        b.iter(|| black_box(read_plainfiles(&mut file)))
    });

    #[cfg(feature = "zstd")]
    {
        let _ = remove_dir_all(path.clone());
        create_dir(path.clone()).unwrap();

        let mut file = File::options()
            .create(true)
            .read(true)
            .write(true)
            .truncate(true)
            .open(path.join("CONFIG"))
            .unwrap();

        let mut arr = create_zstdfiles(path.clone(), data, None);
        arr.save_to_disk(file.try_clone().unwrap()).unwrap();
        sync_all_dir(path.clone());

        group.bench_function("load_zstdfiles", |b| {
            b.iter(|| black_box(read_zstdfiles(black_box(&mut file))))
        });
    }

    #[cfg(feature = "async")]
    {
        let rt = Runtime::new().unwrap();

        let _ = remove_dir_all(path.clone());
        create_dir(path.clone()).unwrap();

        rt.block_on(async {
            let mut file = AsyncFile::options()
                .create(true)
                .write(true)
                .truncate(true)
                .open(path.join("CONFIG"))
                .await
                .unwrap();
            let mut arr = create_plainfiles_async(path.clone(), data).await;
            arr.save_to_disk(&mut file).await.unwrap();
        });
        sync_all_dir(path.clone());

        group.bench_function("async_read_plainfiles", |b| {
            b.to_async(&rt)
                .iter(|| black_box(read_plainfiles_async(black_box(path.join("CONFIG")))))
        });

        #[cfg(feature = "async-zstd")]
        {
            let _ = remove_dir_all(path.clone());
            create_dir(path.clone()).unwrap();

            rt.block_on(async {
                let mut file = AsyncFile::options()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(path.join("CONFIG"))
                    .await
                    .unwrap();
                let mut arr = create_zstdfiles_async(path.clone(), data, None).await;
                arr.save_to_disk(&mut file).await.unwrap();
            });
            sync_all_dir(path.clone());

            group.bench_function("async_read_zstdfiles", |b| {
                b.to_async(&rt)
                    .iter(|| black_box(read_zstdfiles_async(black_box(path.join("CONFIG")))))
            });
        }
    }

    remove_dir_all(path.clone()).unwrap();
}

#[cfg(feature = "zstd")]
fn zstd_setting_benches(c: &mut Criterion) {
    #[cfg(any(feature = "zstdmt", feature = "zstdmt-async"))]
    use backed_array::zstd::set_zstd_multithread;

    use criterion::BenchmarkId;

    let data = complete_works();

    let path = temp_dir().join("file_creation_bench");

    #[cfg(feature = "async-zstd")]
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("zstd_setting_benches");
    for zstd_level in 0..22 {
        #[cfg(any(feature = "zstdmt", feature = "zstdmt-async"))]
        set_zstd_multithread(0);

        let _ = create_dir(path.clone());

        group.bench_with_input(
            BenchmarkId::new("zstd_write", format!("Compression Level: {zstd_level}")),
            &zstd_level,
            |b, zstd_level| {
                let _ = remove_dir_all(path.clone());
                create_dir(path.clone()).unwrap();
                b.iter(|| {
                    create_zstdfiles(black_box(path.clone()), black_box(data), Some(*zstd_level))
                })
            },
        );

        sync_all_dir(path.clone());
        println!(
            "Zstdfiles size with compression {}: {}",
            zstd_level,
            format_size(get_size(path.clone()).unwrap(), BINARY)
        );
        writeln!(
            logfile().lock().unwrap(),
            "Zstdfiles size with compression {}: {}",
            zstd_level,
            format_size(get_size(path.clone()).unwrap(), BINARY)
        )
        .unwrap();

        let mut file = File::options()
            .create(true)
            .read(true)
            .write(true)
            .truncate(true)
            .open(path.join("CONFIG"))
            .unwrap();
        create_zstdfiles(black_box(path.clone()), black_box(data), Some(zstd_level))
            .save_to_disk(file.try_clone().unwrap())
            .unwrap();
        group.bench_with_input(
            BenchmarkId::new("zstd_read", format!("Compression Level: {zstd_level}")),
            &(),
            |b, _| b.iter(|| black_box(read_zstdfiles(&mut file))),
        );

        #[cfg(feature = "async-zstd")]
        {
            group.bench_with_input(
                BenchmarkId::new(
                    "zstd_write_async",
                    format!("Compression Level: {zstd_level}"),
                ),
                &zstd_level,
                |b, zstd_level| {
                    let _ = remove_dir_all(path.clone());
                    create_dir(path.clone()).unwrap();
                    b.to_async(&rt).iter(|| {
                        create_zstdfiles_async(
                            black_box(path.clone()),
                            black_box(data),
                            Some(*zstd_level),
                        )
                    })
                },
            );

            sync_all_dir(path.clone());
            println!(
                "Zstdfiles async size with compression {}: {}",
                zstd_level,
                format_size(get_size(path.clone()).unwrap(), BINARY)
            );
            writeln!(
                logfile().lock().unwrap(),
                "Zstdfiles async size with compression {}: {}",
                zstd_level,
                format_size(get_size(path.clone()).unwrap(), BINARY)
            )
            .unwrap();

            let _ = remove_dir_all(path.clone());
            create_dir(path.clone()).unwrap();

            rt.block_on(async {
                let mut file = AsyncFile::options()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(path.join("CONFIG"))
                    .await
                    .unwrap();
                let mut arr = create_zstdfiles_async(path.clone(), data, None).await;
                arr.save_to_disk(&mut file).await.unwrap();
            });
            sync_all_dir(path.clone());

            group.bench_with_input(
                BenchmarkId::new(
                    "zstd_read_async",
                    format!("Compression Level: {zstd_level}"),
                ),
                &(),
                |b, _| {
                    b.to_async(&rt)
                        .iter(|| black_box(read_zstdfiles_async(black_box(path.join("CONFIG")))))
                },
            );
        }

        #[cfg(any(feature = "zstdmt", feature = "async-zstdmt"))]
        for t_count in 0..5 {
            set_zstd_multithread(t_count);
            #[cfg(feature = "zstdmt")]
            group.bench_with_input(
                BenchmarkId::new(
                    "zstd_write_mt",
                    format!("Compression Level: {zstd_level}, Thread Count: {t_count}"),
                ),
                &zstd_level,
                |b, zstd_level| {
                    let _ = remove_dir_all(path.clone());
                    create_dir(path.clone()).unwrap();
                    b.iter(|| {
                        create_zstdfiles(
                            black_box(path.clone()),
                            black_box(data),
                            Some(*zstd_level),
                        )
                    })
                },
            );

            #[cfg(feature = "async-zstdmt")]
            group.bench_with_input(
                BenchmarkId::new(
                    "zstd_write_async_mt",
                    format!("Compression Level: {zstd_level}, Thread Count: {t_count}"),
                ),
                &zstd_level,
                |b, zstd_level| {
                    let _ = remove_dir_all(path.clone());
                    create_dir(path.clone()).unwrap();
                    b.to_async(&rt).iter(|| {
                        create_zstdfiles_async(
                            black_box(path.clone()),
                            black_box(data),
                            Some(*zstd_level),
                        )
                    })
                },
            );
        }
    }
    group.finish();

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
