//! The parser is total: any text gives a tree, and never a crash.
//!
//! `docs/architecture.md` requires it of every stage: "bad input is
//! reported and processing continues". These tests feed the parser source
//! that is broken in many ways and check, for each text and each of the
//! three ways in, that
//!
//! * parsing returns (no panic, no endless loop, no stack overflow);
//! * the tree's text is the source text (checked by `common::run`);
//! * the tree is no deeper than the parser promises;
//! * an `Error` node comes with a diagnostic.
//!
//! The broken source is made from the specification's own examples, cut
//! short and with tokens removed, from random runs of tokens, and from
//! deep nesting.

mod common;

use common::{Outcome, corpus, run};
use hgl_lexer::TokenKind;
use hgl_parser::{Entry, SyntaxNode};

/// The three ways in.
const ENTRIES: [Entry; 3] = [Entry::SourceFile, Entry::Declarations, Entry::Statements];

/// The depth the parser promises not to exceed. `hgl_parser` does not
/// export its limit; this is the documented value.
const MAX_TREE_DEPTH: usize = 500;

/// The most nodes on any path from `root` down. Walks without calling
/// itself, so that a tree that is too deep fails the test, not the stack.
fn depth(root: &SyntaxNode) -> usize {
    let mut deepest = 0;
    let mut current = 0usize;
    for step in root.preorder() {
        match step {
            rowan::WalkEvent::Enter(_) => {
                current += 1;
                deepest = deepest.max(current);
            }
            rowan::WalkEvent::Leave(_) => current -= 1,
        }
    }
    deepest
}

/// The byte range of each token of `text` that is not whitespace or a
/// comment.
fn tokens_of(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut sources = hgl_source::SourceMap::new();
    let file = sources.add("sample.hgl", text).unwrap();
    hgl_lexer::lex(&sources, file, &mut hgl_diagnostics::Diagnostics::new())
        .iter()
        .filter(|token| !token.kind.is_trivia())
        .map(|token| token.span.range())
        .collect()
}

/// Parses `text` every way and checks what must always hold. Returns the
/// three outcomes, in the order of [`ENTRIES`].
fn survives(text: &str) -> Vec<Outcome> {
    ENTRIES
        .iter()
        .map(|entry| {
            let outcome = run(*entry, text);
            assert!(depth(&outcome.tree) <= MAX_TREE_DEPTH, "{text:.80}");
            assert!(
                !outcome.has_error_node() || !outcome.findings.is_empty(),
                "an Error node without a diagnostic in {text:?} as {entry:?}"
            );
            outcome
        })
        .collect()
}

/// True when `outcome` has the report of a nesting limit.
fn reports_nesting(outcome: &Outcome) -> bool {
    outcome
        .findings
        .iter()
        .any(|finding| finding.code == "syntax.nesting_limit" && !finding.catalogued)
}

#[test]
fn every_prefix_of_the_corpus_parses() {
    // Source cut off after any token: in the middle of a list, a block, a
    // declaration.
    let mut count = 0usize;
    for sample in &corpus() {
        for token in tokens_of(&sample.text) {
            survives(&sample.text[..token.end]);
            count += 1;
        }
    }
    println!("{count} prefixes");
}

#[test]
fn the_corpus_parses_with_any_one_token_removed() {
    // A missing bracket, keyword or operator, one at a time.
    let mut count = 0usize;
    for sample in &corpus() {
        for range in tokens_of(&sample.text) {
            let without = format!(
                "{}{}",
                &sample.text[..range.start],
                &sample.text[range.end..]
            );
            survives(&without);
            count += 1;
        }
    }
    println!("{count} removals");
}

#[test]
fn random_runs_of_tokens_parse() {
    // Every reserved word and symbol, some names and literals, and line
    // breaks, in an order no person would write.
    let mut vocabulary: Vec<&str> = TokenKind::KEYWORDS
        .iter()
        .chain(TokenKind::PUNCTUATION)
        .map(|(text, _)| *text)
        .collect();
    vocabulary.extend([
        "a",
        "b",
        "T",
        "const",
        "native",
        "type",
        "switch",
        "case",
        "default",
        "in",
        "each",
        "properties",
        "identity",
        "raises",
        "delta",
        "list",
        "signal",
        "unbounded",
        "_",
        "1",
        "2.5",
        "\"s\"",
        "1s",
        "@2026-01-01",
        "\n",
        "\n",
        "\n",
        "/** d */",
        "# c\n",
    ]);
    // A linear congruential generator: a few lines that give the same
    // "random" numbers on every run and every machine.
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    let mut next = |bound: usize| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        // The high bits are the better ones.
        ((state >> 33) as usize) % bound
    };
    for _ in 0..3000 {
        let length = 1 + next(40);
        let mut text = String::new();
        for _ in 0..length {
            text.push_str(vocabulary[next(vocabulary.len())]);
            text.push(' ');
        }
        survives(&text);
    }
}

#[test]
fn a_long_list_of_comparisons_is_read_once() {
    // GEN-14: each `name <` might open generic arguments, and finding
    // that it does not means reading ahead. Read naively, each of these
    // would run to the closing bracket: for a list this long, minutes of
    // work. The parser remembers where an unclosed list's entries began.
    let count = 20_000;
    let comparisons: Vec<String> = (0..count).map(|i| format!("a{i} < b{i}")).collect();
    let list = comparisons.join(", ");
    for text in [
        format!("f({list})"),
        format!("[{list}]"),
        // A `>` at the end that closes nothing.
        format!("f({list}, y > z)"),
        // An argument that no generic argument list can hold.
        format!("f({list}, key: value, y > (z))"),
    ] {
        let outcome = run(Entry::Statements, &text);
        assert_eq!(outcome.findings, [], "{text:.40}");
        // Every one is a comparison.
        let comparisons = outcome
            .tree
            .descendants()
            .filter(|node| node.kind() == hgl_parser::SyntaxKind::BinaryExpr)
            .count();
        assert!(comparisons >= count, "{comparisons} in {text:.40}");
    }
}

/// `open` written `count` times, then `middle`, then `close` as often.
fn nested(open: &str, middle: &str, close: &str, count: usize) -> String {
    format!("{}{middle}{}", open.repeat(count), close.repeat(count))
}

#[test]
fn deep_nesting_is_reported_not_fatal() {
    // Each of these would send a recursive-descent parser as deep as the
    // source goes. Far past the limit, the parser reports and stops
    // descending.
    let count = 3000;
    let texts = [
        nested("(", "a", ")", count),
        nested("[", "a", "]", count),
        nested("{", "a", "}", count),
        nested("f(", "a", ")", count),
        nested("a[", "a", "]", count),
        nested("-", "a", "", count),
        nested("!", "a", "", count),
        nested("a +{b: ", "c", "} d", count),
        nested("fn() => ", "a", "", count),
        nested("fn() { ", "a", " }", count),
        nested("if a { ", "b", " }", count),
        nested("if a { b } else ", "{ c }", "", count),
        nested("when { ", "a", " }", count),
        nested("switch a { case 1: ", "b", " }", count),
        format!("let x: {}", nested("list<", "i64", ">", count)),
        format!("let x: {}", nested("Box<", "i64", ">", count)),
        nested("Box<", "i64", ">", count) + "(1)",
        nested("a < ", "b", "", count),
        nested("test { ", "fn f() {}", " }", count),
        format!("fn f() requires {} {{}}", nested("!", "p(T)", "", count)),
        format!("fn f() requires {} {{}}", nested("(", "p(T)", ")", count)),
        format!(
            "fn f() requires T in {} {{}}",
            nested("{", "i64", "}", count)
        ),
        format!(
            "fn f() requires {} {{}}",
            nested("each T in Ts { ", "p(T)", " }", count)
        ),
        // Unclosed: the same, with nothing to come back up for.
        "(".repeat(count),
        "[".repeat(count),
        "{".repeat(count),
        "a<".repeat(count),
    ];
    for text in &texts {
        // Some of these are declarations and some statements; read as
        // what it is, each is too deep.
        let outcomes = survives(text);
        assert!(
            outcomes.iter().any(reports_nesting),
            "no nesting report for {text:.60}"
        );
    }
}

#[test]
fn nesting_within_the_limit_is_accepted() {
    // The limit is the compiler's, not the language's; ordinary depth is
    // nowhere near it.
    let count = 40;
    for text in [
        nested("(", "a", ")", count),
        nested("f(", "a", ")", count),
        nested("if a { ", "b", " }", count),
        format!("var x: {}", nested("list<", "i64", ">", count)),
    ] {
        let outcome = run(Entry::Statements, &text);
        assert_eq!(outcome.findings, [], "{text:.60}");
    }
}

#[test]
fn a_long_chain_is_accepted_and_an_endless_one_reported() {
    // `a + a + a ...` nests to the left: each operator adds a level to
    // the tree though the parser goes no deeper. So do suffixes.
    let sum = |count: usize| format!("a{}", " + a".repeat(count));
    let calls = |count: usize| format!("a{}", ".b()".repeat(count));

    for text in [sum(400), calls(200)] {
        let outcome = run(Entry::Statements, &text);
        assert_eq!(outcome.findings, [], "{text:.40}");
    }
    for text in [sum(50_000), calls(50_000)] {
        // As statements, which is what these are.
        let outcome = &survives(&text)[2];
        let codes: Vec<&str> = outcome
            .findings
            .iter()
            .map(|finding| finding.code.as_str())
            .collect();
        // One report, however long the chain.
        assert_eq!(codes, ["syntax.nesting_limit"], "{text:.40}");
    }
}
