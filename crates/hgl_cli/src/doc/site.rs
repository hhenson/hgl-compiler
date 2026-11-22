//! Publishes an HTML reference in an owned directory (SDOC-10).

use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

use hgl_doc::SiteFile;
use tempfile::{NamedTempFile, tempdir_in};

const MARKER: &str = ".hgl-doc";
const MAGIC: &str = "hgl doc HTML site version 1\n";
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
    for path in &next {
        if output_exists(directory, path)? && !previous.contains(path) {
            return Err(io::Error::new(
                ErrorKind::AlreadyExists,
                format!("cannot replace unrelated documentation file {path}"),
            ));
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
    // Keep even an interrupted publication's staging files outside the site.
    let staging_parent = directory.parent().ok_or_else(|| {
        io::Error::other("cannot stage a documentation site at a filesystem root")
    })?;
    let staging = tempdir_in(staging_parent)?;
    for file in &files {
        let staged = staging.path().join(&file.path);
        fs::create_dir_all(parent(&staged))?;
        fs::write(staged, &file.contents)?;
        fs::create_dir_all(parent(&directory.join(&file.path)))?;
    }

    // A failed publication can be retried. Both generations remain known
    // until every stale file is removed and every replacement is installed.
    write_manifest(directory, previous.union(&next))?;
    for path in stale {
        fs::remove_file(directory.join(path))?;
    }
    // Keep the generator's order, with the entry page published last.
    for file in files {
        fs::rename(staging.path().join(&file.path), directory.join(&file.path))?;
    }
    write_manifest(directory, &next)
}

/// Claims only an empty directory. An existing marker must be the exact
/// format this command writes before it can be used as the lock.
fn lock_site(directory: &Path) -> io::Result<File> {
    if fs::symlink_metadata(directory)?.file_type().is_symlink() {
        return Err(io::Error::other("documentation output is a symbolic link"));
    }
    let path = directory.join(MARKER);
    if !output_exists(directory, MARKER)? {
        if fs::read_dir(directory)?.next().transpose()?.is_some()
            && !output_exists(directory, MARKER)?
        {
            return Err(io::Error::new(
                ErrorKind::AlreadyExists,
                "documentation output is neither empty nor an hgl doc site",
            ));
        }
        let mut marker = NamedTempFile::new_in(directory)?;
        marker.write_all(MAGIC.as_bytes())?;
        if let Err(error) = marker.persist_noclobber(&path)
            && error.error.kind() != ErrorKind::AlreadyExists
        {
            return Err(error.error);
        }
    }
    output_exists(directory, MARKER)?;
    let mut lock = OpenOptions::new().read(true).write(true).open(path)?;
    lock.lock()?;
    let mut contents = String::new();
    (&mut lock)
        .take((MAGIC.len() + 1) as u64)
        .read_to_string(&mut contents)?;
    if contents != MAGIC {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "unrecognized hgl doc site marker",
        ));
    }
    Ok(lock)
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

fn parent(path: &Path) -> &Path {
    path.parent()
        .expect("a staged or published file has a parent")
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
