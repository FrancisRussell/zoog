#![feature(const_trait_impl, let_chains, cow_is_borrowed)]
#![warn(clippy::pedantic)]
#![allow(clippy::missing_errors_doc, clippy::must_use_candidate, clippy::uninlined_format_args, clippy::doc_markdown)]

mod codec;
mod constants;
mod decibels;
mod error;

/// Functionality for escaping and unescaping values for command-line tools
pub mod escaping;

/// Functionality for rewriting Ogg Opus streams with new headers
pub mod header_rewriter;

/// Functionality for rewriting Ogg Opus streams with new comments
pub mod comment_rewrite;

/// Support for detecting an operation should be interrupted
pub mod interrupt;

/// Functionality for rewriting Ogg Opus streams with altered output gain and
/// volume tags
pub mod volume_rewrite;

/// Functionality for determining BS.1770 loudness of Ogg Opus streams
pub mod volume_analyzer;

/// Functionality for manipulating headers
pub mod header;

/// Types for manipulating headers of Ogg Opus streams
pub mod opus;

pub use codec::*;
pub use constants::global::*;
pub use decibels::*;
pub use error::*;
