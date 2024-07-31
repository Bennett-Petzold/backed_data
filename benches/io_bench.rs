#![cfg(all(feature = "bincode", feature = "directory", feature = "unsafe_array"))]

use std::{path::Path, time::Duration};

use backed_data::{directory::StdDirBackedArray, entry::formats::BincodeCoder};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use pprof::criterion::{Output, PProfProfiler};

#[cfg(feature = "zstd")]
use backed_data::directory::ZstdDirBackedArray;

#[cfg(feature = "async_zstd")]
use backed_data::directory::AsyncZstdDirBackedArray;

#[cfg(feature = "async_bincode")]
use backed_data::{directory::AsyncStdDirBackedArray, entry::formats::AsyncBincodeCoder};

#[cfg(feature = "async")]
use {
    futures::{StreamExt, TryStreamExt},
    tokio::runtime,
};

mod io_bench_util;
use io_bench_util::*;

create_fn!(create_plainfiles, StdDirBackedArray<u8, BincodeCoder>,);
create_fn!(parallel create_plainfiles_par, StdDirBackedArray<u8, BincodeCoder>,);
#[cfg(feature = "zstd")]
create_fn!(create_zstdfiles, ZstdDirBackedArray<'a, LEVEL, u8, BincodeCoder>, 'a, const LEVEL: u8,);
create_fn!(parallel create_zstdfiles_par, ZstdDirBackedArray<'a, LEVEL, u8, BincodeCoder>, 'a, const LEVEL: u8,);

#[cfg(feature = "async_bincode")]
create_fn!(async create_plainfiles_async, AsyncStdDirBackedArray<u8, AsyncBincodeCoder>,);
#[cfg(feature = "async_bincode")]
create_fn!(async concurrent create_plainfiles_async_con, AsyncStdDirBackedArray<u8, AsyncBincodeCoder>,);
#[cfg(feature = "async_bincode")]
create_fn!(async parallel create_plainfiles_async_par, AsyncStdDirBackedArray<u8, AsyncBincodeCoder>,);

#[cfg(all(feature = "async_zstd", feature = "async_bincode"))]
create_fn!(async create_zstdfiles_async, AsyncZstdDirBackedArray<LEVEL, u8, AsyncBincodeCoder>, const LEVEL: u8,);
#[cfg(all(feature = "async_zstd", feature = "async_bincode"))]
create_fn!(async concurrent create_zstdfiles_async_con, AsyncZstdDirBackedArray<LEVEL, u8, AsyncBincodeCoder>, const LEVEL: u8,);
#[cfg(all(feature = "async_zstd", feature = "async_bincode"))]
create_fn!(async parallel create_zstdfiles_async_par, AsyncZstdDirBackedArray<LEVEL, u8, AsyncBincodeCoder>, const LEVEL: u8,);

read_dir!(read_plainfiles, StdDirBackedArray<u8, BincodeCoder>);
read_dir!(generic read_plainfiles_generic, StdDirBackedArray<u8, BincodeCoder>);
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

fn file_creation_benches(c: &mut Criterion) {
    let data = complete_works();
    let mut path_cell = None;
    let mut group = c.benchmark_group("file_creation_benches");

    group.bench_function("create_plainfiles", |b| {
        b.iter_batched(
            || create_path(&mut path_cell).clone(),
            |path| create_plainfiles(black_box(path.clone()), black_box(data)),
            criterion::BatchSize::SmallInput,
        )
    });
    log_created_size(&mut path_cell, "Plainfiles");

    group.bench_function("create_plainfiles_parallel", |b| {
        b.iter_batched(
            || create_path(&mut path_cell).clone(),
            |path| create_plainfiles_par(black_box(path.clone()), black_box(data)),
            criterion::BatchSize::SmallInput,
        )
    });
    log_created_size(&mut path_cell, "Parallel plainfiles");

    #[cfg(feature = "zstd")]
    group.bench_function("create_zstdfiles", |b| {
        b.iter_batched(
            || create_path(&mut path_cell).clone(),
            |path| create_zstdfiles::<0, _>(black_box(path.clone()), black_box(data)),
            criterion::BatchSize::SmallInput,
        )
    });
    log_created_size(&mut path_cell, "Zstdfiles");

    #[cfg(feature = "zstd")]
    group.bench_function("create_zstdfiles_parallel", |b| {
        b.iter_batched(
            || create_path(&mut path_cell).clone(),
            |path| create_zstdfiles_par::<0, _>(black_box(path.clone()), black_box(data)),
            criterion::BatchSize::SmallInput,
        )
    });
    log_created_size(&mut path_cell, "Parallel zstdfiles");

    #[cfg(feature = "async")]
    {
        let rt = runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        group.bench_function("async_create_plainfiles", |b| {
            b.to_async(&rt).iter_batched(
                || create_path(&mut path_cell).clone(),
                |path| create_plainfiles_async(black_box(path.clone()), black_box(data)),
                criterion::BatchSize::SmallInput,
            )
        });
        log_created_size(&mut path_cell, "Async plainfiles");

        group.bench_function("async_create_plainfiles_concurrent", |b| {
            b.to_async(&rt).iter_batched(
                || create_path(&mut path_cell).clone(),
                |path| create_plainfiles_async_con(black_box(path.clone()), black_box(data)),
                criterion::BatchSize::SmallInput,
            )
        });
        log_created_size(&mut path_cell, "Async concurrent plainfiles");

        group.bench_function("async_create_plainfiles_parallel", |b| {
            b.to_async(&rt).iter_batched(
                || create_path(&mut path_cell).clone(),
                |path| create_plainfiles_async_par(black_box(path.clone()), black_box(data)),
                criterion::BatchSize::SmallInput,
            )
        });
        log_created_size(&mut path_cell, "Async parallel plainfiles");

        #[cfg(feature = "async_zstd")]
        {
            group.bench_function("async_create_zstdfiles", |b| {
                b.to_async(&rt).iter_batched(
                    || create_path(&mut path_cell).clone(),
                    |path| create_zstdfiles_async::<0, _>(black_box(path.clone()), black_box(data)),
                    criterion::BatchSize::SmallInput,
                )
            });
            log_created_size(&mut path_cell, "Async zstdfiles");

            group.bench_function("async_create_zstdfiles_concurrent", |b| {
                b.to_async(&rt).iter_batched(
                    || create_path(&mut path_cell).clone(),
                    |path| {
                        create_zstdfiles_async_con::<0, _>(black_box(path.clone()), black_box(data))
                    },
                    criterion::BatchSize::SmallInput,
                )
            });
            log_created_size(&mut path_cell, "Async zstdfiles concurrent");

            group.bench_function("async_create_zstdfiles_parallel", |b| {
                b.to_async(&rt).iter_batched(
                    || create_path(&mut path_cell).clone(),
                    |path| {
                        create_zstdfiles_async_par::<0, _>(black_box(path.clone()), black_box(data))
                    },
                    criterion::BatchSize::SmallInput,
                )
            });
            log_created_size(&mut path_cell, "Async zstdfiles parallel");
        }

        rt.shutdown_background();
    }
}

fn file_load_benches(c: &mut Criterion) {
    let mut group = c.benchmark_group("file_load_benches");

    {
        let path = create_files(create_plainfiles);
        group.bench_function("load_plainfiles", |b| {
            b.iter(|| black_box(read_plainfiles(&path)))
        });
    }

    {
        let path = create_files(create_plainfiles);
        group.bench_function("load_plainfiles_generic", |b| {
            b.iter(|| black_box(read_plainfiles_generic(&path)))
        });
    }

    #[cfg(feature = "zstd")]
    {
        {
            let path = create_files(create_zstdfiles::<0, _>);
            group.bench_function("load_zstdfiles", |b| {
                b.iter(|| black_box(read_zstdfiles(&path)))
            });
        }
    }

    #[cfg(feature = "async_bincode")]
    {
        let rt = runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        {
            let path = create_files(|path, data| rt.block_on(create_plainfiles_async(path, data)));
            group.bench_function("async_load_plainfiles", |b| {
                b.to_async(&rt)
                    .iter(|| black_box(read_plainfiles_async(&path)))
            });
        }

        {
            let path = create_files(|path, data| rt.block_on(create_plainfiles_async(path, data)));
            group.bench_function("async_load_plainfiles_concurrent", |b| {
                b.to_async(&rt)
                    .iter(|| black_box(read_plainfiles_async_con(&path)))
            });
        }

        {
            let path = create_files(|path, data| rt.block_on(create_plainfiles_async(path, data)));
            group.bench_function("async_load_plainfiles_parallel", |b| {
                b.to_async(&rt)
                    .iter(|| black_box(read_plainfiles_async_par(&path)))
            });
        }

        #[cfg(feature = "async_zstd")]
        {
            {
                let path = create_files(|path, data| {
                    rt.block_on(create_zstdfiles_async::<0, _>(path, data))
                });
                group.bench_function("async_load_zstdfiles", |b| {
                    b.to_async(&rt)
                        .iter(|| black_box(read_zstdfiles_async(&path)))
                });
            }

            {
                let path = create_files(|path, data| {
                    rt.block_on(create_zstdfiles_async::<0, _>(path, data))
                });
                group.bench_function("async_load_zstdfiles_concurrent", |b| {
                    b.to_async(&rt)
                        .iter(|| black_box(read_zstdfiles_async_con(&path)))
                });
            }

            {
                let path = create_files(|path, data| {
                    rt.block_on(create_zstdfiles_async::<0, _>(path, data))
                });
                group.bench_function("async_load_zstdfiles_parallel", |b| {
                    b.to_async(&rt)
                        .iter(|| black_box(read_zstdfiles_async_par(&path)))
                });
            }
        }

        rt.shutdown_background()
    }
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
    targets = file_creation_benches,
    file_load_benches,
}
criterion_main!(io_benches);
