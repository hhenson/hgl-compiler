//! The IDE's test launch contract and selective debugger workspace reuse.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::{Value, json};

use super::{FLAGS, argument, common::hgl, scratch, sdk_named, text, write};

const ALPHA: &str = concat!(
    "module debug.alpha\n",
    "test first {\n",
    "    assert true\n",
    "}\n",
    "test failing { assert false }\n",
    "test last { assert true }\n",
    "integration_test live_alpha { assert true }\n",
    "integration_test rejected {\n",
    "    # expect-error(type, \"type.mismatch\")\n",
    "    assert 1\n",
    "}\n",
);

const REJECTIONS: &str = concat!(
    "module debug.rejections\n",
    "test good {\n",
    "    # expect-error(type, \"type.mismatch\")\n",
    "    assert 1\n",
    "}\n",
    "test bad {\n",
    "    # expect-error(type, \"type.mismatch\")\n",
    "    assert true\n",
    "}\n",
    "test runnable { assert true }\n",
);

fn plan(output: Output) -> Value {
    assert!(output.status.success(), "{}", text(&output.stderr));
    let plan: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(plan["version"], 1);
    for launch in plan["launches"].as_array().unwrap() {
        for field in ["executable", "workingDirectory", "resultsFile"] {
            assert!(Path::new(launch[field].as_str().unwrap()).is_absolute());
        }
        assert!(launch["environment"].is_object());
        assert!(launch["hgraph"]["libraries"].is_string());
        assert_eq!(launch["arguments"][0], launch["resultsFile"]);
    }
    plan
}

/// TST-14, TST-15: usage errors start no build, and a selected empty
/// subdirectory of a valid source root needs no native toolchain.
#[test]
fn debug_test_usage_and_empty_selection() {
    let root = scratch("debug-test-usage");
    let source = write(&root, "src/alpha.hgl", ALPHA);
    let sources = argument(&root.join("src"));
    let workspace = root.join("debug workspace");
    let directory = argument(&workspace);
    let empty = root.join("src/empty");
    fs::create_dir(&empty).unwrap();
    let empty = argument(&empty);
    let missing = argument(&root.join("src/missing.hgl"));
    let run = |options: &[&str]| {
        let mut args = vec![
            "test",
            &sources,
            "--target",
            "cpp",
            "--debug-build",
            &directory,
        ];
        args.extend_from_slice(options);
        hgl(&args)
    };

    for (options, message) in [
        (&["--test", "missing"][..], "no test is named `missing`"),
        (&["--test-path", &missing][..], missing.as_str()),
        (
            &["--integration", "--integration-only"][..],
            "--integration-only",
        ),
    ] {
        let output = run(options);
        assert_eq!(output.status.code(), Some(2));
        assert!(text(&output.stderr).contains(message));
        assert!(output.stdout.is_empty());
        assert!(!workspace.exists());
    }
    let ordinary = hgl(&["test", &source, "--test", "missing"]);
    assert_eq!(ordinary.status.code(), Some(1));

    let selected = plan(run(&["--test-path", &empty]));
    assert_eq!(
        selected,
        json!({"version": 1, "launches": [], "skipped": []})
    );
    assert!(!workspace.exists());

    let rejection = plan(run(&["--test", "debug.alpha::rejected"]));
    assert_eq!(rejection["launches"], json!([]));
    assert_eq!(rejection["skipped"][0]["name"], "debug.alpha::rejected");
    assert!(
        rejection["skipped"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("rejection")
    );
    assert!(!workspace.exists());
}

/// TST-14: a surviving-source error stops preparation and uses stderr.
#[test]
fn debug_test_source_errors_leave_stdout_empty() {
    let root = scratch("debug-test-source-error");
    let source = write(
        &root,
        "bad.hgl",
        "module debug.bad\ntest bad { assert 1 }\n",
    );
    let workspace = root.join("debug workspace");
    let directory = argument(&workspace);
    let output = hgl(&[
        "test",
        &source,
        "--target",
        "cpp",
        "--debug-build",
        &directory,
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(text(&output.stderr).contains("type.mismatch"));
    assert!(!workspace.exists());

    let ordinary = hgl(&["test", &source]);
    assert_eq!(ordinary.status.code(), Some(1));
    assert_eq!(text(&ordinary.stdout), "no case was run\n");
}

/// TST-13: the debugger description reports a failed rejection using the
/// same source detail as the ordinary test runner.
#[test]
fn debug_test_rejection_failures_preserve_the_plan_and_diagnostic() {
    let root = scratch("debug-test-rejection-error");
    let source = write(&root, "cases.hgl", REJECTIONS);
    let workspace = root.join("debug workspace");
    let directory = argument(&workspace);
    let ordinary = hgl(&["test", &source, "--rejections-only"]);
    let ordinary_stdout = text(&ordinary.stdout);
    assert_eq!(ordinary.status.code(), Some(1));
    let detail = ordinary_stdout
        .lines()
        .map(str::trim_start)
        .find(|line| line.starts_with("expected type/type.mismatch"))
        .unwrap();
    let result = hgl(&[
        "test",
        &source,
        "--target",
        "cpp",
        "--debug-build",
        &directory,
        "--rejections-only",
    ]);
    assert_eq!(result.status.code(), Some(1));
    let plan: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(plan["version"], 1);
    assert_eq!(plan["launches"], json!([]));
    assert_eq!(plan["skipped"][0]["name"], "debug.rejections::good");
    assert!(plan["skipped"][0].get("status").is_none());
    assert_eq!(plan["skipped"][1]["name"], "debug.rejections::bad");
    assert_eq!(plan["skipped"][1]["status"], "failed");
    let reason = plan["skipped"][1]["reason"].as_str().unwrap();
    assert!(reason.contains(detail), "{reason}");
    assert!(text(&result.stderr).contains(detail));
    assert!(!workspace.exists());
}

/// TST-14 to TST-17: selecting another module extends one debug build.
/// The launched test host runs its selected names in order, even after
/// a failed assertion, and records only their positions.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn debug_launches_group_tests_and_build_only_missing_modules() {
    if !sdk_named() {
        return;
    }
    let root = scratch("debug-test-launch");
    let alpha = write(&root, "src/group/alpha space.hgl", ALPHA);
    let beta = write(
        &root,
        "src/group/beta.hgl",
        "module debug.beta\ntest other { assert true }\nintegration_test live { assert true }\n",
    );
    write(&root, "src/flags.hgl", FLAGS);
    write(
        &root,
        "src/unused.hgl",
        "module debug.unused\nexport const fn answer() -> i64 => 42\n",
    );
    let sources = argument(&root.join("src"));
    let group = argument(&root.join("src/group"));
    let workspace = root.join("debug workspace");
    let directory = argument(&workspace);
    let run = |options: &[&str]| {
        let mut args = vec![
            "test",
            &sources,
            "--target",
            "cpp",
            "--debug-build",
            &directory,
        ];
        args.extend_from_slice(options);
        plan(hgl(&args))
    };
    let bin_count = || fs::read_dir(workspace.join("bin")).unwrap().count();
    let ninja_log = || fs::read(workspace.join(".ninja_log")).unwrap();

    let named = run(&["--test", "debug.alpha::first"]);
    let launch = &named["launches"][0];
    assert_eq!(named["launches"].as_array().unwrap().len(), 1);
    assert_eq!(launch["tests"], json!(["debug.alpha::first"]));
    assert!(Path::new(launch["executable"].as_str().unwrap()).is_file());
    assert!(!Path::new(launch["resultsFile"].as_str().unwrap()).exists());
    assert_eq!(bin_count(), 1);
    let generated = fs::read_to_string(workspace.join(hgl_cpp::unit_path("debug.alpha"))).unwrap();
    for line in [3, 7] {
        assert!(
            generated.lines().any(|directive| {
                directive.starts_with(&format!("#line {line} "))
                    && directive.contains("alpha space.hgl")
            }),
            "test statement at HGL line {line} needs a debugger mapping"
        );
    }
    let first_build = ninja_log();
    assert_eq!(run(&["--test", "debug.alpha::first"]), named);
    assert_eq!(ninja_log(), first_build);

    let module = run(&["--test-path", &alpha]);
    let launch = &module["launches"][0];
    assert_eq!(module["launches"].as_array().unwrap().len(), 1);
    assert_eq!(launch["module"], "debug.alpha");
    assert_eq!(
        launch["tests"],
        json!([
            "debug.alpha::first",
            "debug.alpha::failing",
            "debug.alpha::last"
        ])
    );
    assert_eq!(ninja_log(), first_build);

    let integration = run(&["--test-path", &beta, "--test", "debug.beta::live"]);
    assert_eq!(integration["launches"].as_array().unwrap().len(), 1);
    assert_eq!(
        integration["launches"][0]["tests"],
        json!(["debug.beta::live"])
    );
    assert_eq!(
        integration["launches"][0]["arguments"][1],
        "debug.beta::live"
    );
    assert_eq!(bin_count(), 2);
    let second_build = ninja_log();
    assert!(second_build.starts_with(&first_build) && second_build.len() > first_build.len());
    let added = &second_build[first_build.len()..];
    assert!(!text(added).contains("alpha"), "{}", text(added));

    let folder = run(&["--test-path", &group]);
    assert_eq!(folder["launches"].as_array().unwrap().len(), 2);
    assert_eq!(folder["launches"][0], *launch);
    assert_eq!(folder["launches"][1]["tests"], json!(["debug.beta::other"]));
    assert_eq!(ninja_log(), second_build);
    let integrations = run(&["--integration-only"]);
    assert_eq!(
        integrations["launches"][0]["tests"],
        json!(["debug.alpha::live_alpha"])
    );
    assert_eq!(integrations["launches"][1], integration["launches"][0]);
    assert_eq!(integrations["skipped"][0]["name"], "debug.alpha::rejected");
    assert_eq!(ninja_log(), second_build);

    let project = run(&[]);
    assert_eq!(project["launches"].as_array().unwrap().len(), 3);
    assert_eq!(
        project["launches"][2]["tests"],
        json!(["demo.flags::holds"])
    );
    assert_eq!(project["skipped"][0]["name"], "demo.flags::evaluates");
    assert!(
        project["skipped"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("does not write")
    );
    assert_eq!(bin_count(), 3);
    let partial_build = ninja_log();
    assert!(!text(&partial_build).contains("mod-debug-unused"));
    let full = [
        "build", &sources, "--debug", "--tests", "--out", &directory, "--format", "json",
    ];
    let complete = hgl(&full);
    assert!(complete.status.success(), "{}", text(&complete.stderr));
    let description: Value = serde_json::from_slice(&complete.stdout).unwrap();
    assert_eq!(description["reused"], false);
    let full_build = ninja_log();
    assert!(text(&full_build).contains("mod-debug-unused"));
    let warm = hgl(&full);
    assert!(warm.status.success(), "{}", text(&warm.stderr));
    let description: Value = serde_json::from_slice(&warm.stdout).unwrap();
    assert_eq!(description["reused"], true);
    assert_eq!(ninja_log(), full_build);

    let results = Path::new(launch["resultsFile"].as_str().unwrap());
    assert!(!results.exists());
    let output = Command::new(launch["executable"].as_str().unwrap())
        .args(
            launch["arguments"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap()),
        )
        .current_dir(launch["workingDirectory"].as_str().unwrap())
        .envs(
            launch["environment"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(key, value)| (key, value.as_str().unwrap())),
        )
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output.stderr));
    let content = fs::read_to_string(results).unwrap();
    let lines: Vec<_> = content.lines().collect();
    assert_eq!(lines.len(), 6, "{content}");
    assert_eq!(&lines[..3], &["start 0", "passed 0", "start 1"]);
    assert!(lines[3].starts_with("failed 1 5 "), "{content}");
    assert_eq!(&lines[4..], &["start 2", "passed 2"]);

    let rejections = write(&root, "rejections.hgl", REJECTIONS);
    let rejection_workspace = argument(&root.join("rejection workspace"));
    let mixed = hgl(&[
        "test",
        &rejections,
        "--target",
        "cpp",
        "--debug-build",
        &rejection_workspace,
    ]);
    assert_eq!(mixed.status.code(), Some(1));
    let plan: Value = serde_json::from_slice(&mixed.stdout).unwrap();
    assert_eq!(plan["skipped"][1]["status"], "failed");
    let launch = &plan["launches"][0];
    assert_eq!(launch["tests"], json!(["debug.rejections::runnable"]));
    assert!(Path::new(launch["executable"].as_str().unwrap()).is_file());
    assert!(!Path::new(launch["resultsFile"].as_str().unwrap()).exists());
}

/// A failed requested native module gives an error even when Ninja was
/// able to link the other module. There is no partial launch plan.
#[cfg(unix)]
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn debug_native_build_failure_leaves_stdout_empty() {
    use std::os::unix::fs::PermissionsExt as _;

    if !sdk_named() {
        return;
    }
    let root = scratch("debug-test-native-error");
    write(
        &root,
        "src/good.hgl",
        "module debug.good\ntest good { assert true }\n",
    );
    write(
        &root,
        "src/bad.hgl",
        "module debug.bad\ntest bad { assert true }\n",
    );
    let workspace = root.join("debug workspace");
    let ninja = which::which("ninja").expect("the SDK uses Ninja");
    let wrapper = write(
        &root,
        "bin/ninja",
        r#"#!/bin/sh
if [ "$1" = -C ] && [ "$2" = "$HGL_TEST_DEBUG_WORKSPACE" ]; then
    printf '\n#error controlled debug C++ failure\n' >> "$2/modules/mod-debug-bad.cpp"
fi
exec "$HGL_TEST_REAL_NINJA" "$@"
"#,
    );
    fs::set_permissions(wrapper, fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths(
        std::iter::once(root.join("bin"))
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_hgl"))
        .arg("test")
        .arg(root.join("src"))
        .args(["--target", "cpp", "--debug-build"])
        .arg(&workspace)
        .env("PATH", path)
        .env("HGL_TEST_DEBUG_WORKSPACE", &workspace)
        .env("HGL_TEST_REAL_NINJA", ninja)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(text(&output.stderr).contains("controlled debug C++ failure"));
    assert!(workspace.join("bin/test-debug-good").is_file());
    assert!(!workspace.join(".hgl-built.json").exists());
}
