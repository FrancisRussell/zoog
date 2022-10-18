#[path = "../output_file.rs"]
mod output_file;

use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use std::path::PathBuf;

use clap::Parser;
use output_file::OutputFile;
use zoog::comment_rewriter::{CommentHeaderRewrite, CommentRewriterAction, CommentRewriterConfig};
use zoog::header_rewriter::{rewrite_stream, SubmitResult};
use zoog::opus::{parse_comment, CommentList, DiscreteCommentList};
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

    #[clap(short, long, action)]
    /// Replace comments in the Ogg Opus file
    write: bool,

    #[clap(short = 't', long = "tag", id = "TAG")]
    /// Specify a tag
    tags: Vec<String>,

    /// Input file
    input_file: PathBuf,
}

#[derive(Clone, Copy, Debug)]
enum OperationMode {
    List,
    Append,
    Replace,
}

fn comments_to_list<S: AsRef<str>, I: IntoIterator<Item = S>>(comments: I) -> Result<DiscreteCommentList, Error> {
    let comments = comments.into_iter();
    let mut result = DiscreteCommentList::with_capacity(comments.size_hint().0);
    for comment in comments {
        let comment = comment.as_ref();
        let (key, value) = parse_comment(comment)?;
        result.append(&key, &value)?;
    }
    Ok(result)
}

fn main_impl() -> Result<(), Error> {
    let cli = Cli::parse_from(wild::args_os());
    let operation_mode = match (cli.list, cli.append, cli.write) {
        (_, false, false) => OperationMode::List,
        (false, true, false) => OperationMode::Append,
        (false, false, true) => OperationMode::Replace,
        _ => {
            //FIXME: Replace me with an error
            panic!("Conflicting options supplied for mode of operation");
        }
    };

    let tags = comments_to_list(cli.tags)?;
    println!("Operating in mode: {:?} (tags={:?})", operation_mode, tags);

    let action = match operation_mode {
        OperationMode::List => CommentRewriterAction::NoChange,
        OperationMode::Append => todo!("Append not yet implemented"),
        OperationMode::Replace => CommentRewriterAction::Replace(tags),
    };

    let rewriter_config = CommentRewriterConfig { action };
    let input_path = cli.input_file;
    let input_file = File::open(&input_path).map_err(|e| Error::FileOpenError(input_path.to_path_buf(), e))?;
    let mut input_file = BufReader::new(input_file);

    let mut output_file = match operation_mode {
        OperationMode::List => OutputFile::new_sink(),
        OperationMode::Append | OperationMode::Replace => OutputFile::new_target(&input_path)?,
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
        Ok(SubmitResult::HeadersUnchanged(comments)) | Ok(SubmitResult::HeadersChanged { to: comments, .. }) => {
            match operation_mode {
                OperationMode::List => comments.write_as_text(io::stdout()).map_err(Error::ConsoleIoError)?,
                _ => {
                    output_file.commit()?;
                }
            }
        }
    };
    Ok(())
}
