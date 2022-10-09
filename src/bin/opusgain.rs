#[path = "../console_output.rs"]
mod console_output;

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Seek, Write};
use std::path::{Path, PathBuf};

use clap::{Parser, ValueEnum};
use console_output::{ConsoleOutput, DelayedConsoleOutput, Standard};
use ogg::reading::PacketReader;
use ogg::writing::PacketWriter;
use parking_lot::Mutex;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use rayon::ThreadPoolBuilder;
use zoog::opus::{TAG_ALBUM_GAIN, TAG_TRACK_GAIN};
use zoog::rewriter::{OpusGains, OutputGainMode, Rewriter, RewriterConfig, SubmitResult, VolumeTarget};
use zoog::volume_analyzer::VolumeAnalyzer;
use zoog::{Decibels, Error, R128_LUFS, REPLAY_GAIN_LUFS};

fn main() {
    match main_impl() {
        Ok(()) => {}
        Err(e) => {
            eprintln!("Aborted due to error: {}", e);
            std::process::exit(1);
        }
    }
}

fn remove_file_verbose<P: AsRef<Path>>(path: P) {
    let path = path.as_ref();
    if let Err(e) = std::fs::remove_file(path) {
        eprintln!("Unable to delete {} due to error {}", path.display(), e);
    }
}

fn rename_file<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> Result<(), Error> {
    std::fs::rename(from.as_ref(), to.as_ref())
        .map_err(|e| Error::FileMove(PathBuf::from(from.as_ref()), PathBuf::from(to.as_ref()), e))
}

fn apply_volume_analysis<P, C>(
    analyzer: &mut VolumeAnalyzer, path: P, console_output: C, report_error: bool,
) -> Result<(), Error>
where
    P: AsRef<Path>,
    C: ConsoleOutput,
{
    let mut body = || {
        let input_path = path.as_ref();
        let input_file = File::open(input_path).map_err(|e| Error::FileOpenError(input_path.to_path_buf(), e))?;
        let input_file = BufReader::new(input_file);
        let mut ogg_reader = PacketReader::new(input_file);
        loop {
            match ogg_reader.read_packet() {
                Err(e) => break Err(Error::OggDecode(e)),
                Ok(None) => {
                    analyzer.file_complete();
                    writeln!(
                        console_output.out(),
                        "Computed loudness of {} as {:.2} LUFS (ignoring output gain)",
                        input_path.display(),
                        analyzer.last_track_lufs().expect("Last track volume unexpectedly missing").as_f64()
                    )
                    .map_err(Error::ConsoleIoError)?;
                    break Ok(());
                }
                Ok(Some(packet)) => analyzer.submit(packet)?,
            }
        }
    };
    let result = body();
    if report_error {
        if let Err(ref e) = result {
            writeln!(console_output.err(), "Failed to analyze volume of {}: {}", path.as_ref().display(), e)
                .map_err(Error::ConsoleIoError)?;
        }
    }
    result
}

fn print_gains<C: ConsoleOutput>(gains: &OpusGains, console: C) -> Result<(), Error> {
    let do_io = || {
        writeln!(console.out(), "\tOutput Gain: {}", gains.output)?;
        if let Some(gain) = gains.track_r128 {
            writeln!(console.out(), "\t{}: {}", TAG_TRACK_GAIN, gain)?;
        }
        if let Some(gain) = gains.album_r128 {
            writeln!(console.out(), "\t{}: {}", TAG_ALBUM_GAIN, gain)?;
        }
        Ok(())
    };
    do_io().map_err(Error::ConsoleIoError)
}

#[derive(Debug)]
struct AlbumVolume {
    mean: Decibels,
    tracks: HashMap<PathBuf, Decibels>,
}

impl AlbumVolume {
    pub fn get_album_mean(&self) -> Decibels { self.mean }

    pub fn get_track_mean(&self, path: &Path) -> Option<Decibels> { self.tracks.get(path).cloned() }
}

fn compute_album_volume<I, P, C>(paths: I, console_output: C) -> Result<AlbumVolume, Error>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
    P: Sync,
    C: ConsoleOutput + Clone + Sync,
{
    let console_output = &console_output;
    let paths: Vec<_> = paths.into_iter().enumerate().collect();
    let tracks = Mutex::new(HashMap::new());

    // This is a BTreeMap so we process the analyzers in the supplied order
    let analyzers = Mutex::new(BTreeMap::new());

    paths.into_par_iter().panic_fuse().try_for_each(|(idx, input_path)| -> Result<(), Error> {
        let mut analyzer = VolumeAnalyzer::default();
        apply_volume_analysis(
            &mut analyzer,
            input_path.as_ref(),
            &DelayedConsoleOutput::new(console_output.clone()),
            true,
        )?;
        tracks.lock().insert(
            input_path.as_ref().to_path_buf(),
            analyzer.last_track_lufs().expect("Track volume unexpectedly missing"),
        );
        analyzers.lock().insert(idx, analyzer);
        Ok(())
    })?;

    let analyzers = analyzers.into_inner();
    let analyzers: Vec<_> = analyzers.into_values().collect();
    let tracks = tracks.into_inner();
    let mean = VolumeAnalyzer::mean_lufs_across_multiple(analyzers.iter());
    let album_volume = AlbumVolume { tracks, mean };
    Ok(album_volume)
}

fn rewrite_stream<R: Read + Seek, W: Write>(
    input: R, mut output: W, config: &RewriterConfig,
) -> Result<SubmitResult, Error> {
    let mut ogg_reader = PacketReader::new(input);
    let ogg_writer = PacketWriter::new(&mut output);
    let mut rewriter = Rewriter::new(config, ogg_writer);
    let mut result = SubmitResult::Good;
    loop {
        match ogg_reader.read_packet() {
            Err(e) => break Err(Error::OggDecode(e)),
            Ok(None) => {
                // Make sure to flush any buffered data
                break output.flush().map(|_| result).map_err(Error::WriteError);
            }
            Ok(Some(packet)) => {
                let submit_result = rewriter.submit(packet);
                match submit_result {
                    Ok(SubmitResult::Good) => {
                        // We can continue submitting packets
                    }
                    Ok(r @ SubmitResult::ChangingGains { .. }) => {
                        // We can continue submitting packets, but want to save the changed
                        // gains to return as a result
                        result = r;
                    }
                    Ok(SubmitResult::AlreadyNormalized(_)) | Err(_) => {
                        // If we are already normalized or encounter an error, we want to
                        // abort immediately
                        break submit_result;
                    }
                }
            }
        }
    }
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum Preset {
    #[clap(name = "rg")]
    ReplayGain,
    #[clap(name = "r128")]
    R128,
    #[clap(name = "original")]
    ZeroGain,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum OutputGainSetting {
    Auto,
    Track,
}

#[derive(Debug, Parser)]
#[clap(author, version, about)]
struct Cli {
    #[clap(short, long, action)]
    /// Enable album mode
    album: bool,

    #[clap(value_enum, short, long, default_value_t = Preset::ReplayGain)]
    /// Normalizes to loudness used by ReplayGain (rg), EBU R 128 (r128) or
    /// the original source (original)
    preset: Preset,

    #[clap(value_enum, short, long, default_value_t = OutputGainSetting::Auto)]
    /// When "auto" is specified, each track's output gain is chosen to be
    /// per-track or per-album dependent on whether album mode is enabled.
    /// When "track" is specified, each file's output gain will be
    /// track-specific, even in album mode.
    output_gain_mode: OutputGainSetting,

    #[clap(required(true))]
    /// The Opus files to process
    input_files: Vec<PathBuf>,

    #[clap(short, long, action)]
    /// Display output without performing any file modification.
    display_only: bool,

    #[clap(short='j', long, default_value_t = num_cpus::get())]
    /// Number of threads to use for processing. Default is the number of cores
    /// on the system.
    num_threads: usize,
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
    let album_mode = cli.album;
    let num_threads = if cli.num_threads == 0 {
        eprintln!("The number of thread specified must be greater than 0.");
        Err(Error::InvalidThreadCount)
    } else {
        let num_cores = num_cpus::get();
        let rounded = std::cmp::min(cli.num_threads, num_cores);
        if rounded != cli.num_threads {
            eprintln!("Rounding down number of threads from {} to {}.", cli.num_threads, num_cores);
        }
        Ok(rounded)
    }?;
    ThreadPoolBuilder::new().num_threads(num_threads).build_global().expect("Failed to initialize thread pool");

    let output_gain_mode = match cli.output_gain_mode {
        OutputGainSetting::Auto => match album_mode {
            true => OutputGainMode::Album,
            false => OutputGainMode::Track,
        },
        OutputGainSetting::Track => OutputGainMode::Track,
    };
    let volume_target = match cli.preset {
        Preset::ReplayGain => VolumeTarget::LUFS(REPLAY_GAIN_LUFS),
        Preset::R128 => VolumeTarget::LUFS(R128_LUFS),
        Preset::ZeroGain => VolumeTarget::ZeroGain,
    };

    let num_processed = Mutex::new(0);
    let num_already_normalized = Mutex::new(0);

    let display_only = cli.display_only;
    if display_only {
        println!("Display-only mode is enabled so no files will actually be modified.\n");
    }

    let console_output = Standard::default();
    let input_files = cli.input_files;
    let album_volume = if album_mode { Some(compute_album_volume(&input_files, &console_output)?) } else { None };

    // Prevent us from rewriting more than one file at once. This is to stop us
    // consuming too much disk space or leaving lots of temporary files around
    // if we encounter an error.
    let rewrite_mutex = Mutex::new(());

    input_files.into_par_iter().panic_fuse().try_for_each(|input_path| -> Result<(), Error> {
        let console = &DelayedConsoleOutput::new(&console_output);
        let body = || {
            writeln!(
                console.out(),
                "Processing file {} with target loudness of {}...",
                &input_path.display(),
                volume_target.to_friendly_string()
            )
            .map_err(Error::ConsoleIoError)?;
            let track_volume = match &album_volume {
                None => {
                    let mut analyzer = VolumeAnalyzer::default();
                    apply_volume_analysis(&mut analyzer, &input_path, console, false)?;
                    analyzer.last_track_lufs().expect("Last track volume unexpectedly missing")
                }
                Some(album_volume) => {
                    album_volume.get_track_mean(&input_path).expect("Could not find previously computed track volume")
                }
            };
            let rewriter_config = RewriterConfig {
                output_gain: volume_target,
                output_gain_mode,
                track_volume,
                album_volume: album_volume.as_ref().map(|a| a.get_album_mean()),
            };

            let input_dir = input_path.parent().expect("Unable to find parent folder of input file");
            let input_base = input_path.file_name().expect("Unable to find name of input file");
            let input_file = File::open(&input_path).map_err(|e| Error::FileOpenError(input_path.to_path_buf(), e))?;
            let mut input_file = BufReader::new(input_file);

            {
                let rewrite_guard = rewrite_mutex.lock();
                let mut output_file = if display_only {
                    OutputFile::Sink(io::sink())
                } else {
                    let temp = tempfile::Builder::new()
                        .prefix(input_base)
                        .suffix("zoog")
                        .tempfile_in(input_dir)
                        .map_err(Error::TempFileOpenError)?;
                    OutputFile::Temp(temp)
                };
                let rewrite_result = {
                    let output_file = output_file.as_write();
                    let mut output_file = BufWriter::new(output_file);
                    rewrite_stream(&mut input_file, &mut output_file, &rewriter_config)
                };
                *num_processed.lock() += 1;

                match rewrite_result {
                    Err(e) => {
                        writeln!(console.err(), "Failure during processing of {}.", input_path.display())
                            .map_err(Error::ConsoleIoError)?;
                        return Err(e);
                    }
                    Ok(SubmitResult::Good) => {
                        // Either we should already be normalized or get back a result which
                        // indicated we changed the gains in the input file. If we get neither
                        // then something weird happened.
                        writeln!(
                            console.err(),
                            "File {} appeared to be oddly truncated. Doing nothing.",
                            input_path.display(),
                        )
                        .map_err(Error::ConsoleIoError)?;
                    }
                    Ok(SubmitResult::ChangingGains { from: old_gains, to: new_gains }) => {
                        match output_file {
                            OutputFile::Temp(output_file) => {
                                let mut backup_path = input_path.clone();
                                backup_path.set_extension("zoog-orig");
                                rename_file(&input_path, &backup_path)?;
                                output_file
                                    .persist_noclobber(&input_path)
                                    .map_err(Error::PersistError)
                                    .and_then(|f| f.sync_all().map_err(Error::WriteError))?;
                                remove_file_verbose(&backup_path);
                            }
                            OutputFile::Sink(_) => {}
                        }
                        writeln!(console.out(), "Old gain values:").map_err(Error::ConsoleIoError)?;
                        print_gains(&old_gains, console)?;
                        writeln!(console.out(), "New gain values:").map_err(Error::ConsoleIoError)?;
                        print_gains(&new_gains, console)?;
                    }
                    Ok(SubmitResult::AlreadyNormalized(gains)) => {
                        writeln!(console.out(), "All gains are already correct so doing nothing. Existing gains were:")
                            .map_err(Error::ConsoleIoError)?;
                        print_gains(&gains, console)?;
                        *num_already_normalized.lock() += 1;
                    }
                }
                drop(rewrite_guard);
            }
            Ok(())
        };
        let result = body();
        if let Err(ref e) = result {
            writeln!(console.err(), "Failed to rewrite {}: {}", input_path.display(), e)
                .map_err(Error::ConsoleIoError)?;
        }
        writeln!(console.out()).map_err(Error::ConsoleIoError)?;
        result
    })?;

    let num_processed = num_processed.into_inner();
    let num_already_normalized = num_already_normalized.into_inner();
    println!("Processing complete.");
    println!("Total files processed: {}", num_processed);
    println!("Files processed but already normalized: {}", num_already_normalized);
    Ok(())
}
