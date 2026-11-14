//! Every piece of HGL in the pinned specification parses cleanly.
//!
//! The corpus is each fenced `hgl` block in the Markdown under `spec/` and
//! each `.hgl` file there. A block is a whole file when it starts with a
//! `module` line, and otherwise a run of declarations or of statements;
//! each is parsed as what it is. For every item the test checks that
//!
//! * lexing and parsing report nothing;
//! * the tree's text is the source text;
//! * the tree holds no `Error` node.
//!
//! A second test rewrites every item with Windows (CRLF) and classic Mac
//! (CR) line breaks and checks that the tree keeps its shape.
//!
//! This is a smoke test of the parser against real HGL, not conformance:
//! it says the specification's own examples fit the grammar as this parser
//! reads it, not that each tree is the right one. `rules.rs` checks trees.

mod common;

use common::{corpus, entry_for, hgl_blocks, run};
use hgl_parser::Entry;

/// Samples the grammar rejects as the specification now stands, each with
/// the one diagnostic expected. Listed here, they are checked to fail in
/// just that way, so the list cannot outlive its reason.
///
/// Each is reported to the specification's issue tracker, not settled
/// here. An entry is the path of the sample's file under `spec/` and the
/// diagnostic's message. There is none at the pinned revision.
const CONTRADICTIONS: &[(&str, &str)] = &[];

#[test]
fn the_specification_corpus_parses_cleanly() {
    let samples = corpus();
    let mut failures = Vec::new();
    // How many samples were parsed as a file, as declarations and as
    // statements.
    let mut counts = [0usize; 3];
    // Which entries of `CONTRADICTIONS` were met.
    let mut met = vec![false; CONTRADICTIONS.len()];
    for sample in &samples {
        let entry = entry_for(&sample.text);
        counts[entry as usize] += 1;
        let outcome = run(entry, &sample.text);
        if outcome.findings.is_empty() && !outcome.has_error_node() {
            continue;
        }
        let known = CONTRADICTIONS.iter().position(|(file, message)| {
            sample.origin.starts_with(file)
                && outcome.findings.len() == 1
                && outcome.findings[0].message == *message
        });
        match known {
            Some(index) => met[index] = true,
            None => failures.push(format!(
                "{} ({entry:?}):\n{}",
                sample.origin, outcome.report
            )),
        }
    }
    // Shown by `cargo test -- --nocapture`.
    println!(
        "parsed {} samples: {} files, {} runs of declarations, {} runs of statements; \
         {} known contradictions",
        samples.len(),
        counts[0],
        counts[1],
        counts[2],
        CONTRADICTIONS.len()
    );
    // Each way in is exercised; a change to the corpus that removes one
    // should be noticed.
    assert!(counts.iter().all(|count| *count > 0), "{counts:?}");
    assert!(
        failures.is_empty(),
        "{} of {} samples did not parse cleanly:\n{}",
        failures.len(),
        samples.len(),
        failures.join("\n")
    );
    assert!(
        met.iter().all(|met| *met),
        "an entry of CONTRADICTIONS no longer applies: {met:?}"
    );
}

#[test]
fn the_corpus_parses_alike_under_every_line_break_form() {
    // LEX-15: LF, CRLF and CR are each one line break and mean the same.
    // LEX-5: so they separate statements alike.
    //
    // The outline shows a token's text in its debug form, where a line
    // break is the two characters `\` and `n`. CRLF first, so that its
    // carriage return is not rewritten twice.
    let normalised = |outline: String| outline.replace("\\r\\n", "\\n").replace("\\r", "\\n");
    for sample in &corpus() {
        let entry = entry_for(&sample.text);
        let expected = run(entry, &sample.text);
        for (name, line_break) in [("CRLF", "\r\n"), ("CR", "\r")] {
            let rewritten = sample.text.replace('\n', line_break);
            assert_eq!(entry_for(&rewritten), entry);
            let outcome = run(entry, &rewritten);
            assert_eq!(
                normalised(outcome.outline()),
                normalised(expected.outline()),
                "{} with {name} line breaks",
                sample.origin
            );
            // Same diagnostics on the same lines; columns too, since a
            // line break is never inside a line.
            assert_eq!(
                outcome.findings, expected.findings,
                "{} with {name} line breaks",
                sample.origin
            );
        }
    }
}

#[test]
fn the_three_entries_are_told_apart() {
    assert_eq!(entry_for("module a\n"), Entry::SourceFile);
    assert_eq!(
        entry_for("# note\nconst fn f() => 1\n"),
        Entry::Declarations
    );
    assert_eq!(entry_for("native type Handle\n"), Entry::Declarations);
    // `const` alone is a name (LEX-9).
    assert_eq!(entry_for("const = 1\n"), Entry::Statements);
    assert_eq!(entry_for("1.0 / 0.0\n"), Entry::Statements);
}

#[test]
fn block_extraction_finds_only_hgl_fences() {
    let markdown = "\
intro
```hgl
let a = 1
```
```ebnf
a = b ;
```
";
    assert_eq!(hgl_blocks(markdown), [(3, "let a = 1\n".to_string())]);
}
