#![cfg(feature = "unsafe_array")]

use std::{
    env::temp_dir,
    fmt::Debug,
    fs::{create_dir, read_dir, read_to_string, remove_dir_all, File},
    io::Write,
    ops::{Deref, Range},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use backed_data::{
    array::container::{BackedEntryContainerNestedWrite, ResizingContainer},
    directory::DirectoryBackedArray,
    entry::formats::BincodeCoder,
};
use chrono::Local;
use fs_extra::dir::get_size;
use humansize::{format_size, BINARY};
use serde::{Deserialize, Serialize};

#[cfg(feature = "async")]
use backed_data::{
    array::async_impl::BackedEntryContainerNestedAsyncWrite, entry::disks::AsyncWriteDisk,
};

pub fn logfile() -> &'static Mutex<File> {
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

pub fn complete_works() -> &'static Vec<String> {
    static LOGFILE: OnceLock<Vec<String>> = OnceLock::new();
    LOGFILE.get_or_init(|| {
        read_dir("shakespeare-dataset/text")
            .unwrap()
            .map(|text| read_to_string(text.unwrap().path()).unwrap())
            .collect::<Vec<String>>()
    })
}

pub fn sync_all_dir<P: AsRef<Path>>(path: P) {
    read_dir(path).unwrap().for_each(|file_path| {
        File::open(file_path.unwrap().path())
            .unwrap()
            .sync_all()
            .unwrap()
    })
}

// ---------- START DIR CREATION ---------- //

pub fn create<K, E, P: AsRef<Path>>(path: P, data: &[String]) -> DirectoryBackedArray<K, E>
where
    K: Default + Serialize + for<'de> Deserialize<'de>,
    E: Default + Serialize + for<'de> Deserialize<'de>,
    K: ResizingContainer<Data = Range<usize>>,
    E: BackedEntryContainerNestedWrite + ResizingContainer,
    E::Coder: Default,
    E::Disk: From<PathBuf>,
    E::WriteError: From<std::io::Error> + Debug,
    E::Unwrapped: for<'a> From<&'a [u8]>,
{
    let mut arr = DirectoryBackedArray::<K, E>::new(path.as_ref().to_path_buf()).unwrap();
    for inner_data in data {
        arr.append(inner_data.as_ref()).unwrap();
    }
    if arr.save(&BincodeCoder::default()).is_err() {
        panic!()
    };
    arr
}

pub fn create_parallel<K, E, P: AsRef<Path>>(
    path: P,
    data: &'static [String],
) -> DirectoryBackedArray<K, E>
where
    K: Default + Serialize + for<'de> Deserialize<'de> + Send + 'static,
    E: Default + Serialize + for<'de> Deserialize<'de> + Send + 'static,
    K: ResizingContainer<Data = Range<usize>>,
    E: BackedEntryContainerNestedWrite + ResizingContainer,
    E::Coder: Default,
    E::Disk: From<PathBuf> + AsRef<Path>,
    E::WriteError: From<std::io::Error> + Debug,
    E::Unwrapped: for<'a> From<&'a [u8]>,
{
    use std::sync::mpsc::sync_channel;

    let path = path.as_ref().to_path_buf();
    let (tx, rx) = sync_channel(data.len());

    for inner_data in data {
        let path = path.clone();
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut arr: DirectoryBackedArray<K, E> = DirectoryBackedArray::new(path).unwrap();
            arr.append(inner_data.as_ref()).unwrap();
            tx.send(arr)
        });
    }
    drop(tx);

    let mut arr = rx.recv().unwrap();
    while let Ok(next) = rx.recv() {
        arr.append_dir(next).unwrap();
    }

    if arr.save(&BincodeCoder::default()).is_err() {
        panic!()
    };
    arr
}

#[cfg(feature = "async")]
pub async fn a_create<K, E, P: AsRef<Path>>(path: P, data: &[String]) -> DirectoryBackedArray<K, E>
where
    K: Default + Serialize + Send + Sync,
    E: Default + Serialize + Send + Sync,
    K: ResizingContainer<Data = Range<usize>>,
    E: BackedEntryContainerNestedAsyncWrite + ResizingContainer,
    E::Coder: Default,
    E::Disk: From<PathBuf>,
    E::AsyncWriteError: From<std::io::Error> + Debug,
    E::Unwrapped: for<'u> From<&'u [u8]>,
{
    let mut arr = DirectoryBackedArray::<K, E>::new(path.as_ref().to_path_buf()).unwrap();
    for inner_data in data {
        arr.a_append(inner_data.as_ref()).await.unwrap();
    }
    if arr.a_save().await.is_err() {
        panic!()
    };
    arr
}

#[cfg(feature = "async")]
pub async fn a_create_concurrent<K, E, P>(path: P, data: &[String]) -> DirectoryBackedArray<K, E>
where
    P: AsRef<Path>,
    K: Default + Serialize + Send + Sync,
    E: Default + Serialize + Send + Sync,
    K: ResizingContainer<Data = Range<usize>>,
    E: BackedEntryContainerNestedAsyncWrite + ResizingContainer,
    E::Coder: Default,
    E::Disk: From<PathBuf> + AsRef<Path>,
    E::AsyncWriteError: From<std::io::Error> + Debug,
    E::Unwrapped: for<'a> From<&'a [u8]>,
{
    use futures::{stream, StreamExt};
    use std::iter::repeat;

    let mut handles = stream::iter(data.iter().zip(repeat(path.as_ref().to_path_buf())))
        .map(|(point, path)| async move {
            let mut arr: DirectoryBackedArray<K, E> = DirectoryBackedArray::new(path).unwrap();
            arr.a_append(point.as_ref()).await.unwrap();
            arr
        })
        .buffer_unordered(data.len());

    let mut combined = handles.next().await.unwrap();
    while let Some(next) = handles.next().await {
        combined.a_append_dir(next).await.unwrap();
    }

    if combined.a_save().await.is_err() {
        panic!()
    };
    combined
}

#[cfg(feature = "async")]
pub async fn a_create_parallel<K, E, P>(path: P, data: &[String]) -> DirectoryBackedArray<K, E>
where
    P: AsRef<Path> + Send + Sync + 'static,
    K: Default + Serialize + Send + Sync + 'static,
    E: Default + Serialize + Send + Sync + 'static,
    K: ResizingContainer<Data = Range<usize>>,
    E: BackedEntryContainerNestedAsyncWrite + ResizingContainer,
    E::Coder: Default + Send + Sync,
    E::Disk: From<PathBuf> + AsRef<Path> + Send + Sync,
    <E::Disk as AsyncWriteDisk>::WriteDisk: Send + Sync,
    E::AsyncWriteError: From<std::io::Error> + Debug,
    E::Unwrapped: for<'a> From<&'a [u8]>,
{
    use std::iter::repeat;

    let mut handles = data
        .iter()
        .cloned()
        .zip(repeat(path.as_ref().to_path_buf()))
        .map(|(point, path)| {
            tokio::spawn(async move {
                let mut arr: DirectoryBackedArray<K, E> = DirectoryBackedArray::new(path).unwrap();
                arr.a_append(point.as_ref()).await.unwrap();
                arr
            })
        })
        .collect::<Vec<_>>()
        .into_iter();

    let mut combined = handles.next().unwrap().await.unwrap();
    for next in handles {
        combined.a_append_dir(next.await.unwrap()).await.unwrap();
    }

    if combined.a_save().await.is_err() {
        panic!()
    };
    combined
}

#[macro_export]
macro_rules! create_fn {
    (async parallel $fn_name: ident, $output_type: ty, $($extra_generics: tt)*) => {
        async fn $fn_name<$($extra_generics)* P: AsRef<Path> + Clone + Send + Sync + 'static>(
            path: P,
            data: &[String]
        ) -> $output_type {
            a_create_parallel(path, data).await
        }
    };
    (async concurrent $fn_name: ident, $output_type: ty, $($extra_generics: tt)*) => {
        async fn $fn_name<$($extra_generics)* P: AsRef<Path> + Clone>(
            path: P,
            data: &[String]
        ) -> $output_type {
            a_create_concurrent(path, data).await
        }
    };
    (async $fn_name: ident, $output_type: ty, $($extra_generics: tt)*) => {
        async fn $fn_name<$($extra_generics)* P: AsRef<Path>>(
            path: P,
            data: &[String]
        ) -> $output_type {
            a_create(path, data).await
        }
    };
    (parallel $fn_name: ident, $output_type: ty, $( $extra_generics: tt )*) => {
        fn $fn_name<$($extra_generics)* P: AsRef<Path> + Send + 'static>(
            path: P,
            data: &'static [String]
        ) -> $output_type {
            create_parallel(path, data)
        }
    };
    ($fn_name: ident, $output_type: ty, $( $extra_generics: tt )*) => {
        fn $fn_name<$($extra_generics)* P: AsRef<Path>>(
            path: P,
            data: &[String]
        ) -> $output_type {
            create(path, data)
        }
    };
}

// ---------- END DIR CREATION ---------- //
//
// ---------- START TEMPDIR ---------- //

/// An anonymous directory in temp that deletes on drop.
#[derive(Debug, Clone)]
pub struct TempDir(Arc<PathBuf>);

impl Default for TempDir {
    fn default() -> Self {
        Self(Arc::new(temp_dir().join(uuid::Uuid::new_v4().to_string())))
    }
}

impl Deref for TempDir {
    type Target = PathBuf;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<Path> for TempDir {
    fn as_ref(&self) -> &Path {
        self.0.as_ref()
    }
}

impl AsRef<PathBuf> for TempDir {
    fn as_ref(&self) -> &PathBuf {
        &self.0
    }
}

impl From<TempDir> for PathBuf {
    fn from(val: TempDir) -> Self {
        (*val.0).clone()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if let Ok(dir) = Arc::try_unwrap(std::mem::take(&mut self.0)) {
            remove_dir_all(dir).unwrap()
        }
    }
}

pub fn create_path(path_opt: &mut Option<TempDir>) -> &TempDir {
    *path_opt = Some(TempDir::default());
    let path = path_opt.as_ref().unwrap();
    let _ = remove_dir_all(path);
    create_dir(path).unwrap();
    path
}

pub fn log_created_size<S: AsRef<str>>(path_cell: &mut Option<TempDir>, name: S) {
    if let Some(path) = path_cell.take() {
        sync_all_dir(path.clone());
        println!(
            "{} size: {}",
            name.as_ref(),
            format_size(get_size(path.clone()).unwrap(), BINARY)
        );
        writeln!(
            logfile().lock().unwrap(),
            "{} size: {}",
            name.as_ref(),
            format_size(get_size(path.clone()).unwrap(), BINARY)
        )
        .unwrap();
    }
}

pub fn create_files<F, K, E>(init: F) -> TempDir
where
    F: FnOnce(TempDir, &'static [String]) -> DirectoryBackedArray<K, E>,
    E: Serialize,
    K: Serialize,
{
    let path = TempDir::default();
    (init)(path.clone(), complete_works());
    path
}

// ---------- END TEMPDIR ---------- //
//
// ---------- START READ ---------- //

#[macro_export]
macro_rules! read_dir {
    (async parallel $fn_name: ident, $( $type: tt )+) => {
        async fn $fn_name<P: AsRef<Path>>(path: P) -> usize {
            use backed_data::{
                array::BackedArray,
                directory::DirectoryBackedArray
            };
            use futures::stream;
            use tokio::task::spawn;

            let arr: $($type)+ = DirectoryBackedArray::a_load(path).await.unwrap();
            let (arr, _) = arr.deconstruct();
            stream::iter(BackedArray::stream_send(&arr))
                .map(|x| async { spawn(x).await.unwrap() })
                // Lesser buffer, in the interest of completing the bench
                .buffered(arr.entries().len())
                .try_collect::<Vec<&u8>>()
                .await.unwrap().len()
        }
    };
    (async concurrent $fn_name: ident, $( $type: tt )+) => {
        async fn $fn_name<P: AsRef<Path>>(path: P) -> usize {
            use backed_data::directory::DirectoryBackedArray;
            use futures::stream;

            let arr: $($type)+ = DirectoryBackedArray::a_load(path).await.unwrap();
            // Lesser buffer, in the interest of completing the bench
            stream::iter(arr.stream()).buffered(arr.entries().len()).try_collect::<Vec<&u8>>().await.unwrap().len()
        }
    };
    (async $fn_name: ident, $( $type: tt )+) => {
        async fn $fn_name<P: AsRef<Path>>(path: P) -> usize {
            use backed_data::directory::DirectoryBackedArray;
            use futures::stream;

            let arr: $($type)+ = DirectoryBackedArray::a_load(path).await.unwrap();
            stream::iter(arr.stream()).then(|x| x).try_collect::<Vec<&u8>>().await.unwrap().len()
        }
    };
    (generic $fn_name: ident, $( $type: tt )+) => {
        fn $fn_name<P: AsRef<Path>>(path: P) -> usize {
            use backed_data::directory::DirectoryBackedArray;

            let arr: $($type)+ = DirectoryBackedArray::load(path, &BincodeCoder::default()).unwrap();
            arr.generic_iter().collect::<Result<Vec<_>, _>>().unwrap().len()
        }
    };
    ($fn_name: ident, $( $type: tt )+) => {
        fn $fn_name<P: AsRef<Path>>(path: P) -> usize {
            use backed_data::directory::DirectoryBackedArray;

            let arr: $($type)+ = DirectoryBackedArray::load(path, &BincodeCoder::default()).unwrap();
            arr.iter().collect::<Result<Vec<_>, _>>().unwrap().len()
        }
    };
}

// ---------- END READ ---------- //
