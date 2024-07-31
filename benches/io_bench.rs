use std::{
    array,
    env::temp_dir,
    fs::{create_dir, read_dir, read_to_string, remove_dir_all, File},
    io::{Seek, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use backed_array::{
    directory::sync_impl::DirectoryBackedArray, meta::sync_impl::BackedArrayWrapper,
    zstd::sync_impl::ZstdDirBackedArray,
};
use chrono::Local;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use fs_extra::dir::get_size;
use humansize::{format_size, BINARY};

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

fn file_creation_bench(c: &mut Criterion) {
    let data = complete_works();

    let path = temp_dir().join("file_creation_bench");

    let mut group = c.benchmark_group("file_creation_benches");
    group.bench_function("create_plainfiles", |b| {
        let _ = remove_dir_all(path.clone());
        create_dir(path.clone()).unwrap();
        b.iter(|| create_plainfiles(black_box(path.clone()), black_box(&data)))
    });

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
        b.iter(|| create_zstdfiles(black_box(path.clone()), black_box(&data), None))
    });

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

    remove_dir_all(path.clone()).unwrap();
}

fn read_plainfiles(f: &mut File) -> usize {
    f.rewind().unwrap();
    let mut arr: DirectoryBackedArray<u8> = DirectoryBackedArray::load(f).unwrap();
    arr.item_iter_default()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .len()
}

#[cfg(feature = "zstd")]
fn read_zstdfiles(f: &mut File) -> usize {
    f.rewind().unwrap();
    let mut arr: ZstdDirBackedArray<u8> = ZstdDirBackedArray::load(f).unwrap();
    arr.item_iter_default()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .len()
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

    let mut arr = create_plainfiles(path.clone(), &data);
    arr.save_to_disk(file.try_clone().unwrap()).unwrap();

    group.bench_function("load_plainfiles", |b| {
        b.iter(|| black_box(read_plainfiles(&mut file)))
    });

    let _ = remove_dir_all(path.clone());
    create_dir(path.clone()).unwrap();

    let mut file = File::options()
        .create(true)
        .read(true)
        .write(true)
        .truncate(true)
        .open(path.join("CONFIG"))
        .unwrap();

    let mut arr = create_zstdfiles(path.clone(), &data, None);
    arr.save_to_disk(file.try_clone().unwrap()).unwrap();

    #[cfg(feature = "zstd")]
    group.bench_function("load_zstdfiles", |b| {
        b.iter(|| black_box(read_zstdfiles(black_box(&mut file))))
    });

    remove_dir_all(path.clone()).unwrap();
}

#[cfg(feature = "zstd")]
fn zstd_setting_benches(c: &mut Criterion) {
    #[cfg(feature = "zstdmt")]
    use backed_array::zstd::set_zstd_multithread;

    use criterion::BenchmarkId;

    let data = complete_works();

    let path = temp_dir().join("file_creation_bench");

    let mut group = c.benchmark_group("zstd_setting_benches");
    for zstd_level in 0..22 {
        #[cfg(feature = "zstdmt")]
        set_zstd_multithread(0);

        group.bench_with_input(
            BenchmarkId::new("zstd_write", format!("Compression Level: {zstd_level}")),
            &zstd_level,
            |b, zstd_level| {
                let _ = remove_dir_all(path.clone());
                create_dir(path.clone()).unwrap();
                b.iter(|| {
                    create_zstdfiles(black_box(path.clone()), black_box(&data), Some(*zstd_level))
                })
            },
        );

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
        create_zstdfiles(black_box(path.clone()), black_box(&data), Some(zstd_level))
            .save_to_disk(file.try_clone().unwrap())
            .unwrap();
        group.bench_with_input(
            BenchmarkId::new("zstd_read", format!("Compression Level: {zstd_level}")),
            &(),
            |b, _| b.iter(|| black_box(read_zstdfiles(&mut file))),
        );

        #[cfg(feature = "zstdmt")]
        for t_count in 0..5 {
            set_zstd_multithread(t_count);
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
                            black_box(&data),
                            Some(*zstd_level),
                        )
                    })
                },
            );

            let mut file = File::options()
                .create(true)
                .read(true)
                .write(true)
                .truncate(true)
                .open(path.join("CONFIG"))
                .unwrap();
            create_zstdfiles(black_box(path.clone()), black_box(&data), Some(zstd_level))
                .save_to_disk(file.try_clone().unwrap())
                .unwrap();
            group.bench_with_input(
                BenchmarkId::new(
                    "zstd_read_mt",
                    format!("Compression Level: {zstd_level}, Thread Count: {t_count}"),
                ),
                &(),
                |b, _| b.iter(|| black_box(read_zstdfiles(&mut file))),
            );
        }
    }
    group.finish();

    remove_dir_all(path.clone()).unwrap();
}

criterion_group! {
    name = io_benches;
    config = Criterion::default().sample_size(10);
    targets = file_creation_bench,
    file_load_bench,
    zstd_setting_benches
}
criterion_main!(io_benches);
