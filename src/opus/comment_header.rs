use std::borrow::Cow;
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
    fn get_magic() -> Cow<'static, [u8]> { COMMENT_MAGIC.into() }

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
    use rand::distributions::{Distribution, Uniform};
    use rand::rngs::SmallRng;
    use rand::SeedableRng;

    use super::*;
    use crate::header::test_utils::create_random_header;

    #[test]
    fn padding_is_discarded() -> Result<(), Error> {
        let mut rng = SmallRng::seed_from_u64(57128);
        let header: CommentHeader = create_random_header(&mut rng);
        let original_data = header.into_vec()?;
        let padding_size = 1024;
        let padded_data: Vec<u8> =
            original_data.iter().copied().chain(std::iter::repeat(0xFE).take(padding_size)).collect();
        assert!(original_data.len() < padded_data.len());
        let processed_data = {
            let header = CommentHeader::try_parse(&padded_data)?;
            header.into_vec()?
        };
        assert_eq!(original_data, processed_data);
        Ok(())
    }

    #[test]
    fn experimental_data_is_preserved() -> Result<(), Error> {
        let mut rng = SmallRng::seed_from_u64(73295);
        let header: CommentHeader = create_random_header(&mut rng);
        let original_data = header.into_vec()?;
        let experimental_data_size = 1024;
        let experimental_data_dist = Uniform::new_inclusive(0u8, 0xFFu8);
        let padded_data: Vec<u8> = original_data
            .iter()
            .copied()
            .chain(std::iter::once(0x1))
            .chain(experimental_data_dist.sample_iter(&mut rng).take(experimental_data_size))
            .collect();
        assert!(original_data.len() < padded_data.len());
        let processed_data = {
            let header = CommentHeader::try_parse(&padded_data)?;
            header.into_vec()?
        };
        assert_eq!(padded_data, processed_data);
        Ok(())
    }
}
