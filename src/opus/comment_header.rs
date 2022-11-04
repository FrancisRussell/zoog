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
    use rand::distributions::{Standard, Uniform};
    use rand::rngs::SmallRng;
    use rand::{Rng, SeedableRng};

    use super::*;
    use crate::header::{CommentHeader as _, CommentList as _};

    const MAX_STRING_LENGTH: usize = 1024;
    const MAX_COMMENTS: usize = 128;
    const NUM_IDENTITY_TESTS: usize = 256;

    fn random_string<R: Rng>(engine: &mut R, is_key: bool) -> String {
        let min_len = if is_key { 1 } else { 0 };
        let len_distr = Uniform::new_inclusive(min_len, MAX_STRING_LENGTH);
        let len = engine.sample(len_distr);
        let mut result = String::new();
        result.reserve(len);
        if is_key {
            let valid_chars: Vec<char> = (' '..='<').chain('>'..='}').collect();
            let char_index_dist = Uniform::new(0, valid_chars.len());
            for _ in 0..len {
                result.push(valid_chars[engine.sample(char_index_dist)]);
            }
        } else {
            for c in engine.sample_iter(&Standard).take(len) {
                result.push(c);
            }
        }
        result
    }

    fn create_random_header<R: Rng>(engine: &mut R) -> CommentHeader {
        let mut header = CommentHeader::empty();
        header.set_vendor(&random_string(engine, false));
        let num_comments_dist = Uniform::new_inclusive(0, MAX_COMMENTS);
        let num_comments = engine.sample(&num_comments_dist);
        for _ in 0..num_comments {
            let key = random_string(engine, true);
            let value = random_string(engine, false);
            header.push(key.as_str(), value.as_str()).expect("Unable to add comment");
        }
        header
    }

    #[test]
    fn parse_and_encode_is_identity() {
        let mut rng = SmallRng::seed_from_u64(19489);
        for _ in 0..NUM_IDENTITY_TESTS {
            let header_data_original =
                create_random_header(&mut rng).into_vec().expect("Failed to encode comment header");
            let header_data = CommentHeader::try_parse(&header_data_original)
                .expect("Previously generated header was not recognised")
                .into_vec()
                .expect("Failed to encode comment header");
            assert_eq!(header_data_original, header_data);
        }
    }

    #[test]
    fn not_comment_header() {
        let mut header: Vec<u8> = COMMENT_MAGIC.iter().cloned().collect();
        let last_byte = header.last_mut().unwrap();
        *header.last_mut().unwrap() = last_byte.wrapping_add(1);
        assert!(CommentHeader::try_parse(header).is_err());
    }

    #[test]
    fn truncated_header() {
        let header: Vec<u8> = COMMENT_MAGIC.iter().cloned().collect();
        match CommentHeader::try_parse(header) {
            Err(Error::MalformedCommentHeader) => {}
            _ => assert!(false, "Wrong error for malformed header"),
        };
    }
}
