use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub(super) struct Output {
    directory: PathBuf,
    path: PathBuf,
}

impl Output {
    pub(super) fn new(destination: &Path) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let destination = std::path::absolute(destination).map_err(|e| e.to_string())?;
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        loop {
            let directory = parent.join(format!(
                ".still-exposure-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&directory) {
                Ok(()) => {
                    let path = directory
                        .join("image")
                        .with_extension(destination.extension().unwrap_or_default());
                    return Ok(Self { directory, path });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("Failed to stage {}: {e}", destination.display())),
            }
        }
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn persist(self, destination: &Path, replace: bool) -> Result<(), String> {
        if replace {
            fs::rename(&self.path, destination)
        } else {
            // Atomically create a destination only if it is still absent. Both
            // paths live on the same filesystem; failed work never replaces media.
            match fs::hard_link(&self.path, destination) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(e),
                // Removable media such as exFAT may not support hard links.
                // Exclusive creation still prevents overwriting another file.
                Err(_) => copy_new(&self.path, destination),
            }
        }
        .map_err(|e| format!("Failed to save {}: {e}", destination.display()))
    }
}

fn copy_new(source: &Path, destination: &Path) -> std::io::Result<()> {
    let mut source = fs::File::open(source)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    if let Err(error) = std::io::copy(&mut source, &mut output) {
        drop(output);
        let _ = fs::remove_file(destination);
        return Err(error);
    }
    Ok(())
}

impl Drop for Output {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}
