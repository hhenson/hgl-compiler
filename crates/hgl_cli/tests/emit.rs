//! Runs `hgl emit` the way a user would.

mod common;

use std::fs;

use common::{argument, hgl, scratch, text, write};

/// An accepted program is written for the cpp target, the one there
/// is, where none is named: a header and a unit for each module, core's
/// among them, and an entry unit for its tests.
#[test]
fn an_accepted_program_is_written() {
    let directory = scratch("written");
    let source = write(
        &directory,
        "demo.hgl",
        concat!(
            "module demo.pass\n",
            "const fn yes() -> bool => true\n",
            "test holds {\n",
            "    assert yes()\n",
            "}\n",
        ),
    );
    let out = directory.join("out");
    let output = hgl(&["emit", &source, "--out", &argument(&out)]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(
        text(&output.stdout),
        format!("wrote 6 files for cpp to {}\n", argument(&out))
    );
    for file in [
        "modules/core.types.h",
        "modules/core.h",
        "modules/core.cpp",
        "modules/demo.pass.h",
        "modules/demo.pass.cpp",
        "tests/demo.pass.cpp",
    ] {
        assert!(out.join(file).is_file(), "{file}");
    }
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

    // A set of lists, whose element is no scalar, is not written yet.
    let unwritten = write(
        &directory,
        "unwritten.hgl",
        concat!(
            "module unwritten\n",
            "fn pass(value: set<list<bool>>) -> set<list<bool>> {\n",
            "    when { return value }\n",
            "}\n",
        ),
    );
    let output = hgl(&["emit", &unwritten, "--out", out.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        text(&output.stdout),
        "nothing written: the cpp target does not write all of the program yet\n"
    );
    assert!(
        text(&output.stderr).contains(
            "error[build/target.unsupported]: the cpp target does not write a temporal value of `set<list<bool>>` yet"
        ),
        "{}",
        text(&output.stderr)
    );
    assert!(!out.exists());
}

/// `--target` takes `cpp`, the one target: any other value, `hgraph`
/// among them, is a usage error, status 2, that names the values it
/// takes, and nothing is read or written.
#[test]
fn a_target_other_than_cpp_is_refused() {
    let directory = scratch("other_target");
    let source = write(&directory, "demo.hgl", "module demo\n");
    let out = directory.join("out");
    let written_to = argument(&out);
    for arguments in [
        vec!["emit", &source, "--target", "hgraph", "--out", &written_to],
        vec!["test", &source, "--target", "hgraph"],
        vec!["run", &source, "--target", "hgraph"],
    ] {
        let output = hgl(&arguments);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert_eq!(text(&output.stdout), "", "{arguments:?}");
        let said = text(&output.stderr);
        assert!(
            said.starts_with("error: invalid value 'hgraph' for '--target <TARGET>'\n"),
            "{said}"
        );
        assert!(said.contains("[possible values: cpp]"), "{said}");
    }
    assert!(!out.exists());
}
