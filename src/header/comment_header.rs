use crate::header::{CommentList, DiscreteCommentList};

pub trait CommentHeader: CommentList {
    /// Sets the vendor field.
    fn set_vendor(&mut self, vendor: &str);

    /// Returns the comments in the header as a `DiscreteCommentList`.
    fn to_discrete_comment_list(&self) -> DiscreteCommentList;
}
