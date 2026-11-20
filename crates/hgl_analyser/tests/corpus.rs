//! The analyser over the HGL of the pinned specification and core.
//!
//! The analyser reads modules, declarations, imports, names and written
//! types so far, so this checks that much of each example: its
//! declarations do not clash, its parts assemble, and its types are
//! formed as the rules require. It says nothing yet of what an
//! expression is.

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

/// The rejections a sample says it has (TST-12): for each
/// `# expect-error(category, "code")` line, the number of the line after
/// it, counted from 1, and `category/code`.
fn expected_rejections(text: &str) -> Vec<(usize, String)> {
    let mut expected = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let Some(inside) = line
            .trim()
            .strip_prefix("# expect-error(")
            .and_then(|rest| rest.strip_suffix(')'))
        else {
            continue;
        };
        let (category, code) = inside.split_once(',').expect("a category, then a code");
        // The annotation is line `index + 1`; it applies to the next.
        expected.push((
            index + 2,
            format!("{}/{}", category.trim(), code.trim().trim_matches('"')),
        ));
    }
    expected
}

/// The examples whose rejection the specification states in its prose and
/// not with an `expect-error` annotation, each by its module's path, with
/// each finding's line within the example and its `category/code`. TST-3:
/// "Deliberate rejection, complete module, value code in a test ... In
/// `t`, outside an `eval`, `pass(1)` is a call of a graph in value code,
/// and rejects ... Proposed: both reject with `call.role`."
const STATED_IN_PROSE: &[(&str, &[(usize, &str)])] = &[(
    "examples.test_value_code",
    &[(8, "function-kind/call.role")],
)];

/// The rejection cases of core the compiler does not yet reject, each as
/// `(file, declaration)`, the file under `tests/conformance/`: the
/// `[rejections]` of `crates/hgl_hgraph/ci/unsupported_cases.toml`, which
/// `check_core.py` reads too, so that the two agree. Each is listed under a
/// reason that names the rule not yet built.
fn unrejected() -> Vec<(String, String)> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../hgl_hgraph/ci/unsupported_cases.toml");
    let text = std::fs::read_to_string(&path).expect("the list of unsupported cases");
    text.lines()
        // The table `[rejections]`, to the end of the file or the next.
        .skip_while(|line| line.trim() != "[rejections]")
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
        .filter_map(|line| {
            let name = line.trim().strip_prefix('"')?.strip_suffix("\",")?;
            let rest = name.strip_prefix("conformance::")?;
            let (file, declaration) = rest.rsplit_once("::")?;
            Some((file.to_string(), declaration.to_string()))
        })
        .collect()
}

/// The lines of the declaration `name` of `text`, from the line that
/// declares it, counted from 1, to the line before the next declaration.
fn declaration_lines(text: &str, name: &str) -> std::ops::Range<usize> {
    let lines: Vec<&str> = text.lines().collect();
    let starts = |line: &str| {
        ["fn ", "const fn ", "struct ", "enum ", "native "]
            .iter()
            .any(|word| line.starts_with(word))
    };
    let first = lines
        .iter()
        .position(|line| starts(line) && line.contains(&format!(" {name}")))
        .unwrap_or_else(|| panic!("no declaration `{name}`"));
    let end = lines[first + 1..]
        .iter()
        .position(|line| starts(line) || line.starts_with("/**"))
        .map_or(lines.len(), |after| first + 1 + after);
    first + 1..end + 1
}

/// Every complete module among the specification's examples is accepted,
/// but for what the example itself says is rejected: the analyser
/// reports each `expect-error` annotation's code on the line it applies
/// to, and nothing else.
///
/// An example is analysed on its own, as a program of one file. Where a
/// document shows a module as several files, as the native chapter shows
/// an interface and the part that implements it, they are one program.
#[test]
fn every_example_module_is_accepted_but_for_its_stated_rejections() {
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
    let mut rejections = 0;
    for program in &programs {
        let files: Vec<(&str, &str)> = program
            .iter()
            .map(|sample| (sample.origin.as_str(), sample.text.as_str()))
            .collect();
        let checked = check(&files);
        // What each file says is rejected, as `file:line category/code`.
        let mut expected: Vec<String> = files
            .iter()
            .flat_map(|(name, text)| {
                let (path, _) = header(text);
                let stated = STATED_IN_PROSE
                    .iter()
                    .filter(move |(module, _)| *module == path)
                    .flat_map(|(_, findings)| findings.iter())
                    .map(|&(line, code)| (line, code.to_string()));
                expected_rejections(text)
                    .into_iter()
                    .chain(stated)
                    .map(move |(line, code)| format!("{name}:{line} {code}"))
            })
            .collect();
        // A finding is at `file:line:column`; the column is dropped.
        let mut found: Vec<String> = checked
            .findings
            .iter()
            .map(|finding| {
                let (line, _column) = finding.at.rsplit_once(':').unwrap();
                format!("{line} {}", finding.code)
            })
            .collect();
        expected.sort();
        found.sort();
        rejections += expected.len();
        if found != expected {
            failures.push(format!(
                "{}: expected {expected:?}, found {found:?}\n{}",
                files[0].0, checked.report
            ));
        }
        assert_eq!(checked.modules().len(), 1, "{} is one module", files[0].0);
        assembled += usize::from(files.len() > 1);
    }
    assert!(
        failures.is_empty(),
        "{} of {} example programs are not as they say:\n{}",
        failures.len(),
        programs.len(),
        failures.join("\n")
    );
    assert!(assembled > 0, "no example shows a module in several files");
    // TYPE-11's `rolling_window.size_kind` illustration, for one.
    assert!(rejections > 0, "no example states a rejection");
}

/// UNIT-1, CONF-3: core's conformance cases are the parts of one module,
/// and assemble into it with no name declared twice. Its source
/// rejections are a module of their own, `conformance.rejections`.
#[test]
fn the_conformance_cases_assemble_into_one_module() {
    let samples: Vec<common::Sample> = hgl_files("hgl_core/tests/conformance")
        .into_iter()
        .filter(|sample| !sample.origin.contains("/rejections/"))
        .collect();
    let files: Vec<(&str, &str)> = samples
        .iter()
        .map(|sample| (sample.origin.as_str(), sample.text.as_str()))
        .collect();
    let checked = check(&files);
    assert!(checked.findings.is_empty(), "{}", checked.report);

    assert_eq!(checked.modules().len(), 1);
    let module = checked.modules()[0];
    assert_eq!(module.path, "conformance");
    assert_eq!(module.files.len(), samples.len());
    assert!(
        module.files.iter().all(|file| file.part.is_some()),
        "each case file is a named part"
    );
    assert!(!module.declarations.is_empty());
    assert!(!module.tests.is_empty());
}

/// TST-12, CONF-3: core's source rejections, `conformance.rejections`,
/// are each rejected with the code their `# expect-error` annotation
/// names, on the line it applies to, and with nothing else.
#[test]
fn the_conformance_rejections_are_rejected_as_annotated() {
    let samples: Vec<common::Sample> = hgl_files("hgl_core/tests/conformance")
        .into_iter()
        .filter(|sample| sample.origin.contains("/rejections/"))
        .collect();
    assert!(!samples.is_empty(), "core states source rejections");
    let files: Vec<(&str, &str)> = samples
        .iter()
        .map(|sample| (sample.origin.as_str(), sample.text.as_str()))
        .collect();
    let checked = check(&files);
    // Each listed case the compiler does not yet reject, as the lines its
    // expectations stand on: none is reported, and none is expected.
    let listed: Vec<(String, std::ops::Range<usize>)> = unrejected()
        .into_iter()
        .map(|(file, declaration)| {
            let (name, text) = files
                .iter()
                .find(|(name, _)| name.ends_with(&format!("conformance/{file}")))
                .unwrap_or_else(|| panic!("no rejection file {file}"));
            (name.to_string(), declaration_lines(text, &declaration))
        })
        .collect();
    let is_listed = |name: &str, line: usize| {
        listed
            .iter()
            .any(|(file, lines)| file == name && lines.contains(&line))
    };
    let mut expected: Vec<String> = files
        .iter()
        .flat_map(|(name, text)| {
            expected_rejections(text)
                .into_iter()
                .filter(|(line, _)| !is_listed(name, *line))
                .map(move |(line, code)| format!("{name}:{line} {code}"))
        })
        .collect();
    let mut found: Vec<String> = checked
        .findings
        .iter()
        .map(|finding| {
            let (line, _column) = finding.at.rsplit_once(':').unwrap();
            format!("{line} {}", finding.code)
        })
        .collect();
    expected.sort();
    found.sort();
    assert!(!expected.is_empty());
    assert_eq!(found, expected, "{}", checked.report);
    assert_eq!(checked.modules()[0].path, "conformance.rejections");
}
