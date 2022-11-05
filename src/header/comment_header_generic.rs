use std::borrow::Cow;
use std::io::{Cursor, Read, Write};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use derivative::Derivative;

use crate::header::{parse_comment, CommentList, DiscreteCommentList};
use crate::{header, Error, FIELD_NAME_TERMINATOR};

/// Implementation-specific details of comment headers (Opus versus Vorbis)
pub trait CommentHeaderSpecifics {
    /// Return the magic signature which should be present at the start of the
    /// header
    fn get_magic() -> Cow<'static, [u8]>;

    /// Reads any bytes which should be present after comments
    fn read_suffix<R: Read>(&mut self, reader: &mut R) -> Result<(), Error>;

    /// Writes any bytes which should be present after comments
    fn write_suffix<W: Write>(&self, writer: &mut W) -> Result<(), Error>;
}

/// Allows querying and modification of an Opus/Vorbis comment header. This type
/// is parameterized by a type implementing `CommentHeaderSpecifics` which
/// encodes format-specific logic.
#[derive(Derivative)]
#[derivative(Clone, Debug, Default, PartialEq)]
pub struct CommentHeaderGeneric<S>
where
    S: CommentHeaderSpecifics + Clone,
{
    vendor: String,
    user_comments: DiscreteCommentList,
    specifics: S,
}

impl<S> header::CommentHeader for CommentHeaderGeneric<S>
where
    S: CommentHeaderSpecifics + Clone + Default,
{
    fn try_parse(data: &[u8]) -> Result<CommentHeaderGeneric<S>, Error> {
        let magic = S::get_magic();
        let identical = data.iter().take(magic.len()).eq(magic.iter());
        if !identical {
            return Err(Error::MalformedCommentHeader);
        }
        let mut reader = Cursor::new(&data[magic.len()..]);
        let vendor_len = Self::read_length(&mut reader)?;
        let mut vendor = vec![0u8; vendor_len as usize];
        Self::read_exact(&mut reader, &mut vendor)?;
        let vendor = String::from_utf8(vendor)?;
        let num_comments = Self::read_length(&mut reader)?;
        let mut user_comments = DiscreteCommentList::with_capacity(num_comments as usize);
        for _ in 0..num_comments {
            let comment_len = Self::read_length(&mut reader)?;
            let mut comment = vec![0u8; comment_len as usize];
            Self::read_exact(&mut reader, &mut comment)?;
            let comment = String::from_utf8(comment)?;
            let (key, value) = parse_comment(&comment)?;
            user_comments.push(key, value)?;
        }
        let mut specifics = S::default();
        specifics.read_suffix(&mut reader)?;
        let result = CommentHeaderGeneric { vendor, user_comments, specifics };
        Ok(result)
    }

    fn serialize_into<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        writer.write_all(&S::get_magic()).map_err(Error::WriteError)?;
        let vendor = self.vendor.as_bytes();
        let vendor_len = vendor.len().try_into().map_err(|_| Error::UnrepresentableValueInCommentHeader)?;
        writer.write_u32::<LittleEndian>(vendor_len).map_err(Error::WriteError)?;
        writer.write_all(vendor).map_err(Error::WriteError)?;
        let user_comments_len =
            self.user_comments.len().try_into().map_err(|_| Error::UnrepresentableValueInCommentHeader)?;
        writer.write_u32::<LittleEndian>(user_comments_len).map_err(Error::WriteError)?;
        let field_name_terminator = [FIELD_NAME_TERMINATOR];
        for (k, v) in self.user_comments.iter().map(|(k, v)| (k.as_bytes(), v.as_bytes())) {
            let comment_len = k.len() + v.len() + 1;
            let comment_len = comment_len.try_into().map_err(|_| Error::UnrepresentableValueInCommentHeader)?;
            writer.write_u32::<LittleEndian>(comment_len).map_err(Error::WriteError)?;
            writer.write_all(k).map_err(Error::WriteError)?;
            writer.write_all(&field_name_terminator).map_err(Error::WriteError)?;
            writer.write_all(v).map_err(Error::WriteError)?;
        }
        self.specifics.write_suffix(writer)?;
        Ok(())
    }

    fn set_vendor(&mut self, vendor: &str) { self.vendor = vendor.into(); }

    fn get_vendor(&self) -> &str { self.vendor.as_str() }

    fn to_discrete_comment_list(&self) -> DiscreteCommentList { self.user_comments.clone() }
}

impl<S> CommentHeaderGeneric<S>
where
    S: CommentHeaderSpecifics + Clone,
{
    fn read_length<R: Read>(mut reader: R) -> Result<u32, Error> {
        reader.read_u32::<LittleEndian>().map_err(|_| Error::MalformedCommentHeader)
    }

    fn read_exact<R: Read>(mut reader: R, data: &mut [u8]) -> Result<(), Error> {
        reader.read_exact(data).map_err(|_| Error::MalformedCommentHeader)
    }
}

impl<S> CommentList for CommentHeaderGeneric<S>
where
    S: CommentHeaderSpecifics + Clone,
{
    type Iter<'b> = <DiscreteCommentList as CommentList>::Iter<'b> where Self: 'b;

    fn len(&self) -> usize { self.user_comments.len() }

    fn is_empty(&self) -> bool { self.user_comments.is_empty() }

    fn clear(&mut self) { self.user_comments.clear() }

    fn get_first(&self, key: &str) -> Option<&str> { self.user_comments.get_first(key) }

    fn remove_all(&mut self, key: &str) { self.user_comments.remove_all(key) }

    fn replace(&mut self, key: &str, value: &str) -> Result<(), Error> { self.user_comments.replace(key, value) }

    fn push(&mut self, key: &str, value: &str) -> Result<(), Error> { self.user_comments.push(key, value) }

    fn iter(&self) -> Self::Iter<'_> { self.user_comments.iter() }

    fn retain<F: FnMut(&str, &str) -> bool>(&mut self, f: F) { self.user_comments.retain(f) }
}

#[cfg(test)]
mod tests {
    use rand::rngs::SmallRng;
    use rand::SeedableRng;

    use super::*;
    use crate::header::test_utils::create_random_header;

    const NUM_IDENTITY_TESTS: usize = 256;
    const TEST_MAGIC: &[u8] = b"zoogheader";
    const TEST_SUFFIX: &[u8] = b"zoogsuffix";

    #[derive(Clone, Debug, Default)]
    struct TestSpecifics {}

    impl CommentHeaderSpecifics for TestSpecifics {
        fn get_magic() -> Vec<u8> { TEST_MAGIC.into() }

        fn read_suffix<R: Read>(&mut self, reader: &mut R) -> Result<(), Error> {
            let mut suffix = Vec::new();
            reader.read_to_end(&mut suffix).map_err(Error::ReadError)?;
            if suffix != TEST_SUFFIX {
                Err(Error::MalformedCommentHeader)
            } else {
                Ok(())
            }
        }

        fn write_suffix<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
            writer.write_all(TEST_SUFFIX).map_err(Error::WriteError)
        }
    }

    type CommentHeaderTest = CommentHeaderGeneric<TestSpecifics>;

    #[test]
    fn parse_and_encode_is_identity() {
        let mut rng = SmallRng::seed_from_u64(19489);
        for _ in 0..NUM_IDENTITY_TESTS {
            let header_data_original = {
                let header: CommentHeaderTest = create_random_header(&mut rng);
                header.into_vec().expect("Failed to encode comment header")
            };
            let header_data = {
                let header = CommentHeaderTest::try_parse(&header_data_original)
                    .expect("Previously generated header was not recognised");
                header.into_vec().expect("Failed to encode comment header")
            };
            assert_eq!(header_data_original, header_data);
        }
    }

    #[test]
    fn not_comment_header() {
        let mut header: Vec<u8> = TEST_MAGIC.iter().cloned().collect();
        let last_byte = header.last_mut().unwrap();
        *header.last_mut().unwrap() = last_byte.wrapping_add(1);
        assert!(CommentHeaderTest::try_parse(header).is_err());
    }

    #[test]
    fn truncated_header() {
        let header: Vec<u8> = TEST_MAGIC.iter().cloned().collect();
        match CommentHeaderTest::try_parse(header) {
            Err(Error::MalformedCommentHeader) => {}
            _ => assert!(false, "Wrong error for malformed header"),
        };
    }
}
