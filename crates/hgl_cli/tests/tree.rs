//! Runs `hgl tree` the way a user would.
//!
//! `tests/fixtures/accepted.hgl` is the first example block of the
//! specification's expressions-and-bindings chapter, verbatim. The other
//! sources are written by the tests when they run, each built round one
//! of the specification's own illustrations.

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

/// Writes `text` to a file of this test run and gives its path.
fn scratch(name: &str, text: &str) -> String {
    // Cargo gives integration tests a scratch directory under `target/`.
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("tree");
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join(name);
    fs::write(&path, text).unwrap();
    path.display().to_string()
}

/// A stream's bytes as text.
fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("hgl writes UTF-8")
}

/// The kind named on each line of a printed tree, with its indentation:
/// the tree's shape without positions or text.
fn shape(tree: &str) -> Vec<String> {
    tree.lines()
        .map(|line| line.split('@').next().unwrap_or(line).to_string())
        .collect()
}

#[test]
fn accepted_source_prints_its_tree_and_exits_zero() {
    let output = hgl(&["tree", &fixture("accepted.hgl")]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(text(&output.stderr), "");

    let stdout = text(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    // The root, then `module examples.bindings` under it.
    assert!(lines[0].starts_with("SourceFile@0.."), "{stdout}");
    assert_eq!(
        &lines[1..5],
        [
            "  ModuleDecl@0..24",
            "    KwModule@0..6 \"module\"",
            "    Whitespace@6..7 \" \"",
            "    ModulePath@7..24",
        ]
    );
    // `const` is a name to the lexer and a modifier here (FN-5): it sits
    // inside the function's node.
    assert!(
        stdout.contains("  FnDecl@26..128\n    Ident@26..31 \"const\"\n"),
        "{stdout}"
    );
    // The tree is lossless: whitespace and line breaks are in it.
    assert!(stdout.contains("Newline@"), "{stdout}");
    // Accepted source has nothing the parser could not place.
    assert!(!stdout.contains("Error@"), "{stdout}");
}

#[test]
fn rejected_source_prints_a_tree_and_exits_one() {
    // CTRL-9's rejection: the first block is the body, so the second is
    // a second statement on the line (LEX-6).
    let path = scratch(
        "rejected.hgl",
        "module examples.rejection\n\nfn broken() {\n    when { ready } { step() }\n}\n",
    );
    let output = hgl(&["tree", &path]);
    assert_eq!(output.status.code(), Some(1));

    let stderr = text(&output.stderr);
    assert!(
        stderr.contains("error[parse/syntax.expected_token]: expected a line break, found `{`"),
        "{stderr}"
    );
    assert!(stderr.contains("rejected.hgl:4:20"), "{stderr}");
    assert!(stderr.contains("rule LEX-6"), "{stderr}");
    // The code is the specification's.
    assert!(!stderr.contains("compiler-local"), "{stderr}");

    // The parser does not stop at an error: the whole file is in the
    // tree, the second block included.
    let stdout = text(&output.stdout);
    assert!(stdout.contains("WhenStmt@"), "{stdout}");
    assert!(stdout.contains("Ident@62..66 \"step\""), "{stdout}");
    assert!(
        stdout.trim_end().ends_with("Newline@72..73 \"\\n\""),
        "{stdout}"
    );
}

#[test]
fn a_documentation_rejection_has_its_catalogued_code() {
    // SDOC-1 rejects a repeated documentation comment, under the
    // catalogue's `doc.repeated`.
    let path = scratch(
        "documented.hgl",
        "module examples.documented\n\n/** One. */\n/** Two. */\nconst fn one() -> i64 => 1\n",
    );
    let output = hgl(&["tree", &path]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = text(&output.stderr);
    assert!(stderr.contains("error[parse/doc.repeated]"), "{stderr}");
    assert!(stderr.contains("rule SDOC-1"), "{stderr}");
    assert!(!stderr.contains("compiler-local"), "{stderr}");
}

#[test]
fn a_fragment_needs_no_module_line() {
    // EXPR-10's illustration, as the chapter writes it: an expression on
    // its own.
    let path = scratch("fragment.hgl", "1.0 /{zero: .ieee} 0.0\n");

    // As a file it has no `module` line (UNIT-1).
    let output = hgl(&["tree", &path]);
    assert_eq!(output.status.code(), Some(1));
    assert!(text(&output.stderr).contains("rule UNIT-1"));

    let output = hgl(&["tree", "--fragment", "statements", &path]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(text(&output.stderr), "");
    let stdout = text(&output.stdout);
    assert!(
        stdout.starts_with("Fragment@0..23\n  ExprStmt@0..22\n"),
        "{stdout}"
    );
    assert!(stdout.contains("OperatorConfig@5..18"), "{stdout}");
    // TYPE-7: `.ieee` is a member of the option's enum.
    assert!(stdout.contains("ImpliedMember@12..17"), "{stdout}");

    // Statements are not declarations.
    let output = hgl(&["tree", "--fragment", "declarations", &path]);
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn every_line_break_form_gives_the_same_tree() {
    // LEX-15, LEX-5: LF, CRLF and CR separate statements alike.
    // Read as LF whatever the checkout did to the fixture.
    let original = fs::read_to_string(fixture("accepted.hgl"))
        .unwrap()
        .replace("\r\n", "\n");
    let run = |name: &str, line_break: &str| {
        let path = scratch(name, &original.replace('\n', line_break));
        let output = hgl(&["tree", &path]);
        (
            output.status.code(),
            shape(&text(&output.stdout)),
            text(&output.stderr),
        )
    };
    let expected = run("lf.hgl", "\n");
    assert_eq!(expected.0, Some(0));
    assert!(expected.1.len() > 50, "{:?}", expected.1);
    assert_eq!(run("crlf.hgl", "\r\n"), expected);
    assert_eq!(run("cr.hgl", "\r"), expected);
}

#[test]
fn a_missing_file_exits_two() {
    let output = hgl(&["tree", &fixture("no_such_file.hgl")]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(text(&output.stdout), "");
    assert!(text(&output.stderr).starts_with("hgl: cannot read "));
}

#[test]
fn an_unknown_fragment_kind_is_a_usage_error() {
    let output = hgl(&[
        "tree",
        "--fragment",
        "expressions",
        &fixture("accepted.hgl"),
    ]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn a_limit_is_reported_and_can_be_raised() {
    // 150 levels of parentheses: past the default limit of 100.
    let nested = format!("{}a{}\n", "(".repeat(150), ")".repeat(150));
    let path = scratch("nested.hgl", &nested);

    let output = hgl(&["tree", "--fragment", "statements", &path]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = text(&output.stderr);
    assert!(
        stderr.contains(
            "error[parse/syntax.nesting_limit]: nesting deeper than 100 levels is not read"
        ),
        "{stderr}"
    );
    // The limit is the compiler's, so the code is too.
    assert!(stderr.contains("compiler-local"), "{stderr}");

    let output = hgl(&[
        "tree",
        "--fragment",
        "statements",
        "--nesting-limit",
        "200",
        &path,
    ]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(text(&output.stderr), "");

    // A chain is long, not nested: the other limit.
    let chain = format!("a{}\n", " + a".repeat(600));
    let path = scratch("chain.hgl", &chain);
    let output = hgl(&["tree", "--fragment", "statements", &path]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        text(&output.stderr).contains("a syntax tree deeper than 500 levels is not built"),
        "{}",
        text(&output.stderr)
    );
    let output = hgl(&[
        "tree",
        "--fragment",
        "statements",
        "--tree-depth-limit",
        "1000",
        &path,
    ]);
    assert_eq!(output.status.code(), Some(0));

    // Far beyond any stack a first thread has: the command runs on one
    // sized for the limit.
    let deep = format!("{}a{}\n", "(".repeat(20_000), ")".repeat(20_000));
    let path = scratch("deep.hgl", &deep);
    let output = hgl(&[
        "tree",
        "--fragment",
        "statements",
        "--nesting-limit",
        "25000",
        "--tree-depth-limit",
        "50000",
        &path,
    ]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(text(&output.stderr), "");
}
