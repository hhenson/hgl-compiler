//! The grammar the parser implements, against the pinned specification's.
//!
//! Productions carry no rule ID, so no rule test can stand for them. Two
//! tests do:
//!
//! * `fixtures/grammar.ebnf` is a copy of every production the parser was
//!   written against. The first test reads the productions out of the
//!   pinned chapters and compares. Raising the pin shows at once which
//!   productions moved, and so which grammar functions to revisit.
//! * The second parses a file that uses every construct and checks that
//!   every kind of node turns up, so no production is left without a test
//!   that reaches it.
//!
//! After revisiting the parser for a changed production, rewrite the copy
//! by running this test with `HGL_UPDATE_GRAMMAR=1` set, and review the
//! difference like any other change.

mod common;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use common::run;
use hgl_parser::{Entry, SyntaxKind};

/// The chapter whose productions are the lexer's: tokens, not syntax.
const LEXICAL_CHAPTER: &str = "source-text.md";

/// The productions of that chapter the parser does read.
const SHARED_WITH_LEXER: [&str; 3] = ["literal", "NL", "EOF"];

/// The pinned language chapters' directory.
fn chapters() -> PathBuf {
    // CARGO_MANIFEST_DIR is this crate's directory, set by Cargo when it
    // compiles the test. The workspace root is two levels up.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/hgl_spec/spec/language")
}

/// The productions in the fenced `ebnf` blocks of a Markdown document,
/// each on one line with single spaces, ending in ` ;`.
fn productions(markdown: &str) -> Vec<String> {
    // The text of the `ebnf` blocks, one after another.
    let mut grammar = String::new();
    let mut inside = false;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            inside = !inside && line.trim() == "```ebnf";
        } else if inside {
            grammar.push_str(line);
            grammar.push('\n');
        }
    }

    // A production ends at a `;` that is not inside a quoted terminal:
    // `native_hook = ( "start" | "when" | "stop" ), ";" ;` has both.
    let mut found = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut characters = grammar.chars();
    while let Some(character) = characters.next() {
        current.push(character);
        match character {
            '"' => quoted = !quoted,
            // An escaped character inside a terminal, as in `"\""`.
            '\\' if quoted => current.extend(characters.next()),
            ';' if !quoted => {
                // `split_whitespace` drops every run of spaces and line
                // breaks, so a production reads the same however the
                // chapter wraps it.
                let words: Vec<&str> = current.split_whitespace().collect();
                found.push(words.join(" "));
                current.clear();
            }
            _ => {}
        }
    }
    assert!(
        current.trim().is_empty(),
        "text after the last production: {current}"
    );
    found
}

/// The name a production defines: the text before its ` = `.
fn name(production: &str) -> &str {
    production.split(" = ").next().unwrap_or(production)
}

/// The grammar of the pinned chapters as the fixture records it: a
/// `# chapter` line, then that chapter's productions, for each chapter in
/// name order.
fn pinned_grammar() -> String {
    let mut files: Vec<PathBuf> = fs::read_dir(chapters())
        .expect("the `spec/hgl_spec` submodule is checked out")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    files.sort();

    let mut grammar = String::new();
    for path in files {
        let chapter = path.file_name().unwrap().to_string_lossy().to_string();
        let markdown = fs::read_to_string(&path).expect("a readable chapter");
        let wanted: Vec<String> = productions(&markdown)
            .into_iter()
            .filter(|production| {
                chapter != LEXICAL_CHAPTER || SHARED_WITH_LEXER.contains(&name(production))
            })
            .collect();
        if wanted.is_empty() {
            continue;
        }
        grammar.push_str(&format!("# {chapter}\n"));
        for production in wanted {
            grammar.push_str(&production);
            grammar.push('\n');
        }
    }
    grammar
}

#[test]
fn the_pinned_grammar_is_the_one_the_parser_was_written_against() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/grammar.ebnf");
    let pinned = pinned_grammar();
    if std::env::var_os("HGL_UPDATE_GRAMMAR").is_some() {
        fs::write(&fixture, &pinned).expect("the fixture can be written");
    }
    // Read as LF whatever the checkout did to the fixture.
    let recorded = fs::read_to_string(&fixture)
        .expect("tests/fixtures/grammar.ebnf exists")
        .replace("\r\n", "\n");

    let pinned_lines: BTreeSet<&str> = pinned.lines().collect();
    let recorded_lines: BTreeSet<&str> = recorded.lines().collect();
    let moved: Vec<&&str> = pinned_lines.difference(&recorded_lines).collect();
    let gone: Vec<&&str> = recorded_lines.difference(&pinned_lines).collect();
    assert!(
        moved.is_empty() && gone.is_empty(),
        "the pinned grammar differs from the one the parser was written against.\n\
         In the specification, not in the parser's copy:\n{moved:#?}\n\
         In the parser's copy, not in the specification:\n{gone:#?}\n\
         Revisit the grammar functions for these, then run this test with \
         HGL_UPDATE_GRAMMAR=1 to rewrite the copy."
    );
    // Same productions; the order is the chapters', kept for reading.
    assert_eq!(recorded, pinned);
    // The parser's own entry point is among them.
    assert!(pinned.contains("\nsource_file = "));
}

#[test]
fn productions_are_cut_at_the_semicolon_that_ends_them() {
    let markdown = "\
text
```ebnf
a = \"x\", b
  | c ;
hook = ( \"start\" | \"stop\" ), \";\" ;
quote = \"\\\"\", \";\" ;
```
```hgl
let a = 1;
```
";
    assert_eq!(
        productions(markdown),
        [
            "a = \"x\", b | c ;",
            "hook = ( \"start\" | \"stop\" ), \";\" ;",
            "quote = \"\\\"\", \";\" ;"
        ]
    );
    assert_eq!(name("a = \"x\", b | c ;"), "a");
}

/// A file that uses every construct of the grammar at least once.
const TOUR: &str = r#"module tour.all part one

use tour.other::{helper, const}
use tour.other as other

/** Scales a value. */
@suppress_warning("node.no_activation")
export const fn scale<T, ...Rest, const N: i64>(
    value: T,
    const factor: f64 = 2.0,
    rest: ...Rest{1:*},
    named: ...{str},
) -> f64?
    requires T in {i64, f64} && !comparable(T) || (N == 3) && each R in Rest { numeric(R) } {
    return value
}

fn arrow(x: i64) -> i64 => x + 1

export abstract struct Quote<T> : Base, other::Mixin requires T is Shape {
    bid: T = 0.0
    time: datetime
    kind = "quote"
}

export enum Side { buy, sell = 2 }

operator add<T>(left: T, right: T) -> T
    properties<i64> { associative, commutative, identity = 0 }

instantiate add<i64>, convert<_, f64>

export native type Handle

native fn node(value: i64) -> i64 {
    inject out
    start;
    when;
    stop;
}

test {
    fn helper() => 1
}

test everything {
    assert eval(arrow, x: [1, _]) == [2, _]
    assert raises("value.missing_key") { lookup(key) }
}

fn body(
    a: tuple<i64, str>,
    b: list<f64, unbounded>,
    c: set<str>,
    d: dict<str, f64>,
    e: rolling<f64, 5m, 3>,
    f: ref<f64>,
    g: signal,
    h: atomic<Quote<f64>>,
    i: result<f64, error>,
    j: delta<list<f64>>,
    k: other::Thing,
) {
    let x = 1
    var y: i64 = (2)
    state count: i64 = 0
    cache last: f64 = 0.0
    inject out, clock
    y += -x * 3
    let pair = (x, y)
    let values = [1, _, 3]
    let ticks = [1s: 7]
    let quote = Quote<f64>(bid: 1.0)
    let either = a<b>(c)
    let change = delta<list<f64>>(added: values)
    let text = str(x)
    let next = fn(v: i64) -> i64 => v + 1
    let found = other::helper(x)[0].time?
    let ratio = x /{zero: error} y
    start { count = 0 }
    stop { count = 0 }
    when modified(x) { count += 1 }
    while { yield 1s: count }
    for key, item in d { count += item }
    switch x {
        case 1: return 1
        default: return 0
    }
    if x == null { return 0 } else { return 1 }
}
"#;

#[test]
fn every_kind_of_node_is_reached() {
    let file = run(Entry::SourceFile, TOUR);
    assert_eq!(file.findings, [], "{}", file.report);
    let fragment = run(Entry::Statements, "let a = 1\n");
    assert_eq!(fragment.findings, []);

    // `descendants` gives a node and every node under it.
    let reached: BTreeSet<SyntaxKind> = file
        .tree
        .descendants()
        .chain(fragment.tree.descendants())
        .map(|node| node.kind())
        .collect();
    // Every kind that names a node, but `Error`, which accepted source
    // never has.
    let all: BTreeSet<SyntaxKind> = SyntaxKind::ALL
        .iter()
        .copied()
        .filter(|kind| !kind.is_token() && !matches!(kind, SyntaxKind::Eof | SyntaxKind::Error))
        .collect();
    let missing: Vec<&SyntaxKind> = all.difference(&reached).collect();
    assert!(
        missing.is_empty(),
        "no node of kind {missing:?} in the tour"
    );
}
