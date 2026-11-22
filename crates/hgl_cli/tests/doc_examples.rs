//! SDOC-6 and SDOC-9 through the documentation, checking and testing commands.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use common::{argument, fixture, hgl, scratch, text, write};
use serde_json::{Value, json};

#[test]
fn the_runner_lists_and_selects_examples_and_ordinary_tests() {
    let file = fixture("doc_examples.hgl");
    let checked = hgl(&["check", &file]);
    assert!(checked.status.success(), "{}", text(&checked.stderr));

    let listed = hgl(&["test", &file]);
    assert_eq!(listed.status.code(), Some(2), "{}", text(&listed.stderr));
    let listing = text(&listed.stdout);
    assert!(!listing.contains("core::"), "{listing}");
    for name in [
        "ordinary",
        "[module]#example-1",
        "twice#example-1",
        "twice#example-3",
        "choose#example-1",
        "choose~2#example-1",
        "Box#example-1",
        "Box.value#example-1",
        "documented#example-1",
    ] {
        assert!(
            listing.contains(&format!(
                "not run  executable  demo.documentation_examples::{name}\n"
            )),
            "{name}: {listing}"
        );
    }
    assert!(!listing.contains("twice#example-2"), "{listing}");
    let selected = hgl(&[
        "test",
        &file,
        "--test",
        "demo.documentation_examples::twice#example-3",
    ]);
    assert_eq!(
        selected.status.code(),
        Some(2),
        "{}",
        text(&selected.stderr)
    );
    let listing = text(&selected.stdout);
    assert_eq!(
        listing.matches("not run  executable").count(),
        1,
        "{listing}"
    );
    assert!(listing.contains("twice#example-3"), "{listing}");
}

/// TST-14 and SDOC-9: Core is under test when its source is given.
#[test]
fn explicitly_supplied_core_examples_are_checked_and_listed() {
    let root = scratch("explicit-core-examples");
    let core_source =
        "/** Core.\n\nExamples:\n    ```hgl\n    assert 1\n    ```\n*/\nmodule core\n";
    let core = write(&root, "core.hgl", core_source);
    let output = hgl(&["check", &core]);
    assert_eq!(output.status.code(), Some(1));
    let diagnostics = text(&output.stderr);
    assert!(diagnostics.contains("type.mismatch"), "{diagnostics}");
    assert!(diagnostics.contains(&format!("{core}:5:")), "{diagnostics}");

    fs::write(&core, core_source.replace("assert 1", "assert true")).unwrap();
    let output = hgl(&["test", &core]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stderr));
    assert!(
        text(&output.stdout).contains("not run  executable  core::[module]#example-1\n"),
        "{}",
        text(&output.stdout)
    );
}

#[test]
fn example_diagnostics_point_into_the_comment() {
    let root = scratch("diagnostics");
    for (name, example, target, diagnostic) in [
        ("bad_type", "assert 1", "assert 1", "type.mismatch"),
        (
            "name_taken",
            "const fn taken() -> i64 => 1\n\n    assert true",
            "const fn taken",
            "name.duplicate",
        ),
        (
            "late_helper",
            "assert true\n    const fn late() -> bool => true",
            "const fn late",
            "a test helper must precede the test's statements",
        ),
    ] {
        let source = format!(
            "module demo\n\nconst fn taken() -> i64 => 0\n\n/** Example.\n\nExamples:\n    ~~~hgl\n    {example}\n    ~~~\n*/\nconst fn target() -> bool => true\n"
        );
        let line = source
            .lines()
            .enumerate()
            .filter_map(|(index, line)| line.contains(target).then_some(index + 1))
            .last()
            .unwrap();
        let file = write(&root, &format!("{name}.hgl"), &source);
        let output = hgl(&["check", &file]);
        assert_eq!(output.status.code(), Some(1), "{}", text(&output.stdout));
        let diagnostics = text(&output.stderr);
        assert!(diagnostics.contains(diagnostic), "{diagnostics}");
        assert!(
            diagnostics.contains(&format!("{file}:{line}:")),
            "{diagnostics}"
        );
    }
}

#[test]
fn the_model_and_site_mark_the_status_on_the_documented_declaration() {
    let input = fixture("doc_examples.hgl");
    let json_output = hgl(&["doc", &input, "--format", "json", "--private"]);
    assert!(
        json_output.status.success(),
        "{}",
        text(&json_output.stderr)
    );
    let model: Value = serde_json::from_slice(&json_output.stdout).unwrap();
    let modules = model["modules"].as_array().unwrap();
    let module = modules
        .iter()
        .find(|module| module["name"] == "demo.documentation_examples")
        .unwrap();
    let declarations = module["declarations"].as_array().unwrap();
    let twice = declarations
        .iter()
        .find(|declaration| declaration["name"] == "twice")
        .unwrap();
    let examples = twice["documentation"]["examples"].as_array().unwrap();
    let statuses: Vec<_> = examples
        .iter()
        .map(|example| json!([example["position"], example["runnable"]]))
        .collect();
    assert_eq!(
        statuses,
        [
            json!([null, false]),
            json!([1, true]),
            json!([2, false]),
            json!([3, true]),
            json!([null, false]),
        ]
    );
    assert!(
        examples[1]["code"]
            .as_str()
            .unwrap()
            .contains("assert private_twice(3)")
    );
    assert!(
        examples[4]["code"]
            .as_str()
            .unwrap()
            .contains("assert shown_entry_only")
    );
    assert!(!declarations.iter().any(|declaration| {
        let name = declaration["name"].as_str().unwrap();
        name == "local_node" || name.contains("#example-")
    }));

    let site = scratch("site");
    let output = hgl(&["doc", &input, "--out", &argument(&site)]);
    assert!(output.status.success(), "{}", text(&output.stderr));
    let files = generated_files(&site);
    let page =
        std::str::from_utf8(&files[Path::new("modules/mod-demo.documentation_examples.html")])
            .unwrap();
    assert_eq!(page.matches("data-example=\"shown\"").count(), 3, "{page}");
    assert_eq!(
        page.matches("data-example=\"runnable\"").count(),
        8,
        "{page}"
    );
}

#[test]
fn an_application_emits_the_same_files_with_or_without_examples() {
    let root = scratch("application");
    let example = "/** Example.\n\nExamples:\n    ```hgl\n    assert 1\n    ```\n*/\n";
    let setup = "module demo.given\n\n";
    let definition = "export fn echo(const value: i64) -> i64 {\n    yield 1s: value\n}\n";
    let library = write(&root, "demo.hgl", &format!("{setup}{example}{definition}"));
    let application = write(
        &root,
        "main.hgl",
        "application demo.app\nuse demo.given::{echo}\neval echo(value: 2)\n",
    );
    let before = root.join("before");
    let after = root.join("after");
    let emit = |out: &Path| {
        let output = hgl(&["emit", &library, &application, "--out", &argument(out)]);
        assert!(output.status.success(), "{}", text(&output.stderr));
    };
    emit(&before);
    // Keep source line information identical as well as the application code.
    let blank = "\n".repeat(example.lines().count());
    fs::write(&library, format!("{setup}{blank}{definition}")).unwrap();
    emit(&after);
    assert_eq!(generated_files(&before), generated_files(&after));
}

fn generated_files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn collect(root: &Path, directory: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    fs::read(&path).unwrap(),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    collect(root, root, &mut files);
    files
}
