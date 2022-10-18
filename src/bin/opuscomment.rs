#[path = "../output_file.rs"]
mod output_file;

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use std::ops::BitOrAssign;
use std::path::PathBuf;

use clap::Parser;
use output_file::OutputFile;
use zoog::comment_rewriter::{CommentHeaderRewrite, CommentRewriterAction, CommentRewriterConfig};
use zoog::header_rewriter::{rewrite_stream, SubmitResult};
use zoog::opus::{parse_comment, validate_comment_field_name, CommentList, DiscreteCommentList};
use zoog::Error;

fn main() {
    match main_impl() {
        Ok(()) => {}
        Err(e) => {
            eprintln!("Aborted due to error: {}", e);
            std::process::exit(1);
        }
    }
}

#[derive(Debug, Parser)]
#[clap(author, version, about = "List or edit comments in Ogg Opus files.")]
struct Cli {
    #[clap(short, long, action)]
    /// List comments in the Ogg Opus file
    list: bool,

    #[clap(short, long, action)]
    /// Append comments in the Ogg Opus file
    append: bool,

    #[clap(short, long, action, conflicts_with = "append")]
    /// Replace comments in the Ogg Opus file
    write: bool,

    #[clap(short = 't', long = "tag", id = "NAME")]
    /// Specify a tag
    tags: Vec<String>,

    #[clap(short = 'd', long = "rm", id = "NAME[=VALUE]")]
    /// Specify a tag name or name-value mapping to be deleted
    delete: Vec<String>,

    /// Input file
    input_file: PathBuf,

    /// Output file
    output_file: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug)]
enum OperationMode {
    Inspect,
    Append,
    Replace,
}

/// Match type for Opus comment values
#[derive(Clone, Debug)]
enum ValueMatch {
    All,
    ContainedIn(HashSet<String>),
}

impl ValueMatch {
    pub fn singleton(value: String) -> ValueMatch {
        let mut set = HashSet::with_capacity(1);
        set.insert(value);
        ValueMatch::ContainedIn(set)
    }

    pub fn matches(&self, value: &str) -> bool {
        match self {
            ValueMatch::All => true,
            ValueMatch::ContainedIn(values) => values.contains(value),
        }
    }
}

impl Default for ValueMatch {
    fn default() -> ValueMatch { ValueMatch::ContainedIn(HashSet::new()) }
}

impl BitOrAssign for ValueMatch {
    fn bitor_assign(&mut self, rhs: ValueMatch) {
        let mut old_lhs = ValueMatch::All;
        std::mem::swap(self, &mut old_lhs);
        let new_set = match (old_lhs, rhs) {
            (ValueMatch::ContainedIn(mut lhs), ValueMatch::ContainedIn(rhs)) => {
                lhs.extend(rhs.into_iter());
                Some(lhs)
            }
            _ => None,
        };
        if let Some(new_set) = new_set {
            *self = ValueMatch::ContainedIn(new_set);
        }
    }
}

fn parse_new_comment_args<S: AsRef<str>, I: IntoIterator<Item = S>>(comments: I) -> Result<DiscreteCommentList, Error> {
    let comments = comments.into_iter();
    let mut result = DiscreteCommentList::with_capacity(comments.size_hint().0);
    for comment in comments {
        let comment = comment.as_ref();
        let (key, value) = parse_comment(comment)?;
        result.append(&key, &value)?;
    }
    Ok(result)
}

fn parse_delete_comment_args<S, I>(patterns: I) -> Result<HashMap<String, ValueMatch>, Error>
where
    S: AsRef<str>,
    I: IntoIterator<Item = S>,
{
    let patterns = patterns.into_iter();
    let mut result = HashMap::new();
    for pattern_string in patterns {
        let pattern_string = pattern_string.as_ref();
        let (key, value) = match parse_comment(pattern_string) {
            Ok((key, value)) => (key, Some(value)),
            Err(_) => match validate_comment_field_name(pattern_string) {
                Ok(()) => (pattern_string.to_string(), None),
                Err(e) => return Err(e),
            },
        };
        let rhs = match value {
            None => ValueMatch::All,
            Some(value) => ValueMatch::singleton(value),
        };
        *result.entry(key).or_default() |= rhs;
    }
    Ok(result)
}

fn main_impl() -> Result<(), Error> {
    let cli = Cli::parse_from(wild::args_os());
    let list = cli.list;
    let operation_mode = match (cli.append, cli.write) {
        (true, false) => OperationMode::Append,
        (false, true) => OperationMode::Replace,
        (false, false) => OperationMode::Inspect,
        (true, true) => panic!("Append and replace cannot be specified at the same time"),
    };

    let tags = parse_new_comment_args(cli.tags)?;
    let delete_tags = parse_delete_comment_args(cli.delete)?;
    println!("Operating in mode: {:?}", operation_mode);
    println!("tags={:?}", tags);
    println!("delete_tags={:?}", delete_tags);

    let action = match operation_mode {
        OperationMode::Inspect => CommentRewriterAction::NoChange,
        OperationMode::Append => todo!("Append not yet implemented"),
        OperationMode::Replace => CommentRewriterAction::Replace(tags),
    };

    let rewriter_config = CommentRewriterConfig { action };
    let input_path = cli.input_file;
    let input_file = File::open(&input_path).map_err(|e| Error::FileOpenError(input_path.to_path_buf(), e))?;
    let mut input_file = BufReader::new(input_file);

    let mut output_file = match operation_mode {
        OperationMode::Inspect => OutputFile::new_sink(),
        OperationMode::Append | OperationMode::Replace => {
            let output_path = cli.output_file.unwrap_or(input_path.to_path_buf());
            OutputFile::new_target(&output_path)?
        }
    };

    let rewrite_result = {
        let output_file = output_file.as_write();
        let mut output_file = BufWriter::new(output_file);
        let rewrite = CommentHeaderRewrite::new(rewriter_config);
        let abort_on_unchanged = true;
        rewrite_stream(rewrite, &mut input_file, &mut output_file, abort_on_unchanged)
    };
    drop(input_file);

    match rewrite_result {
        Err(e) => {
            eprintln!("Failure during processing of {}.", input_path.display());
            return Err(e);
        }
        Ok(SubmitResult::Good) => {
            // We finished processing the file but never got the headers
            eprintln!("File {} appeared to be oddly truncated. Doing nothing.", input_path.display());
        }
        Ok(SubmitResult::HeadersUnchanged(comments)) => {
            if list {
                comments.write_as_text(io::stdout()).map_err(Error::ConsoleIoError)?;
            }
        }
        Ok(SubmitResult::HeadersChanged { to: comments, .. }) => {
            output_file.commit()?;
            if list {
                comments.write_as_text(io::stdout()).map_err(Error::ConsoleIoError)?;
            }
        }
    };
    Ok(())
}
