use std::borrow::Cow;

use thiserror::Error;

/// The escape character
const ESCAPE_CHAR: char = '\\';

/// Characters which are escaped by tag processing tools
const ESCAPED_CHARS: [char; 4] = ['\0', '\n', '\r', '\\'];

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
    if !value.contains(ESCAPED_CHARS) {
        value.into()
    } else {
        EscapingIterator::new(value.chars()).collect()
    }
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

    fn is_safe(value: &str) -> bool {
        // Escaped strings may still contain the escape character so we don't include it
        !value.contains(&['\0', '\n', '\r'])
    }

    #[test]
    fn escape_non_special() {
        let original = "The quick brown fox jumps over the lazy dog";
        assert!(is_safe(original));

        let escaped = escape_str(original);
        assert!(is_safe(&escaped));
        assert!(escaped.is_borrowed());
        assert_eq!(original, escaped);

        let unescaped = unescape_str(&escaped).expect("Unable to unescape string");
        assert!(unescaped.is_borrowed());
        assert_eq!(original, unescaped);
    }

    #[test]
    fn escape_special() {
        let original = "\0\n\r\\";
        assert!(!is_safe(&original));

        let escaped = escape_str(original);
        assert!(is_safe(&escaped));
        assert!(escaped.is_owned());
        assert_eq!(escaped, "\\0\\n\\r\\\\");

        let unescaped = unescape_str(&escaped).expect("Unable to reverse escaping");
        assert!(unescaped.is_owned());
        assert_eq!(original, unescaped);
    }

    #[test]
    fn escaping_special_by_char() {
        // Pick up bugs in detecting if strings need to be escaped by testing each
        // escaped character individually
        for c in ESCAPED_CHARS.iter() {
            let original = c.to_string();

            let escaped = escape_str(&original);
            assert_eq!(escaped.len(), 2);
            assert!(is_safe(&escaped));
            assert!(escaped.is_owned());

            let unescaped = unescape_str(&escaped).expect("Unable to reverse escaping");
            assert!(unescaped.is_owned());
            assert_eq!(original, unescaped);
        }
    }
}
