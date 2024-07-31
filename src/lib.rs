#![cfg_attr(docsrs, feature(doc_auto_cfg))]
// Don't run doc tests when doing external lto
// Cargo, rustdoc, and external lto do not all mix properly
#![cfg(not(all(any(feature = "zstd-thin-lto", feature = "zstd-fat-lto"), doctest)))]

pub mod array;
pub mod directory;
pub mod entry;
pub mod meta;

#[cfg(feature = "zstd")]
pub mod zstd;
