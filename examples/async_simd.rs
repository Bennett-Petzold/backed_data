#![cfg(all(feature = "simd_json", feature = "async"))]

fn main() {}

/*
use tokio::task::spawn_blocking;

#[tokio::main]
async fn main() {
    let backing_file = temp_dir().join("backed_array_async_simd");
    let data: HashMap<usize, usize> = (0..10_000).map(|x| (x, x * 2)).collect();

    /*
    coder
        .encode(&data, &mut disk.write_disk().unwrap())
        .await
        .unwrap();
    */

    /*
    let coder = SyncCoderAsAsync::<_, Mmap, _, _, _>::new(
        SimdJsonCoder::<HashMap<usize, usize>>::default(),
        |x: EncodeBg<SimdJsonCoder<HashMap<_, _>>, Mmap>| ready(x.call()),
    );

    coder.encode(&data, &mut disk).await.unwrap();
    */

    let _ = remove_dir_all(backing_file);
}
*/
