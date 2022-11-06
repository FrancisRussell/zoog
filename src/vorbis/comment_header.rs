use std::borrow::Cow;
use std::io::{Read, Write};

use crate::header::{self, CommentHeaderGeneric};
use crate::Error;

const COMMENT_MAGIC: &[u8] = b"\x03vorbis";
const FRAMING_BYTE: u8 = 1;

/// Vorbis-specific comment header logic
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Specifics {}

impl header::CommentHeaderSpecifics for Specifics {
    fn get_magic() -> Cow<'static, [u8]> { COMMENT_MAGIC.into() }

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
    use std::io::Cursor;

    use super::*;
    use crate::header::CommentHeaderSpecifics as _;

    #[test]
    fn default_outputs_framing_bit() -> Result<(), Error> {
        let specifics = Specifics::default();
        let mut suffix = Vec::new();
        specifics.write_suffix(&mut suffix)?;
        assert!(!suffix.is_empty());
        assert!((suffix[0] & 1) != 0);
        Ok(())
    }

    #[test]
    fn missing_framing_byte() {
        let mut specifics = Specifics::default();
        let mut reader = Cursor::new(&[]);
        assert!(specifics.read_suffix(&mut reader).is_err());
    }

    #[test]
    fn missing_framing_bit() {
        let mut specifics = Specifics::default();
        let mut reader = Cursor::new(&[0xFE]);
        assert!(specifics.read_suffix(&mut reader).is_err());
    }

    #[test]
    fn present_framing_bit() {
        let mut specifics = Specifics::default();
        let mut reader = Cursor::new(&[0x1]);
        assert!(specifics.read_suffix(&mut reader).is_ok());
    }
}
