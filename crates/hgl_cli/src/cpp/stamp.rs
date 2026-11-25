//! The content key and the successful build it vouches for. The workspace
//! lock covers reading, clearing and publishing the stamp.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::{self, ErrorKind, Read as _};
use std::path::{Component, Path};

use hgl_runner::CaseKind;
use serde::{Deserialize, Serialize};

use super::build::{Workspace, application_target, test_target_stem, write_if_changed};
use super::plan::{Action, Program, TestCase};
use crate::{LimitArgs, ProgramSources, Replacement};

const FILE: &str = ".hgl-built.json";
const PREVIOUS: &str = ".hgl-previous-build.json";
const VERSION: u32 = 2;

/// A successful build for one input key, with its recorded artifacts.
#[derive(Deserialize)]
pub(super) struct Stamp {
    version: u32,
    key: String,
    artifacts: Vec<Artifact>,
    /// A debugger's test selection does not cover the whole program.
    #[serde(default)]
    partial_tests: bool,
    /// The program's built applications and, where built, its tests.
    pub(super) program: Program,
}

/// The last successful build's digests. An invalidated stamp is kept
/// only to verify incremental inputs and never permits direct reuse.
pub(super) struct PreviousBuild {
    /// The artifact inventory from the last successful build.
    pub(super) stamp: Stamp,
    active: bool,
}

impl PreviousBuild {
    /// Finds the current stamp, or the previous one after a failed build.
    pub(super) fn read(workspace: &Workspace) -> Option<Self> {
        if let Some(stamp) = Stamp::read(workspace, FILE) {
            return Some(Self {
                stamp,
                active: true,
            });
        }
        Stamp::read(workspace, PREVIOUS).map(|stamp| Self {
            stamp,
            active: false,
        })
    }

    /// Checks an independently discovered workspace file against the last successful build.
    pub(super) fn matches_artifact(&self, directory: &Path, relative: &str) -> bool {
        self.stamp
            .artifacts
            .iter()
            .any(|artifact| artifact.path == relative && artifact.matches(directory))
    }

    /// Only a current stamp vouches for an executable that can run now.
    pub(super) fn reusable(&self, workspace: &Workspace, key: &str, tests: Option<bool>) -> bool {
        self.active && self.stamp.reusable(workspace, key, tests)
    }

    /// The complete case inventory for an active snapshot, even if only
    /// some of its test executables were built.
    pub(super) fn test_inventory(&self, key: &str) -> Option<&[TestCase]> {
        if self.active && self.stamp.key == key {
            self.stamp.program.tests.as_deref()
        } else {
            None
        }
    }

    /// Whether all selected test executables belong to this active build.
    pub(super) fn tests_reusable(
        &self,
        workspace: &Workspace,
        key: &str,
        stems: &BTreeSet<&str>,
    ) -> bool {
        self.test_inventory(key).is_some()
            && self.stamp.verified_paths(workspace).is_some_and(|paths| {
                stems
                    .iter()
                    .all(|stem| paths.contains(test_target_stem(stem).as_str()))
            })
    }

    /// Atomically withdraws permission to run and keeps the digests. A
    /// later failure with no active stamp leaves those digests alone.
    pub(super) fn invalidate(workspace: &Workspace, previous: Option<&Self>) -> Result<(), String> {
        let path = workspace.directory.join(FILE);
        if !previous.is_some_and(|build| build.active) {
            return remove(&path);
        }
        match fs::rename(&path, workspace.directory.join(PREVIOUS)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!(
                "{} could not be invalidated: {error}",
                path.display()
            )),
        }
    }
}

/// The relative path and BLAKE3 digest of a generated build artifact.
#[derive(Clone, Deserialize, Serialize)]
struct Artifact {
    path: String,
    digest: String,
}

impl Artifact {
    fn digest(directory: &Path, name: &str) -> Option<blake3::Hash> {
        let path = Path::new(name);
        if !path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        {
            return None;
        }
        let path = directory.join(path);
        if !fs::symlink_metadata(&path).ok()?.is_file() {
            return None;
        }
        file_digest(&path).ok()
    }

    fn matches(&self, directory: &Path) -> bool {
        Self::digest(directory, &self.path)
            .is_some_and(|hash| hash.to_hex().as_str() == self.digest)
    }
}

impl Stamp {
    fn read(workspace: &Workspace, name: &str) -> Option<Self> {
        let bytes = fs::read(workspace.directory.join(name)).ok()?;
        let stamp: Stamp = serde_json::from_slice(&bytes).ok()?;
        (stamp.version == VERSION).then_some(stamp)
    }

    fn reusable(&self, workspace: &Workspace, key: &str, tests: Option<bool>) -> bool {
        if self.key != key
            || self.partial_tests
            || tests.is_some_and(|tests| tests != self.program.tests.is_some())
            // The test runner omits rejection owners (TST-13). An ordinary
            // build or run checks the whole source under TST-14.
            || (tests != Some(true)
                && self.program.tests.iter().flatten().any(|case| case.kind == CaseKind::Rejection))
        {
            return false;
        }
        let Some(artifacts) = self.verified_paths(workspace) else {
            return false;
        };
        let listed = |name: String| artifacts.contains(name.as_str());
        self.program
            .applications
            .iter()
            .filter(|app| app.blocked.is_none())
            .all(|app| listed(application_target(&app.path)))
            && self
                .program
                .tests
                .iter()
                .flatten()
                .all(|case| match &case.action {
                    Action::Checked(_) => true,
                    Action::Run(test) => listed(test_target_stem(&test.stem)),
                })
    }

    fn verified_paths(&self, workspace: &Workspace) -> Option<HashSet<&str>> {
        self.artifacts
            .iter()
            .map(|artifact| {
                artifact
                    .matches(&workspace.directory)
                    .then_some(artifact.path.as_str())
            })
            .collect()
    }

    /// Unverified outputs cannot serve as Ninja's incremental inputs. Only
    /// remove paths named by the current build plan, never paths from the stamp.
    pub(super) fn remove_unverified(
        previous: Option<&Self>,
        workspace: &Workspace,
        paths: &[String],
    ) -> Result<(), String> {
        let known: HashMap<_, _> = previous
            .into_iter()
            .flat_map(|stamp| &stamp.artifacts)
            .map(|artifact| (artifact.path.as_str(), artifact))
            .collect();
        for path in paths {
            if !known
                .get(path.as_str())
                .is_some_and(|a| a.matches(&workspace.directory))
            {
                remove(&workspace.directory.join(path))?;
            }
        }
        Ok(())
    }

    /// Reads the artifacts Ninja built or checked before publishing them.
    pub(super) fn describe(
        workspace: &Workspace,
        key: &str,
        artifacts: Vec<String>,
        program: &Program,
        partial_tests: bool,
        previous: Option<&Self>,
    ) -> Result<String, String> {
        // A partial build keeps earlier, still verified artifacts of the
        // same source snapshot. A different key gets a fresh inventory.
        let new: HashSet<_> = artifacts.iter().map(String::as_str).collect();
        let mut retained: Vec<_> = previous
            .filter(|previous| previous.key == key)
            .into_iter()
            .flat_map(|previous| &previous.artifacts)
            .filter(|artifact| {
                !new.contains(artifact.path.as_str()) && artifact.matches(&workspace.directory)
            })
            .cloned()
            .collect();
        let artifacts = artifacts
            .into_iter()
            .map(|path| {
                let digest = Artifact::digest(&workspace.directory, &path)
                    .ok_or_else(|| format!("the build did not produce {path}"))?
                    .to_hex()
                    .to_string();
                Ok(Artifact { path, digest })
            })
            .collect::<Result<Vec<_>, String>>()?;
        retained.extend(artifacts);
        serde_json::to_string(&serde_json::json!({
            "version": VERSION,
            "key": key,
            "artifacts": retained,
            "partial_tests": partial_tests,
            "program": program,
        }))
        .map_err(|error| error.to_string())
    }

    /// Publishes only a finished build, as one atomic rename.
    pub(super) fn save(workspace: &Workspace, json: &str) -> Result<(), String> {
        write_if_changed(&workspace.directory.join(FILE), json)
    }
}

fn remove(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("{} could not be removed: {error}", path.display())),
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
