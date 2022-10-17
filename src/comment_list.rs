use crate::Error;

pub trait CommentList {
    type Iter<'a>: Iterator<Item = (&'a str, &'a str)>
    where
        Self: 'a;

    /// Returns the number of user comments in the header
    fn len(&self) -> usize;

    /// Does the header contain any user comments?
    fn is_empty(&self) -> bool;

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
    fn iter<'a>(&'a self) -> Self::Iter<'a>;
}
