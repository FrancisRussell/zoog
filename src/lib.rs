#![feature(const_trait_impl)]

mod comment_header;
mod constants;
mod decibels;
mod error;
mod fixed_point_gain;
mod opus_header;

/// Functionality for rewriting Ogg Opus streams with new headers
pub mod header_rewriter;
pub mod rewriter;

/// Functionality for determining BS.1770 loudness of Ogg Opus streams
pub mod volume_analyzer;

pub use constants::global::*;
pub use decibels::*;
pub use error::*;

/// Types for manipulating headers of Ogg Opus streams
pub mod opus {
    pub use crate::comment_header::*;
    pub use crate::constants::opus::*;
    pub use crate::fixed_point_gain::*;
    pub use crate::opus_header::*;
}
