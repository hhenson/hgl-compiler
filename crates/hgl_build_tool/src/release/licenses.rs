//! Collect the notices for the compiler and the Rust distribution it uses.

use std::collections::{BTreeSet, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use super::cache::{copy_directory, copy_file};
use crate::{Result, Target, output, text};

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    version: String,
    source: Option<String>,
    manifest_path: PathBuf,
    license_file: Option<PathBuf>,
    repository: Option<String>,
}

/// Match the prefixes used by notices in crates and native dependencies.
pub(super) fn is_notice(name: &OsStr) -> bool {
    let name = name.to_string_lossy().to_ascii_lowercase();
    ["copying", "license", "notice", "unlicense"]
        .into_iter()
        .any(|prefix| name.starts_with(prefix))
}

/// Collect notices only for dependencies reached through normal and build edges.
pub(crate) fn licenses(destination: &Path, target: Target) -> Result<()> {
    let sysroot = text(Command::new("rustc").args(["--print", "sysroot"]))?;
    let docs = Path::new(sysroot.trim()).join("share/doc/rust");
    let rust = destination.join("rust");
    copy_directory(&docs.join("licenses"), &rust)?;
    for entry in fs::read_dir(&docs)? {
        let entry = entry?;
        let name = entry.file_name();
        let printed = name.to_string_lossy();
        if printed.starts_with("COPYRIGHT") && printed.ends_with(".html") {
            copy_file(&entry.path(), &rust.join(name))?;
        }
    }

    let triple = target.triple();
    let tree = text(Command::new("cargo").args([
        "tree",
        "-p",
        "hgl_cli",
        "--target",
        triple,
        "--locked",
        "--edges",
        "normal,build",
        "--prefix",
        "none",
        "--format",
        "{p}",
    ]))?;
    let used: HashSet<_> = tree
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some((fields.next()?, fields.next()?.strip_prefix('v')?))
        })
        .collect();
    let metadata = output(Command::new("cargo").args([
        "metadata",
        "--locked",
        "--format-version",
        "1",
        "--filter-platform",
        triple,
    ]))?;
    let metadata: Metadata = serde_json::from_slice(&metadata.stdout)?;
    for package in metadata.packages {
        if package.source.is_none()
            || !used.contains(&(package.name.as_str(), package.version.as_str()))
        {
            continue;
        }
        let source = package
            .manifest_path
            .parent()
            .ok_or("a registry manifest has no parent")?;
        let target_dir = destination
            .join("crates")
            .join(format!("{}-{}", package.name, package.version));
        fs::create_dir_all(target_dir.parent().expect("crates directory"))?;
        fs::create_dir(&target_dir)?;
        let mut notices = BTreeSet::new();
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            if is_notice(&entry.file_name()) && entry.path().is_file() {
                notices.insert(entry.path());
            }
        }
        if let Some(license) = &package.license_file {
            notices.insert(source.join(license));
        }
        if notices.is_empty() {
            upstream_notice(&package, source, &target_dir)?;
        }
        for notice in notices {
            let name = notice
                .file_name()
                .ok_or("a license notice has no file name")?;
            copy_file(&notice, &target_dir.join(name))?;
        }
    }
    Ok(())
}

fn upstream_notice(package: &Package, source: &Path, destination: &Path) -> Result<()> {
    // These crates ship their notices in the source repository only.
    if package.repository.as_deref() != Some("https://github.com/madsmtm/objc2") {
        return Err(format!("no license files for {} {}", package.name, package.version).into());
    }
    #[derive(Deserialize)]
    struct Vcs {
        git: Git,
    }
    #[derive(Deserialize)]
    struct Git {
        sha1: String,
    }
    let vcs: Vcs = serde_json::from_slice(&fs::read(source.join(".cargo_vcs_info.json"))?)?;
    let revision = vcs.git.sha1;
    if revision.len() != 40
        || !revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!("no upstream license revision for {}", package.name).into());
    }
    let url = format!("https://raw.githubusercontent.com/madsmtm/objc2/{revision}/LICENSE.md");
    let notice = output(Command::new("curl").args([
        "--fail",
        "--location",
        "--silent",
        "--show-error",
        "--max-time",
        "30",
        &url,
    ]))?;
    fs::write(destination.join("LICENSE.md"), notice.stdout)?;
    Ok(())
}
