use std::fs;
use std::io;
use std::path::Path;

pub fn move_file(src: &Path, dest_dir: &Path) -> io::Result<()> {
    let Some(file_name) = src.file_name() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("could not determine filename for {}", src.display()),
        ));
    };

    fs::create_dir_all(dest_dir)?;

    let dest = dest_dir.join(file_name);

    // Try rename first (same filesystem); fall back to copy+delete for cross-device moves.
    if fs::rename(src, &dest).is_err() {
        match fs::copy(src, &dest) {
            Ok(_) => {
                fs::remove_file(src)?;
            }
            Err(e) => {
                return Err(e);
            }
        }
    }

    Ok(())
}
