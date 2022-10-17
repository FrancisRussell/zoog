use std::fs::File;
use std::io::{self, BufReader, BufWriter, Write};
use std::path::PathBuf;

use clap::Parser;
use zoog::comment_rewriter::{CommentHeaderRewrite, CommentRewriterAction, CommentRewriterConfig};
use zoog::header_rewriter::{rewrite_stream, SubmitResult};
use zoog::opus::CommentList;
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
    /// Replace commentsin the  Ogg Opus file
    write: bool,

    /// Input file
    input_file: PathBuf,

    /// Output file
    output_file: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug)]
enum OperationMode {
    List,
    Append,
    Replace,
}

#[derive(Debug)]
enum OutputFile {
    Temp(tempfile::NamedTempFile),
    Sink(io::Sink),
}

impl OutputFile {
    fn as_write(&mut self) -> &mut dyn Write {
        match self {
            OutputFile::Temp(ref mut temp) => temp,
            OutputFile::Sink(ref mut sink) => sink,
        }
    }
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

    println!("Operating in mode: {:?}", operation_mode);

    let mut output_file = match operation_mode {
        OperationMode::List => OutputFile::Sink(io::sink()),
        OperationMode::Append | OperationMode::Replace => todo!("Append and replace not yet implemented"),
    };
    let action = match operation_mode {
        OperationMode::List => CommentRewriterAction::NoChange,
        OperationMode::Append | OperationMode::Replace => todo!("Append and replace not yet implemented"),
    };

    let rewriter_config = CommentRewriterConfig { action };

    let input_path = cli.input_file;
    let input_file = File::open(&input_path).map_err(|e| Error::FileOpenError(input_path.to_path_buf(), e))?;
    let mut input_file = BufReader::new(input_file);

    let rewrite_result = {
        let output_file = output_file.as_write();
        let mut output_file = BufWriter::new(output_file);
        let rewrite = CommentHeaderRewrite::new(rewriter_config);
        let abort_on_unchanged = true;
        rewrite_stream(rewrite, &mut input_file, &mut output_file, abort_on_unchanged)
    };
    match rewrite_result {
        Err(e) => {
            eprintln!("Failure during processing of {}.", input_path.display());
            return Err(e);
        }
        Ok(SubmitResult::Good) => {
            // We finished processing the file but never got the headers
            eprintln!("File {} appeared to be oddly truncated. Doing nothing.", input_path.display());
        }
        Ok(SubmitResult::HeadersUnchanged(comments)) => match operation_mode {
            OperationMode::List => {
                comments.write_as_text(io::stdout()).map_err(Error::ConsoleIoError)?;
            }
            _ => todo!("Headers unchanged for non-list operation"),
        },
        Ok(SubmitResult::HeadersChanged { .. }) => todo!("Headers changed"),
    };
    Ok(())
}
