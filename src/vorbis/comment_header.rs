use std::io::{Read, Write};

use crate::header::{self, CommentHeaderGeneric};
use crate::Error;

const COMMENT_MAGIC: &[u8] = b"\x03vorbis";
const FRAMING_BYTE: u8 = 1;

/// Vorbis-specific comment header logic
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Specifics {}

impl header::CommentHeaderSpecifics for Specifics {
    fn get_magic() -> Vec<u8> { COMMENT_MAGIC.into() }

    fn read_suffix<R: Read>(&mut self, reader: &mut R) -> Result<(), Error> {
        let mut buffer = [0u8];
        if reader.read(&mut buffer).map_err(Error::ReadError)? != 1 || (buffer[0] & 1) == 0 {
            Err(Error::MalformedCommentHeader)
        } else {
            Ok(())
        }
    }

    fn write_suffix<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        let buffer = [FRAMING_BYTE];
        writer.write_all(&buffer).map_err(Error::WriteError)
    }
}

/// Manipulates an Ogg Vorbis comment header
pub type CommentHeader = CommentHeaderGeneric<Specifics>;

#[cfg(test)]
mod tests {
}
