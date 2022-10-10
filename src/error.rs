use std::path::PathBuf;

use ogg::reading::OggReadError;
use tempfile::PersistError;
use thiserror::Error;

/// The Zoog error type
#[derive(Debug, Error)]
pub enum Error {
    /// A specified file could not be opened due to an IO error
    #[error("Unable to open file `{0}` due to `{1}`")]
    FileOpenError(PathBuf, std::io::Error),

    /// A temporary file could not be opened due to an IO error
    #[error("Unable to open temporary file due to `{0}`")]
    TempFileOpenError(std::io::Error),

    /// An Ogg stream failed to decode correctly
    #[error("Ogg decoding error: `{0}`")]
    OggDecode(OggReadError),

    /// A write error to a file
    #[error("Error writing to file: `{0}`")]
    WriteError(std::io::Error),

    /// The stream was not an Opus stream
    #[error("Not an Opus stream")]
    MissingOpusStream,

    /// The Opus comment header was missing
    #[error("Comment header is missing")]
    MissingCommentHeader,

    /// The Opus comment header was invalid
    #[error("Malformed comment header")]
    MalformedCommentHeader,

    /// An invalid UTF-8 sequence was encountered
    #[error("UTF-8 encoding error")]
    UTF8Error(#[from] std::string::FromUtf8Error),

    /// An R128 tag was found to be invalid
    #[error("R128 tag has invalid value: `{0}`")]
    InvalidR128Tag(String),

    /// A gain value was out of bounds for being representable
    #[error("A computed gain value was not representable")]
    GainOutOfBounds,

    /// An error occurred during a file rename
    #[error("Failed to rename `{0}` to `{1}` due to `{2}`")]
    FileMove(PathBuf, PathBuf, std::io::Error),

    /// An error occurred during a file deletion
    #[error("Failed to delete `{0}` due to `{1}`")]
    FileDelete(PathBuf, std::io::Error),

    /// A temporary file could not be persisted
    #[error("Failed to persist temporary file due to `{0}`")]
    PersistError(#[from] PersistError),

    /// An unsupported channel count was found
    #[error("Unsupported channel count: `{0}`")]
    InvalidChannelCount(usize),

    /// An error was returned from the Opus library
    #[error("Opus error: `{0}`")]
    OpusError(opus::Error),

    /// An IO error occurred when interacting with the console
    #[error("Console IO error: `{0}`")]
    ConsoleIoError(std::io::Error),

    /// An invalid thread count was specified
    #[error("An invalid number of threads was specified")]
    InvalidThreadCount,

    /// A parent folder could not be found
    #[error("The parent folder of `{0}` could not be found")]
    NoParentError(PathBuf),

    /// A path did not have a final named component
    #[error("The path `{0}` did not have a final named component")]
    NotAFilePath(PathBuf),
}
