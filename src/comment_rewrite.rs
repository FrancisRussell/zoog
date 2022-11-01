use derivative::Derivative;

use crate::header::{CommentList, DiscreteCommentList};
use crate::header_rewriter::HeaderRewrite;
use crate::opus::{CommentHeader, OpusHeader};
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

impl HeaderRewrite for CommentHeaderRewrite<'_> {
    type Error = Error;
    type Summary = DiscreteCommentList;

    fn summarize(
        &self, _opus_header: &OpusHeader, comment_header: &CommentHeader,
    ) -> Result<DiscreteCommentList, Error> {
        Ok(comment_header.to_discrete_comment_list())
    }

    fn rewrite(&self, _opus_header: &mut OpusHeader, comment_header: &mut CommentHeader) -> Result<(), Error> {
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
