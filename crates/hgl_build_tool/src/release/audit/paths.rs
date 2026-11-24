//! Check that packaged files and symlinks can be relocated together.

use std::collections::BTreeSet;
use std::fmt::Write;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

use regex::bytes::Regex;
use walkdir::WalkDir;

use crate::{Result, output};

const METADATA: [&str; 9] = [
    "cmake", "conf", "ini", "json", "pc", "toml", "txt", "yaml", "yml",
];
const SYSTEM_PATHS: [&str; 10] = [
    "/System/Library",
    "/usr/bin",
    "/usr/include",
    "/usr/lib",
    "/usr/libexec",
    "/usr/sbin",
    "/usr/share",
    "/bin",
    "/dev",
    "/sbin",
];
static SCRATCH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?-u:(?:file://)?(?:(?:/private)?/var/folders/|/(?:Users|home|root)/|/tmp/hgl-release\.)[^\s\x00]+)")
        .expect("absolute build path pattern")
});
static ABSOLUTE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?-u:(?:file://)?(/[A-Za-z0-9._~-]+(?:/[^\s"'\[\]{}()>;\x00,]*)*))"#)
        .expect("absolute metadata path pattern")
});
static ELF_DEBUG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?-u:\.(?:z)?debug_)").expect("ELF debug section pattern"));

/// Resolve links in the existing part of a path, allowing a missing tail.
pub(super) fn resolved(path: &Path) -> io::Result<PathBuf> {
    fn follow(path: &Path, links: usize) -> io::Result<PathBuf> {
        let mut resolved = PathBuf::new();
        for component in path.components() {
            match component {
                Component::ParentDir => {
                    resolved.pop();
                }
                Component::CurDir => {}
                Component::Normal(name) => {
                    resolved.push(name);
                    if let Ok(target) = fs::read_link(&resolved) {
                        if links == 40 {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "too many symlinks",
                            ));
                        }
                        resolved.pop();
                        resolved = follow(&resolved.join(target), links + 1)?;
                    }
                }
                other => resolved.push(other.as_os_str()),
            }
        }
        Ok(resolved)
    }

    let path = std::path::absolute(path)?;
    follow(&path, 0)
}

/// Read ELF debug sections after decompression, as the debugger does.
pub(super) fn debug_bytes(path: &Path) -> Result<Vec<u8>> {
    let bytes = fs::read(path)?;
    if !bytes.starts_with(b"\x7fELF") || !ELF_DEBUG.is_match(&bytes) {
        return Ok(bytes);
    }
    let scratch = tempfile::tempdir()?;
    let plain = scratch.path().join("plain");
    output(
        Command::new("objcopy")
            .arg("--decompress-debug-sections")
            .arg(path)
            .arg(&plain),
    )?;
    Ok(fs::read(plain)?)
}

fn scratch_start(data: &[u8], found: regex::bytes::Match<'_>) -> bool {
    // The string may be a URL path, which has no filesystem significance.
    found.as_bytes().starts_with(b"file://")
        || found.start().checked_sub(1).is_none_or(|index| {
            let byte = data[index];
            !byte.is_ascii_alphanumeric() && !b"_./~%-".contains(&byte)
        })
}

fn scratch_path(data: &[u8]) -> Option<regex::bytes::Match<'_>> {
    let mut next = 0;
    while let Some(found) = SCRATCH.find_at(data, next) {
        if scratch_start(data, found) {
            return Some(found);
        }
        // A rejected match may contain a path with its own boundary.
        next = found.start() + 1;
    }
    None
}

fn forbidden_prefixes(paths: &[PathBuf]) -> Result<Option<Regex>> {
    let mut prefixes = BTreeSet::new();
    for path in paths {
        for candidate in [path.clone(), resolved(path)?] {
            let prefix = candidate.as_os_str().as_encoded_bytes();
            if !prefix.is_empty() && prefix != b"/" {
                let mut bytes = String::new();
                for byte in prefix {
                    write!(bytes, "\\x{byte:02x}").expect("writing to a string succeeds");
                }
                prefixes.insert(bytes);
            }
        }
    }
    if prefixes.is_empty() {
        return Ok(None);
    }
    let alternatives = prefixes.into_iter().collect::<Vec<_>>().join("|");
    Ok(Some(Regex::new(&format!(
        r"(?-u:({alternatives})(?:[^\w.~-]|$))"
    ))?))
}

fn metadata_paths(bundle: &Path, data: &[u8]) -> Result<bool> {
    let mut next = 0;
    while let Some(captured) = ABSOLUTE.captures_at(data, next) {
        let found = captured.get(0).expect("whole match");
        let start = found.start();
        // Match the delimiting characters used for paths in release metadata.
        if start > 0 && !b"=:\"';, (\t\n".contains(&data[start - 1]) {
            next = start + 1;
            continue;
        }
        next = found.end();
        let bytes = captured.get(1).expect("absolute path").as_bytes();
        #[cfg(unix)]
        let path = {
            use std::os::unix::ffi::OsStrExt;
            resolved(Path::new(std::ffi::OsStr::from_bytes(bytes)))?
        };
        #[cfg(not(unix))]
        let path = resolved(Path::new(std::str::from_utf8(bytes)?))?;
        if !path.starts_with(bundle) && !SYSTEM_PATHS.iter().any(|root| path.starts_with(root)) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Reject external links, build paths and external paths in release metadata.
pub(super) fn verify_paths(
    bundle: &Path,
    forbidden: &[PathBuf],
    debug_sources: Option<&Path>,
) -> Result<()> {
    let forbidden = forbidden_prefixes(forbidden)?;
    let mut failures = Vec::new();
    for entry in WalkDir::new(bundle).min_depth(1).sort_by_file_name() {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(bundle)?;
        if entry.file_type().is_symlink() {
            if fs::read_link(path)?.is_absolute() || !path.canonicalize()?.starts_with(bundle) {
                failures.push(format!("link escapes the bundle: {}", relative.display()));
            }
        } else if entry.file_type().is_file() {
            let data = debug_bytes(path)?;
            let found = scratch_path(&data).or_else(|| forbidden.as_ref()?.captures(&data)?.get(1));
            if let Some(found) = found {
                let bytes = found.as_bytes();
                failures.push(format!(
                    "build path in {}: {}",
                    relative.display(),
                    String::from_utf8_lossy(&bytes[..bytes.len().min(120)]),
                ));
            }
            // A source CMakeLists.txt has compiler options such as /bigobj.
            // Its paths are audited above, but it is not installed metadata.
            if debug_sources.is_none_or(|source| !relative.starts_with(source))
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| METADATA.contains(&extension))
                && !metadata_paths(bundle, &data)?
            {
                failures.push(format!("external path in {}", relative.display()));
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n").into())
    }
}

#[cfg(test)]
mod tests;
