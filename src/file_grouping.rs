use std::collections::{HashSet, VecDeque};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use clap::ValueEnum;

use crate::Error;

/// How a list of paths is grouped into albums and singles.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum PathsProcessingMode {
    /// All paths are to singles. Folder paths are invalid.
    #[clap(name = "files-singles")]
    FileListSingles,
    /// All paths are to files in the same album. Folder paths are invalid.
    #[clap(name = "files-album")]
    FileListAlbum,
    /// File paths are considered singles. Folders are considered albums and
    /// recursively traversed to find album tracks.
    #[clap(name = "folders-are-albums")]
    FoldersAreAlbums,
}

impl PathsProcessingMode {
    fn build_grouper(&self) -> Box<dyn FileGrouper> {
        match *self {
            PathsProcessingMode::FileListSingles => Box::new(FileList::new(false)),
            PathsProcessingMode::FileListAlbum => Box::new(FileList::new(true)),
            PathsProcessingMode::FoldersAreAlbums => Box::new(FoldersAreAlbums::default()),
        }
    }
}

/// Represents a file group of either singles or an album
#[derive(Clone, Debug)]
pub enum FileGroup {
    /// Each file is a single
    Singles(Vec<PathBuf>),
    /// A collections of files that should be treated as an album
    Album(Vec<PathBuf>),
}

impl FileGroup {
    pub fn is_album(&self) -> bool { matches!(self, FileGroup::Album(_)) }

    pub fn get_file_paths(&self) -> Vec<PathBuf> {
        match self {
            FileGroup::Singles(files) | FileGroup::Album(files) => files.clone(),
        }
    }
}

/// Errors that can occur when producing groups of albums and singles.
#[derive(Clone, Debug, thiserror::Error)]
pub enum FileGroupingError {
    /// A folder was supplied when doing so is invalid.
    #[error("File grouper did not expect a folder: {0}")]
    UnexpectedFolder(PathBuf),
}

/// Trait for visitors of a list of files / folders.
trait TreeVisitor {
    type Error;

    /// Called when about to enter a folder. Returning `false` causes the folder
    /// not to be entered.
    fn enter_folder(&mut self, path: &Path) -> Result<bool, Self::Error>;
    /// Called when visiting a file.
    fn file(&mut self, path: &Path) -> Result<(), Self::Error>;
    /// Called when exiting a folder.
    fn exit_folder(&mut self, path: &Path) -> Result<(), Self::Error>;
}

/// A trait for types capable of visiting a filesystem hierarchy and producing a
/// list of albums and singles.
trait FileGrouper: TreeVisitor<Error = FileGroupingError> {
    /// Returns paths of files grouped into albums and singles
    fn groups(&self) -> Vec<FileGroup>;
}

/// All paths should be files, which are either singles or part of a single
/// album.
#[derive(Clone, Debug, Default)]
struct FileList {
    files: Vec<PathBuf>,
    is_album: bool,
}

impl FileList {
    /// Constructs a new `FileList`, taking whether the files should be
    /// considered part of a single album.
    pub fn new(is_album: bool) -> FileList { FileList { is_album, files: Vec::new() } }
}

impl TreeVisitor for FileList {
    type Error = FileGroupingError;

    fn enter_folder(&mut self, path: &Path) -> Result<bool, Self::Error> {
        Err(FileGroupingError::UnexpectedFolder(path.to_path_buf()))
    }

    fn file(&mut self, path: &Path) -> Result<(), Self::Error> {
        self.files.push(path.to_path_buf());
        Ok(())
    }

    fn exit_folder(&mut self, path: &Path) -> Result<(), Self::Error> {
        Err(FileGroupingError::UnexpectedFolder(path.to_path_buf()))
    }
}

impl FileGrouper for FileList {
    fn groups(&self) -> Vec<FileGroup> {
        if self.files.is_empty() {
            Vec::new()
        } else if self.is_album {
            vec![FileGroup::Album(self.files.clone())]
        } else {
            vec![FileGroup::Singles(self.files.clone())]
        }
    }
}

/// Supplied files are considered singles. Files in each supplied folder,
/// included nested subfolders, are considered part of an album.
#[derive(Clone, Debug, Default)]
struct FoldersAreAlbums {
    depth: usize,
    singles: Vec<PathBuf>,
    albums: Vec<Vec<PathBuf>>,
    current_album: Option<Vec<PathBuf>>,
}

impl TreeVisitor for FoldersAreAlbums {
    type Error = FileGroupingError;

    fn enter_folder(&mut self, _path: &Path) -> Result<bool, Self::Error> {
        if self.depth == 0 {
            assert!(self.current_album.is_none(), "There should be no album at depth 0");
            self.current_album = Some(Vec::new());
        }
        self.depth += 1;
        Ok(true)
    }

    fn file(&mut self, path: &Path) -> Result<(), Self::Error> {
        let path = path.to_path_buf();
        if self.depth == 0 {
            self.singles.push(path);
        } else {
            self.current_album.as_mut().expect("Missing album at depth > 0").push(path);
        }
        Ok(())
    }

    fn exit_folder(&mut self, _path: &Path) -> Result<(), Self::Error> {
        if self.depth == 1 {
            let current_album = self.current_album.take().expect("There should be an album when exiting a folder");
            if !current_album.is_empty() {
                self.albums.push(current_album);
            }
        }
        self.depth -= 1;
        Ok(())
    }
}

impl FileGrouper for FoldersAreAlbums {
    fn groups(&self) -> Vec<FileGroup> {
        let mut result = Vec::new();
        if !self.singles.is_empty() {
            result.push(FileGroup::Singles(self.singles.clone()));
        }
        result.extend(self.albums.iter().cloned().map(FileGroup::Album));
        result
    }
}

pub fn paths_to_file_groups<I, P>(
    input_paths: I, processing_mode: PathsProcessingMode, file_extensions: &HashSet<OsString>,
) -> Result<Vec<FileGroup>, Error>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    let mut grouper: Box<_> = processing_mode.build_grouper();
    let mut path_stack: Vec<(Option<PathBuf>, VecDeque<PathBuf>)> =
        vec![(None, input_paths.into_iter().map(|p| p.as_ref().to_path_buf()).collect())];

    while !path_stack.is_empty() {
        // All entries in the stack are folder contents except the top-level which is
        // the list of files/folders supplied.
        if let Some(path) = path_stack.last_mut().expect("Path stack unexpectedly empty").1.pop_front() {
            let metadata = path.symlink_metadata().map_err(|e| Error::FileOpenError(path.to_path_buf(), e))?;
            if metadata.is_file() {
                if path.extension().map(|ext| file_extensions.contains(ext)).unwrap_or(false) {
                    grouper.file(&path)?;
                }
            } else if metadata.is_dir() {
                let entries: Result<VecDeque<PathBuf>, _> = path
                    .read_dir()
                    .map_err(|e| Error::FileOpenError(path.to_path_buf(), e))?
                    .map(|entry| entry.map(|entry| entry.path()))
                    .collect();
                let entries = entries.map_err(|e| Error::FileOpenError(path.to_path_buf(), e))?;
                grouper.enter_folder(&path)?;
                path_stack.push((Some(path), entries));
            }
        } else {
            let item = path_stack.pop().expect("Path stack unexpectedly empty");
            if let (Some(folder), _) = item {
                grouper.exit_folder(&folder)?;
            }
        }
    }

    Ok(grouper.groups())
}
