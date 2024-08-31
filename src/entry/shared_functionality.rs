use serde::{Deserialize, Serialize};

use crate::utils::Once;

use super::BackedEntry;

/// Internal implementor for shared [`BackedEntry`] behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackedEntryInner<T: Once, Disk, Coder> {
    #[serde(skip)]
    pub value: T,
    pub disk: Disk,
    pub coder: Coder,
}

impl<T: Once, Disk, Coder> BackedEntry for BackedEntryInner<T, Disk, Coder> {
    type OnceWrapper = T;
    type Disk = Disk;
    type Coder = Coder;

    fn get_storage(&self) -> (&Self::Disk, &Self::Coder) {
        (&self.disk, &self.coder)
    }

    fn get_storage_mut(&mut self) -> (&mut Self::Disk, &mut Self::Coder) {
        (&mut self.disk, &mut self.coder)
    }

    fn is_loaded(&self) -> bool {
        self.value.get().is_some()
    }

    fn unload(&mut self) {
        self.value = T::new();
    }

    fn take(self) -> Option<<Self::OnceWrapper as Once>::Inner> {
        self.value.into_inner()
    }

    fn new(disk: Self::Disk, coder: Self::Coder) -> Self
    where
        Self: Sized,
    {
        Self {
            value: T::new(),
            disk,
            coder,
        }
    }
}

impl<T: Once, Disk, Coder> AsRef<BackedEntryInner<T, Disk, Coder>>
    for BackedEntryInner<T, Disk, Coder>
{
    fn as_ref(&self) -> &BackedEntryInner<T, Disk, Coder> {
        self
    }
}

impl<T: Once, Disk, Coder> AsMut<BackedEntryInner<T, Disk, Coder>>
    for BackedEntryInner<T, Disk, Coder>
{
    fn as_mut(&mut self) -> &mut BackedEntryInner<T, Disk, Coder> {
        self
    }
}

/// Wrapper to minimize bounds specification for [`BackedEntryInner`].
///
/// [`BackedEntryInner`] is always this.
/// Only [`BackedEntryInner`] is this.
pub trait BackedEntryInnerTrait {
    type T: Once;
    type Disk;
    type Coder;

    fn get(self) -> BackedEntryInner<Self::T, Self::Disk, Self::Coder>;
    fn get_ref(&self) -> &BackedEntryInner<Self::T, Self::Disk, Self::Coder>;
    fn get_mut(&mut self) -> &mut BackedEntryInner<Self::T, Self::Disk, Self::Coder>;
}

impl<T: Once, Disk, Coder> BackedEntryInnerTrait for BackedEntryInner<T, Disk, Coder> {
    type T = T;
    type Disk = Disk;
    type Coder = Coder;

    fn get(self) -> BackedEntryInner<Self::T, Self::Disk, Self::Coder> {
        self
    }
    fn get_ref(&self) -> &BackedEntryInner<Self::T, Self::Disk, Self::Coder> {
        self
    }
    fn get_mut(&mut self) -> &mut BackedEntryInner<Self::T, Self::Disk, Self::Coder> {
        self
    }
}
