//! `docs/rules.md` against the pinned specification and this workspace.
//!
//! `docs/rules.md` lists the chapter files under `docs/rules/`, which
//! hold every rule of the pinned specification and core with the stage
//! that implements it. A list kept by hand drifts, so these tests read it
//! and check what can be checked mechanically:
//!
//! * every rule the pinned documents define has an entry, and no entry
//!   names a rule they do not define, so raising the pin shows which
//!   entries to add;
//! * every entry names a known stage and state, and no entry is repeated;
//! * an entry marked `done` or `partial` names a rule that its stage's
//!   code cites;
//! * the entries of one rule are together and a family's rules are in
//!   order.
//!
//! An entry is a stage's line `stage: state` under the rule's `## RULE`
//! heading. They do not check that a `done` or `partial` entry is true. The stage's
//! own tests do that; this file only stops the table claiming a rule
//! nothing mentions.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// The stages an entry may name, each with the crates that implement it.
///
/// A stage with no crate yet has an empty list. Marking one of its rows
/// `done` or `partial` fails until its crates are added here.
const STAGES: [(&str, &[&str]); 10] = [
    ("lexer", &["hgl_source", "hgl_lexer"]),
    ("parser", &["hgl_parser"]),
    ("formatter", &[]),
    ("analyser", &["hgl_analyser"]),
    ("optimiser", &[]),
    ("transpiler", &["hgl_lower", "hgl_cpp"]),
    ("runner", &["hgl_runner"]),
    ("driver", &["hgl_cli"]),
    ("diagnostics", &["hgl_diagnostics"]),
    ("docs", &["hgl_doc"]),
];

/// The states an entry may have.
const STATES: [&str; 4] = ["done", "partial", "planned", "unscheduled"];

/// One stage's entry for a rule in a chapter file of `docs/rules/`.
struct Row {
    /// The rule, for example `LEX-8`.
    rule: String,
    /// The stage that implements it.
    stage: String,
    /// `done`, `partial`, `planned` or `unscheduled`.
    state: String,
    /// Where the entry's `stage: state` line is, as `docs/rules/x.md
    /// line N`, for failure messages.
    at: String,
}

/// The workspace root.
fn root() -> PathBuf {
    // CARGO_MANIFEST_DIR is this crate's directory, set by Cargo when it
    // compiles the test. The workspace root is two levels up.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Reads a file under the workspace root as text.
fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| {
        panic!(
            "cannot read {}: {error}; are the `spec/` submodules checked out?",
            path.display()
        )
    })
}

/// Every file with the given extension directly inside `directory`.
fn files_in(directory: &Path, extension: &str) -> Vec<PathBuf> {
    let entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("cannot list {}: {error}", directory.display()));
    let mut files: Vec<PathBuf> = entries
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| path.extension().is_some_and(|found| found == extension))
        .collect();
    // read_dir promises no order; sorting keeps failure messages stable.
    files.sort();
    files
}

/// Every `.rs` file under `directory`, at any depth.
fn rust_files_under(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    // A list of directories still to visit stands in for recursion.
    let mut pending = vec![directory.to_path_buf()];
    while let Some(next) = pending.pop() {
        let entries = fs::read_dir(&next)
            .unwrap_or_else(|error| panic!("cannot list {}: {error}", next.display()));
        for entry in entries {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|found| found == "rs") {
                found.push(path);
            }
        }
    }
    found
}

/// Splits a rule ID into its family and number: `LEX-8` gives `("LEX", 8)`.
/// Text that is not a rule ID gives `None`.
fn rule_parts(text: &str) -> Option<(&str, u32)> {
    let (family, number) = text.split_once('-')?;
    let is_family = !family.is_empty() && family.bytes().all(|byte| byte.is_ascii_uppercase());
    let is_number = !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit());
    if is_family && is_number {
        Some((family, number.parse().ok()?))
    } else {
        None
    }
}

/// Every rule ID that appears anywhere in `text`, as in a comment that
/// says "LEX-8: digits then letters are a duration".
fn rules_mentioned(text: &str) -> BTreeSet<String> {
    // Cutting at every character that cannot be part of an ID leaves the
    // IDs, and many other words, as whole pieces.
    text.split(|character: char| !(character.is_ascii_alphanumeric() || character == '-'))
        .filter(|piece| rule_parts(piece).is_some())
        .map(str::to_owned)
        .collect()
}

/// The rules the pinned documents define. A definition is a list item that
/// opens with the ID in bold: `- **LEX-8.** Nested generic closers ...`.
fn rules_defined() -> BTreeSet<String> {
    let root = root();
    let mut documents = files_in(&root.join("spec/hgl_spec/spec/language"), "md");
    documents.extend(files_in(&root.join("spec/hgl_core/spec"), "md"));

    let mut defined = BTreeSet::new();
    for document in documents {
        for line in read(&document).lines() {
            let rule = line
                .strip_prefix("- **")
                .and_then(|rest| rest.split_once(".**"))
                .map(|(rule, _)| rule);
            if let Some(rule) = rule.filter(|rule| rule_parts(rule).is_some()) {
                defined.insert(rule.to_owned());
            }
        }
    }
    assert!(!defined.is_empty(), "no rule found in the pinned documents");
    defined
}

/// The entries of every chapter file, in the order `docs/rules.md`
/// lists the chapters: under each `## RULE` heading, the lines of the
/// form `stage: state`.
fn rows() -> Vec<Row> {
    let index = read(&root().join("docs/rules.md"));
    // The chapters are the index's links `- [Title](rules/name.md)`.
    let chapters: Vec<&str> = index
        .lines()
        .filter_map(|line| {
            line.strip_prefix("- [")?
                .split_once("](")?
                .1
                .strip_suffix(')')
        })
        .filter(|link| link.starts_with("rules/"))
        .collect();
    assert!(!chapters.is_empty(), "docs/rules.md lists no chapter");
    let mut rows = Vec::new();
    for chapter in chapters {
        let text = read(&root().join("docs").join(chapter));
        let mut rule: Option<&str> = None;
        for (index, line) in text.lines().enumerate() {
            if let Some(heading) = line.strip_prefix("## ") {
                rule = rule_parts(heading).map(|_| heading);
                continue;
            }
            // `stage: state`, two words of lowercase letters.
            let Some((stage, state)) = line.split_once(": ") else {
                continue;
            };
            let word =
                |text: &str| !text.is_empty() && text.bytes().all(|b| b.is_ascii_lowercase());
            if let Some(rule) = rule.filter(|_| word(stage) && word(state)) {
                rows.push(Row {
                    rule: rule.to_owned(),
                    stage: stage.to_owned(),
                    state: state.to_owned(),
                    at: format!("docs/{chapter} line {}", index + 1),
                });
            }
        }
    }
    assert!(!rows.is_empty(), "docs/rules/ has no rule entry");
    rows
}

#[test]
fn the_table_lists_exactly_the_rules_the_pinned_documents_define() {
    let defined = rules_defined();
    let listed: BTreeSet<String> = rows().into_iter().map(|row| row.rule).collect();

    // `difference` gives the members of one set missing from the other.
    let missing: Vec<&String> = defined.difference(&listed).collect();
    let unknown: Vec<&String> = listed.difference(&defined).collect();
    assert!(
        missing.is_empty() && unknown.is_empty(),
        "docs/rules/ and the pinned documents disagree.\n\
         Defined but not listed: {missing:?}\n\
         Listed but not defined: {unknown:?}"
    );
}

#[test]
fn every_row_names_a_known_stage_and_state_once() {
    let mut seen = BTreeSet::new();
    for row in rows() {
        let at = &row.at;
        assert!(
            STAGES.iter().any(|(stage, _)| *stage == row.stage),
            "{at}: unknown stage `{}`",
            row.stage
        );
        assert!(
            STATES.contains(&row.state.as_str()),
            "{at}: unknown state `{}`",
            row.state
        );
        // `insert` returns false when the set already held the value.
        assert!(
            seen.insert((row.rule.clone(), row.stage.clone())),
            "{at}: a second entry for {} in stage `{}`",
            row.rule,
            row.stage
        );
    }
}

#[test]
fn a_done_or_partial_row_names_a_rule_its_stage_cites() {
    let root = root();
    for (stage, crates) in STAGES {
        // Every rule ID mentioned in the stage's source and tests.
        let mut cited = BTreeSet::new();
        for name in crates {
            for file in rust_files_under(&root.join("crates").join(name)) {
                cited.extend(rules_mentioned(&read(&file)));
            }
        }
        for row in rows() {
            if row.stage == stage && (row.state == "done" || row.state == "partial") {
                assert!(
                    cited.contains(&row.rule),
                    "{}: {} is `{}` in stage `{stage}`, \
                     but no file under {crates:?} cites it",
                    row.at,
                    row.rule,
                    row.state
                );
            }
        }
    }
}

#[test]
fn the_rows_of_a_rule_are_together_and_a_family_is_in_order() {
    // Rules whose rows have ended: seeing one again means its rows are
    // split.
    let mut finished: BTreeSet<String> = BTreeSet::new();
    let mut previous: Option<Row> = None;
    for row in rows() {
        // `if let ... && ...` binds `before` and tests it in one condition.
        if let Some(before) = &previous
            && before.rule != row.rule
        {
            finished.insert(before.rule.clone());
            assert!(
                !finished.contains(&row.rule),
                "{}: the entries of {} are not together",
                row.at,
                row.rule
            );
            let (family_before, number_before) =
                rule_parts(&before.rule).expect("an entry holds a rule ID");
            let (family, number) = rule_parts(&row.rule).expect("an entry holds a rule ID");
            assert!(
                family_before != family || number_before < number,
                "{}: {} follows {}",
                row.at,
                row.rule,
                before.rule
            );
        }
        previous = Some(row);
    }
}
