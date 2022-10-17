use std::sync::Arc;

use crate::opus::{validate_comment_field_name, CommentList};
use crate::Error;

/// Stand-alone representation of an Ogg Opus comment list
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DiscreteCommentList {
    comments: Vec<(Arc<String>, Arc<String>)>,
}

impl DiscreteCommentList {
    fn keys_equal(k1: &str, k2: &str) -> bool { k1.eq_ignore_ascii_case(k2) }

    /// Allocates a list with the specified capacity
    pub fn with_capacity(cap: usize) -> DiscreteCommentList {
        DiscreteCommentList { comments: Vec::with_capacity(cap) }
    }
}

/// Iterator for `DiscreteCommentList`
pub struct Iter<'a> {
    inner: std::slice::Iter<'a, (Arc<String>, Arc<String>)>,
}

impl<'a> Iterator for Iter<'a> {
    type Item = (&'a str, &'a str);

    fn next(&mut self) -> Option<Self::Item> { self.inner.next().map(|(k, v)| (k.as_str(), v.as_str())) }
}

impl CommentList for DiscreteCommentList {
    type Iter<'a> = Iter<'a>;

    fn len(&self) -> usize { self.comments.len() }

    fn is_empty(&self) -> bool { self.comments.is_empty() }

    fn clear(&mut self) { self.comments.clear() }

    fn get_first(&self, key: &str) -> Option<&str> {
        self.comments.iter().find(|(k, _)| Self::keys_equal(k, key)).map(|(_, v)| v.as_str())
    }

    fn remove_all(&mut self, key: &str) { self.comments.retain(|(k, _)| !Self::keys_equal(key, k)); }

    fn replace(&mut self, key: &str, value: &str) -> Result<(), Error> {
        let mut found = false;
        self.comments.retain_mut(|(k, ref mut v)| {
            if Self::keys_equal(k, key) {
                if found {
                    // If we have already found the key, discard this mapping
                    false
                } else {
                    *v = Arc::new(value.into());
                    found = true;
                    true
                }
            } else {
                true
            }
        });

        // If the key did not exist, we append
        if !found {
            self.append(key, value)?;
        }
        Ok(())
    }

    fn append(&mut self, key: &str, value: &str) -> Result<(), Error> {
        validate_comment_field_name(key)?;
        self.comments.push((Arc::new(key.into()), Arc::new(value.into())));
        Ok(())
    }

    fn iter(&self) -> Iter<'_> { Iter { inner: self.comments.iter() } }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_appends_on_missing() -> Result<(), Error> {
        let key = "foo";
        let value = "bar";

        let mut list_1 = DiscreteCommentList::default();
        list_1.append("v0", "k0")?;
        list_1.append(key, value)?;
        list_1.append("v1", "k1")?;

        let mut list_2 = DiscreteCommentList::default();
        list_2.append("v0", "k0")?;
        list_2.replace(key, value)?;
        list_2.append("v1", "k1")?;

        assert_eq!(list_1, list_2);
        Ok(())
    }

    #[test]
    fn replace_replaces_on_duplicates() -> Result<(), Error> {
        let mut list_1 = DiscreteCommentList::default();
        list_1.append("v0", "k0")?;
        list_1.append("v1", "k1")?;
        list_1.append("v2", "k2")?;
        list_1.append("v3", "k3")?;
        list_1.append("v2", "k4")?;
        list_1.append("v5", "k5")?;
        list_1.append("v2", "k6")?;
        list_1.append("v7", "k7")?;
        list_1.replace("v2", "k8")?;

        let mut list_2 = DiscreteCommentList::default();
        list_2.append("v0", "k0")?;
        list_2.append("v1", "k1")?;
        list_2.append("v2", "k8")?;
        list_2.append("v3", "k3")?;
        list_2.append("v5", "k5")?;
        list_2.append("v7", "k7")?;

        assert_eq!(list_1, list_2);
        Ok(())
    }

    #[test]
    fn get_first_case_insensitive() -> Result<(), Error> {
        let mut list_1 = DiscreteCommentList::default();
        list_1.append("FooBar", "1")?;
        list_1.append("FOOBAR", "2")?;
        list_1.append("foobar", "3")?;

        assert_eq!(list_1.get_first("FooBar"), Some("1"));
        assert_eq!(list_1.get_first("FOOBAR"), Some("1"));
        assert_eq!(list_1.get_first("foobar"), Some("1"));
        assert_eq!(list_1.get_first("FoObAr"), Some("1"));
        Ok(())
    }

    #[test]
    fn replace_case_insensitive() -> Result<(), Error> {
        let mut list_1 = DiscreteCommentList::default();
        list_1.append("FooBar", "1")?;
        list_1.append("FOOBAR", "2")?;
        list_1.append("foobar", "3")?;
        list_1.replace("FoObAr", "42")?;

        assert_eq!(list_1.get_first("FOObar"), Some("42"));
        assert_eq!(list_1.len(), 1);
        Ok(())
    }

    #[test]
    fn remove_all_case_insensitive() -> Result<(), Error> {
        let mut list_1 = DiscreteCommentList::default();
        list_1.append("FooBar", "1")?;
        list_1.append("FOOBAR", "2")?;
        list_1.append("v0", "k0")?;
        list_1.append("foobar", "3")?;
        list_1.append("v5", "k5")?;
        list_1.remove_all("FOObar");

        let mut list_2 = DiscreteCommentList::default();
        list_2.append("v0", "k0")?;
        list_2.append("v5", "k5")?;

        assert_eq!(list_1, list_2);
        Ok(())
    }
}
