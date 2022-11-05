use std::io::{Read, Write};

use crate::header::{self, CommentHeaderGeneric};
use crate::Error;

const COMMENT_MAGIC: &[u8] = b"OpusTags";

/// Opus-specific comment header logic
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Specifics {
    suffix_data: Vec<u8>,
}

impl header::CommentHeaderSpecifics for Specifics {
    fn get_magic() -> Vec<u8> { COMMENT_MAGIC.into() }

    fn read_suffix<R: Read>(&mut self, reader: &mut R) -> Result<(), Error> {
        // If the LSB of the first byte following the comments is set, we preserve
        // this data as suggested by the spec, otherwise we discard it.
        let mut buffer = [0u8];
        if reader.read(&mut buffer).map_err(Error::ReadError)? == 1 && (buffer[0] & 1) != 0 {
            self.suffix_data.extend(buffer);
            reader.read_to_end(&mut self.suffix_data).map_err(Error::ReadError)?;
        }
        Ok(())
    }

    fn write_suffix<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        writer.write_all(&self.suffix_data).map_err(Error::WriteError)
    }
}

/// Manipulates an Ogg Opus comment header
pub type CommentHeader = CommentHeaderGeneric<Specifics>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::test::{self as header_test};

    #[test]
    fn parse_and_encode_is_identity() { header_test::parse_and_encode_is_identity::<Specifics>() }

    #[test]
    fn not_comment_header() { header_test::not_comment_header::<Specifics>(COMMENT_MAGIC) }

    #[test]
    fn truncated_header() { header_test::truncated_header::<Specifics>(COMMENT_MAGIC); }
}
