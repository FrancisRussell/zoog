use std::convert::TryInto;
use std::io::{Cursor, Read};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use derivative::Derivative;
use thiserror::Error;

use crate::constants::opus::FIELD_NAME_TERMINATOR;
use crate::opus::{CommentList, DiscreteCommentList, FixedPointGain, TAG_ALBUM_GAIN, TAG_TRACK_GAIN};
use crate::Error;

const COMMENT_MAGIC: &[u8] = b"OpusTags";

/// Allows querying and modification of an Opus comment header
#[derive(Derivative)]
#[derivative(Debug)]
pub struct CommentHeader<'a> {
    #[derivative(Debug = "ignore")]
    data: &'a mut Vec<u8>,
    vendor: String,
    user_comments: DiscreteCommentList,
}

#[derive(Debug, Error)]
enum CommitError {
    #[error("Value unrepresentable in Opus comment header")]
    ValueTooLarge,
}

impl<'a> CommentHeader<'a> {
    fn read_length<R: Read>(mut reader: R) -> Result<u32, Error> {
        reader.read_u32::<LittleEndian>().map_err(|_| Error::MalformedCommentHeader)
    }

    fn read_exact<R: Read>(mut reader: R, data: &mut [u8]) -> Result<(), Error> {
        reader.read_exact(data).map_err(|_| Error::MalformedCommentHeader)
    }

    /// Constructs an empty `CommentHeader`. The comment data will be placed in
    /// the supplied `Vec`. Any existing content will be discarded.
    pub fn empty(data: &'a mut Vec<u8>) -> CommentHeader<'a> {
        CommentHeader { data, vendor: String::new(), user_comments: DiscreteCommentList::default() }
    }

    /// Sets the vendor field.
    pub fn set_vendor(&mut self, vendor: &str) { self.vendor = vendor.into(); }

    /// Attempts to parse the supplied `Vec` as an Opus comment header. An error
    /// is returned if the header is believed to be corrupt, otherwise an
    /// `Option` is returned containing either the parsed header or `None`
    /// if the comment magic string was not found. This enables
    /// distinguishing between a corrupted comment header and a packet which
    /// does not appear to be a comment header.
    pub fn try_parse(data: &'a mut Vec<u8>) -> Result<Option<CommentHeader<'a>>, Error> {
        let identical = data.iter().take(COMMENT_MAGIC.len()).eq(COMMENT_MAGIC.iter());
        if !identical {
            return Ok(None);
        }
        let mut reader = Cursor::new(&data[COMMENT_MAGIC.len()..]);
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
            let offset = comment.find(char::from(FIELD_NAME_TERMINATOR)).ok_or(Error::MalformedCommentHeader)?;
            let (key, value) = comment.split_at(offset);
            user_comments.append(key, &value[1..])?;
        }
        let result = CommentHeader { data, vendor, user_comments };
        Ok(Some(result))
    }

    /// Attempts to parse the first mapping for the specified key as the
    /// fixed-point Decibel representation used in Opus comment headers.
    pub fn get_gain_from_tag(&self, tag: &str) -> Result<Option<FixedPointGain>, Error> {
        let parsed =
            self.get_first(tag).map(|v| v.parse::<FixedPointGain>().map_err(|_| Error::InvalidR128Tag(v.into())));
        match parsed {
            Some(Ok(v)) => Ok(Some(v)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    /// Returns the album gain if present, else the track gain, else `None`.
    pub fn get_album_or_track_gain(&self) -> Result<Option<FixedPointGain>, Error> {
        for tag in [TAG_ALBUM_GAIN, TAG_TRACK_GAIN].iter() {
            if let Some(gain) = self.get_gain_from_tag(tag)? {
                return Ok(Some(gain));
            }
        }
        Ok(None)
    }

    /// Applies the specified delta to either or both of the album and track
    /// gains if present. If neither as present, this function will do
    /// nothing.
    pub fn adjust_gains(&mut self, adjustment: FixedPointGain) -> Result<(), Error> {
        if adjustment.is_zero() {
            return Ok(());
        }
        for tag in [TAG_ALBUM_GAIN, TAG_TRACK_GAIN].iter() {
            if let Some(gain) = self.get_gain_from_tag(tag)? {
                let gain = gain.checked_add(adjustment).ok_or(Error::GainOutOfBounds)?;
                self.replace(tag, &format!("{}", gain.as_fixed_point()))?;
            }
        }
        Ok(())
    }

    /// Returns the comments in the header as a `DiscreteCommentList`.
    pub fn to_discrete_comment_list(&self) -> DiscreteCommentList { self.user_comments.clone() }

    fn commit(&mut self) -> Result<(), CommitError> {
        let data = &mut self.data;
        data.clear();
        data.extend(COMMENT_MAGIC);
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
        Ok(())
    }
}

impl<'a> CommentList for CommentHeader<'a> {
    type Iter<'b> = <DiscreteCommentList as CommentList>::Iter<'b> where Self: 'b;

    fn len(&self) -> usize { self.user_comments.len() }

    fn is_empty(&self) -> bool { self.user_comments.is_empty() }

    fn clear(&mut self) { self.user_comments.clear() }

    fn get_first(&self, key: &str) -> Option<&str> { self.user_comments.get_first(key) }

    fn remove_all(&mut self, key: &str) { self.user_comments.remove_all(key) }

    fn replace(&mut self, key: &str, value: &str) -> Result<(), Error> { self.user_comments.replace(key, value) }

    fn append(&mut self, key: &str, value: &str) -> Result<(), Error> { self.user_comments.append(key, value) }

    fn iter(&self) -> Self::Iter<'_> { self.user_comments.iter() }
}

impl<'a> Drop for CommentHeader<'a> {
    fn drop(&mut self) {
        if let Err(e) = self.commit() {
            panic!("Failed to commit changes to CommentHeader: {}", e);
        }
    }
}

impl<'a> PartialEq for CommentHeader<'a> {
    fn eq(&self, other: &CommentHeader<'a>) -> bool {
        self.vendor == other.vendor && self.user_comments == other.user_comments
    }
}

#[cfg(test)]
mod tests {
    use rand::distributions::{Standard, Uniform};
    use rand::rngs::SmallRng;
    use rand::{Rng, SeedableRng};

    use super::*;

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
            header.append(key.as_str(), value.as_str()).expect("Unable to add comment");
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
