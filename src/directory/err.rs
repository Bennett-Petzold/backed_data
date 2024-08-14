use std::{error::Error, fmt::Display};

use error_stack::{Context, Report};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DiskCreateErr<E: Context + Error> {
    IoErr(std::io::Error),
    DiskErr(E),
}

impl<E: Context + Error> Display for DiskCreateErr<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoErr(x) => x.fmt(f),
            Self::DiskErr(x) => Display::fmt(x, f),
        }
    }
}

impl<E> DiskCreateErr<E>
where
    E: Context + Error,
{
    pub fn disk_err(err: E) -> Report<Self> {
        Report::new(DiskCreateErr::DiskErr(err))
    }

    pub fn io_err(err: std::io::Error) -> Report<Self> {
        Report::new(DiskCreateErr::IoErr(err))
    }
}

#[derive(Debug, Error)]
pub enum DiskWriteErr<A: Context + Error, E: Context + Error> {
    WriteErr(A),
    IoErr(std::io::Error),
    DiskErr(E),
}

impl<A: Context + Error, E: Context + Error> Display for DiskWriteErr<A, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WriteErr(x) => Display::fmt(x, f),
            Self::IoErr(x) => x.fmt(f),
            Self::DiskErr(x) => Display::fmt(x, f),
        }
    }
}

impl<A, E> DiskWriteErr<A, E>
where
    A: Context + Error,
    E: Context + Error,
{
    pub fn write_err(err: A) -> Report<Self> {
        Report::new(Self::WriteErr(err))
    }

    pub fn disk_err(err: E) -> Report<Self> {
        Report::new(Self::DiskErr(err))
    }

    pub fn io_err(err: std::io::Error) -> Report<Self> {
        Report::new(Self::IoErr(err))
    }
}

#[derive(Debug, Error)]
pub enum DiskReadErr<A: Context + Error, E: Context + Error> {
    ReadErr(A),
    IoErr(std::io::Error),
    DiskErr(E),
}

impl<A: Context + Error, E: Context + Error> Display for DiskReadErr<A, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadErr(x) => Display::fmt(x, f),
            Self::IoErr(x) => x.fmt(f),
            Self::DiskErr(x) => Display::fmt(x, f),
        }
    }
}

impl<A, E> DiskReadErr<A, E>
where
    A: Context + Error,
    E: Context + Error,
{
    pub fn read_err(err: A) -> Report<Self> {
        Report::new(Self::ReadErr(err))
    }

    pub fn disk_err(err: E) -> Report<Self> {
        Report::new(Self::DiskErr(err))
    }

    pub fn io_err(err: std::io::Error) -> Report<Self> {
        Report::new(Self::IoErr(err))
    }
}
