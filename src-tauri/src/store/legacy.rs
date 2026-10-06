use std::path::Path;

pub(super) fn remove_listing_files(directory: &Path) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("repos-") && name.ends_with(".json") && entry.path().is_file() {
            if let Err(error) = std::fs::remove_file(entry.path()) {
                eprintln!("Legacy repository cache not removed ({name}): {error}");
            }
        }
    }
}
