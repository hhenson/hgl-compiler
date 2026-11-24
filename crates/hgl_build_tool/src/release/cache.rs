//! Keep the native libraries, relative links and notices together across builds.

use std::collections::BTreeMap;
use std::env;
use std::fmt::Write;
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::{Result, Target};

const NATIVE_DIRECTORIES: [&str; 5] = [
    "libexec/hgraph-sdk",
    "libexec/hgraph-deps",
    "licenses/fmt",
    "licenses/spdlog",
    "licenses/simdjson",
];
const DEBUG_DIRECTORY: &str = "libexec";
const DEBUG_CACHE: &str = "debug/libexec";

/// Hash the native recipe and the toolchain that built the cached libraries.
pub(crate) fn native_key(target: Target) -> Result<String> {
    let recipe = [
        "Cargo.lock",
        "Cargo.toml",
        "HGRAPH_TAG",
        "rust-toolchain.toml",
        "scripts/hgraph-dependencies.sh",
        "scripts/gather-dsyms.sh",
        "scripts/keep-lto-objects.sh",
        "scripts/provision-hgraph-sdk.sh",
        "scripts/package-release.sh",
        "crates/hgl_build_tool/Cargo.toml",
    ];
    let environment = ["CC", "CXX", "CFLAGS", "CXXFLAGS", "LDFLAGS"];
    let mut inputs = BTreeMap::from([("target".to_string(), target.triple().to_string())]);
    inputs.extend(
        environment
            .into_iter()
            .map(|key| (key.to_string(), env::var(key).unwrap_or_default())),
    );

    match target {
        Target::Macos => {
            inputs.extend(
                ["DEVELOPER_DIR", "SDKROOT", "MACOSX_DEPLOYMENT_TARGET"]
                    .into_iter()
                    .map(|key| (key.to_string(), env::var(key).unwrap_or_default())),
            );
            for (key, command) in [
                (
                    "compiler",
                    compiler("CXX", "/usr/bin/clang++").arg("--version"),
                ),
                (
                    "sdk",
                    Command::new("/usr/bin/xcrun").args(["--sdk", "macosx", "--show-sdk-version"]),
                ),
            ] {
                let value = probe(command).unwrap_or_else(|| "unavailable".to_string());
                inputs.insert(key.to_string(), value);
            }
        }
        Target::Linux => {
            // The Linux libraries and split debug symbols depend on the
            // system toolchain. A missing probe cannot identify their cache.
            for (key, command) in [
                ("cc", compiler("CC", "gcc-15").arg("--version")),
                ("compiler", compiler("CXX", "g++-15").arg("--version")),
                ("glibc", Command::new("getconf").arg("GNU_LIBC_VERSION")),
                ("cmake", Command::new("cmake").arg("--version")),
                ("ninja", Command::new("ninja").arg("--version")),
                ("ld", Command::new("ld").arg("--version")),
                ("objcopy", Command::new("objcopy").arg("--version")),
            ] {
                let value = probe(command).ok_or_else(|| format!("cannot fingerprint {key}"))?;
                inputs.insert(key.to_string(), value);
            }
        }
    }
    for path in recipe {
        inputs.insert(path.to_string(), sha256(fs::read(path)?));
    }
    // Shared command and audit modules also affect cache validation.
    for entry in WalkDir::new("crates/hgl_build_tool/src") {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type().is_file()
            && path.extension().is_some_and(|extension| extension == "rs")
        {
            let key = path.to_str().ok_or("non-UTF-8 build tool source path")?;
            inputs.insert(key.to_string(), sha256(fs::read(path)?));
        }
    }
    Ok(sha256(serde_json::to_vec(&inputs)?))
}

fn compiler(variable: &str, default: &str) -> Command {
    Command::new(
        env::var_os(variable)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| default.into()),
    )
}

fn probe(command: &mut Command) -> Option<String> {
    command
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|result| result.status.success())
        .and_then(|result| String::from_utf8(result.stdout).ok())
}

fn sha256(bytes: impl AsRef<[u8]>) -> String {
    let mut hex = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(hex, "{byte:02x}").expect("writing to a string succeeds");
    }
    hex
}

/// Restore native dependencies, retaining their symlinks and file modes.
pub(crate) fn copy_native(source: &Path, destination: &Path, debug: Option<&Path>) -> Result<()> {
    for directory in NATIVE_DIRECTORIES {
        copy_tree(&source.join(directory), &destination.join(directory), false)?;
    }
    if let Some(debug) = debug {
        copy_tree(
            &source.join(DEBUG_CACHE),
            &debug.join(DEBUG_DIRECTORY),
            true,
        )?;
    }
    Ok(())
}

/// Publish validated native contents by rename on the cache's filesystem.
pub(crate) fn save_native(bundle: &Path, cache: &Path, debug: Option<&Path>) -> Result<()> {
    let parent = cache
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let scratch = tempfile::Builder::new()
        .prefix(".hgl-native-")
        .tempdir_in(parent)?;
    let staged = scratch.path().join("native");
    copy_native(bundle, &staged, None)?;
    if let Some(debug) = debug {
        copy_tree(
            &debug.join(DEBUG_DIRECTORY),
            &staged.join(DEBUG_CACHE),
            false,
        )?;
    }
    if let Err(error) = fs::rename(&staged, cache) {
        // A competing build may have published the same verified tree.
        let complete = NATIVE_DIRECTORIES
            .iter()
            .all(|path| cache.join(path).is_dir())
            && (debug.is_none() || cache.join(DEBUG_CACHE).is_dir());
        if !matches!(
            error.kind(),
            io::ErrorKind::AlreadyExists | io::ErrorKind::DirectoryNotEmpty
        ) || !complete
        {
            return Err(error.into());
        }
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path, merge: bool) -> Result<()> {
    let metadata = fs::metadata(source)?;
    if merge {
        fs::create_dir_all(destination)?;
    } else {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir(destination)?;
    }
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let to = destination.join(entry.file_name());
        let from = entry.path();
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            copy_link(&from, &to)?;
        } else if kind.is_dir() {
            copy_tree(&from, &to, merge)?;
        } else if kind.is_file() {
            copy_file(&from, &to)?;
        } else {
            return Err(format!("cannot copy special file {}", from.display()).into());
        }
    }
    fs::set_permissions(destination, metadata.permissions())?;
    set_times(destination, &metadata, false)?;
    Ok(())
}

/// Copy ordinary contents and their timestamps, following any source link.
pub(super) fn copy_file(source: &Path, destination: &Path) -> Result<()> {
    let metadata = fs::metadata(source)?;
    fs::copy(source, destination)?;
    set_times(destination, &metadata, false)?;
    Ok(())
}

/// Copy an entire directory to a new path.
pub(super) fn copy_directory(source: &Path, destination: &Path) -> Result<()> {
    copy_tree(source, destination, false)
}

fn copy_link(source: &Path, destination: &Path) -> Result<()> {
    let target = fs::read_link(source)?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, destination)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::{FileTypeExt, symlink_dir, symlink_file};
        if fs::symlink_metadata(source)?.file_type().is_symlink_dir() {
            symlink_dir(target, destination)?;
        } else {
            symlink_file(target, destination)?;
        }
    }
    set_times(destination, &fs::symlink_metadata(source)?, true)?;
    Ok(())
}

fn set_times(path: &Path, metadata: &fs::Metadata, link: bool) -> io::Result<()> {
    let times = fs::FileTimes::new()
        .set_accessed(metadata.accessed()?)
        .set_modified(metadata.modified()?);
    if link {
        fs::set_times_nofollow(path, times)
    } else {
        fs::set_times(path, times)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn copies_and_reuses_complete_native_caches_with_debug_links() -> Result<()> {
        let root = tempfile::tempdir()?;
        let bundle = root.path().join("bundle");
        let debug = root.path().join("debug-input");
        for path in NATIVE_DIRECTORIES {
            fs::create_dir_all(bundle.join(path))?;
        }
        let sdk = bundle.join(NATIVE_DIRECTORIES[0]);
        fs::write(sdk.join("sdk.1"), "library")?;
        std::os::unix::fs::symlink("sdk.1", sdk.join("sdk"))?;
        fs::create_dir_all(debug.join(DEBUG_DIRECTORY))?;
        fs::write(debug.join(DEBUG_DIRECTORY).join("symbols"), "one")?;
        let cache = root.path().join("cache");
        save_native(&bundle, &cache, Some(&debug))?;
        // A second publisher leaves the existing verified cache unchanged.
        fs::write(debug.join(DEBUG_DIRECTORY).join("symbols"), "two")?;
        save_native(&bundle, &cache, Some(&debug))?;
        let restored = root.path().join("restored");
        let debug_restored = root.path().join("debug-output");
        fs::create_dir_all(debug_restored.join(DEBUG_DIRECTORY))?;
        copy_native(&cache, &restored, Some(&debug_restored))?;
        assert_eq!(
            fs::read_link(restored.join(NATIVE_DIRECTORIES[0]).join("sdk"))?,
            Path::new("sdk.1")
        );
        assert_eq!(
            fs::read_to_string(debug_restored.join(DEBUG_DIRECTORY).join("symbols"))?,
            "one"
        );
        Ok(())
    }
}
