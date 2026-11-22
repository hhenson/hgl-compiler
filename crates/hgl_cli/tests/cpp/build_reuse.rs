//! Checks build coverage and invalidation against the SDK, with a wrapper
//! that records only Ninja calls for the program's workspace.

use std::ffi::OsString;
use std::fs::{self, File, FileTimes};
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::{BACKTEST, CALC, TICKS, argument, scratch, sdk_named, text, write};

const SOURCE: &str = "module demo.stamped\n\
    export fn pulse(const value: i64) -> i64 { yield 1s: value }\n\
    test sums { assert 2 + 3 == 5 }\n";
const APP: &str = "application demo.app\n\
    use demo.stamped::{pulse}\n\
    eval pulse(value: 51)\n";

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
        let wrapper = write(
            root,
            "bin/ninja",
            r#"#!/bin/sh
if [ "$1" = -C ] && [ "$2" = "$HGL_TEST_BUILD_WORKSPACE" ]; then
    if [ -n "$HGL_TEST_BUILD_TERM" ]; then
        trap 'touch "$HGL_TEST_BUILD_TERM"; exit 130' TERM
    fi
    printf '%s\n' build >>"$HGL_TEST_BUILD_TRACE"
    if [ -n "$HGL_TEST_BUILD_TERM" ]; then
        while [ ! -f "$HGL_TEST_BUILD_TERM.release" ]; do sleep 0.02; done
    fi
    if [ -n "$HGL_TEST_BUILD_DELAY" ]; then sleep "$HGL_TEST_BUILD_DELAY"; fi
fi
exec "$HGL_TEST_REAL_NINJA" "$@"
"#,
        );
        fs::set_permissions(wrapper, fs::Permissions::from_mode(0o755)).unwrap();
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
        fs::write(changed, original).unwrap();
        assert!(invoke().contains(expected));
        assert_eq!(ninja.calls(), 2);

        let path = ninja.workspace.join(executable);
        let mut broken = fs::read(&path).unwrap();
        broken[0] ^= u8::MAX;
        for (replacement, calls) in [(broken.as_slice(), 3), (b"foreign artifact".as_slice(), 4)] {
            fs::write(&path, replacement).unwrap();
            assert!(invoke().contains(expected));
            assert_eq!(ninja.calls(), calls);
        }
        assert!(invoke().contains(expected));
        assert_eq!(ninja.calls(), 4);
    }
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
        fs::copy(compiler, &other).unwrap();
        let mut bytes = fs::read(&other).unwrap();
        let version = concat!(env!("CARGO_PKG_VERSION"), " (hgraph ").as_bytes();
        let at = bytes
            .windows(version.len())
            .position(|window| window == version)
            .unwrap();
        bytes[at] = if bytes[at] == b'9' { b'8' } else { b'9' };
        fs::write(&other, bytes).unwrap();
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
    let source = write(&root, "calc.hgl", CALC);
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
    assert_eq!(ninja.calls(), 1);
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
