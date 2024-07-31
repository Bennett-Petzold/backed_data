#![cfg_attr(docsrs, feature(doc_auto_cfg))]

pub mod array;
pub mod directory;
pub mod entry;
pub mod meta;

#[cfg(any(feature = "zstd", feature = "async-zstd"))]
pub mod zstd;

mod test_utils;
