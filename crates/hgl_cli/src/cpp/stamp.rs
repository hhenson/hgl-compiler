//! The content key and the successful build it vouches for. The workspace
//! lock covers reading, clearing and publishing the stamp.

use std::collections::HashSet;
use std::fs;
use std::io::{self, ErrorKind, Read as _};
use std::path::{Component, Path};

use hgl_runner::CaseKind;
use serde::{Deserialize, Serialize};

use super::build::{Workspace, application_target, test_target_stem, write_if_changed};
use super::plan::{Action, Program};
use crate::{LimitArgs, ProgramSources, Replacement};

const FILE: &str = ".hgl-built.json";
const VERSION: u32 = 1;

/// A successful build for one input key, with its recorded artifacts.
#[derive(Deserialize)]
pub(super) struct Stamp {
    version: u32,
    key: String,
    artifacts: Vec<Artifact>,
    /// The program's built applications and, where built, its tests.
    pub(super) program: Program,
}

/// The relative path and length of a generated object or executable.
#[derive(Deserialize, Serialize)]
struct Artifact {
    path: String,
    bytes: u64,
}

impl Artifact {
    fn length(directory: &Path, name: &str) -> Option<u64> {
        let path = Path::new(name);
        if !path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        {
            return None;
        }
        let metadata = fs::symlink_metadata(directory.join(path)).ok()?;
        metadata.is_file().then_some(metadata.len())
    }
}

impl Stamp {
    pub(super) fn open(workspace: &Workspace, key: &str, tests: bool) -> Option<Self> {
        let bytes = fs::read(workspace.directory.join(FILE)).ok()?;
        let stamp: Stamp = serde_json::from_slice(&bytes).ok()?;
        if stamp.version != VERSION
            || stamp.key != key
            || (tests && stamp.program.tests.is_none())
            // The test runner omits rejection owners (TST-13). An ordinary
            // build or run checks the whole source under TST-14.
            || (!tests
                && stamp.program.tests.iter().flatten().any(|case| case.kind == CaseKind::Rejection))
            || !stamp.artifacts.iter().all(|artifact| {
                Artifact::length(&workspace.directory, &artifact.path) == Some(artifact.bytes)
            })
        {
            return None;
        }
        let artifacts: HashSet<_> = stamp
            .artifacts
            .iter()
            .map(|artifact| artifact.path.as_str())
            .collect();
        let listed = |name: String| artifacts.contains(name.as_str());
        if !stamp
            .program
            .applications
            .iter()
            .filter(|app| app.blocked.is_none())
            .all(|app| listed(application_target(&app.path)))
            || !stamp
                .program
                .tests
                .iter()
                .flatten()
                .all(|case| match &case.action {
                    Action::Checked(_) => true,
                    Action::Run(test) => listed(test_target_stem(&test.stem)),
                })
        {
            return None;
        }
        Some(stamp)
    }

    /// Removes a previous success before rebuilding.
    pub(super) fn clear(workspace: &Workspace) -> Result<(), String> {
        let path = workspace.directory.join(FILE);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("{} could not be removed: {error}", path.display())),
        }
    }

    /// Publishes only a finished build, as one atomic rename.
    pub(super) fn save(
        workspace: &Workspace,
        key: &str,
        artifacts: Vec<String>,
        program: &Program,
    ) -> Result<(), String> {
        let artifacts = artifacts
            .into_iter()
            .map(|path| {
                let bytes = Artifact::length(&workspace.directory, &path)
                    .ok_or_else(|| format!("the build did not produce {path}"))?;
                Ok(Artifact { path, bytes })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let json = serde_json::json!({
            "version": VERSION,
            "key": key,
            "artifacts": artifacts,
            "program": program,
        });
        write_if_changed(&workspace.directory.join(FILE), &json.to_string())
    }
}

/// Application inputs supplied on the command line (RUN-2, RUN-6).
pub(crate) struct Replacements<'a> {
    pub(crate) application: Option<&'a str>,
    pub(crate) arguments: &'a [Replacement],
    pub(crate) configuration: &'a [Replacement],
}

/// The build key, over source and compiler content in a framed sequence.
/// Named selectors and other run-only controls are independent of it.
pub(super) fn key(
    sources: &ProgramSources,
    limits: LimitArgs,
    toolchain: &str,
    cache: &Path,
    debug: bool,
    replacements: &Replacements<'_>,
) -> Result<String, String> {
    let mut hash = blake3::Hasher::new();
    let mut add = |bytes: &[u8]| {
        hash.update(&(bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    };
    add(b"hgl program build v1");
    add(env!("CARGO_PKG_VERSION").as_bytes());
    add(compiler()?.as_bytes());
    add(toolchain.as_bytes());
    add(cache.as_os_str().as_encoded_bytes());
    add(&[u8::from(debug)]);
    add(&(limits.nesting_limit as u64).to_le_bytes());
    add(&(limits.tree_depth_limit as u64).to_le_bytes());
    add(&(sources.files.len() as u64).to_le_bytes());
    for (name, bytes) in &sources.files {
        add(name.as_bytes());
        if debug {
            add(debug_source_path(name).as_bytes());
        }
        add(bytes);
    }
    for (name, text) in hgl_analyser::CORE_FILES {
        add(name.as_bytes());
        add(text.as_bytes());
    }
    if !replacements.arguments.is_empty() || !replacements.configuration.is_empty() {
        add(replacements.application.unwrap_or_default().as_bytes());
    }
    for group in [replacements.arguments, replacements.configuration] {
        add(&(group.len() as u64).to_le_bytes());
        for replacement in group {
            add(replacement.name.as_bytes());
            add(replacement.value.as_bytes());
        }
    }
    Ok(hash.finalize().to_hex().to_string())
}

/// The source name written into the generated debug line mappings.
pub(super) fn debug_source_path(name: &str) -> String {
    if name.starts_with('<') {
        return name.to_string();
    }
    std::path::absolute(name).map_or_else(|_| name.to_string(), |path| path.display().to_string())
}

fn compiler() -> Result<blake3::Hash, String> {
    let path = std::env::current_exe()
        .map_err(|error| format!("cannot find the compiler executable: {error}"))?;
    file_digest(&path).map_err(|error| format!("cannot read the compiler executable: {error}"))
}

pub(super) fn file_digest(path: &Path) -> io::Result<blake3::Hash> {
    let mut file = fs::File::open(path)?;
    let mut hash = blake3::Hasher::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let length = file.read(&mut buffer)?;
        if length == 0 {
            return Ok(hash.finalize());
        }
        hash.update(&buffer[..length]);
    }
}
