use std::io::{Cursor, Read, Write};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use derivative::Derivative;

use crate::{Error, FixedPointGain, TAG_ALBUM_GAIN, TAG_TRACK_GAIN};

const COMMENT_MAGIC: &[u8] = &[0x4f, 0x70, 0x75, 0x73, 0x54, 0x61, 0x67, 0x73];

#[derive(Derivative)]
#[derivative(Debug)]
pub struct CommentHeader<'a> {
    #[derivative(Debug = "ignore")]
    data: &'a mut Vec<u8>,
    vendor: String,
    user_comments: Vec<(String, String)>,
}

impl<'a> CommentHeader<'a> {
    fn read_length<R: Read>(mut reader: R) -> Result<u32, Error> {
        reader.read_u32::<LittleEndian>().map_err(|_| Error::MalformedCommentHeader)
    }

    fn read_exact<R: Read>(mut reader: R, data: &mut [u8]) -> Result<(), Error> {
        reader.read_exact(data).map_err(|_| Error::MalformedCommentHeader)
    }

    pub fn empty(data: &'a mut Vec<u8>) -> CommentHeader<'a> {
        CommentHeader { data, vendor: String::new(), user_comments: Vec::new() }
    }

    pub fn set_vendor(&mut self, vendor: &str) { self.vendor = vendor.to_string(); }

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
        let mut user_comments = Vec::with_capacity(num_comments as usize);
        for _ in 0..num_comments {
            let comment_len = Self::read_length(&mut reader)?;
            let mut comment = vec![0u8; comment_len as usize];
            Self::read_exact(&mut reader, &mut comment)?;
            let comment = String::from_utf8(comment)?;
            let offset = comment.find('=').ok_or(Error::MalformedCommentHeader)?;
            let (key, value) = comment.split_at(offset);
            user_comments.push((String::from(key), String::from(&value[1..])));
        }
        let result = CommentHeader { data, vendor, user_comments };
        Ok(Some(result))
    }

    pub fn get_first(&self, key: &str) -> Option<&str> {
        for (k, v) in self.user_comments.iter() {
            if k == key {
                return Some(v);
            }
        }
        None
    }

    pub fn remove_all(&mut self, key: &str) { self.user_comments.retain(|(k, _)| key != k); }

    pub fn replace(&mut self, key: &str, value: &str) {
        self.remove_all(key);
        self.append(key, value);
    }

    pub fn append(&mut self, key: &str, value: &str) {
        self.user_comments.push((String::from(key), String::from(value)));
    }

    pub fn get_gain_from_tag(&self, tag: &str) -> Result<Option<FixedPointGain>, Error> {
        let parsed =
            self.get_first(tag).map(|v| v.parse::<FixedPointGain>().map_err(|_| Error::InvalidR128Tag(v.to_string())));
        match parsed {
            Some(Ok(v)) => Ok(Some(v)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    pub fn get_album_or_track_gain(&self) -> Result<Option<FixedPointGain>, Error> {
        for tag in [TAG_ALBUM_GAIN, TAG_TRACK_GAIN].iter() {
            if let Some(gain) = self.get_gain_from_tag(tag)? {
                return Ok(Some(gain));
            }
        }
        Ok(None)
    }

    pub fn adjust_gains(&mut self, adjustment: FixedPointGain) -> Result<(), Error> {
        if adjustment.is_zero() {
            return Ok(());
        }
        for tag in [TAG_ALBUM_GAIN, TAG_TRACK_GAIN].iter() {
            if let Some(gain) = self.get_gain_from_tag(tag)? {
                let gain = gain.checked_add(adjustment).ok_or(Error::GainOutOfBounds)?;
                self.replace(tag, &format!("{}", gain.as_fixed_point()));
            }
        }
        Ok(())
    }

    pub fn commit(&mut self) {
        //TODO: Look more into why we can't use https://github.com/rust-lang/rust/pull/46830
        let mut writer = Cursor::new(Vec::new());
        writer.write_all(COMMENT_MAGIC).unwrap();
        let vendor = self.vendor.as_bytes();
        writer.write_u32::<LittleEndian>(vendor.len() as u32).unwrap();
        writer.write_all(vendor).unwrap();
        writer.write_u32::<LittleEndian>(self.user_comments.len() as u32).unwrap();
        let equals: &[u8] = &[0x3d];
        for (k, v) in self.user_comments.iter().map(|(k, v)| (k.as_bytes(), v.as_bytes())) {
            let len = k.len() + v.len() + 1;
            writer.write_u32::<LittleEndian>(len as u32).unwrap();
            writer.write_all(k).unwrap();
            writer.write_all(equals).unwrap();
            writer.write_all(v).unwrap();
        }
        *self.data = writer.into_inner();
    }
}

impl<'a> Drop for CommentHeader<'a> {
    fn drop(&mut self) { self.commit(); }
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

    fn random_string<R: Rng>(engine: &mut R, allow_empty: bool) -> String {
        let min_len = if allow_empty { 0 } else { 1 };
        let len_distr = Uniform::new_inclusive(min_len, MAX_STRING_LENGTH);
        let len = engine.sample(len_distr);
        let mut result = String::new();
        result.reserve(len);
        for c in engine.sample_iter(&Standard).take(len) {
            result.push(c);
        }
        result
    }

    fn create_random_header<'a, 'b, R: Rng>(engine: &'b mut R, data: &'a mut Vec<u8>) -> CommentHeader<'a> {
        let mut header = CommentHeader::empty(data);
        header.set_vendor(&random_string(engine, true));
        let num_comments_dist = Uniform::new_inclusive(0, MAX_COMMENTS);
        let num_comments = engine.sample(&num_comments_dist);
        for _ in 0..num_comments {
            let key = random_string(engine, false);
            let value = random_string(engine, true);
            header.append(key.as_str(), value.as_str());
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
