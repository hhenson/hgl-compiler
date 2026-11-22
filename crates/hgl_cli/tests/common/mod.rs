//! Helpers shared by the test files of this directory, which run the
//! built `hgl` program the way a user would.
//!
//! Cargo compiles each file directly under `tests/` as a program of its
//! own. A file in a subdirectory is not one; the test files pull this one
//! in with `mod common;`.

// Each test file uses some of these helpers and not others; without this,
// every unused one would be a warning in that file's build.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Runs `hgl` with `arguments` and waits for it to finish.
pub fn hgl(arguments: &[&str]) -> Output {
    hgl_with(arguments, &[])
}

/// Runs `hgl` with `arguments` and with each of `environment`, a name
/// and a value, set in its environment.
pub fn hgl_with(arguments: &[&str], environment: &[(&str, &str)]) -> Output {
    // Cargo builds the binary before the test and puts its path in this
    // environment variable at compile time.
    Command::new(env!("CARGO_BIN_EXE_hgl"))
        .args(arguments)
        .envs(environment.iter().copied())
        .output()
        .expect("the hgl binary runs")
}

/// A stream's bytes as text.
pub fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("hgl writes UTF-8")
}

/// An empty directory for the test named `test`, in a directory named
/// after the test file: `check/listed` for the test of `check.rs` that
/// asks for `listed`.
pub fn scratch(test: &str) -> PathBuf {
    // Cargo gives integration tests a scratch directory under `target/`,
    // and names the program it compiles from `tests/check.rs` `check`.
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(env!("CARGO_CRATE_NAME"))
        .join(test);
    // Left over from an earlier run, perhaps; `ok()` ignores "not there".
    fs::remove_dir_all(&directory).ok();
    fs::create_dir_all(&directory).unwrap();
    directory
}

/// Writes `text` to `directory/name`, making the directories on the way,
/// and gives the path as `hgl` will print it. `name` is written with `/`
/// between its parts on every system.
pub fn write(directory: &Path, name: &str, text: &str) -> String {
    // Joined a part at a time, so that the path has the system's own
    // separator throughout: `hgl` prints paths as the system gives them.
    let mut path = directory.to_path_buf();
    for part in name.split('/') {
        path.push(part);
    }
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, text).unwrap();
    path.display().to_string()
}

/// Publishes a complete executable fixture at `path`. Opening the final
/// path for writing can race with a process that executes it. Stage the
/// bytes and permissions next to it, close the writer, then rename.
#[cfg(unix)]
pub fn publish_executable(
    path: &Path,
    mut bytes: impl std::io::Read,
    permissions: fs::Permissions,
) {
    let parent = path.parent().unwrap();
    fs::create_dir_all(parent).unwrap();
    let mut staged = tempfile::NamedTempFile::new_in(parent).unwrap();
    std::io::copy(&mut bytes, staged.as_file_mut()).unwrap();
    staged.as_file().set_permissions(permissions).unwrap();
    staged.into_temp_path().persist(path).unwrap();
}

/// A path as an argument.
pub fn argument(path: &Path) -> String {
    path.display().to_string()
}

/// The path of a file in `tests/fixtures`.
pub fn fixture(name: &str) -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    path.display().to_string()
}
