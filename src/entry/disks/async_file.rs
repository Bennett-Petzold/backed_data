use std::path::PathBuf;

cfg_if::cfg_if! {
    if #[cfg(feature = "tokio")] {
        use tokio_util::compat::TokioAsyncReadCompatExt;
        use tokio_util::compat::TokioAsyncWriteCompatExt;
        pub type AsyncFile = tokio_util::compat::Compat<tokio::fs::File>;
    }
    else if #[cfg(feature = "smol")] {
        pub type AsyncFile = smol::fs::file;
    }
}

pub async fn read_file(path: PathBuf) -> std::io::Result<AsyncFile> {
    cfg_if::cfg_if! {
        if #[cfg(feature = "tokio")] {
            Ok(tokio::fs::File::open(path).await?.compat())
        } else if #[cfg(feature = "smol")] {
            smol::fs::File::open(path).await
        }
    }
}

pub async fn write_file(path: PathBuf) -> std::io::Result<AsyncFile> {
    cfg_if::cfg_if! {
        if #[cfg(feature = "tokio")] {
            Ok(tokio::fs::File::options()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)
                .await?
                .compat_write())
        } else if #[cfg(feature = "smol")] {
            smol::fs::File::options()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)
                .await
        }
    }
}
