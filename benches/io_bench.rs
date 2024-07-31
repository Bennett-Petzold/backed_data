#![cfg(all(feature = "bincode", feature = "directory"))]

use std::{cell::OnceCell, path::Path, time::Duration};

use backed_data::{directory::StdDirBackedArray, entry::formats::BincodeCoder};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use pprof::criterion::{Output, PProfProfiler};
use tokio::runtime;

#[cfg(feature = "zstd")]
use backed_data::directory::ZstdDirBackedArray;

#[cfg(feature = "async_zstd")]
use backed_data::directory::AsyncZstdDirBackedArray;

#[cfg(feature = "async_bincode")]
use backed_data::{directory::AsyncStdDirBackedArray, entry::formats::AsyncBincodeCoder};

#[cfg(feature = "async")]
use futures::{StreamExt, TryStreamExt};

mod io_bench_util;
use io_bench_util::*;

create_fn!(create_plainfiles, StdDirBackedArray<u8, BincodeCoder>,);
#[cfg(feature = "zstd")]
create_fn!(create_zstdfiles, ZstdDirBackedArray<'a, LEVEL, u8, BincodeCoder>, 'a, const LEVEL: u8,);

#[cfg(feature = "async_bincode")]
create_fn!(async create_plainfiles_async, AsyncStdDirBackedArray<u8, AsyncBincodeCoder>,);
#[cfg(feature = "async_bincode")]
create_fn!(async parallel create_plainfiles_async_parallel, AsyncStdDirBackedArray<u8, AsyncBincodeCoder>,);

#[cfg(all(feature = "async_zstd", feature = "async_bincode"))]
create_fn!(async create_zstdfiles_async, AsyncZstdDirBackedArray<LEVEL, u8, AsyncBincodeCoder>, const LEVEL: u8,);
create_fn!(async parallel create_zstdfiles_async_parallel, AsyncZstdDirBackedArray<LEVEL, u8, AsyncBincodeCoder>, const LEVEL: u8,);

read_dir!(read_plainfiles, StdDirBackedArray<u8, BincodeCoder>);
#[cfg(feature = "zstd")]
read_dir!(read_zstdfiles, ZstdDirBackedArray<0, u8, BincodeCoder>);

#[cfg(feature = "async_bincode")]
read_dir!(async read_plainfiles_async, AsyncStdDirBackedArray<u8, AsyncBincodeCoder>);
#[cfg(feature = "async_bincode")]
read_dir!(async concurrent read_plainfiles_async_con, AsyncStdDirBackedArray<u8, AsyncBincodeCoder>);
#[cfg(feature = "async_bincode")]
read_dir!(async parallel read_plainfiles_async_par, AsyncStdDirBackedArray<u8, AsyncBincodeCoder>);

#[cfg(all(feature = "async_zstd", feature = "async_bincode"))]
read_dir!(async read_zstdfiles_async, AsyncZstdDirBackedArray<0, u8, AsyncBincodeCoder>);
#[cfg(all(feature = "async_zstd", feature = "async_bincode"))]
read_dir!(async concurrent read_zstdfiles_async_con, AsyncZstdDirBackedArray<0, u8, AsyncBincodeCoder>);
#[cfg(all(feature = "async_zstd", feature = "async_bincode"))]
read_dir!(async parallel read_zstdfiles_async_par, AsyncZstdDirBackedArray<0, u8, AsyncBincodeCoder>);

fn file_creation_bench(c: &mut Criterion) {
    let data = complete_works();
    let mut path_cell = None;
    let mut group = c.benchmark_group("file_creation_benches");

    group.bench_function("create_plainfiles", |b| {
        let path = create_path(&mut path_cell);
        b.iter(|| create_plainfiles(black_box(&path), black_box(data)))
    });
    log_created_size(&mut path_cell, "Plainfiles");

    #[cfg(feature = "zstd")]
    group.bench_function("create_zstdfiles", |b| {
        let path = create_path(&mut path_cell);
        b.iter(|| create_zstdfiles::<0, _>(black_box(path), black_box(data)))
    });
    log_created_size(&mut path_cell, "Zstdfiles");

    #[cfg(feature = "async")]
    {
        let rt = runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        group.bench_function("async_create_plainfiles", |b| {
            let path = create_path(&mut path_cell);
            b.to_async(&rt)
                .iter(|| create_plainfiles_async(black_box(path), black_box(data)))
        });
        log_created_size(&mut path_cell, "Async plainfiles");

        group.bench_function("async_create_plainfiles_parallel", |b| {
            let path = create_path(&mut path_cell);
            b.to_async(&rt)
                .iter(|| create_plainfiles_async_parallel(black_box(path.clone()), black_box(data)))
        });
        log_created_size(&mut path_cell, "Async parallel plainfiles");

        #[cfg(feature = "async_zstd")]
        {
            group.bench_function("async_create_zstdfiles", |b| {
                let path = create_path(&mut path_cell);
                b.to_async(&rt)
                    .iter(|| create_zstdfiles_async::<0, _>(black_box(path), black_box(data)))
            });
            log_created_size(&mut path_cell, "Async zstdfiles");

            group.bench_function("async_create_zstdfiles_parallel", |b| {
                let path = create_path(&mut path_cell);
                b.to_async(&rt).iter(|| async {
                    create_zstdfiles_async_parallel::<0, _>(
                        black_box(path.clone()),
                        black_box(data),
                    )
                    .await
                })
            });
            log_created_size(&mut path_cell, "Async zstdfiles parallel");
        }

        rt.shutdown_background();
    }
}

fn file_load_bench(c: &mut Criterion) {
    let mut path_cell = OnceCell::new();
    let mut group = c.benchmark_group("file_load_benches");

    group.bench_function("load_plainfiles", |b| {
        let path = create_files(&mut path_cell, create_plainfiles);
        b.iter(|| black_box(read_plainfiles(path)))
    });

    #[cfg(feature = "zstd")]
    {
        group.bench_function("load_zstdfiles", |b| {
            let path = create_files(&mut path_cell, create_zstdfiles::<0, _>);
            b.iter(|| black_box(read_zstdfiles(path)))
        });
    }

    #[cfg(feature = "async_bincode")]
    {
        let rt = runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        group.bench_function("async_load_plainfiles", |b| {
            let path = create_files(&mut path_cell, |path, data| {
                rt.block_on(create_plainfiles_async(path, data))
            });
            b.to_async(&rt)
                .iter(|| black_box(read_plainfiles_async(path)))
        });

        #[cfg(feature = "async_zstd")]
        {
            group.bench_function("async_load_zstdfiles", |b| {
                let path = create_files(&mut path_cell, |path, data| {
                    rt.block_on(create_zstdfiles_async::<0, _>(path, data))
                });
                b.to_async(&rt)
                    .iter(|| black_box(read_zstdfiles_async(path)))
            });
        }

        rt.shutdown_background()
    }
}

#[cfg(feature = "zstd")]
fn zstd_setting_benches(c: &mut Criterion) {
    #[cfg(any(feature = "zstdmt", feature = "async_zstdmt"))]
    use backed_data::entry::disks::ZSTD_MULTITHREAD;

    use criterion::BenchmarkId;

    let data = complete_works();
    let mut path_opt = None;
    let mut path_cell = OnceCell::new();
    let mut group = c.benchmark_group("zstd_setting_benches");

    #[cfg(feature = "async_zstd")]
    let rt = runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    seq_macro::seq!(ZSTD_LEVEL in 0..22 {
        #[cfg(any(feature = "zstdmt", feature = "async_zstdmt"))]
        {
            *ZSTD_MULTITHREAD.lock().unwrap() = 0;
        }

        group.bench_with_input(
            BenchmarkId::new("zstd_write", format!("Compression Level: {}", ZSTD_LEVEL)),
            &ZSTD_LEVEL,
            |b, _| {
                let path = create_path(&mut path_opt);
                b.iter(|| create_zstdfiles::<ZSTD_LEVEL, _>(black_box(path), black_box(data)))
            },
        );
            log_created_size(&mut path_opt, format!("Zstdfiles async (compression {})", ZSTD_LEVEL));

        group.bench_with_input(
            BenchmarkId::new("zstd_read", format!("Compression Level: {}", ZSTD_LEVEL)),
            &(),
            |b, _| {
                let path = create_files(&mut path_cell, create_zstdfiles::<ZSTD_LEVEL, _>);
                b.iter(|| black_box(read_zstdfiles(path)))
            },
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
                let path = create_path(&mut path_opt);
                    b.to_async(&rt).iter(|| {
                        create_zstdfiles_async::<ZSTD_LEVEL, _>(
                            black_box(path),
                            black_box(data),
                        )
                    })
                },
            );
            log_created_size(&mut path_opt, format!("Zstdfiles async (compression {})", ZSTD_LEVEL));

            group.bench_with_input(
                BenchmarkId::new(
                    "zstd_read_async",
                    format!("Compression Level: {}", ZSTD_LEVEL),
                ),
                &(),
                |b, _| {
                let path = create_files(&mut path_cell, |path, data| rt.block_on(create_zstdfiles_async::<ZSTD_LEVEL, _>(path, data)));
                    b.to_async(&rt)
                        .iter(|| black_box(read_zstdfiles_async(path)))
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
                let path = create_path(&mut path_opt);
                    b.iter(|| {
                        create_zstdfiles::<ZSTD_LEVEL, _>(
                            black_box(path),
                            black_box(data),
                        )
                    })
                },
            );
            log_created_size(&mut path_opt, format!("Zstdfiles MT (compression: {}, threads: {t_count})", ZSTD_LEVEL));

            #[cfg(feature = "async-zstdmt")]
            group.bench_with_input(
                BenchmarkId::new(
                    "zstd_write_async_mt",
                    format!("Compression Level: {}, Thread Count: {t_count}", ZSTD_LEVEL),
                ),
                &ZSTD_LEVEL,
                |b, _| {
                let path = create_files(&mut path_opt, |path, data| rt.block_on(create_zstdfiles_async::<ZSTD_LEVEL, _>(path, data)));
                    b.to_async(&rt).iter(|| {
                        create_zstdfiles_async::<ZSTD_LEVEL, _>(
                            black_box(path),
                            black_box(data),
                        )
                    })
                },
            );
            log_created_size(&mut path_opt, format!("Zstdfiles async MT (compression: {}, threads: {t_count})", ZSTD_LEVEL));
        }
    });
    group.finish();

    #[cfg(feature = "async_zstd")]
    rt.shutdown_background();
}

criterion_group! {
    name = io_benches;
    config =
        Criterion::default()
            .sample_size(10)
            .measurement_time(Duration::from_secs(10))
            .with_profiler(
                PProfProfiler::new(
                    100,
                    Output::Flamegraph(None)
                )
            );
    targets = file_creation_bench,
    file_load_bench,
    zstd_setting_benches
}
criterion_main!(io_benches);
