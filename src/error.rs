use thiserror::Error;
use std::path::PathBuf;
use tempfile::PersistError;
use ogg::reading::OggReadError;

#[derive(Debug, Error)]
pub enum ZoogError {
    #[error("Unable to open file `{0}` due to `{1}`")]
    FileOpenError(PathBuf, std::io::Error),
    #[error("Unable to open temporary file due to `{0}`")]
    TempFileOpenError(std::io::Error),
    #[error("Ogg decoding error: `{0}`")]
    OggDecode(OggReadError),
    #[error("Error writing to file: `{0}`")]
    WriteError(std::io::Error),
    #[error("Not an Opus stream")]
    MissingOpusStream,
    #[error("Comment header is missing")]
    MissingCommentHeader,
    #[error("Malformed comment header")]
    MalformedCommentHeader,
    #[error("UTF-8 encoding error")]
    UTF8Error(#[from] std::string::FromUtf8Error),
    #[error("R128 tag has invalid value")]
    InvalidR128Tag,
    #[error("Gain out of bounds")]
    GainOutOfBounds,
    #[error("Failed to rename `{0}` to `{1}` due to `{2}`")]
    FileCopy(PathBuf, PathBuf, std::io::Error),
    #[error("Failed to persist temporary file due to `{0}``")]
    PersistError(#[from] PersistError),
}


