/*
#[cfg(test)]
mod tests {
    use std::env::temp_dir;

    use itertools::Itertools;
    use tokio::fs::{remove_dir_all, File};

    use super::*;

    fn values() -> (Vec<String>, Vec<String>) {
        (
            ["TEST STRING".to_string()]
                .into_iter()
                .cycle()
                .take(10_000)
                .collect_vec(),
            ["OTHER VALUE".to_string()]
                .into_iter()
                .cycle()
                .take(10_000)
                .collect_vec(),
        )
    }

    #[tokio::test]
    async fn write() {
        let directory = temp_dir().join("directory_write_async");
        let _ = remove_dir_all(directory.clone()).await;
        let mut arr = DirectoryBackedArray::new(directory.clone()).await.unwrap();
        let (values, second_values) = values();

        arr.append_memory(values).await.unwrap();
        arr.append(second_values).await.unwrap();
        assert_eq!(arr.get(100).await.unwrap(), &"TEST STRING");
        assert_eq!(arr.get(15_000).await.unwrap(), &"OTHER VALUE");

        remove_dir_all(directory).await.unwrap();
    }

    #[tokio::test]
    async fn write_and_read() {
        let directory = temp_dir().join("directory_write_and_read_async");
        let _ = remove_dir_all(directory.clone()).await;
        let mut arr = DirectoryBackedArray::new(directory.clone()).await.unwrap();
        let (values, second_values) = values();

        arr.append(values).await.unwrap();
        arr.append_memory(second_values).await.unwrap();
        arr.save_to_disk(&mut File::create(directory.join("directory")).await.unwrap())
            .await
            .unwrap();
        drop(arr);

        let mut arr: DirectoryBackedArray<String> =
            DirectoryBackedArray::load(&mut File::open(directory.join("directory")).await.unwrap())
                .await
                .unwrap();
        assert_eq!(arr.get(100).await.unwrap(), &"TEST STRING");
        assert_eq!(arr.get(15_000).await.unwrap(), &"OTHER VALUE");
        assert_eq!(arr.get(200).await.unwrap(), &"TEST STRING");
        assert_eq!(arr.get(1).await.unwrap(), &"TEST STRING");

        remove_dir_all(directory).await.unwrap();
    }

    #[tokio::test]
    async fn cross_write_and_read() {
        let directory = temp_dir().join("directory_write_and_read_async_cross");
        let _ = remove_dir_all(directory.clone()).await;
        let mut arr = DirectoryBackedArray::new(directory.clone()).await.unwrap();
        let (values, second_values) = values();

        arr.append(values).await.unwrap();
        arr.append_memory(second_values).await.unwrap();
        arr.save_to_disk(&mut File::create(directory.join("directory")).await.unwrap())
            .await
            .unwrap();
        drop(arr);

        let mut arr: DirectoryBackedArray<String> =
            DirectoryBackedArray::load(&mut File::open(directory.join("directory")).await.unwrap())
                .await
                .unwrap();
        assert_eq!(arr.get(100).await.unwrap(), &"TEST STRING");
        assert_eq!(arr.get(15_000).await.unwrap(), &"OTHER VALUE");
        assert_eq!(arr.get(200).await.unwrap(), &"TEST STRING");
        assert_eq!(arr.get(1).await.unwrap(), &"TEST STRING");

        remove_dir_all(directory).await.unwrap();
    }
}
*/
