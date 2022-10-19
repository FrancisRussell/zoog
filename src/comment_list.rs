use std::io::{self, Write};

use crate::constants::opus::FIELD_NAME_TERMINATOR;
use crate::Error;

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
    fn append(&mut self, key: &str, value: &str) -> Result<(), Error>;

    /// Iterate over the entries of the comment list
    fn iter(&self) -> Self::Iter<'_>;

    /// Retain only the key value mappings for which the predicate returns true
    fn retain<F: FnMut(&str, &str) -> bool>(&mut self, f: F);

    /// Write each comment in the user-friendly textual representation
    fn write_as_text<W: Write>(&self, mut writer: W) -> Result<(), io::Error> {
        for (k, v) in self.iter() {
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
            self.append(key, value)?;
        }
        Ok(())
    }
}

/// Parses the textual representation of an Opus comment
pub fn parse_comment(comment: &str) -> Result<(String, String), Error> {
    let offset = comment.find(char::from(FIELD_NAME_TERMINATOR)).ok_or(Error::MissingOpusCommentSeparator)?;
    let (key, value) = comment.split_at(offset);
    validate_comment_field_name(key)?;
    Ok((key.into(), value[1..].into()))
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
