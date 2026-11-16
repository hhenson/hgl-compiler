//! Runs `hgl emit` the way a user would.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Runs `hgl` with `arguments` and waits for it to finish.
fn hgl(arguments: &[&str]) -> Output {
    // Cargo builds the binary before the test and puts its path in this
    // environment variable at compile time.
    Command::new(env!("CARGO_BIN_EXE_hgl"))
        .args(arguments)
        .output()
        .expect("the hgl binary runs")
}

/// A stream's bytes as text.
fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("hgl writes UTF-8")
}

/// An empty directory for the test named `test`.
fn scratch(test: &str) -> PathBuf {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("emit")
        .join(test);
    fs::remove_dir_all(&directory).ok();
    fs::create_dir_all(&directory).unwrap();
    directory
}

/// An accepted program is written: a file for each module, in a
/// directory for each part of its path, and the support files.
#[test]
fn an_accepted_program_is_written() {
    let directory = scratch("written");
    let source = directory.join("demo.hgl");
    fs::write(
        &source,
        concat!(
            "module demo.pass\n",
            "fn pass(value: i64) -> i64 {\n",
            "    when { return delta_value(value) }\n",
            "}\n",
            "test passes {\n",
            "    assert eval(pass, value: [7, _, 9]) == [7, _, 9]\n",
            "}\n",
        ),
    )
    .unwrap();
    let out = directory.join("out");
    let output = hgl(&[
        "emit",
        source.to_str().unwrap(),
        "--target",
        "hgraph",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(
        text(&output.stdout),
        format!("wrote 2 files for hgraph to {}\n", out.display())
    );
    let module = fs::read_to_string(out.join("demo").join("pass.py")).unwrap();
    assert!(
        module.contains("def pass_(value: TS[int]) -> TS[int]:\n    return value.delta_value\n")
    );
    assert!(module.contains("    assert evaluate(pass_, value=[7, None, 9]) == [7, None, 9]\n"));
    assert!(out.join("hgl_harness.py").is_file());
}

/// A rejected program, and one the target does not write all of, write
/// nothing and exit with 1.
#[test]
fn nothing_is_written_for_what_cannot_be() {
    let directory = scratch("not_written");
    let rejected = directory.join("rejected.hgl");
    fs::write(
        &rejected,
        "module rejected\nconst fn f() -> i64 => \"text\"\n",
    )
    .unwrap();
    let out = directory.join("out");
    let output = hgl(&[
        "emit",
        rejected.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        text(&output.stdout),
        "nothing written: the program is rejected\n"
    );
    assert!(text(&output.stderr).contains("error[type/type.mismatch]"));
    assert!(!out.exists());

    let unwritten = directory.join("unwritten.hgl");
    fs::write(
        &unwritten,
        "module unwritten\nstruct Quote {\n    bid: f64\n}\n",
    )
    .unwrap();
    let output = hgl(&[
        "emit",
        unwritten.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        text(&output.stdout),
        "nothing written: the hgraph target does not write all of the program yet\n"
    );
    assert!(text(&output.stderr).contains(
        "error[build/target.unsupported]: the hgraph target does not write a struct yet"
    ));
    assert!(!out.exists());
}
