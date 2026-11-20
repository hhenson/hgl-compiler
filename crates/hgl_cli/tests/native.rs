//! End-to-end backend checks for an installed prepared-builder SDK and Python HGraph.
//! Run with `--features native-sdk-tests --test native`.
#![cfg(feature = "native-sdk-tests")]

use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::tempdir;

fn hgl(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hgl"))
        .args(args)
        .output()
        .unwrap()
}
fn success(r: Output) -> String {
    assert!(
        r.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&r.stdout),
        String::from_utf8_lossy(&r.stderr)
    );
    String::from_utf8(r.stdout).unwrap()
}

#[test]
fn both_targets_check_the_same_scalar_and_keyed_publications() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let scalar = root.join("spec/hgl_core/tests/conformance/scalar_eval.hgl");
    let keyed = root.join("crates/hgl_hgraph/tests/fixtures/demo.graph_for.hgl");
    let removal = root.join("crates/hgl_hgraph/tests/fixtures/demo.dictionary_removals.hgl");
    let keep = tempdir().unwrap();
    for target in ["hgraph", "hgraph-native"] {
        let dir = keep.path().join(target);
        let text = success(hgl(&[
            "test",
            "--target",
            target,
            "--keep",
            dir.to_str().unwrap(),
            "--test",
            "replay_preserves_equal_ticks",
            "--test",
            "a_dictionary_has_a_child_for_each_key",
            scalar.to_str().unwrap(),
            keyed.to_str().unwrap(),
            "--test",
            "an_absent_key_removal_ticks_only_on_initial_validity",
            removal.to_str().unwrap(),
        ]));
        // END-2, TST-6, CTRL-7 and CTRL-15. Both targets run the same publications and lifecycle assertions.
        assert!(
            text.contains("executable cases: 3 passed, 0 failed, 0 not run"),
            "{text}"
        );
    }
}

// CRUN-8: an outputless entry still executes its graph.
#[test]
fn an_empty_outputless_application_runs_on_both_targets() {
    let work = tempdir().unwrap();
    let source = work.path().join("empty.hgl");
    fs::write(
        &source,
        "application sample.empty\nfn idle() {}\neval(idle())\n",
    )
    .unwrap();
    for target in ["hgraph", "hgraph-native"] {
        let keep = work.path().join(target);
        let output = success(hgl(&[
            "run",
            "--target",
            target,
            "--keep",
            keep.to_str().unwrap(),
            source.to_str().unwrap(),
        ]));
        assert_eq!(
            output.trim_end(),
            format!("ran sample.empty on {target}: its entry has no result")
        );
    }
}

#[test]
fn a_changed_hgl_body_changes_the_trace_and_a_failed_build_cannot_launch_the_old_binary() {
    let work = tempdir().unwrap();
    let module = work.path().join("nodes.hgl");
    let app = work.path().join("app.hgl");
    let output = work.path().join("native");
    fs::write(
        &app,
        "application sample.app\nuse sample.nodes::{entry}\neval(entry())\n",
    )
    .unwrap();
    let source = "module sample.nodes\nfn prices() -> i64 { yield 0us: 7\n yield 2us: 7 }\nfn step(value: i64) -> i64 { state seen: i64 = 0\n when { seen += 1\n return seen } }\nexport fn entry() -> i64 => step(prices())\n";
    for n in [1, 2] {
        fs::write(
            &module,
            source.replace("seen += 1", &format!("seen += {n}")),
        )
        .unwrap();
        let args = |target| {
            vec![
                "run",
                "--target",
                target,
                "--keep",
                output.to_str().unwrap(),
                module.to_str().unwrap(),
                app.to_str().unwrap(),
            ]
        };
        let native = success(hgl(&args("hgraph-native")));
        let python = success(hgl(&args("hgraph")));
        let trace = |s: &str| {
            s.lines()
                .filter(|s| !s.starts_with("ran "))
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert_eq!(trace(&native), trace(&python));
        assert_eq!(
            trace(&native),
            format!(
                "1970-01-01T00:00:00.000001Z  {n}\n1970-01-01T00:00:00.000003Z  {}",
                2 * n
            )
        );
    }
    fs::write(&module, source.replace("seen += 1", "seen += 3")).unwrap();
    let failed = Command::new(env!("CARGO_BIN_EXE_hgl"))
        .args([
            "run",
            "--target",
            "hgraph-native",
            "--keep",
            output.to_str().unwrap(),
            module.to_str().unwrap(),
            app.to_str().unwrap(),
        ])
        .env("RUSTFLAGS", "-C linker=nonexistent-hgl-test-linker")
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("native build failed"));
    assert!(
        failed.stdout.is_empty(),
        "a failed build must not launch the prior recording"
    );
}
