#![feature(let_chains)]

#[path = "../ctrlc_handling.rs"]
mod ctrlc_handling;

#[path = "../output_file.rs"]
mod output_file;

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::ops::BitOrAssign;
use std::path::{Path, PathBuf};

use clap::Parser;
use ctrlc_handling::CtrlCChecker;
use output_file::OutputFile;
use thiserror::Error;
use zoog::comment_rewrite::{CommentHeaderRewrite, CommentRewriterAction, CommentRewriterConfig};
use zoog::header_rewriter::{rewrite_stream_with_interrupt, SubmitResult};
use zoog::opus::{parse_comment, validate_comment_field_name, CommentList, DiscreteCommentList};
use zoog::{escaping, Error};

const OGG_OPUS_EXTENSIONS: [&str; 3] = ["oga", "ogg", "opus"];
const STANDARD_STREAM_NAME: &str = "-";

#[derive(Debug, Error)]
enum AppError {
    #[error("{0}")]
    LibraryError(#[from] Error),

    #[error("Silent exit because error was already printed")]
    SilentExit,

    #[error("Unable to register Ctrl-C handler: `{0}`")]
    CtrlCRegistration(#[from] ctrlc_handling::CtrlCRegistrationError),

    #[error("Failed to read from standard input: `{0}`")]
    StandardInputReadError(io::Error),
}

fn main() {
    if let Err(e) = main_impl() {
        match e {
            AppError::LibraryError(e) => eprintln!("Aborted due to error: {}", e),
            AppError::SilentExit => {}
            e => eprintln!("{}", e),
        }
        std::process::exit(1);
    }
}

#[derive(Debug, Parser)]
#[clap(author, version, about = "List or edit comments in Ogg Opus files.")]
struct Cli {
    #[clap(short, long, action, conflicts_with = "replace", conflicts_with = "modify")]
    /// List comments in the Ogg Opus file
    list: bool,

    #[clap(short, long, action, conflicts_with = "replace")]
    /// Delete specific comments and append new ones to the Ogg Opus file
    modify: bool,

    #[clap(short, long, action)]
    /// Replace comments in the Ogg Opus file
    replace: bool,

    #[clap(short = 't', long = "tag", value_name = "NAME=VALUE", conflicts_with = "list")]
    /// Specify a tag
    tags: Vec<String>,

    #[clap(short, long, value_name = "NAME[=VALUE]", conflicts_with = "replace", conflicts_with = "list")]
    /// Specify a tag name or name-value mapping to be deleted
    delete: Vec<String>,

    #[clap(short, long, action)]
    /// Use escapes \n, \r, \0 and \\ for tag-value input and output
    escapes: bool,

    #[clap(short = 'n', long = "dry-run", action)]
    /// Display output without performing any file modification.
    dry_run: bool,

    #[clap(short = 'I', long = "tags-in", conflicts_with = "list")]
    /// File for reading tags from
    tags_in: Option<PathBuf>,

    #[clap(short = 'O', long = "tags-out", conflicts_with = "modify", conflicts_with = "replace")]
    /// File for writing tags to
    tags_out: Option<PathBuf>,

    /// Input file
    input_file: PathBuf,

    /// Output file (cannot be specified in list mode)
    #[clap(conflicts_with = "list")]
    output_file: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug)]
enum OperationMode {
    List,
    Modify,
    Replace,
}

/// Match type for Opus comment values
#[derive(Clone, Debug)]
enum ValueMatch {
    All,
    ContainedIn(HashSet<String>),
}

impl ValueMatch {
    pub fn singleton(value: String) -> ValueMatch { ValueMatch::ContainedIn(HashSet::from([value])) }

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
        let new_value = match (old_lhs, rhs) {
            (ValueMatch::ContainedIn(mut lhs), ValueMatch::ContainedIn(mut rhs)) => {
                // Preserve the larger set when merging
                if rhs.len() > lhs.len() {
                    std::mem::swap(&mut rhs, &mut lhs);
                }
                lhs.extend(rhs.into_iter());
                ValueMatch::ContainedIn(lhs)
            }
            _ => ValueMatch::All,
        };
        *self = new_value;
    }
}

#[derive(Clone, Debug, Default)]
struct KeyValueMatch {
    keys: HashMap<String, ValueMatch>,
}

impl KeyValueMatch {
    pub fn add(&mut self, key: String, value: ValueMatch) { *self.keys.entry(key).or_default() |= value; }

    pub fn matches(&self, key: &str, value: &str) -> bool {
        match self.keys.get(key) {
            None => false,
            Some(value_match) => value_match.matches(value),
        }
    }
}

fn parse_new_comment_args<S, I>(comments: I, escaped: bool) -> Result<DiscreteCommentList, Error>
where
    S: AsRef<str>,
    I: IntoIterator<Item = S>,
{
    let comments = comments.into_iter();
    let mut result = DiscreteCommentList::with_capacity(comments.size_hint().0);
    for comment in comments {
        let comment = comment.as_ref();
        let (key, value) = parse_comment(comment)?;
        let value = if escaped { escaping::unescape_str(value)? } else { Cow::from(value) };
        result.push(key, &value)?;
    }
    Ok(result)
}

/// Try to protect user against passing a media file as a tags file
fn validate_comment_filename(path: &Path) -> Result<(), AppError> {
    if let Some(ext) = path.extension() {
        let mut ext = ext.to_string_lossy().to_string();
        ext.make_ascii_lowercase();
        if OGG_OPUS_EXTENSIONS.iter().any(|e| ext == *e) {
            eprintln!(
                "Based on file extension {:?} looks like it might be an Opus file. Refusing to use it for tags.",
                path
            );
            return Err(AppError::SilentExit);
        }
    }
    Ok(())
}

fn parse_delete_comment_args<S, I>(patterns: I, escaped: bool) -> Result<KeyValueMatch, Error>
where
    S: AsRef<str>,
    I: IntoIterator<Item = S>,
{
    let patterns = patterns.into_iter();
    let mut result = KeyValueMatch::default();
    for pattern_string in patterns {
        let pattern_string = pattern_string.as_ref();
        let (key, value) = match parse_comment(pattern_string) {
            Ok((key, value)) => {
                let value = if escaped { escaping::unescape_str(value)? } else { Cow::from(value) };
                (key, Some(value))
            }
            Err(_) => match validate_comment_field_name(pattern_string) {
                Ok(()) => (pattern_string, None),
                Err(e) => return Err(e),
            },
        };
        let rhs = match value {
            None => ValueMatch::All,
            Some(value) => ValueMatch::singleton(value.to_string()),
        };
        result.add(key.to_string(), rhs);
    }
    Ok(result)
}

fn read_comments_from_read<R, M, E>(read: R, escaped: bool, error_map: M) -> Result<DiscreteCommentList, E>
where
    R: Read,
    M: Fn(io::Error) -> E,
    E: From<Error>,
{
    let read = BufReader::new(read);
    let mut result = DiscreteCommentList::default();
    for line in read.lines() {
        let line = line.map_err(&error_map)?;
        if line.trim().is_empty() {
            continue;
        }
        let (key, value) = parse_comment(&line)?;
        let value = if escaped { escaping::unescape_str(value).map_err(|e| e.into())? } else { Cow::from(value) };
        result.push(key, &value)?;
    }
    Ok(result)
}

fn read_comments_from_file<P: AsRef<Path>>(path: P, escaped: bool) -> Result<DiscreteCommentList, Error> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|e| Error::FileOpenError(path.to_path_buf(), e))?;
    let error_map = |e| Error::FileReadError(path.to_path_buf(), e);
    read_comments_from_read(file, escaped, error_map)
}

fn read_comments_from_stdin(escaped: bool) -> Result<DiscreteCommentList, AppError> {
    let stdin = io::stdin();
    let error_map = AppError::StandardInputReadError;
    read_comments_from_read(stdin, escaped, error_map)
}

fn main_impl() -> Result<(), AppError> {
    let interrupt_checker = CtrlCChecker::new()?;
    let cli = Cli::parse_from(wild::args_os());
    let operation_mode = match (cli.list, cli.modify, cli.replace) {
        (_, false, false) => OperationMode::List,
        (false, true, false) => OperationMode::Modify,
        (false, false, true) => OperationMode::Replace,
        _ => {
            eprintln!("Invalid combination of modes passed");
            return Err(AppError::SilentExit);
        }
    };

    for comment_file in [&cli.tags_in, &cli.tags_out].iter().copied().flatten() {
        validate_comment_filename(comment_file)?;
    }

    let dry_run = cli.dry_run;
    let escape = cli.escapes;
    let delete_tags = parse_delete_comment_args(cli.delete, escape)?;
    let append = {
        let mut append = parse_new_comment_args(cli.tags, escape)?;
        if let Some(ref file) = cli.tags_in {
            let mut tags = if file == std::ffi::OsStr::new(STANDARD_STREAM_NAME) {
                read_comments_from_stdin(escape)?
            } else {
                read_comments_from_file(file, escape)?
            };
            append.append(&mut tags);
        }
        append
    };

    let action = match operation_mode {
        OperationMode::List => CommentRewriterAction::NoChange,
        OperationMode::Modify => {
            let retain: Box<dyn Fn(&str, &str) -> bool> = Box::new(|k, v| !delete_tags.matches(k, v));
            CommentRewriterAction::Modify { retain, append }
        }
        OperationMode::Replace => CommentRewriterAction::Replace(append),
    };

    let rewriter_config = CommentRewriterConfig { action };
    let input_path = cli.input_file;
    let input_file = File::open(&input_path).map_err(|e| Error::FileOpenError(input_path.to_path_buf(), e))?;
    let mut input_file = BufReader::new(input_file);

    let mut output_file = match operation_mode {
        OperationMode::List => OutputFile::new_sink(),
        OperationMode::Modify | OperationMode::Replace => {
            let output_path = cli.output_file.unwrap_or_else(|| input_path.to_path_buf());
            OutputFile::new_target_or_discard(&output_path, dry_run)?
        }
    };

    let rewrite_result = {
        let output_file = output_file.as_write();
        let mut output_file = BufWriter::new(output_file);
        let rewrite = CommentHeaderRewrite::new(rewriter_config);
        let abort_on_unchanged = true;
        rewrite_stream_with_interrupt(
            rewrite,
            &mut input_file,
            &mut output_file,
            abort_on_unchanged,
            &interrupt_checker,
        )
    };
    drop(input_file); // Important for Windows

    match rewrite_result {
        Err(e) => {
            eprintln!("Failure during processing of {}.", input_path.display());
            return Err(e.into());
        }
        Ok(SubmitResult::Good) => {
            // We finished processing the file but never got the headers
            eprintln!("File {} appeared to be oddly truncated. Doing nothing.", input_path.display());
        }
        Ok(SubmitResult::HeadersUnchanged(comments)) => {
            if let OperationMode::List = operation_mode {
                if let Some(ref path) = cli.tags_out && path != std::ffi::OsStr::new(STANDARD_STREAM_NAME) {
                    let mut comment_file = OutputFile::new_target_or_discard(path, dry_run)?;
                    {
                        let mut comment_file = BufWriter::new(comment_file.as_write());
                        comments
                            .write_as_text(&mut comment_file, escape)
                            .map_err(|e| Error::FileWriteError(path.into(), e))?;
                        comment_file.flush().map_err(|e| Error::FileWriteError(path.into(), e))?;
                    }
                    comment_file.commit()?;
                } else {
                    comments.write_as_text(io::stdout(), escape).map_err(Error::ConsoleIoError)?;
                }
            }
        }
        Ok(SubmitResult::HeadersChanged { .. }) => {
            output_file.commit()?;
        }
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::error::ErrorKind;

    use super::*;

    #[test]
    fn cli_modes_conflict() {
        let result = Cli::try_parse_from(["opuscomment", "--replace", "--list", "input.ogg"]);
        assert_eq!(result.unwrap_err().kind(), ErrorKind::ArgumentConflict);

        let result = Cli::try_parse_from(["opuscomment", "--replace", "--modify", "input.ogg"]);
        assert_eq!(result.unwrap_err().kind(), ErrorKind::ArgumentConflict);

        let result = Cli::try_parse_from(["opuscomment", "--modify", "--list", "input.ogg"]);
        assert_eq!(result.unwrap_err().kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn cli_list_mode() {
        let result = Cli::try_parse_from(["opuscomment", "--list", "input.ogg"]);
        assert!(result.is_ok());

        let result = Cli::try_parse_from(["opuscomment", "--list", "input.ogg", "output.ogg"]);
        assert_eq!(result.unwrap_err().kind(), ErrorKind::ArgumentConflict);

        let result = Cli::try_parse_from(["opuscomment", "--list", "-O", "output.tags", "input.ogg"]);
        assert!(result.is_ok());

        let result = Cli::try_parse_from(["opuscomment", "--list", "-I", "input.tags", "input.ogg"]);
        assert_eq!(result.unwrap_err().kind(), ErrorKind::ArgumentConflict);

        let result = Cli::try_parse_from(["opuscomment", "--list", "-d", "TAG=VALUE", "input.ogg"]);
        assert_eq!(result.unwrap_err().kind(), ErrorKind::ArgumentConflict);

        let result = Cli::try_parse_from(["opuscomment", "--list", "-t", "TAG=VALUE", "input.ogg"]);
        assert_eq!(result.unwrap_err().kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn cli_modify_mode() {
        let result = Cli::try_parse_from(["opuscomment", "--modify", "input.ogg"]);
        assert!(result.is_ok());

        let result = Cli::try_parse_from(["opuscomment", "--modify", "-I", "input.tags", "input.ogg"]);
        assert!(result.is_ok());

        let result = Cli::try_parse_from(["opuscomment", "--modify", "-I", "input.tags", "input.ogg", "output.ogg"]);
        assert!(result.is_ok());

        let result = Cli::try_parse_from(["opuscomment", "--modify", "-O", "output.tags", "input.ogg"]);
        assert_eq!(result.unwrap_err().kind(), ErrorKind::ArgumentConflict);

        let result = Cli::try_parse_from([
            "opuscomment",
            "--modify",
            "-I",
            "input.tags",
            "-d",
            "TAG=VALUE",
            "-t",
            "TAG2=VALUE2",
            "input.ogg",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_replace_mode() {
        let result = Cli::try_parse_from(["opuscomment", "--replace", "input.ogg", "output.ogg"]);
        assert!(result.is_ok());

        let result =
            Cli::try_parse_from(["opuscomment", "--replace", "-I", "input.tags", "-t", "TAG=VALUE", "input.ogg"]);
        assert!(result.is_ok());

        let result = Cli::try_parse_from(["opuscomment", "--replace", "-O", "output.tags", "input.ogg"]);
        assert_eq!(result.unwrap_err().kind(), ErrorKind::ArgumentConflict);

        let result = Cli::try_parse_from(["opuscomment", "--replace", "-d", "TAG=VALUE", "input.ogg"]);
        assert_eq!(result.unwrap_err().kind(), ErrorKind::ArgumentConflict);
    }
}
