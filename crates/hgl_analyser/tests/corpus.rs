//! The analyser over the HGL of the pinned specification and core.
//!
//! The analyser reads modules, declarations and imports so far, so this
//! checks that much of each example: its declarations do not clash and
//! its parts assemble. It says nothing yet of what is inside a body.

mod common;

use std::collections::BTreeMap;

use common::{check, example_modules, hgl_files};

/// The module path and part name on a sample's `module` line.
fn header(text: &str) -> (String, Option<String>) {
    let line = text.trim_start().lines().next().unwrap_or("");
    let mut words = line.split_whitespace();
    assert_eq!(words.next(), Some("module"), "{line}");
    let path = words.next().unwrap_or("").to_string();
    let part = match (words.next(), words.next()) {
        (Some("part"), Some(part)) => Some(part.to_string()),
        _ => None,
    };
    (path, part)
}

/// Every complete module among the specification's examples is accepted.
///
/// An example is analysed on its own, as a program of one file. Where a
/// document shows a module as several files, as the native chapter shows
/// an interface and the part that implements it, they are one program.
#[test]
fn every_example_module_is_accepted() {
    let samples = example_modules();
    // The samples of each document's each module, together.
    let mut groups: BTreeMap<(String, String), Vec<&common::Sample>> = BTreeMap::new();
    for sample in &samples {
        // `origin` is `document:line`.
        let document = sample.origin.rsplit_once(':').unwrap().0.to_string();
        let (path, _) = header(&sample.text);
        groups.entry((document, path)).or_default().push(sample);
    }

    let mut programs: Vec<Vec<&common::Sample>> = Vec::new();
    for group in groups.into_values() {
        let parts: Vec<Option<String>> =
            group.iter().map(|sample| header(&sample.text).1).collect();
        let named = parts.iter().flatten().count();
        // Files of one module: at most one unnamed, the rest parts.
        if group.len() > 1 && named + 1 >= group.len() {
            programs.push(group);
        } else {
            // Separate examples that happen to use one module name.
            programs.extend(group.into_iter().map(|sample| vec![sample]));
        }
    }

    let mut failures = Vec::new();
    let mut assembled = 0;
    for program in &programs {
        let files: Vec<(&str, &str)> = program
            .iter()
            .map(|sample| (sample.origin.as_str(), sample.text.as_str()))
            .collect();
        let checked = check(&files);
        if !checked.findings.is_empty() {
            failures.push(format!("{}:\n{}", files[0].0, checked.report));
        }
        assert_eq!(
            checked.program.modules.len(),
            1,
            "{} is one module",
            files[0].0
        );
        assembled += usize::from(files.len() > 1);
    }
    assert!(
        failures.is_empty(),
        "{} of {} example programs rejected:\n{}",
        failures.len(),
        programs.len(),
        failures.join("\n")
    );
    assert!(assembled > 0, "no example shows a module in several files");
}

/// UNIT-1, CONF-3: core's conformance cases are the parts of one module,
/// and assemble into it with no name declared twice.
#[test]
fn the_conformance_cases_assemble_into_one_module() {
    let samples = hgl_files("hgl_core/tests/conformance");
    let files: Vec<(&str, &str)> = samples
        .iter()
        .map(|sample| (sample.origin.as_str(), sample.text.as_str()))
        .collect();
    let checked = check(&files);
    assert!(checked.findings.is_empty(), "{}", checked.report);

    assert_eq!(checked.program.modules.len(), 1);
    let module = &checked.program.modules[0];
    assert_eq!(module.path, "conformance");
    assert_eq!(module.files.len(), samples.len());
    assert!(
        module.files.iter().all(|file| file.part.is_some()),
        "each case file is a named part"
    );
    assert!(!module.declarations.is_empty());
    assert!(!module.tests.is_empty());
}
