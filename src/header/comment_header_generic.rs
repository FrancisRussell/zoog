use std::io::{Cursor, Read, Write};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use derivative::Derivative;
use thiserror::Error;

use crate::header::{parse_comment, CommentList, DiscreteCommentList};
use crate::{header, Error, FIELD_NAME_TERMINATOR};

/// Implementation-specific details of comment headers (Opus versus Vorbis)
pub trait CommentHeaderSpecifics: Default {
    /// Return the magic signature which should be present at the start of the
    /// header
    fn get_magic() -> Vec<u8>;

    /// Reads any bytes which should be present after comments
    fn read_postfix<R: Read>(&mut self, reader: &mut R) -> Result<(), Error>;

    /// Writes any bytes which should be present after comments
    fn write_postfix<W: Write>(&self, writer: &mut W) -> Result<(), Error>;
}

/// Allows querying and modification of an Opus/Vorbis comment header. This type
/// is parameterized by a type implementing `CommentHeaderSpecifics` which
/// encodes format-specific logic.
#[derive(Derivative)]
#[derivative(Debug)]
pub struct CommentHeaderGeneric<'a, S>
where
    S: CommentHeaderSpecifics,
{
    #[derivative(Debug = "ignore")]
    data: &'a mut Vec<u8>,
    vendor: String,
    user_comments: DiscreteCommentList,
    specifics: S,
}

#[derive(Debug, Error)]
enum CommitError {
    #[error("Value unrepresentable in comment header")]
    ValueTooLarge,
}

impl<S> header::CommentHeader for CommentHeaderGeneric<'_, S>
where
    S: CommentHeaderSpecifics,
{
    fn set_vendor(&mut self, vendor: &str) { self.vendor = vendor.into(); }

    fn to_discrete_comment_list(&self) -> DiscreteCommentList { self.user_comments.clone() }
}

impl<'a, S> CommentHeaderGeneric<'a, S>
where
    S: CommentHeaderSpecifics,
{
    fn read_length<R: Read>(mut reader: R) -> Result<u32, Error> {
        reader.read_u32::<LittleEndian>().map_err(|_| Error::MalformedCommentHeader)
    }

    fn read_exact<R: Read>(mut reader: R, data: &mut [u8]) -> Result<(), Error> {
        reader.read_exact(data).map_err(|_| Error::MalformedCommentHeader)
    }

    /// Constructs an empty `CommentHeader`. The comment data will be placed in
    /// the supplied `Vec`. Any existing content will be discarded.
    pub fn empty(data: &'a mut Vec<u8>) -> CommentHeaderGeneric<'a, S> {
        CommentHeaderGeneric {
            data,
            vendor: String::new(),
            user_comments: DiscreteCommentList::default(),
            specifics: Default::default(),
        }
    }

    /// Attempts to parse the supplied `Vec` as an Opus comment header. An error
    /// is returned if the header is believed to be corrupt, otherwise an
    /// `Option` is returned containing either the parsed header or `None`
    /// if the comment magic string was not found. This enables
    /// distinguishing between a corrupted comment header and a packet which
    /// does not appear to be a comment header.
    pub fn try_parse(data: &'a mut Vec<u8>) -> Result<Option<CommentHeaderGeneric<'a, S>>, Error> {
        let magic = S::get_magic();
        let identical = data.iter().take(magic.len()).eq(magic.iter());
        if !identical {
            return Ok(None);
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
        specifics.read_postfix(&mut reader)?;
        let result = CommentHeaderGeneric { data, vendor, user_comments, specifics };
        Ok(Some(result))
    }

    fn commit(&mut self) -> Result<(), CommitError> {
        let data = &mut *self.data;
        data.clear();
        data.extend(S::get_magic());
        let vendor = self.vendor.as_bytes();
        let vendor_len = vendor.len().try_into().map_err(|_| CommitError::ValueTooLarge)?;
        data.write_u32::<LittleEndian>(vendor_len).expect("Error writing vendor length");
        data.extend(vendor);
        let user_comments_len = self.user_comments.len().try_into().map_err(|_| CommitError::ValueTooLarge)?;
        data.write_u32::<LittleEndian>(user_comments_len).expect("Error writing user comment count");
        for (k, v) in self.user_comments.iter().map(|(k, v)| (k.as_bytes(), v.as_bytes())) {
            let comment_len = k.len() + v.len() + 1;
            let comment_len = comment_len.try_into().map_err(|_| CommitError::ValueTooLarge)?;
            data.write_u32::<LittleEndian>(comment_len).expect("Error writing user comment length");
            data.extend(k);
            data.push(FIELD_NAME_TERMINATOR);
            data.extend(v);
        }
        self.specifics.write_postfix(data).expect("Error writing comment postfix data");
        Ok(())
    }
}

impl<'a, S> CommentList for CommentHeaderGeneric<'a, S>
where
    S: CommentHeaderSpecifics,
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

impl<'a, S> Drop for CommentHeaderGeneric<'a, S>
where
    S: CommentHeaderSpecifics,
{
    fn drop(&mut self) {
        if let Err(e) = self.commit() {
            panic!("Failed to commit changes to CommentHeader: {}", e);
        }
    }
}

impl<'a, S> PartialEq for CommentHeaderGeneric<'a, S>
where
    S: CommentHeaderSpecifics,
{
    fn eq(&self, other: &CommentHeaderGeneric<'a, S>) -> bool {
        self.vendor == other.vendor && self.user_comments == other.user_comments
    }
}