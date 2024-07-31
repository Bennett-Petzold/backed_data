#![cfg(all(feature = "bincode", feature = "directory", feature = "unsafe_array"))]

use std::{path::Path, time::Duration};

use backed_data::{
    directory::{DirectoryBackedArray, StdDirBackedArray},
    entry::{formats::BincodeCoder, BackedEntryArr},
};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use pprof::criterion::{Output, PProfProfiler};

#[cfg(feature = "zstd")]
use backed_data::directory::ZstdDirBackedArray;

#[cfg(feature = "csv")]
use backed_data::entry::formats::CsvCoder;

#[cfg(feature = "mmap")]
use backed_data::entry::disks::Mmap;

#[cfg(feature = "async")]
use backed_data::directory::AsyncStdDirBackedArray;

#[cfg(feature = "async_zstd")]
use backed_data::directory::AsyncZstdDirBackedArray;

#[cfg(feature = "async_bincode")]
use backed_data::entry::formats::AsyncBincodeCoder;

#[cfg(feature = "async_csv")]
use backed_data::entry::formats::AsyncCsvCoder;

#[cfg(feature = "async")]
use {
    futures::{StreamExt, TryStreamExt},
    tokio::runtime,
};

mod io_bench_util;
use io_bench_util::*;

create_fn!(create_plainfiles, StdDirBackedArray<u8, BincodeCoder<Box<[u8]>>>,);
create_fn!(parallel create_plainfiles_par, StdDirBackedArray<u8, BincodeCoder<Box<[u8]>>>,);

#[cfg(feature = "zstd")]
create_fn!(create_zstdfiles, ZstdDirBackedArray<'a, LEVEL, u8, BincodeCoder<Box<[u8]>>>, 'a, const LEVEL: u8,);
#[cfg(feature = "zstd")]
create_fn!(parallel create_zstdfiles_par, ZstdDirBackedArray<'a, LEVEL, u8, BincodeCoder<Box<[u8]>>>, 'a, const LEVEL: u8,);

#[cfg(feature = "csv")]
create_fn!(create_csv, StdDirBackedArray<u8, CsvCoder<Box<[u8]>, u8>>,);

#[cfg(feature = "csv")]
/// CSV reader expects a header row, but CSV writer only provides one if we're
/// using a struct instead of raw data.
fn create_csv_with_header<P: AsRef<Path>>(
    path: P,
    data: &[String],
) -> StdDirBackedArray<U8Wrapper, CsvCoder<Box<[U8Wrapper]>, U8Wrapper>> {
    let mut arr = StdDirBackedArray::new(path.as_ref().to_path_buf()).unwrap();
    for inner_data in data {
        let data: Vec<U8Wrapper> = <String as AsRef<[u8]>>::as_ref(inner_data)
            .iter()
            .map(|x| (*x).into())
            .collect();
        arr.append(data).unwrap();
    }
    if arr.save(&BincodeCoder::default()).is_err() {
        panic!()
    };
    arr
}

#[cfg(feature = "async_csv")]
/// CSV reader expects a header row, but CSV writer only provides one if we're
/// using a struct instead of raw data.
async fn a_create_csv_with_header<P: AsRef<Path>>(
    path: P,
    data: &[String],
) -> AsyncStdDirBackedArray<U8Wrapper, AsyncCsvCoder<Box<[U8Wrapper]>, U8Wrapper>> {
    let mut arr = AsyncStdDirBackedArray::new(path.as_ref().to_path_buf()).unwrap();
    for inner_data in data {
        let data: Vec<U8Wrapper> = <String as AsRef<[u8]>>::as_ref(inner_data)
            .iter()
            .map(|x| (*x).into())
            .collect();
        arr.a_append(data).await.unwrap();
    }
    if arr.a_save(&AsyncBincodeCoder::default()).await.is_err() {
        panic!()
    };
    arr
}

#[cfg(feature = "csv")]
create_fn!(parallel create_csv_par, StdDirBackedArray<u8, CsvCoder<Box<[u8]>, u8>>,);

#[cfg(feature = "mmap")]
create_fn!(
    create_mmap,
    DirectoryBackedArray<Vec<usize>, Vec<BackedEntryArr<u8, Mmap, BincodeCoder<Box<[u8]>>>>>,
);
#[cfg(feature = "mmap")]
create_fn!(
    parallel create_mmap_par,
    DirectoryBackedArray<Vec<usize>, Vec<BackedEntryArr<u8, Mmap, BincodeCoder<Box<[u8]>>>>>,
);

#[cfg(feature = "async_bincode")]
create_fn!(async create_plainfiles_async, AsyncStdDirBackedArray<u8, AsyncBincodeCoder<Box<[u8]>>>,);
#[cfg(feature = "async_bincode")]
create_fn!(async concurrent create_plainfiles_async_con, AsyncStdDirBackedArray<u8, AsyncBincodeCoder<Box<[u8]>>>,);
#[cfg(feature = "async_bincode")]
create_fn!(async parallel create_plainfiles_async_par, AsyncStdDirBackedArray<u8, AsyncBincodeCoder<Box<[u8]>>>,);

#[cfg(all(feature = "async_zstd", feature = "async_bincode"))]
create_fn!(async create_zstdfiles_async, AsyncZstdDirBackedArray<LEVEL, u8, AsyncBincodeCoder<Box<[u8]>>>, const LEVEL: u8,);
#[cfg(all(feature = "async_zstd", feature = "async_bincode"))]
create_fn!(async concurrent create_zstdfiles_async_con, AsyncZstdDirBackedArray<LEVEL, u8, AsyncBincodeCoder<Box<[u8]>>>, const LEVEL: u8,);
#[cfg(all(feature = "async_zstd", feature = "async_bincode"))]
create_fn!(async parallel create_zstdfiles_async_par, AsyncZstdDirBackedArray<LEVEL, u8, AsyncBincodeCoder<Box<[u8]>>>, const LEVEL: u8,);

#[cfg(feature = "async_csv")]
create_fn!(async create_csv_async, AsyncStdDirBackedArray<u8, AsyncCsvCoder<Box<[u8]>, u8>>,);

read_dir!(read_plainfiles, StdDirBackedArray<u8, BincodeCoder<_>>);
read_dir!(generic read_plainfiles_generic, StdDirBackedArray<u8, BincodeCoder<_>>);

#[cfg(feature = "zstd")]
read_dir!(read_zstdfiles, ZstdDirBackedArray<0, u8, BincodeCoder<_>>);
#[cfg(feature = "csv")]
read_dir!(read_csv, StdDirBackedArray<U8Wrapper, CsvCoder<_, _>>);
#[cfg(feature = "mmap")]
read_dir!(
    read_mmap,
    DirectoryBackedArray<Vec<usize>, Vec<BackedEntryArr<u8, Mmap, BincodeCoder<Box<[u8]>>>>>
);

#[cfg(feature = "async_bincode")]
read_dir!(async read_plainfiles_async, AsyncStdDirBackedArray<u8, AsyncBincodeCoder<_>>);
#[cfg(feature = "async_bincode")]
read_dir!(async concurrent read_plainfiles_async_con, AsyncStdDirBackedArray<u8, AsyncBincodeCoder<_>>);
#[cfg(feature = "async_bincode")]
read_dir!(async parallel read_plainfiles_async_par, AsyncStdDirBackedArray<u8, AsyncBincodeCoder<_>>);

#[cfg(all(feature = "async_zstd", feature = "async_bincode"))]
read_dir!(async read_zstdfiles_async, AsyncZstdDirBackedArray<0, u8, AsyncBincodeCoder<_>>);
#[cfg(all(feature = "async_zstd", feature = "async_bincode"))]
read_dir!(async concurrent read_zstdfiles_async_con, AsyncZstdDirBackedArray<0, u8, AsyncBincodeCoder<_>>);
#[cfg(all(feature = "async_zstd", feature = "async_bincode"))]
read_dir!(async parallel read_zstdfiles_async_par, AsyncZstdDirBackedArray<0, u8, AsyncBincodeCoder<_>>);

#[cfg(all(feature = "async_csv", feature = "async_bincode"))]
read_dir!(async read_csv_async, AsyncStdDirBackedArray<u8, AsyncCsvCoder<_, _>>);

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

    #[cfg(feature = "csv")]
    group.bench_function("create_csv", |b| {
        b.iter_batched(
            || create_path(&mut path_cell).clone(),
            |path| create_csv(black_box(path.clone()), black_box(data)),
            criterion::BatchSize::SmallInput,
        )
    });
    log_created_size(&mut path_cell, "CSV");

    #[cfg(feature = "csv")]
    group.bench_function("create_csv_parallel", |b| {
        b.iter_batched(
            || create_path(&mut path_cell).clone(),
            |path| create_csv_par(black_box(path.clone()), black_box(data)),
            criterion::BatchSize::SmallInput,
        )
    });
    log_created_size(&mut path_cell, "Parallel CSV");

    #[cfg(feature = "mmap")]
    group.bench_function("create_mmap", |b| {
        b.iter_batched(
            || create_path(&mut path_cell).clone(),
            |path| create_mmap(black_box(path.clone()), black_box(data)),
            criterion::BatchSize::SmallInput,
        )
    });
    log_created_size(&mut path_cell, "mmap");

    #[cfg(feature = "mmap")]
    group.bench_function("create_mmap_parallel", |b| {
        b.iter_batched(
            || create_path(&mut path_cell).clone(),
            |path| create_mmap_par(black_box(path.clone()), black_box(data)),
            criterion::BatchSize::SmallInput,
        )
    });
    log_created_size(&mut path_cell, "Parallel mmap");

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

            group.bench_function("async_create_csv", |b| {
                b.to_async(&rt).iter_batched(
                    || create_path(&mut path_cell).clone(),
                    |path| create_csv_async(black_box(path.clone()), black_box(data)),
                    criterion::BatchSize::SmallInput,
                )
            });
            log_created_size(&mut path_cell, "Async csv");
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

    #[cfg(feature = "csv")]
    {
        {
            let path = create_files(create_csv_with_header);
            group.bench_function("load_csv", |b| b.iter(|| black_box(read_csv(&path))));
        }
    }

    #[cfg(feature = "mmap")]
    {
        {
            let path = create_files(create_mmap);
            group.bench_function("load_mmap", |b| b.iter(|| black_box(read_mmap(&path))));
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

        #[cfg(feature = "async_csv")]
        {
            let path = create_files(|path, data| rt.block_on(a_create_csv_with_header(path, data)));
            group.bench_function("async_load_csv", |b| {
                b.to_async(&rt).iter(|| black_box(read_csv_async(&path)))
            });
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
