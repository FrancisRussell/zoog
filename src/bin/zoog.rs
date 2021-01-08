use clap::{App, Arg};
use ogg::reading::PacketReader;
use ogg::writing::PacketWriter;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use zoog::constants::{R128_LUFS, REPLAY_GAIN_LUFS};
use zoog::rewriter::{OperationMode, RewriteResult, Rewriter};
use zoog::ZoogError;

pub const VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");
pub const AUTHORS: Option<&'static str> = option_env!("CARGO_PKG_AUTHORS");
pub const DESCRIPTION: Option<&'static str> = option_env!("CARGO_PKG_DESCRIPTION");

fn get_version() -> String {
    VERSION.map(String::from).unwrap_or_else(|| String::from("Unknown version"))
}

fn get_authors() -> String {
    AUTHORS.map(String::from).unwrap_or_else(|| String::from("Unknown author"))
}

fn get_description() -> String {
    DESCRIPTION.map(String::from).unwrap_or_else(|| String::from("Missing description"))
}

fn main() {
    match main_impl() {
        Ok(()) => {}
        Err(e) => {
            eprintln!("Error was: {}", e);
            std::process::exit(1);
        }
    }
}

fn remove_file_verbose<P: AsRef<Path>>(path: P) {
    let path = path.as_ref();
    if let Err(e) = std::fs::remove_file(path) {
        eprintln!("Unable to delete {} due to error {}", path.to_string_lossy(), e);
    }
}

fn rename_file<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> Result<(), ZoogError> {
    std::fs::rename(from.as_ref(), to.as_ref()).map_err(|e| {
        ZoogError::FileCopy(PathBuf::from(from.as_ref()), PathBuf::from(to.as_ref()), e)
    })
}

fn main_impl() -> Result<(), ZoogError> {
    let matches = App::new("Zoog")
        .author(get_authors().as_str())
        .about(get_description().as_str())
        .version(get_version().as_str())
        .arg(Arg::with_name("preset")
            .long("preset")
            .possible_values(&["rg", "r128", "none"])
            .default_value("rg")
            .multiple(false)
            .help("Normalizes to loudness used by ReplayGain (rg), EBU R 128 (r128) or original (none)"))
        .arg(Arg::with_name("input_files")
            .multiple(true)
            .required(true)
            .help("The Opus files to process"))
        .get_matches();

    let mode = match matches.value_of("preset").unwrap() {
        "rg" => OperationMode::TargetLUFS(REPLAY_GAIN_LUFS),
        "r128" => OperationMode::TargetLUFS(R128_LUFS),
        "none" => OperationMode::ZeroOutputGain,
        p => panic!("Unknown preset: {}", p),
    };

    let mut num_processed: usize = 0;
    let mut num_already_normalized: usize = 0;
    let mut num_missing_r128: usize = 0;

    let input_files = matches.values_of("input_files").expect("No input files");
    for input_path in input_files {
        let input_path = PathBuf::from(input_path);
        println!("Processing file {:#?} with target loudness of {}...", input_path, mode.to_friendly_string());
        let input_file = File::open(&input_path).map_err(|e| ZoogError::FileOpenError(input_path.clone(), e))?;
        let input_file = BufReader::new(input_file);

        let input_dir = input_path.parent().expect("Unable to find parent folder of input file");
        let input_base = input_path.file_name().expect("Unable to find name of input file");
        let mut output_file = tempfile::Builder::new()
            .prefix(input_base)
            .suffix("zoog")
            .tempfile_in(input_dir)
            .map_err(ZoogError::TempFileOpenError)?;

        let rewrite_result = {
            let mut ogg_reader = PacketReader::new(input_file);
            let mut output_file = BufWriter::new(&mut output_file);
            let ogg_writer = PacketWriter::new(&mut output_file);
            let mut rewriter = Rewriter::new(mode, ogg_writer, true);
            loop {
                match ogg_reader.read_packet() {
                    Err(e) => break Err(ZoogError::OggDecode(e)),
                    Ok(None) => {
                        // Make sure to flush the buffered writer
                        break output_file.flush()
                            .map(|_| RewriteResult::Ready)
                            .map_err(ZoogError::WriteError);
                    }
                    Ok(Some(packet)) => {
                        let submit_result = rewriter.submit(packet);
                        match submit_result {
                            Ok(RewriteResult::Ready) => {}
                            _ => break submit_result,
                        }
                    }
                }
            }
        };

        num_processed += 1;
        match rewrite_result {
            Err(e) => {
                println!("Failure during processing of {:#?}.", input_path);
                return Err(e);
            }
            Ok(RewriteResult::Ready) => {
                let mut backup_path = input_path.clone();
                backup_path.set_extension("zoog-orig");
                rename_file(&input_path, &backup_path)?;
                output_file.persist_noclobber(&input_path)
                    .map_err(ZoogError::PersistError)
                    .and_then(|f| f.sync_all().map_err(ZoogError::WriteError))?;
                remove_file_verbose(&backup_path);
            }
            Ok(RewriteResult::NoR128Tags) => {
                println!("No R128 tags found in file so doing nothing.");
                num_missing_r128 += 1;
            }
            Ok(RewriteResult::AlreadyNormalized) => {
                println!("All gains are already correct so doing nothing.");
                num_already_normalized += 1;
            }
        }
        println!();
    }
    println!("Processing complete.");
    println!("Total files processed: {}", num_processed);
    println!("Files processed but already normalized: {}", num_already_normalized);
    println!("Files skipped due to missing R128 tags: {}", num_missing_r128);
    Ok(())
}
