use ogg::reading::OggReadError;
use std::path::PathBuf;
use tempfile::PersistError;
use thiserror::Error;

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
    #[error("R128 tag has invalid value: `{0}`")]
    InvalidR128Tag(String),
    #[error("A computed gain value was not representable")]
    GainOutOfBounds,
    #[error("Failed to rename `{0}` to `{1}` due to `{2}`")]
    FileMove(PathBuf, PathBuf, std::io::Error),
    #[error("Failed to persist temporary file due to `{0}`")]
    PersistError(#[from] PersistError),
    #[error("Unsupported channel count: `{0}`")]
    InvalidChannelCount(usize),
    #[error("libopus error: `{0}`")]
    OpusError(opus::Error),
}
