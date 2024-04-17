use core::fmt::Display;
use std::error;

/// Type of error
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ErrorKind {
    /// System is already running
    AlreadyRunning,

    /// System monitoring error
    HarnessError,

    /// Error connecting to pipe
    PipeError,

    /// Error while serializing data
    SerializationError,

    /// General I/O errors
    IO,
}

/// System harness error
#[derive(Debug)]
#[allow(dead_code)]
pub struct Error {
    kind: ErrorKind,
    error: Box<dyn error::Error + Send + Sync>,
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.error.to_string())
    }
}

impl Error {
    pub fn new<E: Into<Box<dyn error::Error + Send + Sync>>>(kind: ErrorKind, error: E) -> Self {
        Self {
            kind,
            error: error.into(),
        }
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::new(ErrorKind::IO, error)
    }
}

impl From<std::ffi::NulError> for Error {
    fn from(error: std::ffi::NulError) -> Self {
        Self::new(ErrorKind::IO, error)
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::new(ErrorKind::IO, error)
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(error: std::str::Utf8Error) -> Self {
        Self::new(ErrorKind::IO, error)
    }
}

#[cfg(feature = "serde")]
impl std::error::Error for Error {}

#[cfg(feature = "serde")]
impl serde::ser::Error for Error {
    fn custom<T>(msg: T) -> Self
    where
        T: Display,
    {
        Self {
            kind: ErrorKind::SerializationError,
            error: std::io::Error::new(std::io::ErrorKind::Other, format!("{msg}")).into(),
        }
    }
}
