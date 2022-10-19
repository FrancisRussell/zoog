use std::borrow::Cow;

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
                '\\'
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
}
