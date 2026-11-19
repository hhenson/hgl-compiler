//! Runs `hgl emit` the way a user would.

mod common;

use std::fs;

use common::{hgl, scratch, text};

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
            "    assert eval(pass(value: [7, _, 9])) == [7, _, 9]\n",
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
        format!("wrote 6 files for hgraph to {}\n", out.display())
    );
    let module = fs::read_to_string(
        out.join("_hgl")
            .join("_m_demo")
            .join("_m_pass")
            .join("__init__.py"),
    )
    .unwrap();
    assert!(module.contains(
        "def pass_(value: _hg.TS[_py.int]) -> _hg.TS[_py.int]:\n    return value.delta_value\n"
    ));
    assert!(
        module
            .contains("    if not (_harness.same(_harness.evaluate(pass_, {\"value\": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__i64, value=[7, None, 9]), [7, None, 9])):\n        raise _py.AssertionError()\n")
    );
    assert!(out.join("_hgl").join("_harness.py").is_file());
    // The harness imports the provider, which is written with it, and
    // the provider what the generated code names by hand.
    assert!(out.join("_hgl").join("_core_native.py").is_file());
    assert!(out.join("_hgl").join("_core_support.py").is_file());
    assert!(out.join("_hgl").join("__init__.py").is_file());
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
        "module unwritten\nconst fn total(rest: ...i64) -> i64 => 1\n",
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
        "error[build/target.unsupported]: the hgraph target does not write a pack parameter yet"
    ));
    assert!(!out.exists());
}
