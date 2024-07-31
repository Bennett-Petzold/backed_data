#![cfg_attr(docsrs, feature(doc_auto_cfg))]

pub mod array;
pub mod directory;
pub mod entry;
pub mod meta;

#[cfg(feature = "zstd")]
pub mod zstd;
