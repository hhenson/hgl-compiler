//! Builds the C++ runtime library's sources into the crate.
//!
//! The generated code links `libhglrt`, this repository's C++ runtime
//! library, whose CMake project is `runtime/cpp`. The build driver
//! configures and builds it once for each SDK, from the sources `hgl`
//! carries: this script writes `runtime_files.rs` into Cargo's output
//! directory, a list of every file under `runtime/cpp` with its bytes,
//! which `src/runtime.rs` includes. An installed `hgl` then needs no
//! checkout of the repository.
//!
//! `HGL_CPP_RUNTIME_SOURCE`, where it is set when `hgl` is built, names
//! another directory to take the library from: a checkout of it under
//! development, say. Where neither is there the list is empty, and the
//! driver says so when it is asked to build. Cargo is told to run this
//! script again when a file of the directory changes; while there is
//! none it watches only this script, since a path that is not there
//! would have the crate rebuilt on every build.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    // CARGO_MANIFEST_DIR is this crate's directory; the workspace root
    // is two levels up.
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("set by Cargo"));
    println!("cargo::rerun-if-env-changed=HGL_CPP_RUNTIME_SOURCE");
    let runtime = match env::var_os("HGL_CPP_RUNTIME_SOURCE").filter(|named| !named.is_empty()) {
        Some(named) => PathBuf::from(named),
        None => manifest.join("../../runtime/cpp"),
    };
    let mut files = Vec::new();
    if runtime.join("CMakeLists.txt").is_file() {
        println!("cargo::rerun-if-changed={}", runtime.display());
        collect(&runtime, &runtime, &mut files);
    } else {
        println!("cargo::rerun-if-changed=build.rs");
    }
    files.sort();
    let mut text = String::from("&[\n");
    for (relative, path) in &files {
        // `{:?}` writes a Rust string literal, escapes included.
        writeln!(
            text,
            "    ({relative:?}, include_bytes!({:?})),",
            path.display().to_string()
        )
        .expect("writing to a string cannot fail");
    }
    text.push(']');
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("set by Cargo"));
    fs::write(out.join("runtime_files.rs"), text).expect("the output directory is writable");
}

/// Every file under `directory`, as its path relative to `root` with
/// `/` between its parts and its absolute path. A name that starts with
/// `.` is left out, and so is a directory that starts with `build`: an
/// editor's files and a build made in the source tree are no part of
/// the library.
fn collect(root: &Path, directory: &Path, files: &mut Vec<(String, PathBuf)>) {
    let entries = fs::read_dir(directory).expect("runtime/cpp is readable");
    for entry in entries {
        let entry = entry.expect("runtime/cpp is readable");
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            if !name.starts_with("build") {
                collect(root, &path, files);
            }
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .expect("under the root")
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let absolute = fs::canonicalize(&path).expect("the file is there");
        files.push((relative, absolute));
    }
}
