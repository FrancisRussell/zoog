use std::path::PathBuf;

use clap::Parser;
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
}

#[derive(Clone, Copy, Debug)]
enum OperationMode {
    List,
    Append,
    Replace,
}

fn main_impl() -> Result<(), Error> {
    let cli = Cli::parse_from(wild::args_os());
    let operation_mode = match (cli.list, cli.append, cli.write) {
        (_, false, false) => OperationMode::List,
        (false, true, false) => OperationMode::Append,
        (false, false, true) => OperationMode::Replace,
        _ => {
            //TODO: Replace me with an error
            panic!("Conflicting options supplied for mode of operation");
        }
    };
    println!("Operating in mode: {:?}", operation_mode);
    Ok(())
}
