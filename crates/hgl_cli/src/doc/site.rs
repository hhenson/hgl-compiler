//! Publishes an HTML reference in an owned directory (SDOC-10).

use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

use hgl_doc::SiteFile;
use tempfile::NamedTempFile;

const MARKER: &str = ".hgl-doc-site-v2";
const LEGACY_MARKER: &str = ".hgl-doc";
const LEGACY_MAGIC: &str = "hgl doc HTML site version 1\n";
const MANIFEST: &str = ".hgl-doc-files";

/// Replaces generated files and removes stale outputs. The stable marker
/// holds the directory lock. The manifest records only this command's files.
pub(super) fn write(directory: &Path, files: Vec<SiteFile>) -> io::Result<()> {
    fs::create_dir_all(directory)?;
    let _lock = lock_site(directory)?;
    let directory = fs::canonicalize(directory)?;
    let directory = directory.as_path();

    let previous = manifest(directory)?;
    let next = files
        .iter()
        .map(|file| output_name(&file.path))
        .collect::<io::Result<BTreeSet<_>>>()?;
    let mut replacing = BTreeSet::new();
    for path in &next {
        if output_exists(directory, path)? {
            if !previous.contains(path) {
                return Err(io::Error::new(
                    ErrorKind::AlreadyExists,
                    format!("cannot replace unrelated documentation file {path}"),
                ));
            }
            replacing.insert(path.as_str());
        }
    }

    let stale = previous
        .difference(&next)
        .filter_map(|path| match output_exists(directory, path) {
            Ok(true) => Some(Ok(path)),
            Ok(false) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<io::Result<Vec<_>>>()?;
    for file in &files {
        let path = directory.join(&file.path);
        fs::create_dir_all(path.parent().expect("a site file has a parent"))?;
    }

    // A failed publication can be retried. Both generations remain known
    // until every stale file is removed and every replacement is installed.
    write_manifest(directory, previous.union(&next))?;
    for path in stale {
        fs::remove_file(directory.join(path))?;
    }
    // The contents are already rendered. Write each file at its destination,
    // with the entry page last and no staged pages left by an interruption.
    for file in files {
        let path = directory.join(&file.path);
        if replacing.contains(file.path.as_str()) {
            fs::remove_file(&path)?;
        }
        // Exclusive creation never truncates an existing hard link or file.
        File::create_new(path)?.write_all(&file.contents)?;
    }
    write_manifest(directory, &next)
}

/// Claims an empty directory with an empty, versioned lock file. The
/// first creation makes the marker immediately valid for every writer.
fn lock_site(directory: &Path) -> io::Result<File> {
    if fs::symlink_metadata(directory)?.file_type().is_symlink() {
        return Err(io::Error::other("documentation output is a symbolic link"));
    }
    let marker = match existing_marker(directory)? {
        Some(marker) => marker,
        None if fs::read_dir(directory)?.next().transpose()?.is_some() => {
            // Another writer may have claimed the directory after the
            // first lookup. Its marker appears before any generated file.
            existing_marker(directory)?.ok_or_else(|| {
                io::Error::new(
                    ErrorKind::AlreadyExists,
                    "documentation output is neither empty nor an hgl doc site",
                )
            })?
        }
        None => {
            if let Err(error) = File::create_new(directory.join(MARKER))
                && error.kind() != ErrorKind::AlreadyExists
            {
                return Err(error);
            }
            MARKER
        }
    };
    output_exists(directory, marker)?;
    let mut lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(directory.join(marker))?;
    lock.lock()?;
    let valid = if marker == LEGACY_MARKER {
        let mut contents = String::new();
        (&mut lock)
            .take((LEGACY_MAGIC.len() + 1) as u64)
            .read_to_string(&mut contents)?;
        contents == LEGACY_MAGIC
    } else {
        lock.metadata()?.len() == 0
    };
    if !valid {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "unrecognized hgl doc site marker",
        ));
    }
    Ok(lock)
}

fn existing_marker(directory: &Path) -> io::Result<Option<&'static str>> {
    for marker in [LEGACY_MARKER, MARKER] {
        if output_exists(directory, marker)? {
            return Ok(Some(marker));
        }
    }
    Ok(None)
}

fn manifest(directory: &Path) -> io::Result<BTreeSet<String>> {
    if !output_exists(directory, MANIFEST)? {
        return Ok(BTreeSet::new());
    }
    fs::read_to_string(directory.join(MANIFEST))?
        .lines()
        .map(output_name)
        .collect()
}

/// Records output names with platform-independent separators.
fn write_manifest<'a>(
    directory: &Path,
    paths: impl IntoIterator<Item = &'a String>,
) -> io::Result<()> {
    let mut manifest = NamedTempFile::new_in(directory)?;
    for path in paths {
        writeln!(manifest, "{path}")?;
    }
    manifest
        .persist(directory.join(MANIFEST))
        .map_err(|error| error.error)?;
    Ok(())
}

/// Validates names from the manifest against the files a site can own.
fn output_name(path: &str) -> io::Result<String> {
    let parts: Vec<_> = path.split('/').collect();
    let generated = match parts.as_slice() {
        ["index.html" | "site.css" | "search-index.js" | "search.js" | "renderers.js"] => true,
        ["modules", page] => page.starts_with("mod-") && page.ends_with(".html"),
        ["vendor", "katex", "katex.min.js" | "LICENSE"] => true,
        [
            "vendor",
            "mermaid",
            "mermaid.min.js" | "LICENSE" | "THIRD-PARTY-NOTICES.txt",
        ] => true,
        _ => false,
    };
    if !generated || path.contains(['\\', ':', '\0', '\r', '\n']) {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "invalid file name in the hgl doc site",
        ));
    }
    Ok(path.to_owned())
}

/// Uses no symbolic links to find a generated file or its parent directories.
fn output_exists(directory: &Path, path: &str) -> io::Result<bool> {
    let mut resolved = PathBuf::from(directory);
    let mut parts = path.split('/').peekable();
    while let Some(part) = parts.next() {
        resolved.push(part);
        match fs::symlink_metadata(&resolved) {
            Ok(metadata) if parts.peek().is_some() && metadata.is_dir() => {}
            Ok(metadata) if parts.peek().is_none() && metadata.is_file() => return Ok(true),
            Ok(_) => {
                return Err(io::Error::other(format!(
                    "unexpected file type in documentation output at {}",
                    resolved.display()
                )));
            }
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        }
    }
    unreachable!("a file path has at least one component")
}
