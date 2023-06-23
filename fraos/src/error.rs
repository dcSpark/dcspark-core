use crate::index_on_mmaps::MmapIndexError;
use std::fmt::{Display, Formatter};
use std::{error, fmt, io, path::PathBuf};

#[derive(Debug)]
pub enum FileError {
    /// Failed to open file.
    FileOpen(PathBuf, io::Error),
    /// In read only mode path is not found
    PathNotFound,
    /// Database path already exists and does not point to a directory
    PathNotDir,
    /// Failed to extend a file
    Extend(io::Error),
    /// Failed to extend a file
    Shrink(io::Error),
    /// Failed to get file metadata
    Metadata(io::Error),
}

impl Display for FileError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            FileError::FileOpen(path, error) => {
                write!(f, "can't open file at {:?}, error: {}", path, error)
            }
            FileError::PathNotFound => write!(f, "path is not found"),
            FileError::PathNotDir => write!(f, "path is file, not folder"),
            FileError::Extend(error) => write!(f, "failed to extend a database file: {}", error),
            FileError::Metadata(error) => write!(f, "failed to get file metadata: {}", error),
            FileError::Shrink(error) => write!(f, "failed to shrink file size: {}", error),
        }
    }
}

impl error::Error for FileError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            FileError::FileOpen(_, source)
            | FileError::Extend(source)
            | FileError::Metadata(source) => Some(source),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum MmapError {
    /// Failed to create mmap.
    Mmap(io::Error),
    /// Failed to write data to mmap.
    MmapWrite(io::Error),
    /// Failed to flush database records to disk
    Flush(io::Error),
    /// Failed to make a memory mapping page immutable
    Protect(io::Error),
    /// Failed to access mmap
    Access,
    /// Failed to access mmap
    DataLength { actual: usize, requested: usize },
}

impl Display for MmapError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            MmapError::Mmap(error) => write!(f, "memory map failed: {}", error),
            MmapError::MmapWrite(error) => write!(f, "memory map write failed: {}", error),
            MmapError::Flush(error) => {
                write!(f, "failed to flush database records to disk: {}", error)
            }
            MmapError::Protect(error) => write!(
                f,
                "failed to make a memory mapping page immutable: {}",
                error
            ),
            MmapError::Access => write!(f, "failed to access memory mapping data"),
            MmapError::DataLength { actual, requested } => write!(
                f,
                "invalid data length requested. actual: {}, requrested: {}",
                actual, requested
            ),
        }
    }
}

impl error::Error for MmapError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            MmapError::Mmap(source)
            | MmapError::MmapWrite(source)
            | MmapError::Flush(source)
            | MmapError::Protect(source) => Some(source),
            MmapError::Access | MmapError::DataLength { .. } => None,
        }
    }
}

/// Datbase error.
#[derive(Debug)]
pub enum FraosError {
    /// Error related to files work
    FileError(FileError),
    /// Error related to memory mappings
    MmapError(MmapError),
    /// Index error
    IndexError(MmapIndexError),

    /// Records in the data file are incorrect.
    DataFileDamaged,
    /// Sequential number index is broken
    IndexFileDamaged,

    /// Failed to acquire storage lock
    StorageLock { description: String },

    /// Failed to extend
    StorageZeroExtension,
}

impl error::Error for FraosError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            FraosError::FileError(err) => err.source(),
            FraosError::MmapError(err) => err.source(),
            FraosError::IndexError(err) => err.source(),
            _ => None,
        }
    }
}

impl fmt::Display for FraosError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FraosError::FileError(error) => write!(f, "file error: {}", error),
            FraosError::MmapError(error) => write!(f, "mmap error: {}", error),
            FraosError::IndexError(error) => write!(f, "index error: {}", error),
            FraosError::DataFileDamaged => write!(f, "data file damaged"),
            FraosError::IndexFileDamaged => write!(f, "index file damaged"),
            FraosError::StorageLock { description } => write!(f, "poisoned lock: {}", description),
            FraosError::StorageZeroExtension => {
                write!(f, "tried to extend storage file with empty data")
            }
        }
    }
}
