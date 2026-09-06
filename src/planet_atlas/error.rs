//! Atlas creation, storage, validation, and compatibility errors.

use crate::planet::Face;

use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum AtlasError {
    Io(std::io::Error),
    Cancelled,
    InvalidDimensions {
        side: u16,
        count: usize,
    },
    InvalidPosition {
        face: Face,
        u: u16,
        v: u16,
        side: u16,
    },
    UnsupportedVersion(String),
    Corrupt(String),
    Incomplete(String),
    AlreadyExists(PathBuf),
}

impl fmt::Display for AtlasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Cancelled => f.write_str("planet creation cancelled"),
            Self::InvalidDimensions { side, count } => write!(
                f,
                "invalid atlas dimensions: side {side}, cell count {count}"
            ),
            Self::InvalidPosition { face, u, v, side } => {
                write!(f, "atlas position {face}/{u}/{v} is outside side {side}")
            }
            Self::UnsupportedVersion(message) => write!(f, "unsupported planet atlas: {message}"),
            Self::Corrupt(message) => write!(f, "corrupt planet atlas: {message}"),
            Self::Incomplete(message) => write!(f, "incomplete planet atlas: {message}"),
            Self::AlreadyExists(path) => {
                write!(f, "planet atlas already exists at {}", path.display())
            }
        }
    }
}

impl std::error::Error for AtlasError {}

impl From<std::io::Error> for AtlasError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
