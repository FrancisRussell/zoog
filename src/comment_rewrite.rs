use derivative::Derivative;

use crate::header::{self, CommentList, DiscreteCommentList};
use crate::header_rewriter::{HeaderRewriteGeneric, HeaderSummarizeGeneric};
use crate::Error;

/// Mode type for `CommentRewriter`
#[derive(Derivative)]
#[derivative(Debug)]
pub enum CommentRewriterAction<'a> {
    NoChange,
    Modify {
        #[derivative(Debug = "ignore")]
        retain: Box<dyn Fn(&str, &str) -> bool + 'a>,
        append: DiscreteCommentList,
    },
    Replace(DiscreteCommentList),
}

/// Configuration type for `CommentRewriter`
#[derive(Debug)]
pub struct CommentRewriterConfig<'a> {
    /// The action to be performed
    pub action: CommentRewriterAction<'a>,
}

/// Parameterization struct for `HeaderRewriter` to rewrite ouput gain and R128
/// tags.
#[derive(Debug)]
pub struct CommentHeaderRewrite<'a> {
    config: CommentRewriterConfig<'a>,
}

impl CommentHeaderRewrite<'_> {
    pub fn new(config: CommentRewriterConfig) -> CommentHeaderRewrite { CommentHeaderRewrite { config } }
}

/// Summarizes codec headers by returning the comment list
#[derive(Debug, Default)]
pub struct CommentHeaderSummary {}

impl HeaderSummarizeGeneric for CommentHeaderSummary {
    type Error = Error;
    type Summary = DiscreteCommentList;

    fn summarize<I, C>(&self, _id_header: &I, comment_header: &C) -> Result<DiscreteCommentList, Error>
    where
        I: header::IdHeader,
        C: header::CommentHeader,
    {
        Ok(comment_header.to_discrete_comment_list())
    }
}

impl HeaderRewriteGeneric for CommentHeaderRewrite<'_> {
    type Error = Error;

    fn rewrite<I, C>(&self, _idheader: &mut I, comment_header: &mut C) -> Result<(), Error>
    where
        I: header::IdHeader,
        C: header::CommentHeader,
    {
        match &self.config.action {
            CommentRewriterAction::NoChange => {}
            CommentRewriterAction::Replace(tags) => {
                comment_header.clear();
                comment_header.extend(tags.iter())?;
            }
            CommentRewriterAction::Modify { retain, append } => {
                comment_header.retain(retain);
                comment_header.extend(append.iter())?;
            }
        }
        Ok(())
    }
}
