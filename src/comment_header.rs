use crate::constants::{TAG_ALBUM_GAIN, TAG_TRACK_GAIN};
use crate::error::ZoogError;
use crate::gain::Gain;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use derivative::Derivative;
use std::io::{Cursor, Read, Write};

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
    fn read_length<R: Read>(mut reader: R) -> Result<u32, ZoogError> {
        reader.read_u32::<LittleEndian>().map_err(|_| ZoogError::MalformedCommentHeader)
    }

    fn read_exact<R: Read>(mut reader: R, data: &mut [u8]) -> Result<(), ZoogError> {
        reader.read_exact(data).map_err(|_| ZoogError::MalformedCommentHeader)
    }

    fn try_parse(data: &'a mut Vec<u8>) -> Result<CommentHeader<'a>, ZoogError> {
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
            let offset = comment.find('=').ok_or(ZoogError::MalformedCommentHeader)?;
            let (key, value) = comment.split_at(offset);
            user_comments.push((String::from(key), String::from(&value[1..])));
        }
        let result = CommentHeader {
            data,
            vendor,
            user_comments,
        };
        Ok(result)
    }

    pub fn try_new(data: &'a mut Vec<u8>) -> Result<Option<CommentHeader<'a>>, ZoogError> {
        let identical = data.iter().take(COMMENT_MAGIC.len()).eq(COMMENT_MAGIC.iter());
        if !identical { return Ok(None); }
         Self::try_parse(data).map(Some)
    }

    pub fn get_first(&self, key: &str) -> Option<&str> {
        for (k, v) in self.user_comments.iter() {
            if k == key { return Some(v); }
        }
        None
    }

    pub fn remove_all(&mut self, key: &str) {
        self.user_comments = self.user_comments.iter().filter(|(k, _)| key != k).cloned().collect();
    }

    pub fn replace(&mut self, key: &str, value: &str) {
        self.remove_all(key);
        self.user_comments.push((String::from(key), String::from(value)));
    }

    pub fn get_gain_from_tag(&self, tag: &str) -> Result<Option<Gain>, ZoogError> {
        let parsed = self.get_first(tag)
            .map(|v| v.parse::<Gain>().map_err(|_| ZoogError::InvalidR128Tag(v.to_string())));
        match parsed {
            Some(Ok(v)) => Ok(Some(v)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    pub fn get_album_or_track_gain(&self) -> Result<Option<Gain>, ZoogError> {
        for tag in [TAG_ALBUM_GAIN, TAG_TRACK_GAIN].iter() {
            if let Some(gain) = self.get_gain_from_tag(*tag)? {
                return Ok(Some(gain));
            }
        }
        Ok(None)
    }

    pub fn adjust_gains(&mut self, adjustment: Gain) -> Result<(), ZoogError> {
        if adjustment.is_none() { return Ok(()); }
        for tag in [TAG_ALBUM_GAIN, TAG_TRACK_GAIN].iter() {
            if let Some(gain) = self.get_gain_from_tag(*tag)? {
                let gain = gain.checked_add(adjustment).ok_or(ZoogError::GainOutOfBounds)?;
                self.replace(*tag, &format!("{}", gain.as_fixed_point()));
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
