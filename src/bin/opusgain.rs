use clap::{App, Arg};
use ogg::reading::PacketReader;
use ogg::writing::PacketWriter;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use zoog::constants::{R128_LUFS, REPLAY_GAIN_LUFS};
use zoog::rewriter::{OperationMode};
use zoog::ZoogError;
use zoog::VolumeAnalyzer;

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
        ZoogError::FileMove(PathBuf::from(from.as_ref()), PathBuf::from(to.as_ref()), e)
    })
}

fn apply_volume_analysis<P: AsRef<Path>>(analyzer: &mut VolumeAnalyzer, path: P) -> Result<(), ZoogError> {
    let input_path = path.as_ref();
    println!("Computing loudness of file {:#?}...", input_path);
    let input_file = File::open(&input_path).map_err(|e| ZoogError::FileOpenError(input_path.to_path_buf(), e))?;
    let input_file = BufReader::new(input_file);
    let mut ogg_reader = PacketReader::new(input_file);
    loop {
        match ogg_reader.read_packet() {
            Err(e) => return Err(ZoogError::OggDecode(e)),
            Ok(None) => {
                analyzer.file_complete();
                return Ok(());
            }
            Ok(Some(packet)) => analyzer.submit(packet)?,
        }
    }
}

fn compute_album_power<I: IntoIterator<Item=P>, P: AsRef<Path>>(paths: I) -> Result<f64, ZoogError> {
    let mut analyzer = VolumeAnalyzer::new(true);
    for input_path in paths.into_iter() {
        apply_volume_analysis(&mut analyzer, input_path.as_ref())?;
    }
    Ok(analyzer.mean_power())
}

fn main_impl() -> Result<(), ZoogError> {
    let matches = App::new("Opusgain")
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
        .arg(Arg::with_name("album")
            .long("album")
            .short("a")
            .takes_value(false)
            .help("Enable album mode")
        ).get_matches();

    let album_mode = matches.is_present("album");
    let mode = match matches.value_of("preset").unwrap() {
        "rg" => OperationMode::TargetLUFS(REPLAY_GAIN_LUFS),
        "r128" => OperationMode::TargetLUFS(R128_LUFS),
        "none" => OperationMode::ZeroOutputGain,
        p => panic!("Unknown preset: {}", p),
    };

    let mut num_processed: usize = 0;
    let mut num_already_normalized: usize = 0;
    let mut num_missing_r128: usize = 0;

    let input_files: Vec<_> = matches.values_of("input_files").expect("No input files").collect();
    let album_power = if album_mode {
        Some(compute_album_power(&input_files)?)
    } else {
        None
    };
    println!("Album power: {:?}", album_power);
    for input_path in input_files {
        let input_path = PathBuf::from(input_path);
        println!("Processing file {:#?} with target loudness of {}...", input_path, mode.to_friendly_string());
        let input_file = File::open(&input_path).map_err(|e| ZoogError::FileOpenError(input_path.clone(), e))?;
        let input_file = BufReader::new(input_file);

        let input_dir = input_path.parent().expect("Unable to find parent folder of input file");
        let input_base = input_path.file_name().expect("Unable to find name of input file");
        /*
        let mut output_file = tempfile::Builder::new()
            .prefix(input_base)
            .suffix("zoog")
            .tempfile_in(input_dir)
            .map_err(ZoogError::TempFileOpenError)?;
        */

        let rewrite_result = {
            let mut ogg_reader = PacketReader::new(input_file);
            let mut analyzer = VolumeAnalyzer::new(true);
            loop {
                match ogg_reader.read_packet() {
                    Err(e) => break Err(ZoogError::OggDecode(e)),
                    Ok(None) => {
                        analyzer.file_complete();
                        println!("Computed mean power: {} dB", analyzer.mean_power());
                        break Ok(())
                        // Make sure to flush the buffered writer
                        /*
                        break output_file.flush()
                            .map(|_| ())
                            .map_err(ZoogError::WriteError);
                        */
                    }
                    Ok(Some(packet)) => {
                        let submit_result = analyzer.submit(packet);
                        match submit_result {
                            Ok(_) => {}
                            _ => break submit_result,
                        }
                    }
                }
            }
        };

        num_processed += 1;
        println!();
    }
    println!("Processing complete.");
    println!("Total files processed: {}", num_processed);
    println!("Files processed but already normalized: {}", num_already_normalized);
    println!("Files skipped due to missing R128 tags: {}", num_missing_r128);
    Ok(())
}
