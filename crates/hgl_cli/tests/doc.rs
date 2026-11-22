//! Exercises the JSON documentation stage through the command that publishes
//! its model, using core and the declaration forms of SDOC-1.

mod common;

use std::fs;

use common::{argument, fixture, hgl, scratch, text, write};
use serde_json::{Value, json};

fn module<'a>(model: &'a Value, name: &str) -> &'a Value {
    model["modules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|module| module["name"] == name)
        .unwrap()
}

fn declaration<'a>(module: &'a Value, name: &str) -> &'a Value {
    module["declarations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|declaration| declaration["name"] == name)
        .unwrap()
}

fn document(arguments: &[&str]) -> Vec<u8> {
    let mut argv = vec!["doc", "--format", "json"];
    argv.extend(arguments);
    let output = hgl(&argv);
    assert!(output.status.success(), "{}", text(&output.stderr));
    assert!(output.stderr.is_empty(), "{}", text(&output.stderr));
    output.stdout
}

/// SDOC-1, SDOC-2, SDOC-3 and SDOC-10. The module, functions, operator, struct,
/// field, enum, member, native type and named test keep their own comments.
#[test]
fn declaration_fixture_preserves_interfaces_and_comment_structure() {
    let model: Value = serde_json::from_slice(&document(&[&fixture("doc")])).unwrap();
    assert_eq!(model["schemaVersion"], 1);
    let api = module(&model, "examples.apidoc");
    assert_eq!(
        api["parts"][0]["documentation"]["summary"],
        "The application programming interface."
    );
    assert_eq!(api["parts"][1]["name"], "shifts");
    assert_eq!(api["parts"][1]["source"]["file"], "parts/shift.hgl");
    assert_eq!(
        api["parts"][1]["documentation"]["summary"],
        "A part that contains candidates."
    );
    assert_eq!(api["declarations"].as_array().unwrap().len(), 8);
    let value = declaration(api, "identity");
    assert_eq!(value["canonicalName"], "examples.apidoc::identity");
    assert_eq!(value["role"], "value");
    assert_eq!(value["genericParameters"][0]["kind"], "type");
    assert_eq!(value["genericParameters"][1]["kind"], "value");
    assert_eq!(value["genericParameters"][1]["type"], "i64");
    assert_eq!(value["genericParameters"][1]["default"], "3");
    assert_eq!(value["parameters"][1]["default"], "2");
    assert_eq!(value["result"]["type"], "T");
    assert_eq!(value["requires"], "requires N == 3");
    assert_eq!(
        value["source"],
        json!({
            "module": "examples.apidoc",
            "part": null,
            "file": "api.hgl",
            "line": 57,
            "column": 17
        })
    );
    let comment = &value["documentation"];
    assert_eq!(comment["summary"], "First public callable.");
    assert_eq!(
        comment["sections"],
        json!([
            {
                "heading": "Args:",
                "recognized": true,
                "text": "input: A café value with  two spaces.\n    A continuation that keeps its indent.\ncount:",
                "entries": [
                    {"key": "input", "text": "A café value with  two spaces.\n    A continuation that keeps its indent."},
                    {"key": "count", "text": ""}
                ]
            },
            {
                "heading": "Type Args:",
                "recognized": true,
                "text": "T: Its type.\nN: Its maximum.",
                "entries": [{"key": "T", "text": "Its type."}, {"key": "N", "text": "Its maximum."}]
            },
            {"heading": "Unknown:", "recognized": false, "text": "A heading that stays prose.", "entries": []},
            {"heading": "Returns:", "recognized": true, "text": "The same value, `input`.", "entries": []},
            {"heading": "Requires:", "recognized": true, "text": "N: The constraint is prose until key spelling is specified.", "entries": []},
            {"heading": "Notes:", "recognized": true, "text": "|x|     |y|\n| ----- | - |\n| café  | \\\\ |", "entries": []},
            {"heading": "Examples:", "recognized": true, "text": "~~~hgl\nassert false\n~~~", "entries": []}
        ])
    );
    let temporal = &api["declarations"][5];
    assert_eq!(temporal["canonicalName"], value["canonicalName"]);
    assert_eq!(temporal["role"], "temporal");
    assert_eq!(
        temporal["documentation"]["summary"],
        "The temporal callable with the same canonical name."
    );
    let structure = declaration(api, "Box");
    assert_eq!(structure["kind"], "struct");
    assert_eq!(structure["documentation"]["summary"], "A typed box.");
    assert_eq!(structure["members"][0]["kind"], "field");
    assert_eq!(structure["members"][0]["type"], "T");
    assert_eq!(
        structure["members"][0]["documentation"]["summary"],
        "The payload."
    );
    assert_eq!(structure["members"][1]["default"], "\"café\"");
    let side = declaration(api, "Side");
    assert_eq!(side["kind"], "enum");
    assert_eq!(side["documentation"]["summary"], "Named sides.");
    assert_eq!(side["members"][0]["kind"], "enumMember");
    assert_eq!(side["members"][0]["documentation"]["summary"], "A buyer.");
    assert_eq!(side["members"][1]["value"], "3");
    let handle = declaration(api, "Handle");
    assert_eq!(handle["kind"], "nativeType");
    assert_eq!(handle["documentation"]["summary"], "A native handle.");
    assert_eq!(handle["members"][0]["kind"], "nativeProperty");
    let native = declaration(api, "identify");
    assert_eq!(native["kind"], "nativeFunction");
    assert_eq!(native["documentation"]["summary"], "A host function.");
    assert_eq!(native["parameters"][0]["default"], "\"host\"");
    assert!(declaration(api, "meaning")["documentation"].is_null());

    let model: Value = serde_json::from_slice(&document(&[&fixture("doc"), "--private"])).unwrap();
    let api = module(&model, "examples.apidoc");
    assert_eq!(api["declarations"].as_array().unwrap().len(), 10);
    assert_eq!(declaration(api, "internal")["exported"], false);
    let test = declaration(api, "works");
    assert_eq!(test["kind"], "test");
    assert_eq!(test["documentation"]["summary"], "A named test.");
}

/// SDOC-2 and SDOC-3. The JSON layout isolates the first paragraph and
/// preserves author-supplied headings without making their content keyed.
#[test]
fn first_paragraph_and_authored_headings_keep_their_own_sections() {
    let root = scratch("documentation-sections");
    let source = write(
        &root,
        "sections.hgl",
        r#"module sections

/**
Rules:
    A check must pass.
Status:
    Covered.
*/
test works { assert true }

/** Keep the value.
    Args:
        value: Passed through unchanged.
    Options:
        fallback: Prose, even with a key-like name.
            Keep the relative indentation.

        A second paragraph about the option.
    Returns:
        The supplied value.
*/
export const fn retain(value: i64) -> i64 => value

/** A short lead
    still in the first paragraph.

    A second paragraph introduces this table:

    | Value | Meaning |
    | ----- | ------- |
    | One   | Start   |

    Notes:
        More detail.
*/
export const one: i64 = 1
"#,
    );
    let model: Value = serde_json::from_slice(&document(&[&source, "--private"])).unwrap();
    let sections = module(&model, "sections");
    let test = &declaration(sections, "works")["documentation"];
    assert_eq!(test["summary"], "");
    assert_eq!(
        test["sections"],
        json!([
            {"heading": "Rules:", "recognized": false, "text": "A check must pass.", "entries": []},
            {"heading": "Status:", "recognized": false, "text": "Covered.", "entries": []}
        ])
    );
    let function = &declaration(sections, "retain")["documentation"];
    assert_eq!(function["summary"], "Keep the value.");
    assert_eq!(
        function["sections"],
        json!([
            {
                "heading": "Args:",
                "recognized": true,
                "text": "value: Passed through unchanged.",
                "entries": [{"key": "value", "text": "Passed through unchanged."}]
            },
            {
                "heading": "Options:",
                "recognized": false,
                "text": "fallback: Prose, even with a key-like name.\n    Keep the relative indentation.\n\nA second paragraph about the option.",
                "entries": []
            },
            {"heading": "Returns:", "recognized": true, "text": "The supplied value.", "entries": []}
        ])
    );
    let table = &declaration(sections, "one")["documentation"];
    assert_eq!(
        table["summary"],
        "A short lead\nstill in the first paragraph."
    );
    assert_eq!(
        table["sections"],
        json!([
            {
                "heading": null,
                "recognized": false,
                "text": "A second paragraph introduces this table:\n\n| Value | Meaning |\n| ----- | ------- |\n| One   | Start   |",
                "entries": []
            },
            {"heading": "Notes:", "recognized": true, "text": "More detail.", "entries": []}
        ])
    );
}

/// SDOC-2, SDOC-3 and SDOC-7. Markdown fences preserve literal headings
/// and apparent keys while ordinary headings retain their meaning.
#[test]
fn fenced_markdown_preserves_literal_headings_and_keys() {
    let root = scratch("documentation-fences");
    let source = write(
        &root,
        "fences.hgl",
        r#"module fences

/** Shows comment syntax.
~~~text
Args:
    sample: Shown literally.
Options:
    A literal authored heading.
~~~

````text
Returns:
    This is literal too.
```
Status:
    A shorter fence did not end this block.
````

Args:
    ~~~text
    not_a_parameter: Also literal.
    ~~~
    value: The parameter.
Returns:
    The value.
*/
export const fn show(value: i64) -> i64 => value
"#,
    );
    let checked = hgl(&["check", &source]);
    assert!(checked.status.success(), "{}", text(&checked.stderr));

    let model: Value = serde_json::from_slice(&document(&[&source])).unwrap();
    let show = declaration(module(&model, "fences"), "show");
    let comment = &show["documentation"];
    let fences = concat!(
        "~~~text\nArgs:\n    sample: Shown literally.\n",
        "Options:\n    A literal authored heading.\n~~~\n\n",
        "````text\nReturns:\n    This is literal too.\n```\n",
        "Status:\n    A shorter fence did not end this block.\n````"
    );
    assert_eq!(comment["summary"], "Shows comment syntax.");
    assert_eq!(
        comment["text"],
        format!(
            "Shows comment syntax.\n{fences}\n\nArgs:\n    ~~~text\n    not_a_parameter: Also literal.\n    ~~~\n    value: The parameter.\nReturns:\n    The value."
        )
    );
    assert_eq!(
        comment["sections"],
        json!([
            {"heading": null, "recognized": false, "text": fences, "entries": []},
            {
                "heading": "Args:",
                "recognized": true,
                "text": "~~~text\nnot_a_parameter: Also literal.\n~~~\nvalue: The parameter.",
                "entries": [{"key": "value", "text": "The parameter."}]
            },
            {"heading": "Returns:", "recognized": true, "text": "The value.", "entries": []}
        ])
    );
}

/// SDOC-4. Candidates follow their operator, including native candidates
/// and a candidate implemented in another module. The interface and each
/// candidate retain their own text, signature and source part.
#[test]
fn operator_candidates_keep_their_own_documentation() {
    let model: Value = serde_json::from_slice(&document(&[&fixture("doc")])).unwrap();
    let api = module(&model, "examples.apidoc");
    let operator = declaration(api, "shifted");
    assert_eq!(operator["kind"], "operator");
    assert_eq!(operator["exported"], true);
    assert_eq!(
        operator["documentation"]["summary"],
        "A public operator's own documentation."
    );
    assert_eq!(operator["configurationParameters"][0]["default"], "2");
    assert_eq!(operator["configurationParameters"][0]["name"], "offset");
    assert_eq!(operator["parameters"][0]["name"], "input");
    let candidates = operator["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 3);
    assert_eq!(candidates[0]["canonicalName"], operator["canonicalName"]);
    assert_eq!(candidates[0]["kind"], "candidate");
    assert_eq!(candidates[0]["parameters"][0]["type"], "i64");
    assert_eq!(candidates[0]["result"]["type"], "i64");
    assert_eq!(
        candidates[0]["documentation"]["summary"],
        "An integer candidate's text."
    );
    assert_eq!(candidates[0]["source"]["part"], "shifts");
    assert_eq!(candidates[1]["kind"], "nativeCandidate");
    assert_eq!(candidates[1]["result"]["type"], "f64");
    assert_eq!(
        candidates[1]["documentation"]["summary"],
        "A native floating candidate's text."
    );
    assert_eq!(candidates[2]["result"]["type"], "duration");
    assert_eq!(candidates[2]["source"]["module"], "examples.other");
    assert_eq!(
        candidates[2]["documentation"]["summary"],
        "An externally declared duration candidate."
    );
    assert_eq!(module(&model, "examples.other")["declarations"], json!([]));
}

/// SDOC-2 and LEX-15. Identical logical source, with roots reordered and
/// written elsewhere using CR and CRLF, gives the exact same UTF-8 bytes.
/// `--out` writes those bytes without also printing them.
#[test]
fn roots_location_newlines_and_destination_preserve_model_bytes() {
    let original = fixture("doc");
    let files = [
        ("api.hgl", include_str!("fixtures/doc/api.hgl"), "\r\n"),
        (
            "foreign.hgl",
            include_str!("fixtures/doc/foreign.hgl"),
            "\r",
        ),
        (
            "parts/shift.hgl",
            include_str!("fixtures/doc/parts/shift.hgl"),
            "\n",
        ),
    ];
    let relocated = scratch("relocated");
    for (name, source, ending) in files {
        write(&relocated, name, &source.replace('\n', ending));
    }
    // Add the two copies of the nested directory as roots in opposite
    // orders. Each copy has the same pair of relative roots.
    let first = document(&[&original, &fixture("doc/parts"), "--private"]);
    let second = document(&[
        &argument(&relocated.join("parts")),
        &argument(&relocated),
        "--private",
    ]);
    assert_eq!(first, second);
    assert_eq!(first.last(), Some(&b'\n'));
    assert!(!first.contains(&b'\r'));
    assert!(!text(&first).contains(&original));

    // The source walker reads a file only once, even when a root also
    // names it explicitly with a different spelling.
    let alias = argument(&relocated.join("parts/../parts/shift.hgl"));
    assert_eq!(
        document(&[&alias, &argument(&relocated), "--private"]),
        document(&[&original, "--private"])
    );

    let out = scratch("output").join("new");
    let output = hgl(&[
        "doc",
        &argument(&relocated),
        "--format",
        "json",
        "--out",
        &argument(&out),
        "--private",
    ]);
    assert!(output.status.success(), "{}", text(&output.stderr));
    assert!(output.stdout.is_empty());
    assert_eq!(
        fs::read(out.join("documentation.json")).unwrap(),
        document(&[&original, "--private"])
    );
}

/// SDOC-4. Core is read from its own source root and exposes the interfaces,
/// modes and native candidates from that program.
#[test]
fn pinned_core_has_a_model_of_its_own_source_parts() {
    let core_root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/hgl_core/hgl");
    let core_root = argument(&core_root);
    let bytes = document(&[&core_root]);
    let model: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(model["modules"].as_array().unwrap().len(), 1);
    let core = module(&model, "core");
    assert_eq!(core["parts"][0]["source"]["file"], "core/core.hgl");
    assert!(core["parts"][0]["name"].is_null());
    let append = declaration(core, "append");
    assert_eq!(append["source"]["file"], "native/lists.hgl");
    assert_eq!(append["source"]["part"], "lists");
    assert_eq!(append["genericParameters"][0]["name"], "V");
    assert_eq!(append["parameters"][0]["type"], "list<V>");
    assert_eq!(append["parameters"][0]["var"], true);
    let add = declaration(core, "add_");
    assert_eq!(add["kind"], "operator");
    let candidates = add["candidates"].as_array().unwrap();
    assert!(candidates.iter().any(|candidate| {
        candidate["kind"] == "nativeCandidate"
            && candidate["parameters"][0]["type"] == "i64"
            && candidate["result"]["type"] == "i64"
            && candidate["source"]["part"] == "scalar_operators"
    }));
    assert!(
        core["declarations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|declaration| declaration["exported"] == true)
    );
    assert!(!text(&bytes).contains("<core>/"));
}

/// SDOC-3. Analysis still rejects invalid documentation keys, with no
/// partially written model and with diagnostics on standard error.
#[test]
fn a_rejected_program_writes_diagnostics_instead_of_a_model() {
    let root = scratch("rejected");
    let source = write(
        &root,
        "rejected.hgl",
        "module rejected\n/**\nArgs:\n    missing: Documentation.\n*/\nexport const fn accepted(value: i64) -> i64 => value\n",
    );
    let out = root.join("output");
    let output = hgl(&["doc", &source, "--format", "json", "--out", &argument(&out)]);
    assert_eq!(output.status.code(), Some(1));
    assert!(text(&output.stderr).contains("doc.unknown_key"));
    assert!(output.stdout.is_empty());
    assert!(!out.exists());
}
