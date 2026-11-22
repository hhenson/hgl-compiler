//! Validates `hgl test` path selectors against the supplied source roots.
//! Returns source names as the runner and native build inventory see them.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use crate::{EXIT_FAILED, discover_sources};

/// The program's complete source list and the names selected from it.
pub(super) struct SelectedSources {
    /// Each source path, once, in the same order discovery found it.
    pub(super) paths: Vec<PathBuf>,
    /// The selected sources' display names as cases will carry them.
    pub(super) names: HashSet<String>,
}

/// Resolves a union of files and directories. None means there was no
/// path filter and ordinary source discovery will supply the program.
pub(super) fn sources(
    roots: &[PathBuf],
    selectors: &[PathBuf],
) -> Result<Option<SelectedSources>, ExitCode> {
    if selectors.is_empty() {
        return Ok(None);
    }
    let source_roots: Vec<_> = roots
        .iter()
        .filter_map(|root| Place::new(root).ok())
        .collect();
    let selected = selectors
        .iter()
        .map(|path| {
            let selected = Place::new(path)
                .map_err(|error| failure(path, &format!("cannot read the path: {error}")))?;
            if !selected.file && !selected.directory {
                return Err(failure(path, "expected a file or directory"));
            }
            if !source_roots
                .iter()
                .any(|root| root.includes(Some(&selected.absolute), &selected.canonical))
            {
                return Err(failure(path, "the path is outside the source roots"));
            }
            Ok(selected)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut names = HashSet::new();
    let paths = discover_sources(roots, |written, canonical, source_name| {
        let absolute = std::path::absolute(written).ok();
        if selected
            .iter()
            .any(|path| path.includes(absolute.as_deref(), canonical))
        {
            names.insert(source_name.display().to_string());
        }
    })?;
    Ok(Some(SelectedSources { paths, names }))
}

fn failure(path: &Path, reason: &str) -> ExitCode {
    eprintln!("hgl: --test-path `{}`: {reason}", path.display());
    ExitCode::from(EXIT_FAILED)
}

/// A path as written and as identified by the filesystem.
struct Place {
    absolute: PathBuf,
    canonical: PathBuf,
    file: bool,
    directory: bool,
}

impl Place {
    fn new(path: &Path) -> io::Result<Self> {
        let canonical = fs::canonicalize(path)?;
        let metadata = fs::metadata(&canonical)?;
        Ok(Self {
            absolute: std::path::absolute(path)?,
            canonical,
            file: metadata.is_file(),
            directory: metadata.is_dir(),
        })
    }

    fn includes(&self, absolute: Option<&Path>, canonical: &Path) -> bool {
        if self.directory {
            // Source discovery follows links under a directory. The
            // written path also locates a linked file or subtree there.
            canonical.starts_with(&self.canonical)
                || absolute.is_some_and(|path| {
                    path.strip_prefix(&self.absolute).is_ok_and(|tail| {
                        tail.components()
                            .all(|part| matches!(part, Component::Normal(_)))
                    })
                })
        } else {
            self.file && self.canonical == canonical
        }
    }
}
