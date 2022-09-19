use clap::{App, Arg};
use ogg::reading::PacketReader;
use ogg::writing::PacketWriter;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, Write};
use std::path::{Path, PathBuf};
use zoog::constants::{R128_LUFS, REPLAY_GAIN_LUFS};
use zoog::rewriter::{RewriteResult, Rewriter, RewriterConfig, VolumeTarget};
use zoog::VolumeAnalyzer;
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

#[derive(Debug)]
struct AlbumVolume {
    mean: f64,
    tracks: HashMap<PathBuf, f64>,
}

impl AlbumVolume {
    pub fn get_album_mean(&self) -> f64 {
        self.mean
    }

    pub fn get_track_mean(&self, path: &Path) -> Option<f64> {
        self.tracks.get(path).cloned()
    }
}

fn compute_album_volume<I: IntoIterator<Item=P>, P: AsRef<Path>>(paths: I) -> Result<AlbumVolume, ZoogError> {
    let mut analyzer = VolumeAnalyzer::new();
    let mut tracks = HashMap::new();
    for input_path in paths.into_iter() {
        apply_volume_analysis(&mut analyzer, input_path.as_ref())?;
        tracks.insert(
            input_path.as_ref().to_path_buf(),
            analyzer.last_track_lufs().expect("Track volume unexpectedly missing")
        );
    }
    let album_volume = AlbumVolume {
        tracks,
        mean: analyzer.mean_lufs(),
    };
    Ok(album_volume)
}

fn rewrite_stream<R: Read + Seek, W: Write>(input: R, mut output: W, config: &RewriterConfig) -> Result<RewriteResult, ZoogError> {
    let mut ogg_reader = PacketReader::new(input);
    let ogg_writer = PacketWriter::new(&mut output);
    let mut rewriter = Rewriter::new(config, ogg_writer, true);
    let result = loop {
        match ogg_reader.read_packet() {
            Err(e) => break Err(ZoogError::OggDecode(e)),
            Ok(None) => {
                // Make sure to flush any buffered data
                break output.flush()
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
    };
    result
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
        "rg" => VolumeTarget::LUFS(REPLAY_GAIN_LUFS),
        "r128" => VolumeTarget::LUFS(R128_LUFS),
        "none" => VolumeTarget::ZeroGain,
        p => panic!("Unknown preset: {}", p),
    };

    let mut num_processed: usize = 0;
    let mut num_already_normalized: usize = 0;

    let input_files: Vec<_> = matches.values_of("input_files").expect("No input files").collect();
    let album_volume = if album_mode {
        Some(compute_album_volume(&input_files)?)
    } else {
        None
    };
    for input_path in input_files {
        let input_path = PathBuf::from(input_path);
        println!("Processing file {:#?} with target loudness of {}...", &input_path, mode.to_friendly_string());
        let track_volume = match &album_volume {
            None => {
                let mut analyzer = VolumeAnalyzer::new();
                apply_volume_analysis(&mut analyzer, &input_path)?;
                analyzer.last_track_lufs().expect("Last track volume unexpectedly missing")
            },
            Some(album_volume) => {
                album_volume.get_track_mean(&input_path).expect("Could not find previously computed track volume")
            },
        };
        let rewriter_config = RewriterConfig::new(
            mode,
            track_volume,
            album_volume.as_ref().map(|a| a.get_album_mean())
        );

        let input_dir = input_path.parent().expect("Unable to find parent folder of input file");
        let input_base = input_path.file_name().expect("Unable to find name of input file");
        let input_file = File::open(&input_path).map_err(|e| ZoogError::FileOpenError(input_path.to_path_buf(), e))?;
        let mut input_file = BufReader::new(input_file);

        let mut output_file = tempfile::Builder::new()
            .prefix(input_base)
            .suffix("zoog")
            .tempfile_in(input_dir)
            .map_err(ZoogError::TempFileOpenError)?;
        let rewrite_result = {
            let mut output_file = BufWriter::new(&mut output_file);
            rewrite_stream(&mut input_file, &mut output_file, &rewriter_config)
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
    Ok(())
}
