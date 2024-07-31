#![cfg_attr(docsrs, feature(doc_auto_cfg))]

pub mod entry;
pub mod examples;
pub mod utils;

#[cfg(feature = "array")]
pub mod array;
#[cfg(feature = "directory")]
pub mod directory;

#[cfg(any(test, feature = "test"))]
pub mod test_utils;
