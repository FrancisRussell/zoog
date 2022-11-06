use rand::distributions::{Standard, Uniform};
use rand::Rng;

use crate::{header, Error};

const MAX_STRING_LENGTH: usize = 1024;
const MAX_COMMENTS: usize = 128;

pub(crate) fn random_string<R: Rng>(engine: &mut R, is_key: bool) -> String {
    let min_len = if is_key { 1 } else { 0 };
    let len_distr = Uniform::new_inclusive(min_len, MAX_STRING_LENGTH);
    let len = engine.sample(len_distr);
    let mut result = String::new();
    result.reserve(len);
    if is_key {
        let valid_chars: Vec<char> = (' '..='<').chain('>'..='}').collect();
        let char_index_dist = Uniform::new(0, valid_chars.len());
        for _ in 0..len {
            result.push(valid_chars[engine.sample(char_index_dist)]);
        }
    } else {
        for c in engine.sample_iter(&Standard).take(len) {
            result.push(c);
        }
    }
    result
}

pub(crate) fn create_random_header<H: header::CommentHeader + Default, R: Rng>(engine: &mut R) -> H {
    let mut header = H::default();
    header.set_vendor(&random_string(engine, false));
    let num_comments_dist = Uniform::new_inclusive(0, MAX_COMMENTS);
    let num_comments = engine.sample(&num_comments_dist);
    for _ in 0..num_comments {
        let key = random_string(engine, true);
        let value = random_string(engine, false);
        header.push(key.as_str(), value.as_str()).expect("Unable to add comment");
    }
    header
}

pub(crate) fn comment_header_as_vec<C: header::CommentHeader>(c: &C) -> Result<Vec<u8>, Error> {
    let mut serialized = Vec::new();
    c.serialize_into(&mut serialized)?;
    Ok(serialized)
}
