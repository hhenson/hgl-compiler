//! Runs the built `hgl` program the way a user would.
//!
//! The fixtures hold only HGL taken from the specification:
//!
//! * `accepted.hgl` is the first example block of the
//!   expressions-and-bindings chapter, verbatim;
//! * `rejected.hgl` is the source-text chapter's four deliberate lexical
//!   rejections, one per line.

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

/// The path of a file in `tests/fixtures`.
fn fixture(name: &str) -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    path.display().to_string()
}

/// A stream's bytes as text.
fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("hgl writes UTF-8")
}

#[test]
fn accepted_source_prints_tokens_and_exits_zero() {
    let output = hgl(&["tokens", &fixture("accepted.hgl")]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(text(&output.stderr), "");

    let stdout = text(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    // `module examples.bindings`, then the line ending.
    let columns = |line: &str| {
        line.split_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    assert_eq!(columns(lines[0]), ["1:1", "KwModule", "\"module\""]);
    assert_eq!(columns(lines[1]), ["1:8", "Ident", "\"examples\""]);
    assert_eq!(columns(lines[4]), ["1:25", "Newline", "\"\\n\""]);
    // `const` is contextual (LEX-9): an identifier to the lexer.
    assert!(stdout.contains("Ident        \"const\""), "{stdout}");
    // A literal shows the value read from it.
    assert!(
        lines
            .iter()
            .any(|line| columns(line) == ["9:34", "Int", "\"3\"", "3"]),
        "{stdout}"
    );
    // Whitespace is left out unless asked for.
    assert!(!stdout.contains("Whitespace"), "{stdout}");
}

#[test]
fn trivia_flag_prints_every_token() {
    let path = fixture("accepted.hgl");
    let output = hgl(&["tokens", "--trivia", &path]);
    assert_eq!(output.status.code(), Some(0));

    let stdout = text(&output.stdout);
    assert!(stdout.contains("Whitespace"), "{stdout}");
    // Leaving trivia out removes lines and changes nothing else.
    let without = text(&hgl(&["tokens", &path]).stdout);
    let kept: Vec<&str> = stdout
        .lines()
        .filter(|line| !line.contains(" Whitespace "))
        .collect();
    assert_eq!(kept, without.lines().collect::<Vec<_>>());
    assert!(kept.len() < stdout.lines().count());
}

#[test]
fn rejected_source_reports_each_error_and_exits_one() {
    let output = hgl(&["tokens", &fixture("rejected.hgl")]);
    assert_eq!(output.status.code(), Some(1));

    // Tokens are still printed: the lexer never stops at an error.
    let stdout = text(&output.stdout);
    assert!(stdout.contains("Duration"), "{stdout}");

    // One diagnostic per rejection, in source order, each naming its rule.
    let stderr = text(&output.stderr);
    let headers: Vec<&str> = stderr
        .lines()
        .filter(|line| line.starts_with("error["))
        .map(|line| line.split(':').next().unwrap())
        .collect();
    assert_eq!(
        headers,
        [
            "error[parse/lex.invalid_duration]",
            "error[parse/lex.invalid_duration]",
            "error[parse/lex.invalid_temporal]",
            "error[parse/lex.unclosed_string]",
        ]
    );
    for rule in ["rule LEX-13", "rule LEX-11", "rule LEX-2"] {
        assert!(stderr.contains(rule), "{stderr}");
    }
    assert!(stderr.contains("rejected.hgl:3:1"), "{stderr}");
}

#[test]
fn a_missing_file_exits_two() {
    let output = hgl(&["tokens", &fixture("no_such_file.hgl")]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(text(&output.stdout), "");
    assert!(text(&output.stderr).starts_with("hgl: cannot read "));
}

#[test]
fn a_usage_error_exits_two() {
    let output = hgl(&["tokens"]);
    assert_eq!(output.status.code(), Some(2));
}
