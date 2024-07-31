#[cfg(feature = "async")]
pub mod async_impl;
pub mod sync_impl;

use std::path::PathBuf;

use serde::de::Visitor;

struct PathBufVisitor;

impl<'de> Visitor<'de> for PathBufVisitor {
    type Value = PathBuf;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("A valid path to a file")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(PathBuf::from(v))
    }
}
