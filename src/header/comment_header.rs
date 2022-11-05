use std::io::Write;

use crate::header::{CommentList, DiscreteCommentList};
use crate::Error;

pub trait CommentHeader: CommentList {
    /// Attempts to parse the supplied slice as a comment header. An error
    /// is returned if the header is believed to be corrupt, otherwise the
    /// parsed header is returned.
    fn try_parse(data: &[u8]) -> Result<Self, Error>
    where
        Self: Sized;

    /// Sets the vendor field.
    fn set_vendor(&mut self, vendor: &str);

    /// Returns the comments in the header as a `DiscreteCommentList`.
    fn to_discrete_comment_list(&self) -> DiscreteCommentList;

    /// Gets the vendor field.
    fn get_vendor(&self) -> &str;

    /// Writes the serialized header
    fn serialize_into<W: Write>(&self, writer: &mut W) -> Result<(), Error>;
}
