use std::borrow::Cow;
use std::io::{self, Write};

use crate::header::FixedPointGain;
use crate::{escaping, Error, FIELD_NAME_TERMINATOR};

/// Provides functionality for manipulating comment lists
pub trait CommentList {
    type Iter<'a>: Iterator<Item = (&'a str, &'a str)>
    where
        Self: 'a;

    /// Returns the number of user comments in the header
    fn len(&self) -> usize;

    /// Does the header contain any user comments?
    fn is_empty(&self) -> bool { self.len() == 0 }

    /// Removes all items
    fn clear(&mut self);

    /// Returns the first mapped value for the specified key.
    fn get_first(&self, key: &str) -> Option<&str>;

    /// If the key already exists, update the first mapping's value to the one
    /// supplied and discard any later mappings. If the key does not exist,
    /// append the mapping to the end of the list.
    fn replace(&mut self, key: &str, value: &str) -> Result<(), Error>;

    /// Removes all mappings for the specified key.
    fn remove_all(&mut self, key: &str);

    /// Appends the specified mapping.
    fn push(&mut self, key: &str, value: &str) -> Result<(), Error>;

    /// Iterate over the entries of the comment list
    fn iter(&self) -> Self::Iter<'_>;

    /// Retain only the key value mappings for which the predicate returns true
    fn retain<F: FnMut(&str, &str) -> bool>(&mut self, f: F);

    /// Write each comment in the user-friendly textual representation
    fn write_as_text<W: Write>(&self, mut writer: W, escape: bool) -> Result<(), io::Error> {
        for (k, v) in self.iter() {
            let v = if escape { escaping::escape_str(v) } else { Cow::from(v) };
            writeln!(writer, "{}{}{}", k, FIELD_NAME_TERMINATOR as char, v)?;
        }
        Ok(())
    }

    /// Extend with mappings from supplied iterator
    fn extend<K, V, I>(&mut self, comments: I) -> Result<(), Error>
    where
        K: AsRef<str>,
        V: AsRef<str>,
        I: IntoIterator<Item = (K, V)>,
    {
        let comments = comments.into_iter();
        for (key, value) in comments {
            let (key, value) = (key.as_ref(), value.as_ref());
            self.push(key, value)?;
        }
        Ok(())
    }

    /// Attempts to parse the first mapping for the specified key as the
    /// fixed-point Decibel representation used in Opus comment headers.
    fn get_gain_from_tag(&self, tag: &str) -> Result<Option<FixedPointGain>, Error> {
        let parsed =
            self.get_first(tag).map(|v| v.parse::<FixedPointGain>().map_err(|_| Error::InvalidR128Tag(v.into())));
        match parsed {
            Some(Ok(v)) => Ok(Some(v)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    /// Sets the specified tag to the supplied gain using the fixed-point
    /// representation used in Ogg Opus comment headers. All other mappings
    /// for the same tag will be removed.
    fn set_tag_to_gain(&mut self, tag: &str, gain: FixedPointGain) -> Result<(), Error> {
        self.replace(tag, &format!("{}", gain.as_fixed_point()))
    }
}

/// Parses the textual representation of an Opus comment
pub fn parse_comment(comment: &str) -> Result<(&str, &str), Error> {
    let offset = comment.find(char::from(FIELD_NAME_TERMINATOR)).ok_or(Error::MissingCommentSeparator)?;
    let (key, value) = comment.split_at(offset);
    validate_comment_field_name(key)?;
    Ok((key, &value[1..]))
}

/// Validates the field name of a comment
pub fn validate_comment_field_name(field_name: &str) -> Result<(), Error> {
    for c in field_name.chars() {
        match c {
            ' '..='<' | '>'..='}' => {}
            _ => return Err(Error::InvalidOpusCommentFieldName(field_name.into())),
        }
    }
    Ok(())
}
