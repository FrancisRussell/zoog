use std::borrow::Cow;

use thiserror::Error;

const ESCAPE_CHAR: char = '\\';

/// Wraps an iterator to apply `vorbiscomemnt`-style character escaping
#[derive(Debug)]
struct EscapingIterator<I> {
    inner: I,
    delayed: Option<char>,
}

impl<I> EscapingIterator<I> {
    pub fn new(inner: I) -> EscapingIterator<I> { EscapingIterator { inner, delayed: None } }
}

impl<I> Iterator for EscapingIterator<I>
where
    I: Iterator<Item = char>,
{
    type Item = char;

    fn next(&mut self) -> Option<char> {
        if self.delayed.is_none() && let Some(c) = self.inner.next() {
            match c {
                '\0' => self.delayed = Some('0'),
                '\n' => self.delayed = Some('n'),
                '\r' => self.delayed = Some('r'),
                '\\' => self.delayed = Some('\\'),
                _ => {},
            }
            Some(if self.delayed.is_some() {
                ESCAPE_CHAR
            } else {
                c
            })
        } else {
            self.delayed.take()
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) { (self.inner.size_hint().0, None) }
}

/// Escapes a string slice using `vorbiscomment`-style escaping
pub fn escape_str(value: &str) -> Cow<str> {
    // We may be able to return a reference to the original string if no escaping is
    // needed
    EscapingIterator::new(value.chars()).collect()
}

#[derive(Debug, Error)]
pub enum EscapeDecodeError {
    /// The string ended with a backslash
    #[error("Trailing backslash in escaped string")]
    TrailingBackSlash,

    /// An invalid character followd a backlash in an escaped string
    #[error("Invalid character following backslash in escaped string: `{0}`")]
    InvalidEscape(char),
}

/// Unescapes a string slice using `vorbiscomment`-style escaping
pub fn unescape_str(value: &str) -> Result<Cow<str>, EscapeDecodeError> {
    if !value.contains(ESCAPE_CHAR) {
        return Ok(value.into());
    }
    let mut result = String::with_capacity(value.len());
    let mut is_escape = false;
    for c in value.chars() {
        if is_escape {
            match c {
                '0' => result.push('\0'),
                'n' => result.push('\n'),
                'r' => result.push('\r'),
                '\\' => result.push('\\'),
                _ => return Err(EscapeDecodeError::InvalidEscape(c)),
            }
            is_escape = false;
        } else if c == ESCAPE_CHAR {
            is_escape = true;
        } else {
            result.push(c);
        }
    }

    if is_escape {
        Err(EscapeDecodeError::TrailingBackSlash)
    } else {
        Ok(result.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_non_special() {
        let test_string = "The quick brown fox jumps over the lazy dog";
        let escaped = escape_str(test_string);
        assert_eq!(test_string, escaped);
    }

    #[test]
    fn escape_special() {
        let test_string = "\0\n\r\\";
        let escaped = escape_str(test_string);
        assert_eq!(escaped, "\\0\\n\\r\\\\");
    }

    #[test]
    fn escaping_is_invertible() {
        let test_string = "\0\n\r\\";
        let escaped = escape_str(test_string);
        let unescaped = unescape_str(&escaped).expect("Unable to reverse escaping");
        assert_eq!(test_string, unescaped);
    }
}
