//! `hgl_diagnostics::catalogue` against the pinned specification.
//!
//! The catalogue is written out in the crate. These tests read the
//! chapter it was copied from, so that a pin which adds, removes or moves
//! a row fails here and shows which.

use std::fs;
use std::path::Path;

use hgl_diagnostics::{Category, catalogue};

/// The pinned `errors-and-diagnostics.md`.
fn chapter() -> String {
    // CARGO_MANIFEST_DIR is this crate's directory, set by Cargo when it
    // compiles the test. The workspace root is two levels up.
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/hgl_spec/spec/language/errors-and-diagnostics.md");
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "cannot read {}: {error}; is the `spec/hgl_spec` submodule checked out?",
            path.display()
        )
    })
}

/// The first two cells of each table row of `document` whose second cell
/// is a code in backquotes: `| parse | `string.unclosed` | ... |` gives
/// `("parse", "string.unclosed")`.
fn rows(document: &str) -> Vec<(String, String)> {
    document
        .lines()
        .filter_map(|line| {
            let mut cells = line.strip_prefix('|')?.split('|').map(str::trim);
            let first = cells.next()?;
            let second = cells.next()?;
            let code = second.strip_prefix('`')?.strip_suffix('`')?;
            Some((first.to_string(), code.to_string()))
        })
        .collect()
}

/// ERR-1 and the catalogue: "Source categories: `parse`, `name`, ..."
#[test]
fn the_categories_are_the_chapters_list() {
    let chapter = chapter();
    let sentence = chapter
        .lines()
        .skip_while(|line| !line.starts_with("Source categories:"))
        .take_while(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    // The sentence ends at its first full stop; `build` is in the next.
    let list = sentence.split('.').next().unwrap();
    let listed: Vec<&str> = list
        .split('`')
        // The names are the odd pieces: between a pair of backquotes.
        .skip(1)
        .step_by(2)
        .collect();
    let known: Vec<&str> = Category::ALL
        .iter()
        .map(|category| category.as_str())
        .collect();
    assert_eq!(known, listed);
    for category in Category::ALL {
        assert_eq!(Category::from_name(category.as_str()), Some(category));
    }
    assert_eq!(
        Category::from_name("build"),
        None,
        "`build` is infrastructure"
    );
    assert_eq!(Category::from_name("Parse"), None);
}

/// TST-12: "Category/code must be catalogued." The crate's list is the
/// chapter's source rows, in order.
#[test]
fn the_source_codes_are_the_chapters_rows() {
    let chapter = chapter();
    let source_rows: Vec<(String, String)> = rows(&chapter)
        .into_iter()
        .filter(|(category, _)| Category::from_name(category).is_some())
        .collect();
    assert!(!source_rows.is_empty());
    let known: Vec<(String, String)> = catalogue::SOURCE_CODES
        .iter()
        .map(|(category, code)| (category.as_str().to_string(), code.to_string()))
        .collect();
    assert_eq!(known, source_rows);
}

/// "The source and execution catalogs are disjoint."
#[test]
fn no_execution_code_is_a_source_code() {
    let chapter = chapter();
    let execution: Vec<String> = chapter
        .lines()
        .skip_while(|line| !line.starts_with("| Execution code"))
        .take_while(|line| line.starts_with('|'))
        .filter_map(|line| {
            let cell = line.strip_prefix('|')?.split('|').next()?.trim();
            Some(cell.strip_prefix('`')?.strip_suffix('`')?.to_string())
        })
        .collect();
    assert!(!execution.is_empty());
    for code in execution {
        assert_eq!(catalogue::category_of(&code), None, "{code}");
    }
}
