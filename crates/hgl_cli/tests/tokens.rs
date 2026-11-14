//! Runs the built `hgl` program the way a user would.
//!
//! The fixtures hold only HGL taken from the specification:
//!
//! * `accepted.hgl` is the first example block of the
//!   expressions-and-bindings chapter, verbatim;
//! * `rejected.hgl` is the source-text chapter's four deliberate lexical
//!   rejections, one per line.
//!
//! The fixtures are stored with LF line breaks. The line-break test writes
//! its CRLF and CR copies when it runs, so it does not depend on how the
//! fixtures were checked out.

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
    // `module examples.bindings`, then the line break.
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
            "error[parse/duration.invalid]",
            "error[parse/duration.invalid]",
            "error[parse/date_time.invalid_field]",
            "error[parse/string.unclosed]",
        ]
    );
    for rule in ["rule LEX-13", "rule LEX-11", "rule LEX-2"] {
        assert!(stderr.contains(rule), "{stderr}");
    }
    assert!(stderr.contains("rejected.hgl:3:1"), "{stderr}");
    // The codes are the specification's, so none is marked local.
    assert!(!stderr.contains("compiler-local"), "{stderr}");
}

#[test]
fn a_malformed_temporal_literal_is_reported_once() {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("literals");
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("malformed-date.hgl");
    // LEX-18 rejects the first token as malformed. LEX-11 still checks the
    // calendar fields of the second token, whose shape is valid.
    fs::write(
        &path,
        "let malformed = @2026-02-29x\nlet invalid = @2026-02-29\n",
    )
    .unwrap();

    let output = hgl(&["tokens", &path.display().to_string()]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = text(&output.stderr);
    let headers: Vec<&str> = stderr
        .lines()
        .filter_map(|line| line.strip_prefix("error["))
        .map(|header| header.split_once(']').unwrap().0)
        .collect();
    assert_eq!(
        headers,
        ["parse/date_time.malformed", "parse/date_time.invalid_field"],
        "{stderr}"
    );
    assert!(stderr.contains("malformed-date.hgl:1:17"), "{stderr}");
    assert!(stderr.contains("malformed-date.hgl:2:15"), "{stderr}");
}

#[test]
fn source_that_is_not_utf8_is_rejected_not_unreadable() {
    // LEX-16: the bytes are source the language rejects, so the exit
    // status is 1, with one diagnostic at the first offending byte.
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("encoding");
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("latin1.hgl");
    // `caf\xe9` is "café" in Latin-1: 0xE9 begins no UTF-8 character here.
    fs::write(&path, b"let a = 1\nlet b = \"caf\xe9\"\nlet c = 30m1h\n").unwrap();
    let path = path.display().to_string();

    let output = hgl(&["tokens", &path]);
    assert_eq!(output.status.code(), Some(1));

    let stderr = text(&output.stderr);
    let headers: Vec<&str> = stderr
        .lines()
        .filter(|line| line.starts_with("error["))
        .collect();
    assert_eq!(
        headers,
        ["error[parse/syntax.invalid_encoding]: source is not well-formed UTF-8"]
    );
    assert!(stderr.contains("latin1.hgl:2:13"), "{stderr}");
    assert!(stderr.contains("rule LEX-16"), "{stderr}");
    assert!(stderr.contains("0xE9"), "{stderr}");

    // The tokens before the byte are printed; nothing after it was read.
    let stdout = text(&output.stdout);
    assert!(stdout.starts_with("1:1 "), "{stdout}");
    assert!(!stdout.contains("Duration"), "{stdout}");
}

#[test]
fn a_leading_byte_order_mark_is_ignored() {
    // LEX-16: positions count from the first character after it.
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("encoding");
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("marked.hgl");
    fs::write(&path, b"\xef\xbb\xbflet a = 1\n").unwrap();

    let output = hgl(&["tokens", &path.display().to_string()]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(text(&output.stderr), "");
    let stdout = text(&output.stdout);
    let first: Vec<&str> = stdout.lines().next().unwrap().split_whitespace().collect();
    assert_eq!(first, ["1:1", "KwLet", "\"let\""]);
}

#[test]
fn every_line_break_form_gives_the_same_report() {
    // LEX-15.
    // Read as LF whatever the checkout did to the fixture.
    let original = fs::read_to_string(fixture("rejected.hgl"))
        .unwrap()
        .replace("\r\n", "\n");

    // Cargo gives integration tests a scratch directory under `target/`.
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("line-breaks");
    fs::create_dir_all(&directory).unwrap();

    // Runs `hgl tokens` on the fixture rewritten with `line_break`. Returns
    // the exit code, each token's position and kind, and the report with
    // the file's path replaced by its name.
    let run = |name: &str, line_break: &str| {
        let path = directory.join(name);
        fs::write(&path, original.replace('\n', line_break)).unwrap();
        let path = path.display().to_string();
        let output = hgl(&["tokens", &path]);
        let tokens: Vec<String> = text(&output.stdout)
            .lines()
            .map(|line| {
                line.split_whitespace()
                    .take(2)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        let report = text(&output.stderr).replace(&path, "rejected.hgl");
        (output.status.code(), tokens, report)
    };

    let expected = run("lf.hgl", "\n");
    assert_eq!(expected.0, Some(1));
    assert!(
        expected.1.contains(&"3:1 Temporal".to_string()),
        "{:?}",
        expected.1
    );
    assert!(expected.2.contains("rejected.hgl:4:1"), "{}", expected.2);
    assert_eq!(run("crlf.hgl", "\r\n"), expected);
    assert_eq!(run("cr.hgl", "\r"), expected);
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
