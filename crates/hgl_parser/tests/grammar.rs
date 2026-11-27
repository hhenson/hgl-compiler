//! The pinned grammar and coverage of the syntax the parser implements.
//!
//! Productions carry no rule ID, so no rule test can stand for them. Two
//! tests do:
//!
//! * `fixtures/grammar.ebnf` records the pinned productions. The first
//!   test compares it with the chapters. Pending grammar is tracked in
//!   `docs/rules.md` and the specification corpus test.
//! * The second test checks that the parser's syntax nodes are exercised.
//!
//! When moving the pin, run this test with `HGL_UPDATE_GRAMMAR=1` to
//! refresh the fixture and review the changes against the rule inventory.

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
fn the_grammar_fixture_matches_the_pinned_chapters() {
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
        "the pinned grammar differs from the fixture.\n\
         In the specification, not in the fixture:\n{moved:#?}\n\
         In the fixture, not in the specification:\n{gone:#?}\n\
         Review these productions and their implementation status, then run with \
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

/// A file that reaches each of the parser's current language constructs.
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
    requires T in {i64, f64} && !comparable(T) || (N == 3) && each R in Rest { numeric(R) } && any S in types(Rest) { S == i64 } && enabled {
    return value
}

fn arrow{const by: i64 = 1}(x: i64) -> i64 => x + by

# GEN-3: category keywords are membership operands, never source types.
const fn record<T>(value: T) -> T requires T in struct => value
const fn tag<T>(value: T) -> T requires T in enum => value

const fn apply(f: const fn(value: i64) -> i64, x: i64) -> i64 => f(x)
const fn via_generic(x: i64) -> i64 => apply(other::increase<i64>, x)
const fn forward<...Args>(args: ...Args) => another(...args)

export const limit: i64 = 2 * 5

# TYPE-18: an unevaluated type query has its own expression node.
const known: Type = type(limit)

export abstract struct Quote<T> : Base, other::Mixin requires T is Shape {
    bid: T = 0.0
    time: datetime
    kind = "quote"
}

export enum Side { buy, sell = 2 }

operator add<T>(left: T, right: T) -> T
    properties<i64> { associative, commutative, identity = 0 }

# FN-8 and GEN-17: a dependent call pack and a member type inside a constraint.
operator pass<K, R, ...Ps, ...As>(f: fn(...{Ps}) -> R, args: ...{As for f}) -> R
    requires each arg in arg_keys(As) { As[arg] == dict<K, Ps[arg]> }

instantiate add<i64>, convert<_, f64>

export native type Handle

native type Clock {
    evaluation_time: datetime
}

native const fn valid<T>(input: endpoint<T>) -> bool

native fn node(ref value: i64) -> ref i64 {
    inject out
    start;
    when;
    stop;
}

test {
    fn helper() => 1
}

test everything {
    assert eval arrow{by: 1}(x: [1, _]) == [2, _]
    assert raises("value.missing_key") { lookup(key) }
}

@timeout(5s)
integration_test live {
    let seen = run arrow(x: [1])
}

fn body(
    a: tuple<i64, str>,
    b: list<f64, unbounded>,
    c: set<str>,
    d: dict<str, f64>,
    e: rolling_window<f64, 5m, 3>,
    ref f: f64,
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
    let found = other::helper(x)[0].time
    let ratio = x /{zero: .error} y
    start { count = 0 }
    stop { count = 0 }
    when modified(x) { count += 1 }
    while { yield 1s: count }
    for key, item in d { count += item }
    let members = {1, 2}
    let keyed = {"a": 1}
    let squares = [for i in 0..<4: i * i]
    helper(...{for key in arg_keys(Ps): if key in arg_keys(As) { args[key] }})
    switch x {
        case 1: return 1
        default: return 0
    }
    if x == null { return 0 } else { return 1 }
}
"#;

/// An application's file: what a module's file has not (UNIT-14).
const APPLICATION: &str = r#"application tour.live

use tour.all::{arrow}

inject global_state
put(global_state, "limit", 3)

run arrow(1)
"#;

#[test]
fn every_kind_of_node_is_reached() {
    let file = run(Entry::SourceFile, TOUR);
    assert_eq!(file.findings, [], "{}", file.report);
    let application = run(Entry::SourceFile, APPLICATION);
    assert_eq!(application.findings, [], "{}", application.report);
    let fragment = run(Entry::Statements, "let a = 1\n");
    assert_eq!(fragment.findings, []);

    // `descendants` gives a node and every node under it.
    let reached: BTreeSet<SyntaxKind> = file
        .tree
        .descendants()
        .chain(application.tree.descendants())
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
