use std::io::{Read, Write};

use crate::header::{self, CommentHeaderGeneric};
use crate::Error;

const COMMENT_MAGIC: &[u8] = b"OpusTags";

/// Opus-specific comment header logic
#[derive(Debug, Default, PartialEq)]
pub struct CommentHeaderSpecifics {}

impl header::CommentHeaderSpecifics for CommentHeaderSpecifics {
    fn get_magic() -> Vec<u8> { COMMENT_MAGIC.into() }

    fn read_postfix<R: Read>(&mut self, _reader: &mut R) -> Result<(), Error> { Ok(()) }

    fn write_postfix<W: Write>(&self, _writer: &mut W) -> Result<(), Error> { Ok(()) }
}

/// Manipulates and Ogg Opus header
pub type CommentHeader<'a> = CommentHeaderGeneric<'a, CommentHeaderSpecifics>;

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

    fn create_random_header<'a, 'b, R: Rng>(engine: &'b mut R, data: &'a mut Vec<u8>) -> CommentHeader<'a> {
        let mut header = CommentHeader::empty(data);
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
    fn drop_does_commit() {
        let mut rng = SmallRng::seed_from_u64(24745);
        let mut header_data = Vec::new();
        {
            create_random_header(&mut rng, &mut header_data);
        }
        assert_ne!(header_data.len(), 0);
    }

    #[test]
    fn parse_and_commit_is_identity() {
        let mut rng = SmallRng::seed_from_u64(19489);
        for _ in 0..NUM_IDENTITY_TESTS {
            let mut header_data = Vec::new();
            {
                create_random_header(&mut rng, &mut header_data);
            }
            let header_data_original = header_data.clone();
            {
                CommentHeader::try_parse(&mut header_data)
                    .expect("Error parsing generated header")
                    .expect("Previously generated header was not recognised");
            }
            assert_eq!(header_data_original, header_data);
        }
    }

    #[test]
    fn not_comment_header() {
        let mut header: Vec<u8> = COMMENT_MAGIC.iter().cloned().collect();
        let last_byte = header.last_mut().unwrap();
        *header.last_mut().unwrap() = last_byte.wrapping_add(1);
        assert!(CommentHeader::try_parse(&mut header).unwrap().is_none());
    }

    #[test]
    fn truncated_header() {
        let mut header: Vec<u8> = COMMENT_MAGIC.iter().cloned().collect();
        match CommentHeader::try_parse(&mut header) {
            Err(Error::MalformedCommentHeader) => {}
            _ => assert!(false, "Wrong error for malformed header"),
        };
    }
}
