//! Every piece of HGL in the pinned specification parses cleanly.
//!
//! The corpus is each fenced `hgl` block in the Markdown under `spec/` and
//! each `.hgl` file there. A block is a whole file when it starts with a
//! `module` line, and otherwise a run of declarations or of statements;
//! each is parsed as what it is. For every item the test checks that
//!
//! * lexing and parsing report nothing, except for named pending samples;
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

/// A sample that awaits specific grammar work or a documentation correction.
struct Pending {
    /// The document's path under `spec/`.
    file: &'static str,
    /// The sample's opening line, stable when the document's line numbers move.
    opening: &'static str,
    /// The missing rules and the issues that will remove this entry.
    reason: &'static str,
}

const FUNCTIONS: &str = "hgl_spec/spec/language/functions-and-phases.md";
const GENERICS: &str = "hgl_spec/spec/language/generics-and-operators.md";
const MAPPING: &str = "hgl_core/spec/mapping.md";

const PENDING: &[Pending] = &[
    Pending {
        file: FUNCTIONS,
        opening: "module examples.configuration_parameters",
        reason: "FN-18, EXPR-10 (#363)",
    },
    Pending {
        file: FUNCTIONS,
        opening: "module examples.pack_forms",
        reason: "LEX-29, EXPR-16, EXPR-17, FN-19 (#368)",
    },
    Pending {
        file: FUNCTIONS,
        opening: "module examples.function_parameters",
        reason: "GEN-17 (#366)",
    },
    Pending {
        file: GENERICS,
        opening: "module examples.rooms",
        reason: "TYPE-3: comma between struct fields (hgl_spec#196)",
    },
    Pending {
        file: GENERICS,
        opening: "operator call_through<R, ...Ps, ...As>(f: fn(...{Ps}) -> R, args: ...{As for f}) -> R",
        reason: "GEN-17 and FN-8 (#366)",
    },
    Pending {
        file: GENERICS,
        opening: "operator keyed<K, R, ...Ps, ...As>(",
        reason: "GEN-17 and FN-8 (#366), GEN-10 and GEN-18 (#367)",
    },
    Pending {
        file: GENERICS,
        opening: r#"operator each_wanted<K, R, ...Ps, ...As, const key: str = "key">{"#,
        reason: "GEN-17 and FN-8 (#366), FN-18 and EXPR-10 (#363)",
    },
    Pending {
        file: GENERICS,
        opening: r#"operator each_slot<R, ...Ps, ...As, const key: str = "key", const index: str = "index">("#,
        reason: "GEN-17 and FN-8 (#366)",
    },
    Pending {
        file: MAPPING,
        opening: "export enum Join { inner, outer, left }",
        reason: "GEN-17 and FN-8 (#366), FN-18 (#363), GEN-10 and GEN-18 (#367), EXPR-16, EXPR-17 and FN-19 (#368)",
    },
    Pending {
        file: MAPPING,
        opening: "module examples.mapping",
        reason: "EXPR-10 (#363)",
    },
];

#[test]
fn the_specification_corpus_parses_cleanly() {
    let samples = corpus();
    let mut failures = Vec::new();
    // How many samples were parsed as a file, as declarations and as
    // statements.
    let mut counts = [0usize; 3];
    let mut met = vec![0; PENDING.len()];
    for sample in &samples {
        let entry = entry_for(&sample.text);
        counts[entry as usize] += 1;
        let outcome = run(entry, &sample.text);
        let accepted = outcome.findings.is_empty() && !outcome.has_error_node();
        let path = sample
            .origin
            .rsplit_once(':')
            .map_or(sample.origin.as_str(), |(path, _)| path);
        let opening = sample.text.trim_start().lines().next().unwrap_or("");
        if let Some((index, pending)) = PENDING
            .iter()
            .enumerate()
            .find(|(_, item)| item.file == path && item.opening == opening)
        {
            met[index] += 1;
            assert!(
                !accepted,
                "{} now parses: remove its pending entry ({})",
                sample.origin, pending.reason
            );
        } else if !accepted {
            failures.push(format!(
                "{} ({entry:?}):\n{}",
                sample.origin, outcome.report
            ));
        }
    }
    // Shown by `cargo test -- --nocapture`.
    println!(
        "parsed {} samples: {} files, {} runs of declarations, {} runs of statements; \
         {} pending samples",
        samples.len(),
        counts[0],
        counts[1],
        counts[2],
        PENDING.len()
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
    for (pending, count) in PENDING.iter().zip(met) {
        assert_eq!(
            count, 1,
            "pending sample {}: {} ({})",
            pending.file, pending.opening, pending.reason
        );
    }
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
