use std::ffi::{OsStr, OsString};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use tempfile::NamedTempFile;
use zoog::Error;

#[derive(Debug)]
enum FileEnum {
    Temp(tempfile::NamedTempFile, PathBuf),
    Sink(io::Sink),
}

#[derive(Debug)]
pub struct OutputFile {
    file_enum: FileEnum,
}

fn make_sibling_temporary_file(path: &Path, distinguisher: &OsStr) -> Result<NamedTempFile, Error> {
    let parent_dir = path.parent().ok_or_else(|| Error::NoParentError(path.to_path_buf()))?;
    let file_stem = path.file_stem().ok_or_else(|| Error::NotAFilePath(path.to_path_buf()))?;
    let file_ext = path.extension().map(|e| {
        let mut ext = OsString::from(".");
        ext.push(e);
        ext
    });
    let file_stem = {
        let mut stem = file_stem.to_os_string();
        stem.push("-");
        stem.push(distinguisher);
        stem
    };
    let mut builder = tempfile::Builder::new();
    builder.prefix(&file_stem);
    if let Some(file_ext) = file_ext.as_ref() {
        builder.suffix(file_ext);
    }
    let temp = builder.tempfile_in(parent_dir).map_err(|e| Error::TempFileOpenError(parent_dir.to_path_buf(), e))?;
    Ok(temp)
}

impl OutputFile {
    /// Creates a new output that discards all data written
    pub fn new_sink() -> OutputFile { OutputFile { file_enum: FileEnum::Sink(io::sink()) } }

    /// Writes to a temporary that replaces the specified path on `commit()`.
    pub fn new_target(path: &Path) -> Result<OutputFile, Error> {
        let temp = make_sibling_temporary_file(path, OsStr::new("new"))?;
        Ok(OutputFile { file_enum: FileEnum::Temp(temp, path.to_path_buf()) })
    }

    /// Writes to a temporary that replaces the specified path on `commit()` if
    /// `discard` is `false`. Otherwise discards all data written.
    pub fn new_target_or_discard(path: &Path, discard: bool) -> Result<OutputFile, Error> {
        if discard {
            Ok(Self::new_sink())
        } else {
            Self::new_target(path)
        }
    }

    /// Returns the underlying file as a `Write`.
    pub fn as_write(&mut self) -> &mut dyn Write {
        match self.file_enum {
            FileEnum::Temp(ref mut temp, _) => temp,
            FileEnum::Sink(ref mut sink) => sink,
        }
    }

    /// Deletes the underlying file.
    #[allow(dead_code)]
    pub fn abort(self) -> Result<(), Error> {
        match self.file_enum {
            FileEnum::Sink(_) => {}
            FileEnum::Temp(temp, _) => {
                let temp_path = temp.path().to_path_buf();
                temp.close().map_err(|e| Error::FileDelete(temp_path, e))?;
            }
        }
        Ok(())
    }

    /// Persists the file to the intended path.
    pub fn commit(self) -> Result<(), Error> {
        match self.file_enum {
            FileEnum::Sink(_) => {}
            FileEnum::Temp(temp, final_path) => {
                // How to write this code so that it minimizes the chance of
                // data loss is an open question.

                // Sync all data of the new file to disk
                temp.as_file().sync_all().map_err(Error::WriteError)?;

                // Persist the temporary to the final path
                temp.persist(final_path)
                    .map_err(Error::PersistError)
                    .and_then(|f| f.sync_all().map_err(Error::WriteError))?;
            }
        }
        Ok(())
    }
}
