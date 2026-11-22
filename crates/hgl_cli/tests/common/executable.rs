//! Installs disposable executables for the CLI's unit and integration tests.

use std::fs;
use std::io::{self, Read};
use std::path::Path;

/// Publishes a complete executable fixture at `path`. Opening the final
/// path for writing can race with a process that executes it. Stage the
/// bytes and permissions next to it, close the writer, then rename.
pub(crate) fn publish_executable(path: &Path, mut bytes: impl Read, permissions: fs::Permissions) {
    let parent = path.parent().unwrap();
    fs::create_dir_all(parent).unwrap();
    let mut staged = tempfile::NamedTempFile::new_in(parent).unwrap();
    io::copy(&mut bytes, staged.as_file_mut()).unwrap();
    staged.as_file().set_permissions(permissions).unwrap();
    // Keeping only the path closes the staging file before publication.
    let staged_path = staged.into_temp_path();
    staged_path.persist(path).unwrap();
}
