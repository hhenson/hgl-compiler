//! Checks build coverage and invalidation against the SDK, with a wrapper
//! that records only Ninja calls for the program's workspace.

use std::ffi::OsString;
use std::fs::{self, File, FileTimes};
use std::io::Read as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;
use walkdir::WalkDir;

use super::executable::publish_executable;
use super::{BACKTEST, CALC, TICKS, argument, hgl_in, scratch, sdk_named, text, write};

const SOURCE: &str = "module demo.stamped\n\
    export fn pulse(const value: i64) -> i64 { yield 1s: value }\n\
    test sums { assert 2 + 3 == 5 }\n";
const APP: &str = "application demo.app\n\
    use demo.stamped::{pulse}\n\
    eval pulse(value: 51)\n";

/// A cold toolchain uses the SDK's fmt even when CPATH could shadow it.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn native_build_ignores_inherited_header_search_paths() {
    if !sdk_named() {
        return;
    }
    let root = scratch("native-search-paths");
    let source = write(&root, "stamped.hgl", SOURCE);
    write(
        &root,
        "shadow/fmt/chrono.h",
        "#include <hgl_test_cpath_header_was_selected.h>\n",
    );
    let shadow = argument(&root.join("shadow"));
    let cache = argument(&root.join("native-cache"));
    let output = hgl_in(
        &["test", &source, "--target", "cpp"],
        &[("CPATH", &shadow), ("HGL_NATIVE_CACHE", &cache)],
        &[],
    );
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.ends_with("executable cases: 1 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}

struct NinjaTrace {
    native: PathBuf,
    path: OsString,
    trace: PathBuf,
    workspace: PathBuf,
}

impl NinjaTrace {
    fn new(root: &Path) -> Self {
        let native = which::which("ninja").expect("the SDK uses Ninja");
        let bin = root.join("bin");
        publish_executable(
            &bin.join("ninja"),
            r#"#!/bin/sh
if [ "$1" = -C ] && [ "$2" = "$HGL_TEST_BUILD_WORKSPACE" ] && [ "$3" != -t ]; then
    if [ -n "$HGL_TEST_BUILD_TERM" ]; then
        trap 'exec </dev/null; touch "$HGL_TEST_BUILD_TERM"; exit 130' TERM
    fi
    printf '%s\n' build >>"$HGL_TEST_BUILD_TRACE"
    if [ -n "$HGL_TEST_BUILD_FAIL" ]; then
        printf '%s\n' 'The test stopped Ninja before compiling.' >&2
        exit 1
    fi
    if [ -n "$HGL_TEST_BUILD_HOLD_JOB" ]; then
        "$HGL_TEST_REAL_NINJA" -C "$2" -f hold.ninja </dev/null &
        wait "$!"
        exit "$?"
    fi
    if [ -n "$HGL_TEST_BUILD_TERM" ]; then
        while [ ! -f "$HGL_TEST_BUILD_TERM.release" ]; do sleep 0.02; done
    fi
    if [ -n "$HGL_TEST_BUILD_DELAY" ]; then sleep "$HGL_TEST_BUILD_DELAY"; fi
fi
exec "$HGL_TEST_REAL_NINJA" "$@"
"#
            .as_bytes(),
            fs::Permissions::from_mode(0o755),
        );
        let path = std::env::join_paths(
            std::iter::once(bin).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
        )
        .unwrap();
        Self {
            native,
            path,
            trace: root.join("ninja.log"),
            workspace: root.join("workspace"),
        }
    }

    fn command(&self, hgl: &Path, arguments: &[&str]) -> Command {
        let mut command = Command::new(hgl);
        command
            .args(arguments)
            .env("PATH", &self.path)
            .env("HGL_TEST_REAL_NINJA", &self.native)
            .env("HGL_TEST_BUILD_TRACE", &self.trace)
            .env("HGL_TEST_BUILD_WORKSPACE", &self.workspace);
        command
    }

    fn hgl(&self, arguments: &[&str]) -> Command {
        self.command(Path::new(env!("CARGO_BIN_EXE_hgl")), arguments)
    }

    fn calls(&self) -> usize {
        fs::read_to_string(&self.trace)
            .unwrap_or_default()
            .lines()
            .count()
    }
}

/// Clean rebuilds unchanged units and retires generated outputs left by an earlier program.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn clean_rebuilds_the_workspace_and_removes_retired_units() {
    if !sdk_named() {
        return;
    }
    let root = scratch("clean-rebuild");
    write(&root, "src/stamped.hgl", SOURCE);
    let app = write(&root, "src/app.hgl", APP);
    let ninja = NinjaTrace::new(&root);
    let sources = argument(&root.join("src"));
    let workspace = argument(&ninja.workspace);
    let build = |options: &[&str]| {
        let out = ninja
            .hgl(&[
                "build",
                &sources,
                "--out",
                &workspace,
                "--compile-commands",
                "--format",
                "json",
            ])
            .args(options)
            .output()
            .unwrap();
        serde_json::from_str::<Value>(&success(out)).unwrap()
    };
    assert_eq!(build(&["--clean", "--tests"])["reused"], false);
    assert_eq!(build(&["--tests"])["reused"], true);
    // Debug builds stamp each bundled source copy with its digest; a later
    // ordinary build carries that provenance so `--clean` still removes
    // unmodified copies while preserving edited copies and neighbours.
    build(&["--debug", "--tests"]);
    let debug_copy = ninja.workspace.join("debug-sources/core.hgl.source");
    let edited_copy = ninja.workspace.join("debug-sources/calendar.hgl.source");
    assert!(debug_copy.is_file());
    assert!(edited_copy.is_file());
    fs::write(&edited_copy, "user edit").unwrap();
    let nearby = write(
        &ninja.workspace,
        "debug-sources/notes.hgl.source",
        "handwritten",
    );
    build(&["--tests"]);
    build(&["--clean", "--tests"]);
    assert!(!debug_copy.exists());
    assert_eq!(fs::read_to_string(&edited_copy).unwrap(), "user edit");
    assert_eq!(fs::read_to_string(nearby).unwrap(), "handwritten");
    fs::remove_file(&edited_copy).unwrap();

    // Run real test and application hosts so recorded result files exist before retiring them.
    success(
        ninja
            .hgl(&["test", &sources, "--target", "cpp", "--keep", &workspace])
            .output()
            .unwrap(),
    );
    success(
        ninja
            .hgl(&["run", &sources, "--target", "cpp", "--keep", &workspace])
            .output()
            .unwrap(),
    );
    let recorded_test = ninja.workspace.join("results/test-demo-stamped.0.txt");
    let recorded_app = ninja.workspace.join("results/app-demo-app.run.txt");
    assert!(recorded_test.is_file());
    assert!(recorded_app.is_file());

    fs::remove_file(app).unwrap();
    build(&[]);
    let retired = [
        "applications/app-demo-app.cpp",
        "run/app-demo-app",
        "obj/applications/app-demo-app.o",
        "tests/test-demo-stamped.cpp",
        "bin/test-demo-stamped",
        "obj/tests/test-demo-stamped.o",
    ];
    for file in retired {
        assert!(ninja.workspace.join(file).is_file(), "{file}");
    }
    let object = ninja.workspace.join("obj/modules/mod-demo-stamped.o");
    let old = File::open(&object).unwrap();
    old.set_modified(std::time::SystemTime::UNIX_EPOCH).unwrap();
    let note = write(
        &ninja.workspace,
        "results/test-demo-stamped.notes.txt",
        "keep",
    );
    let handwritten = write(&ninja.workspace, "modules/manual.cpp", "// mine");
    let other = write(
        &ninja.workspace,
        "debug/compile_commands.json",
        "other profile",
    );
    let got = build(&["--clean"]);
    assert_eq!(got["reused"], false);
    assert_eq!(
        got["compileCommands"],
        argument(&ninja.workspace.join("compile_commands.json"))
    );
    assert_ne!(
        fs::metadata(object).unwrap().modified().unwrap(),
        old.metadata().unwrap().modified().unwrap()
    );
    for file in retired {
        assert!(!ninja.workspace.join(file).exists(), "{file}");
    }
    assert!(!recorded_test.exists());
    assert!(!recorded_app.exists());
    assert_eq!(fs::read_to_string(note).unwrap(), "keep");
    assert_eq!(fs::read_to_string(handwritten).unwrap(), "// mine");
    assert_eq!(fs::read_to_string(other).unwrap(), "other profile");
    assert_eq!(build(&[])["reused"], true);

    write(
        &root,
        "src/stamped.hgl",
        "module demo.stamped\nconst fn broken() -> i64 => false",
    );
    assert!(
        !ninja
            .hgl(&["build", &sources, "--out", &workspace, "--clean"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(!ninja.workspace.join(".hgl-built.json").exists());
    assert!(
        !ninja
            .workspace
            .join("obj/modules/mod-demo-stamped.o")
            .exists()
    );
    write(&root, "src/stamped.hgl", SOURCE);
    assert_eq!(build(&[])["reused"], false);
}

/// Clean removes optional files only when unchanged from recorded stamp
/// provenance, preserving user-created files even in valid host/compdb format.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn clean_requires_recorded_provenance_for_optional_workspace_files() {
    if !sdk_named() {
        return;
    }
    let root = scratch("clean-provenance");
    write(&root, "src/stamped.hgl", SOURCE);
    write(&root, "src/app.hgl", APP);
    let ninja = NinjaTrace::new(&root);
    let sources = argument(&root.join("src"));
    let workspace = argument(&ninja.workspace);
    let compdb = ninja.workspace.join("compile_commands.json");
    let build = |options: &[&str]| {
        let out = ninja
            .hgl(&["build", &sources, "--out", &workspace, "--format", "json"])
            .args(options)
            .output()
            .unwrap();
        serde_json::from_str::<Value>(&success(out)).unwrap()
    };

    // Build without `--compile-commands` and without running tests or the app.
    assert_eq!(build(&["--tests"])["reused"], false);
    // Place user-created files at expected names, including valid host format
    // and valid Ninja compdb format, before HGL has ever produced them.
    let lookalike_compdb = success(
        Command::new(&ninja.native)
            .arg("-C")
            .arg(&ninja.workspace)
            .args(["-t", "compdb", "unit", "entry"])
            .output()
            .unwrap(),
    );
    fs::write(&compdb, &lookalike_compdb).unwrap();
    let valid_test_result = "start 0\npassed 0\n";
    let valid_app_result = "start\npublished 1970-01-01T00:00:01Z 51\nstopped\n";
    let user_test_result = write(
        &ninja.workspace,
        "results/test-demo-stamped.0.txt",
        valid_test_result,
    );
    let user_plus_result = write(
        &ninja.workspace,
        "results/test-demo-stamped.+0.txt",
        valid_test_result,
    );
    let user_app_result = write(
        &ninja.workspace,
        "results/app-demo-app.run.txt",
        valid_app_result,
    );
    let user_debug_result = write(
        &ninja.workspace,
        "debug-results/test-demo-stamped.txt",
        valid_test_result,
    );
    let top_level_results = write(&ninja.workspace, "results.txt", valid_app_result);
    assert_eq!(build(&["--clean", "--tests"])["reused"], false);
    assert_eq!(fs::read_to_string(&compdb).unwrap(), lookalike_compdb);
    assert_eq!(
        fs::read_to_string(&user_test_result).unwrap(),
        valid_test_result
    );
    assert_eq!(
        fs::read_to_string(&user_plus_result).unwrap(),
        valid_test_result
    );
    assert_eq!(
        fs::read_to_string(&user_app_result).unwrap(),
        valid_app_result
    );
    assert_eq!(
        fs::read_to_string(&user_debug_result).unwrap(),
        valid_test_result
    );
    assert_eq!(
        fs::read_to_string(&top_level_results).unwrap(),
        valid_app_result
    );

    // Export a genuine compilation database, edit it, and confirm `--clean` preserves it.
    assert_eq!(build(&["--tests", "--compile-commands"])["reused"], true);
    let generated = fs::read_to_string(&compdb).unwrap();
    let edited_db = format!("{generated}\n");
    fs::write(&compdb, &edited_db).unwrap();
    assert_eq!(build(&["--clean", "--tests"])["reused"], false);
    assert_eq!(fs::read_to_string(&compdb).unwrap(), edited_db);

    // An unmodified compiler-generated database survives an ordinary rebuild
    // and is removed by `--clean` when `--compile-commands` is omitted.
    assert_eq!(build(&["--tests", "--compile-commands"])["reused"], true);
    assert_eq!(build(&[])["reused"], false);
    assert!(compdb.is_file());
    assert_eq!(build(&["--clean", "--tests"])["reused"], false);
    assert!(!compdb.exists());

    // Run real `hgl test` and `hgl run`: unmodified recorded result is removed,
    // while a user-edited recorded result is preserved.
    success(
        ninja
            .hgl(&["test", &sources, "--target", "cpp", "--keep", &workspace])
            .output()
            .unwrap(),
    );
    success(
        ninja
            .hgl(&["run", &sources, "--target", "cpp", "--keep", &workspace])
            .output()
            .unwrap(),
    );
    fs::write(&user_app_result, "user edited run output\n").unwrap();
    assert_eq!(build(&["--clean"])["reused"], false);
    assert!(!Path::new(&user_test_result).exists());
    assert_eq!(
        fs::read_to_string(&user_app_result).unwrap(),
        "user edited run output\n"
    );
    assert_eq!(
        fs::read_to_string(&top_level_results).unwrap(),
        valid_app_result
    );

    // Multi-step debug -> ordinary -> ordinary -> clean removes unmodified
    // debug copies while keeping edited copies and handwritten neighbours.
    assert_eq!(build(&["--debug", "--tests"])["reused"], false);
    let debug_core = ninja.workspace.join("debug-sources/core.hgl.source");
    let debug_calendar = ninja.workspace.join("debug-sources/calendar.hgl.source");
    assert!(debug_core.is_file());
    assert!(debug_calendar.is_file());
    fs::write(&debug_calendar, "edited copy").unwrap();
    let neighbor = write(
        &ninja.workspace,
        "debug-sources/notes.hgl.source",
        "handwritten",
    );
    assert_eq!(build(&["--tests"])["reused"], false);
    assert_eq!(build(&[])["reused"], false);
    assert_eq!(build(&["--clean"])["reused"], false);
    assert!(!debug_core.exists());
    assert_eq!(fs::read_to_string(&debug_calendar).unwrap(), "edited copy");
    assert_eq!(fs::read_to_string(&neighbor).unwrap(), "handwritten");

    // A failed first debug build records the written debug copies in
    // `.hgl-previous-build.json`; a subsequent `--clean` removes unmodified
    // copies while preserving edits.
    let failed_ws = root.join("failed-debug-ws");
    let failed_arg = argument(&failed_ws);
    let failed = ninja
        .hgl(&[
            "build",
            &sources,
            "--out",
            &failed_arg,
            "--debug",
            "--tests",
        ])
        .env("HGL_TEST_BUILD_WORKSPACE", &failed_ws)
        .env("HGL_TEST_BUILD_FAIL", "1")
        .output()
        .unwrap();
    assert!(!failed.status.success());
    let failed_core = failed_ws.join("debug-sources/core.hgl.source");
    let failed_calendar = failed_ws.join("debug-sources/calendar.hgl.source");
    assert!(failed_core.is_file());
    assert!(failed_calendar.is_file());
    fs::write(&failed_calendar, "edited after failure").unwrap();
    success(
        ninja
            .hgl(&["build", &sources, "--out", &failed_arg, "--clean"])
            .env("HGL_TEST_BUILD_WORKSPACE", &failed_ws)
            .output()
            .unwrap(),
    );
    assert!(!failed_core.exists());
    assert_eq!(
        fs::read_to_string(&failed_calendar).unwrap(),
        "edited after failure"
    );
}

/// A generated source name alone cannot authorize deleting an object or
/// executable in an emit-only directory, after an unrelated build, or after
/// the user replaced a previously built executable.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn clean_preserves_unrecorded_executables_and_objects() {
    if !sdk_named() {
        return;
    }
    let root = scratch("clean-unrecorded-exec");
    let audit_app = "application audit.runit\n\
        fn pulse(const value: i64) -> i64 { yield 1s: value }\n\
        eval pulse(value: 7)\n";
    let other_mod = "module other.keep\nexport const fn answer() -> i64 => 42\n";
    write(&root, "audit/runit.hgl", audit_app);
    write(&root, "other/keep.hgl", other_mod);
    let audit_src = argument(&root.join("audit"));
    let other_src = argument(&root.join("other"));
    let ninja = NinjaTrace::new(&root);

    // 1. Emit-only directory with a handwritten launcher at `run/app-audit-runit`.
    let emit_dir = root.join("emit-only");
    let emit_arg = argument(&emit_dir);
    success(
        ninja
            .hgl(&["emit", &audit_src, "--target", "cpp", "--out", &emit_arg])
            .output()
            .unwrap(),
    );
    let emit_launcher = write(&emit_dir, "run/app-audit-runit", "#!/bin/sh\necho emit\n");
    let emit_obj = write(&emit_dir, "obj/applications/app-audit-runit.o", "user obj");
    success(
        ninja
            .hgl(&["build", &other_src, "--clean", "--out", &emit_arg])
            .env("HGL_TEST_BUILD_WORKSPACE", &emit_dir)
            .output()
            .unwrap(),
    );
    assert!(!emit_dir.join("applications/app-audit-runit.cpp").exists());
    assert_eq!(
        fs::read_to_string(&emit_launcher).unwrap(),
        "#!/bin/sh\necho emit\n"
    );
    assert_eq!(fs::read_to_string(&emit_obj).unwrap(), "user obj");

    // 2. Owned workspace built for an unrelated program whose manifest omitted
    //    `audit.runit`, with `applications/app-audit-runit.cpp` present beside
    //    a handwritten launcher and object.
    let unrelated_dir = root.join("unrelated-owned");
    let unrelated_arg = argument(&unrelated_dir);
    success(
        ninja
            .hgl(&["build", &other_src, "--out", &unrelated_arg])
            .env("HGL_TEST_BUILD_WORKSPACE", &unrelated_dir)
            .output()
            .unwrap(),
    );
    success(
        ninja
            .hgl(&[
                "emit",
                &audit_src,
                "--target",
                "cpp",
                "--out",
                &unrelated_arg,
            ])
            .output()
            .unwrap(),
    );
    let unrelated_launcher = write(
        &unrelated_dir,
        "run/app-audit-runit",
        "#!/bin/sh\necho owned\n",
    );
    let unrelated_obj = write(
        &unrelated_dir,
        "obj/applications/app-audit-runit.o",
        "handwritten obj",
    );
    success(
        ninja
            .hgl(&["build", &other_src, "--clean", "--out", &unrelated_arg])
            .env("HGL_TEST_BUILD_WORKSPACE", &unrelated_dir)
            .output()
            .unwrap(),
    );
    assert!(
        !unrelated_dir
            .join("applications/app-audit-runit.cpp")
            .exists()
    );
    assert_eq!(
        fs::read_to_string(&unrelated_launcher).unwrap(),
        "#!/bin/sh\necho owned\n"
    );
    assert_eq!(
        fs::read_to_string(&unrelated_obj).unwrap(),
        "handwritten obj"
    );

    // 3. Built application whose executable was replaced by a handwritten
    //    launcher: clean preserves the replacement while removing unmodified
    //    recorded objects from the retired application.
    let replaced_dir = root.join("replaced-exec");
    let replaced_arg = argument(&replaced_dir);
    success(
        ninja
            .hgl(&["build", &audit_src, "--out", &replaced_arg])
            .env("HGL_TEST_BUILD_WORKSPACE", &replaced_dir)
            .output()
            .unwrap(),
    );
    let replaced_launcher = replaced_dir.join("run/app-audit-runit");
    let built_obj = replaced_dir.join("obj/applications/app-audit-runit.o");
    assert!(replaced_launcher.is_file());
    assert!(built_obj.is_file());
    fs::write(&replaced_launcher, "#!/bin/sh\necho custom\n").unwrap();
    success(
        ninja
            .hgl(&["build", &other_src, "--clean", "--out", &replaced_arg])
            .env("HGL_TEST_BUILD_WORKSPACE", &replaced_dir)
            .output()
            .unwrap(),
    );
    assert_eq!(
        fs::read_to_string(&replaced_launcher).unwrap(),
        "#!/bin/sh\necho custom\n"
    );
    assert!(!built_obj.exists());
}

/// A partial clean failure (such as a temporarily read-only `obj/modules`)
/// must preserve the workspace stamps until the corresponding products have
/// been removed, so a retry after restoring permissions cleans them cleanly.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn clean_preserves_provenance_through_partial_failure_until_retry() {
    if !sdk_named() {
        return;
    }
    let root = scratch("clean-partial-failure");
    write(&root, "src/stamped.hgl", SOURCE);
    let sources = argument(&root.join("src"));
    let ninja = NinjaTrace::new(&root);
    let workspace = argument(&ninja.workspace);

    // Build a debug+test workspace, run its test to record a result file, then
    // switch to an ordinary build so both stamps exist and the test artifacts
    // and debug copies are retired.
    success(
        ninja
            .hgl(&["build", &sources, "--debug", "--tests", "--out", &workspace])
            .output()
            .unwrap(),
    );
    success(
        ninja
            .hgl(&["test", &sources, "--target", "cpp", "--keep", &workspace])
            .output()
            .unwrap(),
    );
    success(
        ninja
            .hgl(&["build", &sources, "--out", &workspace])
            .output()
            .unwrap(),
    );
    let debug_core = ninja.workspace.join("debug-sources/core.hgl.source");
    let debug_calendar = ninja.workspace.join("debug-sources/calendar.hgl.source");
    let recorded_result = ninja.workspace.join("results/test-demo-stamped.0.txt");
    let retired_test_bin = ninja.workspace.join("bin/test-demo-stamped");
    assert!(debug_core.is_file());
    assert!(debug_calendar.is_file());
    assert!(recorded_result.is_file());
    assert!(retired_test_bin.is_file());
    let neighbour = write(
        &ninja.workspace,
        "debug-sources/notes.hgl.source",
        "handwritten neighbour",
    );
    let unrecorded_db = write(&ninja.workspace, "compile_commands.json", "[]\n");
    let unrecorded_result = write(
        &ninja.workspace,
        "results/test-demo-stamped.9.txt",
        "start 0\npassed 0\n",
    );

    let obj_modules = ninja.workspace.join("obj/modules");
    fs::set_permissions(&obj_modules, fs::Permissions::from_mode(0o555)).unwrap();
    let failed = ninja
        .hgl(&["build", &sources, "--clean", "--out", &workspace])
        .output()
        .unwrap();
    fs::set_permissions(&obj_modules, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(!failed.status.success());
    // Stamps and manifest must remain on disk while recorded products could not be removed.
    assert!(ninja.workspace.join(".hgl-built.json").is_file());
    assert!(ninja.workspace.join(".hgl-previous-build.json").is_file());
    assert!(ninja.workspace.join("build.ninja").is_file());

    // Restoring write access and running the same clean must remove all
    // recorded debug copies and retired test outputs while keeping neighbours
    // and unrecorded optional files intact.
    let recovered: Value = serde_json::from_str(&success(
        ninja
            .hgl(&[
                "build", &sources, "--clean", "--out", &workspace, "--format", "json",
            ])
            .output()
            .unwrap(),
    ))
    .unwrap();
    assert_eq!(recovered["reused"], false);
    assert!(!debug_core.exists());
    assert!(!debug_calendar.exists());
    assert!(!recorded_result.exists());
    assert!(!retired_test_bin.exists());
    assert_eq!(
        fs::read_to_string(&neighbour).unwrap(),
        "handwritten neighbour"
    );
    assert_eq!(fs::read_to_string(&unrecorded_db).unwrap(), "[]\n");
    assert_eq!(
        fs::read_to_string(&unrecorded_result).unwrap(),
        "start 0\npassed 0\n"
    );
}

/// After a failed debug upgrade writes newer bundled Core bytes into a
/// workspace whose `.hgl-previous-build.json` still holds the prior build's
/// `artifacts`, both direct `--clean` and carry-forward across an ordinary
/// release build must use the newest writer's digest: remove untouched latest
/// bytes and preserve restored old bytes.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn clean_prefers_latest_debug_copy_digest_after_failed_upgrade() {
    if !sdk_named() {
        return;
    }
    let root = scratch("clean-failed-debug-upgrade");
    let ninja = NinjaTrace::new(&root);

    let setup_failed_upgrade = |name: &str| -> (PathBuf, String, String, Vec<u8>) {
        let ws = root.join(name);
        let ws_arg = argument(&ws);
        let src_dir = root.join(format!("{name}-src"));
        let source_file = write(&src_dir, "stamped.hgl", SOURCE);
        let sources = argument(&src_dir);
        success(
            ninja
                .hgl(&["build", &sources, "--debug", "--tests", "--out", &ws_arg])
                .env("HGL_TEST_BUILD_WORKSPACE", &ws)
                .output()
                .unwrap(),
        );
        // Simulate an earlier compiler release whose bundled `core.hgl.source`
        // bytes differed from the current compiler's bundled Core.
        let core_path = ws.join("debug-sources/core.hgl.source");
        let latest_bytes = fs::read(&core_path).unwrap();
        let mut old_bytes = b"// prior compiler release core\n".to_vec();
        old_bytes.extend_from_slice(&latest_bytes);
        fs::write(&core_path, &old_bytes).unwrap();
        let stamp_path = ws.join(".hgl-built.json");
        let mut stamp: Value = serde_json::from_slice(&fs::read(&stamp_path).unwrap()).unwrap();
        let old_digest = blake3::hash(&old_bytes).to_hex().to_string();
        for artifact in stamp["artifacts"].as_array_mut().unwrap() {
            if artifact["path"] == "debug-sources/core.hgl.source" {
                artifact["digest"] = Value::String(old_digest.clone());
            }
        }
        fs::write(&stamp_path, serde_json::to_string(&stamp).unwrap()).unwrap();

        // Trigger a debug rebuild whose module C++ changes while `obj/modules`
        // is temporarily read-only: the upgrade writes the newer
        // `core.hgl.source` and records its digest in `.hgl-previous-build.json`'s
        // `retained_artifacts` before native compilation fails.
        fs::write(
            &source_file,
            SOURCE.replace("yield 1s: value", "yield 2s: value"),
        )
        .unwrap();
        let obj_modules = ws.join("obj/modules");
        fs::set_permissions(&obj_modules, fs::Permissions::from_mode(0o555)).unwrap();
        let failed = ninja
            .hgl(&["build", &sources, "--debug", "--tests", "--out", &ws_arg])
            .env("HGL_TEST_BUILD_WORKSPACE", &ws)
            .output()
            .unwrap();
        fs::set_permissions(&obj_modules, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(!failed.status.success());
        assert!(!stamp_path.exists());
        assert_eq!(fs::read(&core_path).unwrap(), latest_bytes);
        let prev: Value =
            serde_json::from_slice(&fs::read(ws.join(".hgl-previous-build.json")).unwrap())
                .unwrap();
        assert!(prev["artifacts"].as_array().unwrap().iter().any(|a| {
            a["path"] == "debug-sources/core.hgl.source" && a["digest"] == old_digest
        }));
        let latest_digest = blake3::hash(&latest_bytes).to_hex().to_string();
        assert!(
            prev["retained_artifacts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| {
                    a["path"] == "debug-sources/core.hgl.source" && a["digest"] == latest_digest
                })
        );
        (ws, ws_arg, sources, old_bytes)
    };

    // 1. Direct clean with untouched latest bytes removes `core.hgl.source`.
    let (ws_latest, ws_latest_arg, sources, _) = setup_failed_upgrade("direct-latest");
    success(
        ninja
            .hgl(&["build", &sources, "--clean", "--out", &ws_latest_arg])
            .env("HGL_TEST_BUILD_WORKSPACE", &ws_latest)
            .output()
            .unwrap(),
    );
    assert!(!ws_latest.join("debug-sources/core.hgl.source").exists());

    // 2. Direct clean with old bytes restored from backup preserves `core.hgl.source`.
    let (ws_restored, ws_restored_arg, sources, old_bytes) =
        setup_failed_upgrade("direct-restored");
    let restored_core = ws_restored.join("debug-sources/core.hgl.source");
    fs::write(&restored_core, &old_bytes).unwrap();
    success(
        ninja
            .hgl(&["build", &sources, "--clean", "--out", &ws_restored_arg])
            .env("HGL_TEST_BUILD_WORKSPACE", &ws_restored)
            .output()
            .unwrap(),
    );
    assert_eq!(fs::read(&restored_core).unwrap(), old_bytes);

    // 3. Intervening ordinary release build carries forward the newest writer's
    //    digest for untouched latest bytes, then `--clean` removes it.
    let (ws_carry_latest, ws_carry_latest_arg, sources, _) = setup_failed_upgrade("carry-latest");
    success(
        ninja
            .hgl(&["build", &sources, "--out", &ws_carry_latest_arg])
            .env("HGL_TEST_BUILD_WORKSPACE", &ws_carry_latest)
            .output()
            .unwrap(),
    );
    success(
        ninja
            .hgl(&["build", &sources, "--clean", "--out", &ws_carry_latest_arg])
            .env("HGL_TEST_BUILD_WORKSPACE", &ws_carry_latest)
            .output()
            .unwrap(),
    );
    assert!(
        !ws_carry_latest
            .join("debug-sources/core.hgl.source")
            .exists()
    );

    // 4. Intervening ordinary release build with restored old bytes does not
    //    fall back to the older `artifacts` digest; `--clean` preserves it.
    let (ws_carry_restored, ws_carry_restored_arg, sources, old_bytes) =
        setup_failed_upgrade("carry-restored");
    let carry_restored_core = ws_carry_restored.join("debug-sources/core.hgl.source");
    fs::write(&carry_restored_core, &old_bytes).unwrap();
    success(
        ninja
            .hgl(&["build", &sources, "--out", &ws_carry_restored_arg])
            .env("HGL_TEST_BUILD_WORKSPACE", &ws_carry_restored)
            .output()
            .unwrap(),
    );
    success(
        ninja
            .hgl(&[
                "build",
                &sources,
                "--clean",
                "--out",
                &ws_carry_restored_arg,
            ])
            .env("HGL_TEST_BUILD_WORKSPACE", &ws_carry_restored)
            .output()
            .unwrap(),
    );
    assert_eq!(fs::read(&carry_restored_core).unwrap(), old_bytes);
}

/// When a legacy 0.2.9 debug build followed by an ordinary release build left
/// `debug-sources/core.hgl.source` recorded only in `.hgl-previous-build.json`,
/// a failing or interrupted ordinary build of another program must persist that
/// earlier verified inventory before withdrawing `.hgl-built.json`, so a later
/// `--clean` still removes unmodified copies while keeping edited neighbours.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn failed_ordinary_build_after_legacy_debug_then_release_preserves_provenance() {
    if !sdk_named() {
        return;
    }
    let root = scratch("clean-legacy-failed-ordinary");
    write(&root, "src/stamped.hgl", SOURCE);
    write(
        &root,
        "other/keep.hgl",
        "module other.keep\nexport const fn answer() -> i64 => 42\n",
    );
    let sources = argument(&root.join("src"));
    let other_sources = argument(&root.join("other"));
    let ninja = NinjaTrace::new(&root);

    let setup_legacy_debug_then_release = |name: &str| -> (PathBuf, String) {
        let ws = root.join(name);
        let ws_arg = argument(&ws);
        // Step 1: debug build writes and stamps debug copies.
        success(
            ninja
                .hgl(&["build", &sources, "--debug", "--tests", "--out", &ws_arg])
                .env("HGL_TEST_BUILD_WORKSPACE", &ws)
                .output()
                .unwrap(),
        );
        // Step 2: ordinary release build moves debug stamp to `.hgl-previous-build.json`.
        success(
            ninja
                .hgl(&["build", &sources, "--out", &ws_arg])
                .env("HGL_TEST_BUILD_WORKSPACE", &ws)
                .output()
                .unwrap(),
        );
        // Strip `retained_artifacts` from `.hgl-built.json` to model a 0.2.9
        // release stamp where `debug-sources/*` lives ONLY in `.hgl-previous-build.json`.
        let stamp_path = ws.join(".hgl-built.json");
        let mut active: Value = serde_json::from_slice(&fs::read(&stamp_path).unwrap()).unwrap();
        active.as_object_mut().unwrap().remove("retained_artifacts");
        fs::write(&stamp_path, serde_json::to_string(&active).unwrap()).unwrap();

        // Edited debug copy and handwritten neighbour controls.
        fs::write(ws.join("debug-sources/calendar.hgl.source"), "edited copy").unwrap();
        write(&ws, "debug-sources/notes.hgl.source", "handwritten");
        assert!(ws.join("debug-sources/core.hgl.source").is_file());
        (ws, ws_arg)
    };

    // 1. Failing ordinary build of another program while `obj/modules` is read-only.
    let (ws_ro, ws_ro_arg) = setup_legacy_debug_then_release("readonly-fail");
    let obj_modules = ws_ro.join("obj/modules");
    fs::set_permissions(&obj_modules, fs::Permissions::from_mode(0o555)).unwrap();
    let failed = ninja
        .hgl(&["build", &other_sources, "--out", &ws_ro_arg])
        .env("HGL_TEST_BUILD_WORKSPACE", &ws_ro)
        .output()
        .unwrap();
    fs::set_permissions(&obj_modules, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(!failed.status.success());
    assert!(!ws_ro.join(".hgl-built.json").exists());
    assert!(ws_ro.join(".hgl-previous-build.json").is_file());

    success(
        ninja
            .hgl(&["build", &other_sources, "--clean", "--out", &ws_ro_arg])
            .env("HGL_TEST_BUILD_WORKSPACE", &ws_ro)
            .output()
            .unwrap(),
    );
    assert!(!ws_ro.join("debug-sources/core.hgl.source").exists());
    assert_eq!(
        fs::read_to_string(ws_ro.join("debug-sources/calendar.hgl.source")).unwrap(),
        "edited copy"
    );
    assert_eq!(
        fs::read_to_string(ws_ro.join("debug-sources/notes.hgl.source")).unwrap(),
        "handwritten"
    );

    // 2. Interrupted ordinary build of another program withdraws `.hgl-built.json`
    //    and still preserves the earlier verified `core.hgl.source` provenance.
    let (ws_int, ws_int_arg) = setup_legacy_debug_then_release("interrupted-ordinary");
    let calls = ninja.calls();
    let building = ninja
        .hgl(&["build", &other_sources, "--out", &ws_int_arg])
        .env("HGL_TEST_BUILD_WORKSPACE", &ws_int)
        .env("HGL_TEST_BUILD_DELAY", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    assert!(wait_until(|| ninja.calls() > calls));
    assert!(
        Command::new("kill")
            .args(["-INT", &building.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let interrupted = building.wait_with_output().unwrap();
    assert_eq!(interrupted.status.code(), Some(2));
    assert!(!ws_int.join(".hgl-built.json").exists());
    assert!(ws_int.join(".hgl-previous-build.json").is_file());

    success(
        ninja
            .hgl(&["build", &other_sources, "--clean", "--out", &ws_int_arg])
            .env("HGL_TEST_BUILD_WORKSPACE", &ws_int)
            .output()
            .unwrap(),
    );
    assert!(!ws_int.join("debug-sources/core.hgl.source").exists());
    assert_eq!(
        fs::read_to_string(ws_int.join("debug-sources/calendar.hgl.source")).unwrap(),
        "edited copy"
    );
    assert_eq!(
        fs::read_to_string(ws_int.join("debug-sources/notes.hgl.source")).unwrap(),
        "handwritten"
    );
}

/// The compilation database comes from the exact workspace Ninja rules,
/// can be restored on a hit, and preserves a reader's complete snapshot.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn writes_ninjas_compilation_database_without_rebuilding() {
    if !sdk_named() {
        return;
    }
    let root = scratch("compile-commands");
    write(&root, "src/stamped.hgl", SOURCE);
    write(&root, "src/app.hgl", APP);
    let mut ninja = NinjaTrace::new(&root);
    ninja.workspace = root.join("workspace with spaces");
    let sources = argument(&root.join("src"));
    let workspace = argument(&ninja.workspace);
    let path = ninja.workspace.join("compile_commands.json");
    let cache = root.join("native-cache");
    let trace = root.join("compiler.log");
    let compiler = root.join("bin/compiler");
    publish_executable(
        &compiler,
        r#"#!/bin/sh
if [ "$1" = -dM ] && [ "$2" = -E ]; then
    printf '%s\n' identify >>"$HGL_TEST_CXX_TRACE"
else
    printf '%s\n' compile >>"$HGL_TEST_CXX_TRACE"
fi
exec "$HGL_TEST_REAL_CXX" "$@"
"#
        .as_bytes(),
        fs::Permissions::from_mode(0o755),
    );
    let real = std::env::var_os("CXX").unwrap_or_else(|| {
        if cfg!(target_os = "macos") {
            "clang++"
        } else {
            "g++-15"
        }
        .into()
    });
    let real = which::which(real).expect("the SDK uses a C++ compiler");
    let build = |options: &[&str]| -> Value {
        let output = ninja
            .hgl(&[
                "build", &sources, "--out", &workspace, "--tests", "--format", "json",
            ])
            .args(options)
            .env("CXX", &compiler)
            .env("HGL_TEST_CXX_TRACE", &trace)
            .env("HGL_TEST_REAL_CXX", &real)
            .env("HGL_NATIVE_CACHE", &cache)
            .output()
            .unwrap();
        serde_json::from_str(&success(output)).unwrap()
    };
    let first = build(&["--compile-commands"]);
    assert_eq!(first["compileCommands"], argument(&path));
    assert_eq!(first["reused"], false);
    let original = ninja_database(&ninja, &path);
    let entries: Vec<Value> = serde_json::from_str(&original).unwrap();
    let files: Vec<_> = entries
        .iter()
        .map(|entry| entry["file"].as_str().unwrap())
        .collect();
    assert!(files.contains(&"modules/mod-demo-stamped.cpp"));
    assert!(files.contains(&"applications/app-demo-app.cpp"));
    assert!(files.contains(&"tests/test-demo-stamped.cpp"));
    assert_eq!(entries.len(), first["units"].as_array().unwrap().len() + 2);

    let known_time = std::time::UNIX_EPOCH + Duration::from_secs(1234567);
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(known_time)
        .unwrap();
    assert_eq!(build(&["--compile-commands"])["reused"], true);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), known_time);
    assert!(build(&[]).get("compileCommands").is_none());
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), known_time);

    let artifacts = native_times(&ninja.workspace);
    let libraries = native_times(&cache);
    fs::remove_file(&path).unwrap();
    assert_eq!(build(&[])["reused"], true);
    assert!(!path.exists());
    fs::write(&trace, "").unwrap();
    assert_eq!(build(&["--compile-commands"])["reused"], true);
    assert_eq!(fs::read_to_string(&trace).unwrap(), "identify\n");
    assert_eq!(ninja.calls(), 1);
    assert_eq!(ninja_database(&ninja, &path), original);
    assert_eq!(native_times(&cache), libraries);
    // The file restored on the hit is the only workspace file newly written.
    assert_eq!(native_times(&ninja.workspace), artifacts);

    let mut reader = File::open(&path).unwrap();
    write(
        &root,
        "src/extra.hgl",
        "module demo.extra\nexport fn next(value: i64) -> i64 => value + 1\n",
    );
    assert_eq!(build(&["--compile-commands"])["reused"], false);
    let changed = ninja_database(&ninja, &path);
    assert_ne!(changed, original);
    assert!(changed.contains("modules/mod-demo-extra.cpp"));
    let mut snapshot = String::new();
    reader.read_to_string(&mut snapshot).unwrap();
    assert_eq!(snapshot, original);

    let output = ninja
        .hgl(&[
            "build",
            &sources,
            "--debug",
            "--compile-commands",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let launched: Value = serde_json::from_str(&success(output)).unwrap();
    let debug = PathBuf::from(launched["workspace"].as_str().unwrap());
    assert!(debug.parent().unwrap().ends_with("debug-programs"));
    let debug_path = debug.join("compile_commands.json");
    assert_eq!(launched["compileCommands"], argument(&debug_path));
    let debug_database = ninja_database(&ninja, &debug_path);
    let entries: Vec<Value> = serde_json::from_str(&debug_database).unwrap();
    assert_eq!(
        entries.len(),
        launched["units"].as_array().unwrap().len() + 1
    );
    assert!(entries.iter().all(|entry| {
        entry["command"]
            .as_str()
            .unwrap()
            .split_whitespace()
            .any(|word| word == "-g")
    }));
    assert!(!debug_database.contains("tests/test-demo-stamped.cpp"));
}

fn ninja_database(ninja: &NinjaTrace, path: &Path) -> String {
    let workspace = path.parent().unwrap();
    let expected = success(
        Command::new(&ninja.native)
            .arg("-C")
            .arg(workspace)
            .args(["-t", "compdb", "unit", "entry"])
            .output()
            .unwrap(),
    );
    let actual = fs::read_to_string(path).unwrap();
    assert_eq!(actual, expected);
    let entries: Vec<Value> = serde_json::from_str(&actual).unwrap();
    assert!(!entries.is_empty());
    for entry in entries {
        assert_eq!(entry["directory"], argument(workspace));
    }
    actual
}

fn native_times(root: &Path) -> std::collections::BTreeMap<PathBuf, std::time::SystemTime> {
    WalkDir::new(root)
        .into_iter()
        .map(Result::unwrap)
        .filter(|entry| entry.file_type().is_file() && entry.file_name() != "compile_commands.json")
        .map(|entry| {
            (
                entry.path().to_owned(),
                entry.metadata().unwrap().modified().unwrap(),
            )
        })
        .collect()
}

fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}{}",
        text(&output.stdout),
        text(&output.stderr)
    );
    text(&output.stdout).to_string()
}

fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while !condition() && start.elapsed() < Duration::from_secs(30) {
        std::thread::sleep(Duration::from_millis(10));
    }
    condition()
}

/// A miss must take away permission to run without forgetting which
/// native artifacts a previous successful build produced.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn failed_or_incomplete_builds_keep_verified_incremental_outputs() {
    if !sdk_named() {
        return;
    }
    let root = scratch("build-retain-artifacts");
    let source = write(&root, "src/stamped.hgl", SOURCE);
    write(&root, "src/app.hgl", APP);
    let ninja = NinjaTrace::new(&root);
    let sources = argument(&root.join("src"));
    let workspace = argument(&ninja.workspace);
    let stamp = ninja.workspace.join(".hgl-built.json");
    let build = ["build", &sources, "--out", &workspace, "--format", "json"];
    success(ninja.hgl(&build).output().unwrap());
    let description: Value = serde_json::from_slice(&fs::read(&stamp).unwrap()).unwrap();
    let artifact_times = || {
        description["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|artifact| {
                let path = ninja.workspace.join(artifact["path"].as_str().unwrap());
                fs::metadata(path).unwrap().modified().unwrap()
            })
            .collect::<Vec<_>>()
    };
    let expected = artifact_times();
    let recover = || {
        assert!(!stamp.exists());
        let calls = ninja.calls();
        let recovered: Value =
            serde_json::from_str(&success(ninja.hgl(&build).output().unwrap())).unwrap();
        assert_eq!(recovered["reused"], false);
        assert_eq!(artifact_times(), expected);
        assert_eq!(ninja.calls(), calls + 1);
        assert!(stamp.is_file());
    };

    fs::write(&source, SOURCE.replace("yield 1s: value", "yield 1s: true")).unwrap();
    assert!(!ninja.hgl(&build).output().unwrap().status.success());
    fs::write(&source, SOURCE).unwrap();
    recover();

    let invalid = ninja
        .hgl(&["run", &sources, "--out", &workspace, "--arg", "value=true"])
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    recover();

    // A different parser bound forces a build of the same generated C++,
    // so Ninja's eventual decision can still be to compile nothing.
    let failed = ninja
        .hgl(&build)
        .args(["--tree-depth-limit", "501"])
        .env("HGL_TEST_BUILD_FAIL", "1")
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert!(text(&failed.stderr).contains("The test stopped Ninja before compiling."));
    recover();

    let calls = ninja.calls();
    let building = ninja
        .hgl(&build)
        .args(["--tree-depth-limit", "501"])
        .env("HGL_TEST_BUILD_DELAY", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    assert!(wait_until(|| ninja.calls() == calls + 1));
    assert!(
        Command::new("kill")
            .args(["-INT", &building.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let interrupted = building.wait_with_output().unwrap();
    assert_eq!(interrupted.status.code(), Some(2));
    assert!(text(&interrupted.stderr).contains("build was interrupted"));
    recover();

    let calls = ninja.calls();
    success(
        ninja
            .hgl(&[
                "test",
                &sources,
                "--target",
                "cpp",
                "--out",
                &workspace,
                "--rejections-only",
            ])
            .output()
            .unwrap(),
    );
    assert_eq!(ninja.calls(), calls);
    recover();
}

/// Rebuilding in a workspace with a CLI that does not update the stamp
/// must not vouch for the new native artifacts, even when sizes match.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn run_and_test_repair_rewritten_workspace_artifacts() {
    if !sdk_named() {
        return;
    }
    for command in ["run", "test"] {
        let root = scratch(&format!("build-repair-{command}"));
        let source = write(&root, "src/stamped.hgl", SOURCE);
        let app = write(&root, "src/app.hgl", APP);
        let ninja = NinjaTrace::new(&root);
        let workspace = argument(&ninja.workspace);
        let sources = argument(&root.join("src"));
        let args = [command, &sources, "--target", "cpp", "--out", &workspace];
        let (changed, original, replacement, executable, expected) = if command == "test" {
            (
                &source,
                SOURCE,
                SOURCE.replace("== 5", "== 6"),
                "bin/test-demo-stamped",
                "passed   executable  demo.stamped::sums\n",
            )
        } else {
            (
                &app,
                APP,
                APP.replace("51", "52"),
                "run/app-demo-app",
                "Z  51\n",
            )
        };
        let invoke = || success(ninja.hgl(&args).output().unwrap());
        assert!(invoke().contains(expected));
        let stamp = ninja.workspace.join(".hgl-built.json");
        let old_stamp = fs::read(&stamp).unwrap();
        let executable = format!("{executable}{}", std::env::consts::EXE_SUFFIX);
        let reject = || {
            fs::write(changed, "module\n").unwrap();
            assert!(!ninja.hgl(&args).output().unwrap().status.success());
            assert!(!stamp.exists());
            fs::write(changed, original).unwrap();
        };

        // Model an older driver writing the same generated paths and
        // invoking Ninja without replacing this command's stamp.
        fs::write(changed, replacement).unwrap();
        success(
            ninja
                .hgl(&["emit", &sources, "--out", &workspace])
                .output()
                .unwrap(),
        );
        success(
            Command::new(&ninja.native)
                .args(["-C", &workspace, &executable])
                .output()
                .unwrap(),
        );
        assert_eq!(fs::read(&stamp).unwrap(), old_stamp);
        reject();
        assert!(invoke().contains(expected));
        assert_eq!(ninja.calls(), 2);

        let path = ninja.workspace.join(executable);
        let mut broken = fs::read(&path).unwrap();
        broken[0] ^= u8::MAX;
        for (replacement, calls) in [(broken.as_slice(), 3), (b"foreign artifact".as_slice(), 4)] {
            publish_executable(
                &path,
                replacement,
                fs::metadata(&path).unwrap().permissions(),
            );
            reject();
            assert!(invoke().contains(expected));
            assert_eq!(ninja.calls(), calls);
        }
        assert!(invoke().contains(expected));
        assert_eq!(ninja.calls(), 4);
    }
}

/// TST-14, TST-17: file and kind selectors change the executions without
/// changing the native inventory shared by subsequent test invocations.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn changing_test_paths_and_kinds_reuses_the_full_build() {
    if !sdk_named() {
        return;
    }
    let root = scratch("build-test-paths");
    write(
        &root,
        "src/dependency.hgl",
        "module selected.dependency\nexport const fn answer() -> i64 => 7\n",
    );
    let first = write(
        &root,
        "src/first file.hgl",
        concat!(
            "module selected.first\n",
            "use selected.dependency::{answer}\n",
            "test unit { assert answer() == 7 }\n",
            "integration_test integration_first { assert true }\n",
        ),
    );
    let second = write(
        &root,
        "src/sub/second.hgl",
        concat!(
            "module selected.second\n",
            "# expect-error(type, \"type.mismatch\")\n",
            "const fn outside_mismatch() -> i64 => 1\n",
            "test failing_unit { assert false }\n",
            "integration_test integration_second { assert true }\n",
            "integration_test rejected_integration {\n",
            "    # expect-error(type, \"type.mismatch\")\n",
            "    assert 1\n",
            "}\n",
        ),
    );
    let ninja = NinjaTrace::new(&root);
    let source = argument(&root.join("src"));
    let workspace = argument(&ninja.workspace);
    let run = |options: &[&str]| {
        success(
            ninja
                .hgl(&["test", &source, "--target", "cpp", "--keep", &workspace])
                .args(options)
                .output()
                .unwrap(),
        )
    };

    let written = run(&["--test-path", &first]);
    assert!(
        written.contains("passed   executable  selected.first::unit\n"),
        "{written}"
    );
    assert!(!written.contains("integration_first"), "{written}");
    assert!(!written.contains("failing_unit"), "{written}");
    assert!(!written.contains("outside_mismatch"), "{written}");
    assert_eq!(ninja.calls(), 1);

    let written = run(&["--test-path", &second, "--integration-only"]);
    assert!(
        written.contains("passed   executable  selected.second::integration_second\n"),
        "{written}"
    );
    assert!(!written.contains("integration_first"), "{written}");
    assert!(!written.contains("failing_unit"), "{written}");
    assert!(!written.contains("outside_mismatch"), "{written}");
    assert!(
        written.contains("passed   rejection   selected.second::rejected_integration\n"),
        "{written}"
    );
    assert_eq!(ninja.calls(), 1);

    let written = run(&["--integration-only"]);
    assert!(
        written.contains("passed   executable  selected.first::integration_first\n"),
        "{written}"
    );
    assert!(
        written.contains("passed   executable  selected.second::integration_second\n"),
        "{written}"
    );
    assert!(!written.contains("failing_unit"), "{written}");
    assert_eq!(ninja.calls(), 1);
}

/// A default build has no test executable. A test fills that coverage,
/// reuses it on a hit, and rebuilds for content changes or missing files.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn build_reuse_tracks_test_coverage_and_invalidates_by_content() {
    if !sdk_named() {
        return;
    }
    let root = scratch("build-reuse");
    let source = write(&root, "calc.hgl", CALC);
    let ninja = NinjaTrace::new(&root);
    let workspace = argument(&ninja.workspace);
    let build = ["build", &source, "--out", &workspace, "--format", "json"];
    let tests = [
        "test", &source, "--target", "cpp", "--keep", &workspace, "--test", "sums",
    ];
    let executable = ninja.workspace.join(format!(
        "bin/test-demo-calc{}",
        std::env::consts::EXE_SUFFIX
    ));
    let description: Value =
        serde_json::from_str(&success(ninja.hgl(&build).output().unwrap())).unwrap();
    assert_eq!(description["version"], 1);
    assert_eq!(description["workspace"], workspace);
    assert_eq!(description["tests"], serde_json::json!([]));
    assert!(!executable.exists());
    assert_eq!(ninja.calls(), 1);

    let run = || success(ninja.hgl(&tests).output().unwrap());
    assert!(run().starts_with("passed   executable  demo.calc::sums\n"));
    assert!(executable.is_file());
    assert_eq!(ninja.calls(), 2);
    run();
    assert_eq!(ninja.calls(), 2);

    let old_time = fs::metadata(&source).unwrap().modified().unwrap();
    File::options()
        .write(true)
        .open(&source)
        .unwrap()
        .set_modified(std::time::SystemTime::now())
        .unwrap();
    run();
    assert_eq!(ninja.calls(), 2);
    fs::write(
        &source,
        CALC.replace("total(2, 3) == 5", "total(2, 4) == 6"),
    )
    .unwrap();
    File::options()
        .write(true)
        .open(&source)
        .unwrap()
        .set_times(FileTimes::new().set_modified(old_time))
        .unwrap();
    run();
    assert_eq!(ninja.calls(), 3);

    let flags = std::env::var("HGL_CPP_FLAGS").unwrap_or_default() + " -DHGL_TEST_STAMP_FLAG=1";
    let flagged = || {
        success(
            ninja
                .hgl(&tests)
                .env("HGL_CPP_FLAGS", &flags)
                .output()
                .unwrap(),
        )
    };
    flagged();
    assert_eq!(ninja.calls(), 4);
    fs::remove_file(&executable).unwrap();
    flagged();
    assert!(executable.is_file());
    assert_eq!(ninja.calls(), 5);

    #[cfg(target_os = "linux")]
    {
        // Give the same executable machinery a distinct compiler version.
        // Linux permits a patched version string in this disposable ELF.
        let compiler = Path::new(env!("CARGO_BIN_EXE_hgl"));
        let other = root.join("other-hgl");
        let mut bytes = fs::read(compiler).unwrap();
        let version = concat!(env!("CARGO_PKG_VERSION"), " (hgraph ").as_bytes();
        let at = bytes
            .windows(version.len())
            .position(|window| window == version)
            .unwrap();
        bytes[at] = if bytes[at] == b'9' { b'8' } else { b'9' };
        publish_executable(
            &other,
            bytes.as_slice(),
            fs::metadata(compiler).unwrap().permissions(),
        );
        let version_of = |hgl: &Path| success(Command::new(hgl).arg("--version").output().unwrap());
        assert_ne!(version_of(compiler), version_of(&other));
        success(
            ninja
                .command(&other, &tests)
                .env("HGL_CPP_FLAGS", &flags)
                .output()
                .unwrap(),
        );
        assert_eq!(ninja.calls(), 6);
    }
}

/// Application replacements are part of its input key (RUN-2).
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn build_reuse_tracks_application_replacements() {
    if !sdk_named() {
        return;
    }
    let root = scratch("build-replacements");
    write(&root, "src/ticks.hgl", TICKS);
    write(&root, "src/backtest.hgl", BACKTEST);
    let ninja = NinjaTrace::new(&root);
    let sources = argument(&root.join("src"));
    let workspace = argument(&ninja.workspace);
    success(
        ninja
            .hgl(&[
                "build",
                &sources,
                "--out",
                &workspace,
                "--arg",
                "values=[2]",
            ])
            .output()
            .unwrap(),
    );
    assert!(!ninja.workspace.join("results").exists());
    assert_eq!(ninja.calls(), 1);
    let run = |value| {
        success(
            ninja
                .hgl(&["run", &sources, "--keep", &workspace, "--arg", value])
                .output()
                .unwrap(),
        )
    };
    assert!(run("values=[2]").contains("Z  4\n"));
    assert_eq!(ninja.calls(), 1);
    assert!(run("values=[7]").contains("Z  14\n"));
    assert_eq!(ninja.calls(), 2);
}

/// The test waits for the in-progress build of its workspace and runs
/// the executable the build stamped.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn a_test_waits_for_its_workspaces_build() {
    if !sdk_named() {
        return;
    }
    let root = scratch("build-lock");
    let example = "/** A test from the module's documentation.\n\nExamples:\n    ```hgl\n    assert true\n    ```\n*/\n";
    let source = write(&root, "calc.hgl", &format!("{example}{CALC}"));
    let ninja = NinjaTrace::new(&root);
    let workspace = argument(&ninja.workspace);
    let mut build = ninja
        .hgl(&["build", &source, "--out", &workspace, "--tests"])
        .env("HGL_TEST_BUILD_DELAY", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while ninja.calls() == 0 && start.elapsed() < Duration::from_secs(30) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(ninja.calls(), 1);
    assert!(build.try_wait().unwrap().is_none());
    let test = ninja
        .hgl(&[
            "test", &source, "--target", "cpp", "--keep", &workspace, "--test", "sums",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    success(build.wait_with_output().unwrap());
    assert!(success(test.wait_with_output().unwrap()).contains("demo.calc::sums"));
    let selected = "demo.calc::[module]#example-1";
    let example = ninja
        .hgl(&[
            "test", &source, "--target", "cpp", "--keep", &workspace, "--test", selected,
        ])
        .output()
        .unwrap();
    assert!(success(example).contains(&format!("passed   executable  {selected}")));
    assert_eq!(ninja.calls(), 1);
}

/// A stop during preparation exits even when the workspace lock is held.
#[cfg(target_os = "linux")]
#[test]
fn a_stop_while_preparing_exits_without_waiting_for_the_workspace() {
    let root = scratch("stop-before-build");
    let source = write(&root, "calc.hgl", CALC);
    let workspace = root.join("workspace");
    fs::create_dir(&workspace).unwrap();
    let lock_path = workspace.join(".hgl-lock");
    let lock = File::create(&lock_path).unwrap();
    lock.lock().unwrap();
    let lock_path = lock_path.canonicalize().unwrap();
    let mut build = Command::new(env!("CARGO_BIN_EXE_hgl"))
        .args(["build", &source, "--out", &argument(&workspace)])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let open_files = format!("/proc/{}/fd", build.id());
    let ready = wait_until(|| {
        fs::read_dir(&open_files).is_ok_and(|entries| {
            entries
                .filter_map(Result::ok)
                .any(|entry| fs::read_link(entry.path()).is_ok_and(|path| path == lock_path))
        })
    });
    let signalled = ready
        && Command::new("kill")
            .args(["-TERM", &build.id().to_string()])
            .status()
            .unwrap()
            .success();
    let mut stopped = false;
    let start = Instant::now();
    while signalled && start.elapsed() < Duration::from_secs(2) {
        if build.try_wait().unwrap().is_some() {
            stopped = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    if !stopped {
        let _ = build.kill();
    }
    let output = build.wait_with_output().unwrap();
    assert!(ready, "hgl reached its workspace lock");
    assert!(
        stopped,
        "hgl stopped while the workspace lock was still held"
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(text(&output.stderr).contains("build was interrupted"));
    assert!(!workspace.join(".hgl-built.json").exists());
    drop(lock);
}

/// An interrupt of the builder alone leaves no successful stamp behind.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn an_interrupted_build_leaves_no_stamp() {
    if !sdk_named() {
        return;
    }
    let root = scratch("build-stop");
    let source = write(&root, "calc.hgl", CALC);
    let ninja = NinjaTrace::new(&root);
    let workspace = argument(&ninja.workspace);
    let arguments = ["build", &source, "--out", &workspace];
    success(ninja.hgl(&arguments).output().unwrap());
    let stamp = ninja.workspace.join(".hgl-built.json");
    assert!(stamp.is_file());
    fs::write(
        &source,
        CALC.replace("total(2, 3) == 5", "total(2, 4) == 6"),
    )
    .unwrap();
    let build = ninja
        .hgl(&arguments)
        .env("HGL_TEST_BUILD_DELAY", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while ninja.calls() == 1 && start.elapsed() < Duration::from_secs(30) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(ninja.calls(), 2);
    let signalled = Command::new("kill")
        .args(["-INT", &build.id().to_string()])
        .status()
        .unwrap();
    assert!(signalled.success());
    let output = build.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(text(&output.stderr).contains("build was interrupted"));
    assert!(!stamp.exists());
    success(ninja.hgl(&arguments).output().unwrap());
    assert!(stamp.is_file());
    assert_eq!(ninja.calls(), 3);
}

/// Stopping hgl alone during a run's build must prevent publication
/// and keep the application from starting.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn a_run_interrupted_during_the_build_leaves_no_stamp() {
    if !sdk_named() {
        return;
    }
    let root = scratch("build-run-stop");
    write(&root, "src/stamped.hgl", SOURCE);
    let app = write(&root, "src/app.hgl", APP);
    let ninja = NinjaTrace::new(&root);
    let arguments = [
        "run",
        &argument(&root.join("src")),
        "--out",
        &argument(&ninja.workspace),
    ];
    assert!(success(ninja.hgl(&arguments).output().unwrap()).contains("Z  51\n"));
    let stamp = ninja.workspace.join(".hgl-built.json");
    assert!(stamp.is_file());
    fs::write(app, APP.replace("51", "52")).unwrap();
    let run = ninja
        .hgl(&arguments)
        .env("HGL_TEST_BUILD_DELAY", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    assert!(wait_until(|| ninja.calls() == 2));
    assert!(
        Command::new("kill")
            .args(["-INT", &run.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let stopped = run.wait_with_output().unwrap();
    assert_eq!(stopped.status.code(), Some(2));
    assert!(text(&stopped.stderr).contains("build was interrupted"));
    assert!(stopped.stdout.is_empty());
    assert!(!stamp.exists());
    assert!(success(ninja.hgl(&arguments).output().unwrap()).contains("Z  52\n"));
    assert!(stamp.is_file());
}

/// The first stop waits for Ninja. A second stop exits the driver and
/// the native builder receives the existing parent-death signal.
#[cfg(target_os = "linux")]
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn a_second_stop_exits_while_waiting_for_ninja() {
    if !sdk_named() {
        return;
    }
    let root = scratch("second-build-stop");
    let source = write(&root, "calc.hgl", CALC);
    let ninja = NinjaTrace::new(&root);
    let workspace = argument(&ninja.workspace);
    let arguments = ["build", &source, "--out", &workspace];
    let stopped = root.join("ninja-stopped");
    let mut build = ninja
        .hgl(&arguments)
        .env("HGL_TEST_BUILD_TERM", &stopped)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let signal = |name: &str, pid: u32| {
        Command::new("kill")
            .args([name, &pid.to_string()])
            .status()
            .unwrap()
            .success()
    };
    let started = wait_until(|| ninja.calls() == 1);
    let first = started && signal("-INT", build.id());
    std::thread::sleep(Duration::from_millis(100));
    let waiting = first && build.try_wait().unwrap().is_none() && !stopped.exists();
    let second = waiting && signal("-TERM", build.id());
    let mut exited = false;
    let start = Instant::now();
    while second && start.elapsed() < Duration::from_secs(2) {
        if build.try_wait().unwrap().is_some() {
            exited = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    if !exited {
        let _ = build.kill();
    }
    let output = build.wait_with_output().unwrap();
    let child_exited = started && wait_until(|| stopped.is_file());
    fs::write(stopped.with_extension("release"), "").unwrap();
    assert!(waiting, "the first stop waits for Ninja");
    assert!(exited, "the second stop exits hgl promptly");
    assert!(child_exited, "Ninja was told its parent exited");
    assert_eq!(output.status.code(), Some(2));
    assert!(text(&output.stderr).contains("build was interrupted"));
    assert!(!ninja.workspace.join(".hgl-built.json").exists());
}

/// A nested native job holds the workspace until it exits, even when
/// its launcher and hgl have both stopped and its stdin is /dev/null.
#[cfg(target_os = "linux")]
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn a_retry_waits_for_native_jobs_after_two_stops() {
    if !sdk_named() {
        return;
    }
    let root = scratch("native-job-lock");
    let source = write(&root, "stamped.hgl", SOURCE);
    let ninja = NinjaTrace::new(&root);
    let workspace = argument(&ninja.workspace);
    let arguments = ["test", &source, "--target", "cpp", "--out", &workspace];
    write(
        &ninja.workspace,
        "hold.ninja",
        "rule hold\n  command = sh hold.sh\nbuild job-done: hold\n",
    );
    write(
        &ninja.workspace,
        "hold.sh",
        r#"touch job-started
attempt=0
while [ "$attempt" -lt 400 ]; do
    if [ -f job-release ]; then
        touch job-done
        exit 0
    fi
    attempt=$((attempt + 1))
    sleep 0.02
done
exit 1
"#,
    );
    let launcher_stopped = root.join("launcher-stopped");
    let stderr = root.join("first-build.stderr");
    let mut first = ninja
        .hgl(&arguments)
        .env("HGL_TEST_BUILD_HOLD_JOB", "1")
        .env("HGL_TEST_BUILD_TERM", &launcher_stopped)
        .stdout(Stdio::null())
        .stderr(File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    assert!(wait_until(|| ninja.workspace.join("job-started").is_file()));
    let signal = |name: &str| {
        assert!(
            Command::new("kill")
                .args([name, &first.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
    };
    signal("-INT");
    std::thread::sleep(Duration::from_millis(100));
    signal("-TERM");
    assert!(wait_until(|| first.try_wait().unwrap().is_some()));
    assert!(wait_until(|| launcher_stopped.is_file()));

    let lock = File::options()
        .read(true)
        .write(true)
        .open(ninja.workspace.join(".hgl-lock"))
        .unwrap();
    let held = matches!(lock.try_lock(), Err(fs::TryLockError::WouldBlock));
    drop(lock);
    let retry = ninja
        .hgl(&arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(250));
    let overlapped = ninja.calls() != 1 || ninja.workspace.join("job-done").exists();
    write(&ninja.workspace, "job-release", "");
    let stdout = success(retry.wait_with_output().unwrap());
    assert!(wait_until(|| ninja.workspace.join("job-done").is_file()));
    assert_eq!(first.wait().unwrap().code(), Some(2));
    assert!(
        fs::read_to_string(stderr)
            .unwrap()
            .contains("build was interrupted")
    );
    assert!(held, "the earlier job still owns the workspace lock");
    assert!(!overlapped, "the retry waits for the earlier build's job");
    assert_eq!(ninja.calls(), 2);
    assert!(stdout.contains("executable cases: 1 passed, 0 failed, 0 not run"));
}

/// On Linux the native builder receives a termination signal even when
/// hgl is killed without running its interrupt handler.
#[cfg(target_os = "linux")]
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn ninja_stops_when_hgl_is_killed() {
    if !sdk_named() {
        return;
    }
    let root = scratch("build-parent-killed");
    let source = write(&root, "calc.hgl", CALC);
    let ninja = NinjaTrace::new(&root);
    let arguments = ["build", &source, "--keep", &argument(&ninja.workspace)];
    let stopped = root.join("ninja-stopped");
    let mut build = ninja
        .hgl(&arguments)
        .env("HGL_TEST_BUILD_TERM", &stopped)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let started = wait_until(|| ninja.calls() == 1);
    build.kill().unwrap();
    build.wait().unwrap();
    let signalled = started && wait_until(|| stopped.is_file());
    // Release the wrapper on a failure too, so it cannot outlive the test.
    fs::write(stopped.with_extension("release"), "").unwrap();
    assert!(signalled);
    assert!(!ninja.workspace.join(".hgl-built.json").exists());
    success(ninja.hgl(&arguments).output().unwrap());
    assert_eq!(ninja.calls(), 2);
}

/// The new debug command builds the requested workspace and reports an
/// executable with HGL line mappings for its debugger (RUN-1).
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn build_debug_reports_the_application_launch() {
    if !sdk_named() {
        return;
    }
    let root = scratch("build-debug");
    for directory in ["first", "second"] {
        write(&root, &format!("{directory}/src/ticks.hgl"), TICKS);
        write(&root, &format!("{directory}/src/backtest.hgl"), BACKTEST);
    }
    let ninja = NinjaTrace::new(&root);
    let workspace = argument(&ninja.workspace);
    for directory in ["first", "second"] {
        let current = root.join(directory);
        let result = success(
            ninja
                .hgl(&[
                    "build",
                    "src",
                    "--debug",
                    "--out",
                    &workspace,
                    "--format",
                    "json",
                    "--application",
                    "demo.ticks.backtest",
                ])
                .current_dir(&current)
                .output()
                .unwrap(),
        );
        let launch: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(launch["version"], 1);
        assert_eq!(launch["workspace"], workspace);
        assert_eq!(launch["reused"], false, "{launch}");
        let launch = &launch["debugLaunch"];
        assert!(launch.get("version").is_none());
        let executable = launch["executable"].as_str().unwrap();
        assert!(Path::new(executable).starts_with(&ninja.workspace));
        assert!(Path::new(executable).is_file());
        assert_eq!(launch["arguments"][1], "--debug-console");
        assert!(!Path::new(launch["arguments"][0].as_str().unwrap()).exists());
        let unit = fs::read_to_string(ninja.workspace.join("modules/mod-demo-ticks-backtest.cpp"))
            .unwrap();
        let original = current.join("src/backtest.hgl");
        assert!(unit.contains(&format!("{original:?}")), "{unit}");
        #[cfg(target_os = "linux")]
        super::check_backtest_line_table(executable);
    }
    assert_eq!(ninja.calls(), 2);
}

/// A test build omits rejection owners (TST-13). An ordinary command
/// checks them as source even if the test artifacts are stamped (TST-14).
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn ordinary_commands_check_the_sources_omitted_from_test_builds() {
    if !sdk_named() {
        return;
    }
    let root = scratch("build-with-rejections");
    write(&root, "src/ticks.hgl", TICKS);
    write(&root, "src/backtest.hgl", BACKTEST);
    write(
        &root,
        "src/rejected.hgl",
        concat!(
            "module demo.rejected\n\n",
            "# expect-error(type, \"type.mismatch\")\n",
            "const fn wrong() -> i64 => true\n",
        ),
    );
    let ninja = NinjaTrace::new(&root);
    let sources = argument(&root.join("src"));
    let workspace = argument(&ninja.workspace);
    let build_tests = || {
        success(
            ninja
                .hgl(&["build", &sources, "--out", &workspace, "--tests"])
                .output()
                .unwrap(),
        );
        assert!(ninja.workspace.join(".hgl-built.json").is_file());
    };
    for ordinary in [
        ["run", &sources, "--keep", &workspace],
        ["build", &sources, "--out", &workspace],
    ] {
        build_tests();
        let output = ninja.hgl(&ordinary).output().unwrap();
        assert_eq!(output.status.code(), Some(1), "{}", text(&output.stdout));
        assert!(text(&output.stderr).contains("type.mismatch"));
        assert!(!ninja.workspace.join(".hgl-built.json").exists());
        assert!(!ninja.workspace.join("results").exists());
    }
    assert_eq!(ninja.calls(), 2);
}
