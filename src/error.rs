use std::path::PathBuf;

use ogg::reading::OggReadError;
use tempfile::PersistError;
use thiserror::Error;

use crate::{escaping, Codec};

/// The Zoog error type
#[derive(Debug, Error)]
pub enum Error {
    /// A specified file could not be opened due to an IO error
    #[error("Unable to open file `{0}` due to `{1}`")]
    FileOpenError(PathBuf, std::io::Error),

    /// An error occurred reading from the file
    #[error("Unable to read from file `{0}` due to `{1}`")]
    FileReadError(PathBuf, std::io::Error),

    /// An error occurred writing to the file
    #[error("Unable to write to file `{0}` due to `{1}`")]
    FileWriteError(PathBuf, std::io::Error),

    /// A temporary file could not be opened due to an IO error
    #[error("Unable to open temporary file in `{0}` due to `{1}`")]
    TempFileOpenError(PathBuf, std::io::Error),

    /// An Ogg stream failed to decode correctly
    #[error("Ogg decoding error: `{0}`")]
    OggDecode(OggReadError),

    /// A read error from a file
    #[error("Error reading from file: `{0}`")]
    ReadError(std::io::Error),

    /// A write error to a file
    #[error("Error writing to file: `{0}`")]
    WriteError(std::io::Error),

    /// The stream was not of the expected codec
    #[error("Not a stream of type {0}")]
    MissingStream(Codec),

    /// The stream was not of a recognised codec
    #[error("Unknown codec")]
    UnknownCodec,

    /// The codec identification header was invalid
    #[error("Malformed identification header")]
    MalformedIdentificationHeader,

    /// The comment header was invalid
    #[error("Malformed comment header")]
    MalformedCommentHeader,

    /// Missing comment separator
    #[error("Missing separator in comment")]
    MissingCommentSeparator,

    /// An invalid UTF-8 sequence was encountered
    #[error("UTF-8 encoding error")]
    UTF8Error(#[from] std::string::FromUtf8Error),

    /// An R128 tag was found to be invalid
    #[error("R128 tag has invalid value: `{0}`")]
    InvalidR128Tag(String),

    /// A gain value was out of bounds for being representable
    #[error("A computed gain value was not representable")]
    GainOutOfBounds,

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

    /// Invalid Opus comment field name
    #[error("Invalid Opus comment field name: `{0}`")]
    InvalidOpusCommentFieldName(String),

    /// An escaped string was invalid
    #[error("{0}")]
    EscapeDecodeError(#[from] escaping::EscapeDecodeError),

    /// An interrupt was detected
    #[error("The operation was interrupted")]
    Interrupted,

    /// Unsupported codec version
    #[error("Version {1} of codec {0} is not supported")]
    UnsupportedCodecVersion(Codec, u64),

    /// Unsupported codec
    #[error("The codec {0} was not supported for this operation")]
    UnsupportedCodec(Codec),

    /// Unrepresentable value in comment header
    #[error("A value could not be represented in a comment header")]
    UnrepresentableValueInCommentHeader,

    /// Unexpected logical stream in Ogg file
    #[error("Unexpected logical stream in Ogg file, serial {0:#x}")]
    UnexpectedLogicalStream(u32),

    /// Audio parameters changed
    #[error("Channel count and/or sample rate changed between concatenated audio streams")]
    UnexpectedAudioParametersChange,
}
