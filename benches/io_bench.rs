use std::{
    array,
    env::temp_dir,
    fs::{create_dir, remove_dir_all, File},
    io::Seek,
    path::PathBuf,
};

use backed_array::{
    directory::sync_impl::DirectoryBackedArray, meta::sync_impl::BackedArrayWrapper,
    zstd::sync_impl::ZstdDirBackedArray,
};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rand::{rngs::StdRng, thread_rng, Rng, SeedableRng};

const DATA_SIZE: usize = 1024 * 16;

fn create_plainfiles(path: PathBuf, data: &[[u8; DATA_SIZE]]) {
    let mut arr: DirectoryBackedArray<u8> = DirectoryBackedArray::new(path).unwrap();
    for inner_data in data {
        arr.append(inner_data).unwrap();
    }
}

#[cfg(feature = "zstd")]
fn create_zstdfiles(path: PathBuf, data: &[[u8; DATA_SIZE]]) {
    let mut arr: ZstdDirBackedArray<u8> = ZstdDirBackedArray::new(path, None).unwrap();
    for inner_data in data {
        arr.append(inner_data).unwrap();
    }
}

fn file_creation_bench(c: &mut Criterion) {
    let data: &mut [[u8; DATA_SIZE]] = &mut [[0; DATA_SIZE]; 8];
    let mut rng = StdRng::seed_from_u64(3448);
    data.iter_mut()
        .for_each(|subdata| rng.fill(&mut subdata[..]));

    let path = temp_dir().join("file_creation_bench");

    c.bench_function("create_plainfiles", |b| {
        let _ = remove_dir_all(path.clone());
        create_dir(path.clone()).unwrap();
        b.iter(|| create_plainfiles(black_box(path.clone()), black_box(data)))
    });

    #[cfg(feature = "zstd")]
    c.bench_function("create_zstdfiles", |b| {
        let _ = remove_dir_all(path.clone());
        create_dir(path.clone()).unwrap();
        b.iter(|| create_zstdfiles(black_box(path.clone()), black_box(data)))
    });

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
    let data: &mut [[u8; DATA_SIZE]] = &mut [[0; DATA_SIZE]; 8];
    let mut rng = StdRng::seed_from_u64(3448);
    data.iter_mut()
        .for_each(|subdata| rng.fill(&mut subdata[..]));

    let path = temp_dir().join("file_read_bench");
    let _ = remove_dir_all(path.clone());
    create_dir(path.clone()).unwrap();

    let mut arr: DirectoryBackedArray<u8> = DirectoryBackedArray::new(path.clone()).unwrap();
    for inner_data in &mut *data {
        arr.append(inner_data).unwrap();
    }
    let mut file = File::options()
        .create(true)
        .read(true)
        .write(true)
        .truncate(true)
        .open(path.join("CONFIG"))
        .unwrap();
    arr.save_to_disk(file.try_clone().unwrap()).unwrap();

    c.bench_function("read_plainfiles", |b| {
        b.iter(|| black_box(read_plainfiles(&mut file)))
    });

    let _ = remove_dir_all(path.clone());
    create_dir(path.clone()).unwrap();

    let mut arr: ZstdDirBackedArray<u8> = ZstdDirBackedArray::new(path.clone(), None).unwrap();
    for inner_data in &mut *data {
        arr.append(inner_data).unwrap();
    }
    let mut file = File::options()
        .create(true)
        .read(true)
        .write(true)
        .truncate(true)
        .open(path.join("CONFIG"))
        .unwrap();
    arr.save_to_disk(file.try_clone().unwrap()).unwrap();

    #[cfg(feature = "zstd")]
    c.bench_function("read_zstdfiles", |b| {
        b.iter(|| black_box(read_zstdfiles(black_box(&mut file))))
    });

    remove_dir_all(path.clone()).unwrap();
}

criterion_group!(benches, file_creation_bench, file_load_bench);
criterion_main!(benches);
