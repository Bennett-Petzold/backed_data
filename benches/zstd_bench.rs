#![cfg(all(feature = "bincode", feature = "directory", feature = "zstd"))]

use std::{path::Path, time::Duration};

use backed_data::directory::ZstdDirBackedArray;
use backed_data::entry::formats::BincodeCoder;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use pprof::criterion::{Output, PProfProfiler};

#[cfg(feature = "async_zstd")]
use backed_data::directory::AsyncZstdDirBackedArray;

#[cfg(feature = "async_bincode")]
use backed_data::entry::formats::AsyncBincodeCoder;

#[cfg(feature = "async")]
use {
    futures::{StreamExt, TryStreamExt},
    tokio::runtime,
};

#[allow(dead_code)]
mod io_bench_util;
use io_bench_util::*;

create_fn!(create_zstdfiles, ZstdDirBackedArray<'a, LEVEL, u8, BincodeCoder>, 'a, const LEVEL: u8,);

#[cfg(all(feature = "async_zstd", feature = "async_bincode"))]
create_fn!(async create_zstdfiles_async, AsyncZstdDirBackedArray<LEVEL, u8, AsyncBincodeCoder>, const LEVEL: u8,);

read_dir!(read_zstdfiles, ZstdDirBackedArray<0, u8, BincodeCoder>);

#[cfg(all(feature = "async_zstd", feature = "async_bincode"))]
read_dir!(async read_zstdfiles_async, AsyncZstdDirBackedArray<0, u8, AsyncBincodeCoder>);

fn zstd_setting_benches(c: &mut Criterion) {
    #[cfg(any(feature = "zstdmt", feature = "async_zstdmt"))]
    use backed_data::entry::disks::ZSTD_MULTITHREAD;

    use criterion::BenchmarkId;

    let data = complete_works();
    let mut path_opt = None;
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
                b.iter_batched(
                    || create_path(&mut path_opt).clone(),
                    |path| create_zstdfiles::<ZSTD_LEVEL, _>(black_box(path.clone()), black_box(data)),
                    criterion::BatchSize::SmallInput,
                )
            },
        );
            log_created_size(&mut path_opt, format!("Zstdfiles (compression {})", ZSTD_LEVEL));

                let path = create_files(create_zstdfiles::<ZSTD_LEVEL, _>);
        group.bench_with_input(
            BenchmarkId::new("zstd_read", format!("Compression Level: {}", ZSTD_LEVEL)),
            &(),
            |b, _| {
                b.iter(|| black_box(read_zstdfiles(&path)))
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
                    b.to_async(&rt).iter_batched(
                        || create_path(&mut path_opt).clone(),
                        |path| create_zstdfiles_async::<ZSTD_LEVEL, _>(black_box(path.clone()), black_box(data)),
                        criterion::BatchSize::SmallInput,
                    )
                }
            );
            log_created_size(&mut path_opt, format!("Zstdfiles async (compression {})", ZSTD_LEVEL));

                let path = create_files(|path, data| rt.block_on(create_zstdfiles_async::<ZSTD_LEVEL, _>(path, data)));
            group.bench_with_input(
                BenchmarkId::new(
                    "zstd_read_async",
                    format!("Compression Level: {}", ZSTD_LEVEL),
                ),
                &(),
                |b, _| {
                    b.to_async(&rt)
                        .iter(|| black_box(read_zstdfiles_async(&path)))
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
                    b.iter_batched(
                        || create_path(&mut path_opt).clone(),
                        |path| create_zstdfiles::<ZSTD_LEVEL, _>(black_box(path.clone()), black_box(data)),
                        criterion::BatchSize::SmallInput,
                    )
                },
            );
            log_created_size(&mut path_opt, format!("Zstdfiles MT (compression: {}, threads: {t_count})", ZSTD_LEVEL));

            #[cfg(feature = "async-zstdmt")] {
            group.bench_with_input(
                BenchmarkId::new(
                    "zstd_write_async_mt",
                    format!("Compression Level: {}, Thread Count: {t_count}", ZSTD_LEVEL),
                ),
                &ZSTD_LEVEL,
                |b, _| {
                    b.to_async(&rt).iter_batched(
                        || create_path(&mut path_opt).clone(),
                        |path| create_zstdfiles_async::<ZSTD_LEVEL, _>(black_box(path.clone()), black_box(data)),
                        criterion::BatchSize::SmallInput,
                    )
                }
            );
            log_created_size(&mut path_opt, format!("Zstdfiles async MT (compression: {}, threads: {t_count})", ZSTD_LEVEL));
                }
        }
    });
    group.finish();

    #[cfg(feature = "async_zstd")]
    rt.shutdown_background();
}

criterion_group! {
    name = zstd_benches;
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
    targets = zstd_setting_benches,
}
criterion_main!(zstd_benches);
