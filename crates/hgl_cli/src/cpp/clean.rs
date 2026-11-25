//! Clean a locked program workspace, including outputs left by older compilers.
//! Generated C++ sources are identified by their header, while compiled objects,
//! executables and optional files require a matching digest in the workspace stamps.
//! The build directory can also contain user files or the other build profile.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use super::build::{MANIFEST_HEADER, Workspace};
use super::stamp::PreviousBuild;

const COMPILE_COMMANDS: &str = "compile_commands.json";

/// The caller holds the workspace lock until the replacement build completes.
pub(super) fn workspace(
    workspace: &Workspace,
    previous: Option<&PreviousBuild>,
) -> Result<(), String> {
    clean(&workspace.directory, previous)
        .map_err(|error| format!("cannot clean {}: {error}", workspace.directory.display()))
}

fn has_header(path: &Path, header: &str) -> io::Result<bool> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take((header.len() + 1) as u64)
        .read_to_end(&mut bytes)?;
    Ok(bytes.strip_suffix(b"\n") == Some(header.as_bytes()))
}

fn is_canonical_index(text: &str) -> bool {
    text == "0"
        || (!text.is_empty()
            && !text.starts_with('0')
            && text.bytes().all(|byte| byte.is_ascii_digit()))
}

fn is_recorded_product_name(dir: &str, name: &str) -> bool {
    match dir {
        "obj/modules" => {
            (name.starts_with("mod-") || name.starts_with("test-mod-")) && name.ends_with(".o")
        }
        "obj/tests" => name.starts_with("test-") && name.ends_with(".o"),
        "obj/applications" => name.starts_with("app-") && name.ends_with(".o"),
        "bin" => name.starts_with("test-") && name.ends_with(std::env::consts::EXE_SUFFIX),
        "run" => name.starts_with("app-") && name.ends_with(std::env::consts::EXE_SUFFIX),
        "results" => {
            name.strip_suffix(".txt")
                .and_then(|s| s.rsplit_once('.'))
                .is_some_and(|(stem, position)| {
                    stem.starts_with("test-") && is_canonical_index(position)
                })
                || (name.starts_with("app-") && name.ends_with(".run.txt"))
        }
        "debug-sources" | "debug-sources/native" => name.ends_with(".hgl.source"),
        _ => false,
    }
}

fn remove_files(paths: &[PathBuf]) -> io::Result<()> {
    for path in paths {
        match fs::remove_file(path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => {
                return Err(io::Error::new(e.kind(), format!("{}: {e}", path.display())));
            }
            _ => {}
        }
    }
    Ok(())
}

fn clean(root: &Path, previous: Option<&PreviousBuild>) -> io::Result<()> {
    // Check every output directory before inspecting children. Never follow a
    // directory link into a source tree, a shared native cache or another build.
    for dir in [
        "",
        "modules",
        "tests",
        "applications",
        "obj",
        "obj/modules",
        "obj/tests",
        "obj/applications",
        "run",
        "bin",
        "results",
        "debug-results",
        "debug-sources",
        "debug-sources/native",
    ] {
        let path = if dir.is_empty() {
            root.to_owned()
        } else {
            root.join(dir)
        };
        match fs::symlink_metadata(&path) {
            Ok(metadata) if !metadata.is_dir() => {
                return Err(io::Error::other(format!(
                    "{} is not a build directory",
                    path.display()
                )));
            }
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    let manifest = root.join("build.ninja");
    let owned = match fs::symlink_metadata(&manifest) {
        Ok(info) if info.is_file() && has_header(&manifest, MANIFEST_HEADER)? => true,
        Ok(_) => return Err(io::Error::other("build.ninja is not an HGL build manifest")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => previous.is_some(),
        Err(e) => return Err(e),
    };
    let stamp_files = [".hgl-built.json", ".hgl-previous-build.json"];
    let ninja_files = [".ninja_log", ".ninja_deps"];
    // A failed first build may leave its directory empty. A foreign workspace
    // with colliding files must never be adopted by a clean.
    if !owned {
        for name in stamp_files
            .into_iter()
            .chain(ninja_files)
            .chain(std::iter::once(COMPILE_COMMANDS))
        {
            let path = root.join(name);
            match fs::symlink_metadata(&path) {
                Ok(_) => {
                    return Err(io::Error::other(format!(
                        "{} has no HGL build manifest",
                        path.display()
                    )));
                }
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
    }

    // Phase 1: compiled objects, executables and optional workspace files whose
    // ownership is proven by an exact digest in the workspace stamps. Remove
    // these before touching stamps so a partial failure never orphans them.
    let mut recorded_products = Vec::new();
    if owned && let Some(previous) = previous {
        for dir in [
            "obj/modules",
            "obj/tests",
            "obj/applications",
            "bin",
            "run",
            "results",
            "debug-sources",
            "debug-sources/native",
        ] {
            let entries = match fs::read_dir(root.join(dir)) {
                Ok(entries) => entries,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            };
            for entry in entries {
                let entry = entry?;
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                if is_recorded_product_name(dir, name)
                    && entry.file_type()?.is_file()
                    && previous.matches_artifact(root, &format!("{dir}/{name}"))
                {
                    recorded_products.push(entry.path());
                }
            }
        }
        if previous.matches_artifact(root, COMPILE_COMMANDS) {
            recorded_products.push(root.join(COMPILE_COMMANDS));
        }
    }

    // Phase 2: generated C++ sources identified by their compiler header, and
    // Ninja's workspace logs once all recorded products are removed.
    let mut generated_sources = Vec::new();
    for dir in ["modules", "tests", "applications"] {
        let entries = match fs::read_dir(root.join(dir)) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let path = entry.path();
            let generated_name = match dir {
                "applications" => name.starts_with("app-") && name.ends_with(".cpp"),
                "tests" => name.starts_with("test-") && name.ends_with(".cpp"),
                _ => {
                    (name.starts_with("mod-") && (name.ends_with(".h") || name.ends_with(".cpp")))
                        || (name.starts_with("test-mod-") && name.ends_with(".cpp"))
                        || (name.starts_with("type-") && name.ends_with(".h"))
                }
            };
            if generated_name
                && entry.file_type()?.is_file()
                && has_header(&path, hgl_cpp::GENERATED_HEADER)?
            {
                generated_sources.push(path);
            }
        }
    }
    if owned {
        generated_sources.extend(ninja_files.into_iter().map(|p| root.join(p)));
    }

    // Phase 3: ownership and provenance markers are removed last, only after
    // every product and generated source they vouch for has been dealt with.
    let mut markers = Vec::new();
    if owned {
        markers.extend(stamp_files.into_iter().map(|p| root.join(p)));
        markers.push(manifest);
    }

    for path in recorded_products
        .iter()
        .chain(&generated_sources)
        .chain(&markers)
    {
        match fs::symlink_metadata(path) {
            Ok(info) if info.is_dir() => {
                return Err(io::Error::other(format!(
                    "expected a file at {}",
                    path.display()
                )));
            }
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    remove_files(&recorded_products)?;
    remove_files(&generated_sources)?;
    remove_files(&markers)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_manifest(root: &Path) {
        fs::write(root.join("build.ninja"), format!("{MANIFEST_HEADER}\n")).unwrap();
    }

    fn write_generated_cpp(root: &Path, relative: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            path,
            format!(
                "{}\nint main() {{ return 0; }}\n",
                hgl_cpp::GENERATED_HEADER
            ),
        )
        .unwrap();
    }

    #[test]
    fn clean_refuses_a_foreign_compilation_database() {
        let workspace = tempfile::tempdir().unwrap();
        let file = workspace.path().join(COMPILE_COMMANDS);
        fs::write(&file, "keep").unwrap();
        assert!(clean(workspace.path(), None).is_err());
        assert_eq!(fs::read_to_string(file).unwrap(), "keep");
    }

    /// 0.2.9's v2 stamp records the exact bytes of debug copies. Preserve
    /// handwritten neighbours and copies the user has edited since the build.
    #[test]
    fn cleans_only_matching_debug_copies_from_a_v2_workspace() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let workspace = Workspace::open(Some(root), &[], root, false).unwrap();
        let copies = root.join("debug-sources");
        fs::create_dir(&copies).unwrap();
        fs::write(copies.join("core.hgl.source"), "original").unwrap();
        fs::write(copies.join("calendar.hgl.source"), "user edit").unwrap();
        fs::write(copies.join("notes.hgl.source"), "original").unwrap();
        let digest = blake3::hash(b"original").to_hex().to_string();
        let stamp = serde_json::json!({
            "version": 2, "key": "old", "program": {"units": [], "applications": [], "tests": null},
            "artifacts": [
                {"path":"debug-sources/core.hgl.source", "digest": digest},
                {"path":"debug-sources/calendar.hgl.source", "digest": digest}
            ]
        });
        fs::write(root.join(".hgl-built.json"), stamp.to_string()).unwrap();
        let previous = PreviousBuild::read(&workspace).unwrap();
        clean(root, Some(&previous)).unwrap();
        assert!(!copies.join("core.hgl.source").exists());
        assert_eq!(
            fs::read_to_string(copies.join("calendar.hgl.source")).unwrap(),
            "user edit"
        );
        assert_eq!(
            fs::read_to_string(copies.join("notes.hgl.source")).unwrap(),
            "original"
        );
    }

    /// A generated source name alone must never establish ownership of its
    /// object or executable in either an emit-only directory or an owned
    /// workspace whose stamps omit or do not match that artifact.
    #[test]
    fn preserves_unrecorded_objects_and_executables_in_emit_and_unrelated_workspaces() {
        let app_exec = format!("run/app-audit-runit{}", std::env::consts::EXE_SUFFIX);
        let test_exec = format!("bin/test-demo-stamped{}", std::env::consts::EXE_SUFFIX);

        // Emit-only directory: no manifest or stamp.
        let emitted = tempfile::tempdir().unwrap();
        let root = emitted.path();
        write_generated_cpp(root, "applications/app-audit-runit.cpp");
        fs::create_dir_all(root.join("run")).unwrap();
        fs::create_dir_all(root.join("obj/applications")).unwrap();
        fs::write(root.join(&app_exec), "#!/bin/sh\necho keep\n").unwrap();
        fs::write(
            root.join("obj/applications/app-audit-runit.o"),
            "foreign obj",
        )
        .unwrap();
        clean(root, None).unwrap();
        assert!(!root.join("applications/app-audit-runit.cpp").exists());
        assert_eq!(
            fs::read_to_string(root.join(&app_exec)).unwrap(),
            "#!/bin/sh\necho keep\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("obj/applications/app-audit-runit.o")).unwrap(),
            "foreign obj"
        );

        // Owned workspace built for another program (stamps omit app-audit-runit
        // and test-demo-stamped, while recording mod-other.o and a replaced launcher).
        let owned = tempfile::tempdir().unwrap();
        let root = owned.path();
        let workspace = Workspace::open(Some(root), &[], root, false).unwrap();
        write_manifest(root);
        write_generated_cpp(root, "modules/mod-other.cpp");
        write_generated_cpp(root, "applications/app-audit-runit.cpp");
        write_generated_cpp(root, "tests/test-demo-stamped.cpp");
        fs::create_dir_all(root.join("obj/modules")).unwrap();
        fs::create_dir_all(root.join("obj/applications")).unwrap();
        fs::create_dir_all(root.join("run")).unwrap();
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::write(root.join("obj/modules/mod-other.o"), "built obj").unwrap();
        fs::write(
            root.join("obj/applications/app-audit-runit.o"),
            "handwritten obj",
        )
        .unwrap();
        fs::write(root.join(&app_exec), "#!/bin/sh\necho launcher\n").unwrap();
        fs::write(root.join(&test_exec), "replaced binary").unwrap();
        let stamp = serde_json::json!({
            "version": 2, "key": "other", "program": {"units": [], "applications": [], "tests": null},
            "artifacts": [
                {"path": "obj/modules/mod-other.o", "digest": blake3::hash(b"built obj").to_hex().to_string()},
                {"path": test_exec, "digest": blake3::hash(b"original binary").to_hex().to_string()}
            ]
        });
        fs::write(root.join(".hgl-built.json"), stamp.to_string()).unwrap();
        let previous = PreviousBuild::read(&workspace).unwrap();
        clean(root, Some(&previous)).unwrap();
        assert!(!root.join("obj/modules/mod-other.o").exists());
        assert_eq!(
            fs::read_to_string(root.join("obj/applications/app-audit-runit.o")).unwrap(),
            "handwritten obj"
        );
        assert_eq!(
            fs::read_to_string(root.join(&app_exec)).unwrap(),
            "#!/bin/sh\necho launcher\n"
        );
        assert_eq!(
            fs::read_to_string(root.join(&test_exec)).unwrap(),
            "replaced binary"
        );
    }

    /// A 0.2.9 debug build followed by an ordinary release build retains the
    /// debug copy digests in `.hgl-previous-build.json`. Unstamped legacy
    /// debug copies are preserved even when their bytes match bundled Core.
    #[test]
    fn cleans_recorded_previous_debug_copies_and_preserves_unstamped_legacy_copies() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let workspace = Workspace::open(Some(root), &[], root, false).unwrap();
        write_manifest(root);
        let copies = root.join("debug-sources");
        fs::create_dir(&copies).unwrap();
        fs::write(copies.join("core.hgl.source"), "prior core").unwrap();
        fs::write(copies.join("calendar.hgl.source"), "edited calendar").unwrap();
        fs::write(copies.join("notes.hgl.source"), "prior core").unwrap();
        let digest = blake3::hash(b"prior core").to_hex().to_string();
        let debug_stamp = serde_json::json!({
            "version": 2, "key": "debug", "program": {"units": [], "applications": [], "tests": null},
            "artifacts": [
                {"path": "debug-sources/core.hgl.source", "digest": digest},
                {"path": "debug-sources/calendar.hgl.source", "digest": digest}
            ]
        });
        let release_stamp = serde_json::json!({
            "version": 2, "key": "release", "program": {"units": [], "applications": [], "tests": null},
            "artifacts": []
        });
        fs::write(
            root.join(".hgl-previous-build.json"),
            debug_stamp.to_string(),
        )
        .unwrap();
        fs::write(root.join(".hgl-built.json"), release_stamp.to_string()).unwrap();
        let previous = PreviousBuild::read(&workspace).unwrap();
        clean(root, Some(&previous)).unwrap();
        assert!(!copies.join("core.hgl.source").exists());
        assert_eq!(
            fs::read_to_string(copies.join("calendar.hgl.source")).unwrap(),
            "edited calendar"
        );
        assert_eq!(
            fs::read_to_string(copies.join("notes.hgl.source")).unwrap(),
            "prior core"
        );

        // Unstamped legacy debug copy: even exact bundled Core bytes are
        // preserved when no workspace stamp vouches for them.
        let legacy = tempfile::tempdir().unwrap();
        let legacy_root = legacy.path();
        write_manifest(legacy_root);
        let legacy_copies = legacy_root.join("debug-sources");
        fs::create_dir(&legacy_copies).unwrap();
        let (_, core_text) = hgl_analyser::CORE_FILES
            .iter()
            .find(|(name, _)| *name == "<core>/core/core.hgl")
            .unwrap();
        fs::write(legacy_copies.join("core.hgl.source"), core_text).unwrap();
        clean(legacy_root, None).unwrap();
        assert_eq!(
            fs::read_to_string(legacy_copies.join("core.hgl.source")).unwrap(),
            *core_text
        );
    }

    /// When a failed debug upgrade records a newer copy digest in
    /// `.hgl-previous-build.json`'s `retained_artifacts` beside the older
    /// successful build's `artifacts`, clean and carry-forward must prefer the
    /// newest writer: remove untouched latest bytes and preserve restored old bytes.
    #[test]
    fn prefers_newest_retained_digest_over_older_artifact_after_failed_debug_upgrade() {
        let old_bytes = b"older release core source";
        let new_bytes = b"newer release core source";
        let old_digest = blake3::hash(old_bytes).to_hex().to_string();
        let new_digest = blake3::hash(new_bytes).to_hex().to_string();
        let previous_stamp = serde_json::json!({
            "version": 2,
            "key": "old-debug",
            "program": {"units": [], "applications": [], "tests": null},
            "artifacts": [
                {"path": "debug-sources/core.hgl.source", "digest": old_digest}
            ],
            "retained_artifacts": [
                {"path": "debug-sources/core.hgl.source", "digest": new_digest}
            ]
        });

        // Control 1: untouched latest bytes are removed on clean.
        let latest = tempfile::tempdir().unwrap();
        let root = latest.path();
        let workspace = Workspace::open(Some(root), &[], root, false).unwrap();
        write_manifest(root);
        fs::create_dir(root.join("debug-sources")).unwrap();
        fs::write(root.join("debug-sources/core.hgl.source"), new_bytes).unwrap();
        fs::write(
            root.join(".hgl-previous-build.json"),
            previous_stamp.to_string(),
        )
        .unwrap();
        let previous = PreviousBuild::read(&workspace).unwrap();
        clean(root, Some(&previous)).unwrap();
        assert!(!root.join("debug-sources/core.hgl.source").exists());

        // Control 2: restored old bytes differ from the newest writer and are preserved.
        let restored = tempfile::tempdir().unwrap();
        let root = restored.path();
        let workspace = Workspace::open(Some(root), &[], root, false).unwrap();
        write_manifest(root);
        fs::create_dir(root.join("debug-sources")).unwrap();
        fs::write(root.join("debug-sources/core.hgl.source"), old_bytes).unwrap();
        fs::write(
            root.join(".hgl-previous-build.json"),
            previous_stamp.to_string(),
        )
        .unwrap();
        let previous = PreviousBuild::read(&workspace).unwrap();
        clean(root, Some(&previous)).unwrap();
        assert_eq!(
            fs::read(root.join("debug-sources/core.hgl.source")).unwrap(),
            old_bytes
        );
    }

    /// Invalidating an active stamp must persist still-verified entries from
    /// `.hgl-previous-build.json` before replacing it, preserving last-write
    /// precedence and withdrawing `.hgl-built.json`.
    #[test]
    fn invalidate_persists_earlier_verified_inventory_with_last_write_precedence() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let workspace = Workspace::open(Some(root), &[], root, false).unwrap();
        write_manifest(root);
        fs::create_dir_all(root.join("obj/modules")).unwrap();
        fs::create_dir_all(root.join("debug-sources")).unwrap();
        fs::write(root.join("obj/modules/mod-demo.o"), b"release obj").unwrap();
        fs::write(root.join("debug-sources/core.hgl.source"), b"debug core").unwrap();
        fs::write(
            root.join("debug-sources/calendar.hgl.source"),
            b"edited calendar",
        )
        .unwrap();
        fs::write(root.join("debug-sources/notes.hgl.source"), b"neighbour").unwrap();
        // Restored old bytes at a path whose newer active stamp recorded a different digest.
        fs::write(
            root.join("debug-sources/mapping.hgl.source"),
            b"old mapping",
        )
        .unwrap();

        let earlier_stamp = serde_json::json!({
            "version": 2,
            "key": "debug",
            "program": {"units": [], "applications": [], "tests": null},
            "artifacts": [
                {"path": "debug-sources/core.hgl.source", "digest": blake3::hash(b"debug core").to_hex().to_string()},
                {"path": "debug-sources/calendar.hgl.source", "digest": blake3::hash(b"orig calendar").to_hex().to_string()},
                {"path": "debug-sources/mapping.hgl.source", "digest": blake3::hash(b"old mapping").to_hex().to_string()}
            ]
        });
        let active_stamp = serde_json::json!({
            "version": 2,
            "key": "release",
            "program": {"units": [], "applications": [], "tests": null},
            "artifacts": [
                {"path": "obj/modules/mod-demo.o", "digest": blake3::hash(b"release obj").to_hex().to_string()},
                {"path": "debug-sources/mapping.hgl.source", "digest": blake3::hash(b"new mapping").to_hex().to_string()}
            ]
        });
        fs::write(
            root.join(".hgl-previous-build.json"),
            earlier_stamp.to_string(),
        )
        .unwrap();
        fs::write(root.join(".hgl-built.json"), active_stamp.to_string()).unwrap();

        let previous = PreviousBuild::read(&workspace).unwrap();
        PreviousBuild::invalidate(&workspace, Some(&previous)).unwrap();
        assert!(!root.join(".hgl-built.json").exists());
        assert!(root.join(".hgl-previous-build.json").is_file());

        let after = PreviousBuild::read(&workspace).unwrap();
        clean(root, Some(&after)).unwrap();
        assert!(!root.join("obj/modules/mod-demo.o").exists());
        assert!(!root.join("debug-sources/core.hgl.source").exists());
        assert_eq!(
            fs::read(root.join("debug-sources/calendar.hgl.source")).unwrap(),
            b"edited calendar"
        );
        assert_eq!(
            fs::read(root.join("debug-sources/notes.hgl.source")).unwrap(),
            b"neighbour"
        );
        assert_eq!(
            fs::read(root.join("debug-sources/mapping.hgl.source")).unwrap(),
            b"old mapping"
        );
    }

    /// Optional compilation databases and host-formatted result files are
    /// removed only when unchanged from a recorded stamp digest.
    #[test]
    fn preserves_unrecorded_or_edited_optional_files_even_in_valid_hgl_format() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let workspace = Workspace::open(Some(root), &[], root, false).unwrap();
        write_manifest(root);
        write_generated_cpp(root, "modules/mod-demo.cpp");
        write_generated_cpp(root, "tests/test-demo-stamped.cpp");
        write_generated_cpp(root, "applications/app-demo-app.cpp");
        fs::create_dir(root.join("results")).unwrap();
        fs::create_dir(root.join("debug-results")).unwrap();

        let lookalike_compdb = serde_json::json!([{
            "directory": root.to_str().unwrap(),
            "command": "g++-15 -std=c++23 -Winvalid-pch -include pch.h -MD -MF obj/modules/mod-demo.o.d -c modules/mod-demo.cpp -o obj/modules/mod-demo.o",
            "file": "modules/mod-demo.cpp",
            "output": "obj/modules/mod-demo.o",
        }])
        .to_string();
        let valid_test_result = "start 0\npassed 0\n";
        let valid_app_result = "start\npublished 1970-01-01T00:00:01Z 51\nstopped\n";

        // User-created files in valid HGL/host format with no recorded stamp entry.
        fs::write(root.join(COMPILE_COMMANDS), &lookalike_compdb).unwrap();
        fs::write(
            root.join("results/test-demo-stamped.0.txt"),
            valid_test_result,
        )
        .unwrap();
        fs::write(
            root.join("results/test-demo-stamped.+0.txt"),
            valid_test_result,
        )
        .unwrap();
        fs::write(root.join("results/app-demo-app.run.txt"), valid_app_result).unwrap();
        fs::write(
            root.join("debug-results/test-demo-stamped.txt"),
            valid_test_result,
        )
        .unwrap();
        fs::write(root.join("results.txt"), valid_app_result).unwrap();

        let empty_stamp = serde_json::json!({
            "version": 2, "key": "k", "program": {"units": [], "applications": [], "tests": null},
            "artifacts": []
        });
        fs::write(root.join(".hgl-built.json"), empty_stamp.to_string()).unwrap();
        let previous = PreviousBuild::read(&workspace).unwrap();
        clean(root, Some(&previous)).unwrap();
        assert_eq!(
            fs::read_to_string(root.join(COMPILE_COMMANDS)).unwrap(),
            lookalike_compdb
        );
        assert_eq!(
            fs::read_to_string(root.join("results/test-demo-stamped.0.txt")).unwrap(),
            valid_test_result
        );
        assert_eq!(
            fs::read_to_string(root.join("results/test-demo-stamped.+0.txt")).unwrap(),
            valid_test_result
        );
        assert_eq!(
            fs::read_to_string(root.join("results/app-demo-app.run.txt")).unwrap(),
            valid_app_result
        );
        assert_eq!(
            fs::read_to_string(root.join("debug-results/test-demo-stamped.txt")).unwrap(),
            valid_test_result
        );
        assert_eq!(
            fs::read_to_string(root.join("results.txt")).unwrap(),
            valid_app_result
        );

        // Recorded optional files are cleaned only when their bytes match.
        let recorded = tempfile::tempdir().unwrap();
        let root = recorded.path();
        let workspace = Workspace::open(Some(root), &[], root, false).unwrap();
        write_manifest(root);
        write_generated_cpp(root, "tests/test-demo-stamped.cpp");
        write_generated_cpp(root, "applications/app-demo-app.cpp");
        fs::create_dir(root.join("results")).unwrap();
        fs::write(root.join(COMPILE_COMMANDS), &lookalike_compdb).unwrap();
        fs::write(
            root.join("results/test-demo-stamped.0.txt"),
            valid_test_result,
        )
        .unwrap();
        fs::write(
            root.join("results/test-demo-stamped.1.txt"),
            "edited after test\n",
        )
        .unwrap();
        fs::write(root.join("results/app-demo-app.run.txt"), valid_app_result).unwrap();
        let compdb_digest = blake3::hash(lookalike_compdb.as_bytes())
            .to_hex()
            .to_string();
        let test_digest = blake3::hash(valid_test_result.as_bytes())
            .to_hex()
            .to_string();
        let app_digest = blake3::hash(valid_app_result.as_bytes())
            .to_hex()
            .to_string();
        let stamp = serde_json::json!({
            "version": 2, "key": "k", "program": {"units": [], "applications": [], "tests": null},
            "artifacts": [],
            "retained_artifacts": [
                {"path": COMPILE_COMMANDS, "digest": compdb_digest},
                {"path": "results/test-demo-stamped.0.txt", "digest": test_digest},
                {"path": "results/test-demo-stamped.1.txt", "digest": test_digest},
                {"path": "results/app-demo-app.run.txt", "digest": app_digest}
            ]
        });
        fs::write(root.join(".hgl-built.json"), stamp.to_string()).unwrap();
        let previous = PreviousBuild::read(&workspace).unwrap();
        clean(root, Some(&previous)).unwrap();
        assert!(!root.join(COMPILE_COMMANDS).exists());
        assert!(!root.join("results/test-demo-stamped.0.txt").exists());
        assert!(!root.join("results/app-demo-app.run.txt").exists());
        assert_eq!(
            fs::read_to_string(root.join("results/test-demo-stamped.1.txt")).unwrap(),
            "edited after test\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn preserves_stamps_through_partial_clean_failure_until_retry_succeeds() {
        use std::os::unix::fs::PermissionsExt as _;

        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let workspace = Workspace::open(Some(root), &[], root, false).unwrap();
        write_manifest(root);
        write_generated_cpp(root, "modules/mod-demo.cpp");
        fs::create_dir_all(root.join("obj/modules")).unwrap();
        fs::create_dir_all(root.join("debug-sources")).unwrap();
        fs::create_dir_all(root.join("results")).unwrap();
        fs::write(root.join("obj/modules/mod-demo.o"), "obj bytes").unwrap();
        fs::write(root.join("debug-sources/core.hgl.source"), "core bytes").unwrap();
        fs::write(root.join("debug-sources/calendar.hgl.source"), "cal bytes").unwrap();
        fs::write(root.join("debug-sources/notes.hgl.source"), "handwritten").unwrap();
        fs::write(root.join("results/test-demo.0.txt"), "start 0\npassed 0\n").unwrap();
        fs::write(root.join("results/test-demo.9.txt"), "start 0\npassed 0\n").unwrap();
        let debug_stamp = serde_json::json!({
            "version": 2, "key": "debug", "program": {"units": [], "applications": [], "tests": null},
            "artifacts": [
                {"path": "debug-sources/core.hgl.source", "digest": blake3::hash(b"core bytes").to_hex().to_string()},
                {"path": "debug-sources/calendar.hgl.source", "digest": blake3::hash(b"cal bytes").to_hex().to_string()}
            ]
        });
        let current_stamp = serde_json::json!({
            "version": 2, "key": "curr", "program": {"units": [], "applications": [], "tests": null},
            "artifacts": [
                {"path": "obj/modules/mod-demo.o", "digest": blake3::hash(b"obj bytes").to_hex().to_string()}
            ],
            "retained_artifacts": [
                {"path": "results/test-demo.0.txt", "digest": blake3::hash(b"start 0\npassed 0\n").to_hex().to_string()}
            ]
        });
        fs::write(
            root.join(".hgl-previous-build.json"),
            debug_stamp.to_string(),
        )
        .unwrap();
        fs::write(root.join(".hgl-built.json"), current_stamp.to_string()).unwrap();

        let obj_modules = root.join("obj/modules");
        fs::set_permissions(&obj_modules, fs::Permissions::from_mode(0o555)).unwrap();
        let previous = PreviousBuild::read(&workspace).unwrap();
        let failed = clean(root, Some(&previous));
        fs::set_permissions(&obj_modules, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(failed.is_err());
        // Both stamps and manifest remain intact so provenance survives the failure.
        assert!(root.join(".hgl-built.json").is_file());
        assert!(root.join(".hgl-previous-build.json").is_file());
        assert!(root.join("build.ninja").is_file());
        assert!(root.join("debug-sources/core.hgl.source").is_file());
        assert!(root.join("results/test-demo.0.txt").is_file());

        let retry_previous = PreviousBuild::read(&workspace).unwrap();
        clean(root, Some(&retry_previous)).unwrap();
        assert!(!root.join("obj/modules/mod-demo.o").exists());
        assert!(!root.join("debug-sources/core.hgl.source").exists());
        assert!(!root.join("debug-sources/calendar.hgl.source").exists());
        assert!(!root.join("results/test-demo.0.txt").exists());
        assert!(!root.join(".hgl-built.json").exists());
        assert!(!root.join(".hgl-previous-build.json").exists());
        assert!(!root.join("build.ninja").exists());
        assert_eq!(
            fs::read_to_string(root.join("debug-sources/notes.hgl.source")).unwrap(),
            "handwritten"
        );
        assert_eq!(
            fs::read_to_string(root.join("results/test-demo.9.txt")).unwrap(),
            "start 0\npassed 0\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinked_stamp_and_preserves_symlinked_optional_files() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let stamp = serde_json::json!({
            "version": 2, "key": "other", "program": {"units": [], "applications": [], "tests": null},
            "artifacts": []
        });
        let external_stamp = outside.path().join(".hgl-built.json");
        fs::write(&external_stamp, stamp.to_string()).unwrap();
        std::os::unix::fs::symlink(&external_stamp, workspace.path().join(".hgl-built.json"))
            .unwrap();
        let foreign_db = workspace.path().join(COMPILE_COMMANDS);
        fs::write(&foreign_db, "keep").unwrap();
        let open = Workspace::open(Some(workspace.path()), &[], workspace.path(), false).unwrap();
        let previous = PreviousBuild::read(&open);
        assert!(previous.is_none());
        assert!(clean(workspace.path(), previous.as_ref()).is_err());
        assert_eq!(fs::read_to_string(&foreign_db).unwrap(), "keep");
        assert!(
            fs::symlink_metadata(workspace.path().join(".hgl-built.json"))
                .unwrap()
                .is_symlink()
        );

        // Even if a stamp recorded the path and matching digest, symlinks at
        // optional file slots are never unlinked.
        let owned = tempfile::tempdir().unwrap();
        let root = owned.path();
        let open = Workspace::open(Some(root), &[], root, false).unwrap();
        write_manifest(root);
        write_generated_cpp(root, "tests/test-demo-stamped.cpp");
        fs::create_dir(root.join("results")).unwrap();
        let outside_result = outside.path().join("real.txt");
        fs::write(&outside_result, "start 0\npassed 0\n").unwrap();
        let linked_result = root.join("results/test-demo-stamped.0.txt");
        std::os::unix::fs::symlink(&outside_result, &linked_result).unwrap();
        let outside_db = outside.path().join("db.json");
        fs::write(&outside_db, "[]\n").unwrap();
        let linked_db = root.join(COMPILE_COMMANDS);
        std::os::unix::fs::symlink(&outside_db, &linked_db).unwrap();
        let stamp = serde_json::json!({
            "version": 2, "key": "k", "program": {"units": [], "applications": [], "tests": null},
            "artifacts": [],
            "retained_artifacts": [
                {"path": COMPILE_COMMANDS, "digest": blake3::hash(b"[]\n").to_hex().to_string()},
                {"path": "results/test-demo-stamped.0.txt", "digest": blake3::hash(b"start 0\npassed 0\n").to_hex().to_string()}
            ]
        });
        fs::write(root.join(".hgl-built.json"), stamp.to_string()).unwrap();
        let previous = PreviousBuild::read(&open).unwrap();
        clean(root, Some(&previous)).unwrap();
        assert!(fs::symlink_metadata(&linked_result).unwrap().is_symlink());
        assert!(fs::symlink_metadata(&linked_db).unwrap().is_symlink());
        assert_eq!(
            fs::read_to_string(&outside_result).unwrap(),
            "start 0\npassed 0\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_object_directory_is_rejected_before_deletion() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let db = workspace.path().join(COMPILE_COMMANDS);
        write_manifest(workspace.path());
        fs::write(&db, "db").unwrap();
        fs::create_dir(workspace.path().join("obj")).unwrap();
        std::os::unix::fs::symlink(outside.path(), workspace.path().join("obj/modules")).unwrap();
        let sentinel = outside.path().join("mod-demo.o");
        fs::write(&sentinel, "keep").unwrap();
        assert!(clean(workspace.path(), None).is_err());
        assert!(workspace.path().join("build.ninja").is_file());
        assert!(db.is_file());
        assert_eq!(fs::read_to_string(sentinel).unwrap(), "keep");
    }
}
