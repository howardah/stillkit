use std::{
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

/// Write a fully encoded preview beside its destination and only replace the
/// destination after the write succeeds.
pub(super) fn write_atomically(output: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = write_temporary(output, bytes)?;
    let result = replace_output(&temporary, output);
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub(super) fn write_temporary(output: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let name = output.file_name().unwrap_or_default();
    for _ in 0..32 {
        let suffix = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
        let mut temporary_name = OsString::from(".");
        temporary_name.push(name);
        temporary_name.push(format!(".still-{}-{suffix}.tmp", std::process::id()));
        let temporary = parent.join(temporary_name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(mut file) => {
                if let Err(error) = file.write_all(bytes) {
                    let _ = fs::remove_file(&temporary);
                    return Err(format!(
                        "Failed to write temporary preview beside {}: {error}",
                        output.display()
                    ));
                }
                return Ok(temporary);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "Failed to create temporary preview beside {}: {error}",
                    output.display()
                ));
            }
        }
    }
    Err(format!(
        "Could not reserve a temporary preview path beside {}",
        output.display()
    ))
}

pub(super) fn replace_output(temporary: &Path, output: &Path) -> Result<(), String> {
    match fs::rename(temporary, output) {
        Ok(()) => Ok(()),
        // Windows does not replace an existing destination with rename. The
        // temporary replacement is complete and verified, but the required
        // delete/rename fallback has a small non-atomic gap.
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            fs::remove_file(output).map_err(|remove_error| {
                format!(
                    "Failed to replace output file {}: {remove_error}",
                    output.display()
                )
            })?;
            fs::rename(temporary, output).map_err(|rename_error| {
                format!(
                    "Failed to place output file {}: {rename_error}",
                    output.display()
                )
            })
        }
        Err(error) => Err(format!(
            "Failed to place output file {}: {error}",
            output.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_an_existing_output_after_writing_the_replacement() {
        let directory = std::env::temp_dir().join(format!(
            "stillkit-staged-write-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        let output = directory.join("preview.jpg");
        fs::write(&output, b"previous preview").unwrap();

        write_atomically(&output, b"replacement preview").unwrap();

        assert_eq!(fs::read(&output).unwrap(), b"replacement preview");
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_dir_all(directory).unwrap();
    }
}
