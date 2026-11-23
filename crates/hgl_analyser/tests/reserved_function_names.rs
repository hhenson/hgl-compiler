//! UNIT-10 reserves activation-form names on function declarations.

mod common;

use common::check;

/// Value functions, temporal phases, native declarations and test helpers
/// all use the name of their function declaration (UNIT-10, TIME-9).
#[test]
fn functions_named_for_an_activation_form_report_the_declared_name() {
    let checked = check(&[
        (
            "graph.hgl",
            "module temporal_graph\nfn scheduled(value: i64) -> i64 => value\n",
        ),
        (
            "native.hgl",
            "module native_api\nnative const fn scheduled() -> i64\nnative fn scheduled(value: i64) -> i64\n",
        ),
        (
            "native_part.hgl",
            "module native_api part provider\nnative const fn scheduled() -> i64 {}\nnative fn scheduled(value: i64) -> i64 { when; }\n",
        ),
        (
            "node.hgl",
            "module node_body\nfn scheduled(value: i64) -> i64 { when { return value } }\n",
        ),
        (
            "source.hgl",
            "module source_body\nfn scheduled() -> i64 {\n    yield 1s: 1\n}\n",
        ),
        (
            "test.hgl",
            "module test_helper\ntest named {\n    const fn scheduled() -> i64 => 1\n    assert true\n}\n",
        ),
        (
            "value.hgl",
            "module ordinary_value\nconst fn scheduled() -> i64 => 1\n",
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "graph.hgl:2:4 name/name.reserved UNIT-10",
            "native.hgl:2:17 name/name.reserved UNIT-10",
            "native.hgl:3:11 name/name.reserved UNIT-10",
            "node.hgl:2:4 name/name.reserved UNIT-10",
            "source.hgl:2:4 name/name.reserved UNIT-10",
            "test.hgl:3:14 name/name.reserved UNIT-10",
            "value.hgl:2:10 name/name.reserved UNIT-10",
        ],
        "{}",
        checked.report
    );
    assert!(checked.report.contains("the activation form `scheduled()`"));
}

/// UNIT-10's reservation on other declaration kinds remains proposed.
#[test]
fn other_declarations_and_bindings_keep_their_names() {
    let checked = check(&[
        (
            "operator.hgl",
            "module ordinary_operator\noperator scheduled(value: i64) -> i64\nimpl const fn scheduled(value: i64) -> i64 => value\n",
        ),
        (
            "struct.hgl",
            "module ordinary_struct\nstruct scheduled { value: i64 }\n",
        ),
        (
            "enum.hgl",
            "module ordinary_enum\nenum scheduled { first, second }\n",
        ),
        (
            "constant.hgl",
            "module ordinary_constant\nconst scheduled: i64 = 2\nconst fn read() -> i64 => scheduled\n",
        ),
        (
            "binding.hgl",
            concat!(
                "module bindings\n",
                "struct Record { scheduled: i64 }\n",
                "const fn keep(scheduled: i64) -> i64 => scheduled\n",
                "const fn keep_local(value: i64) -> i64 {\n",
                "    let scheduled = value\n",
                "    Record(scheduled: scheduled).scheduled\n",
                "}\n",
            ),
        ),
    ]);
    assert!(checked.summary().is_empty(), "{}", checked.report);
}

/// A selective import reports the reserved name at the function's
/// declaration in its defining module (UNIT-2, UNIT-10).
#[test]
fn an_imported_function_reports_at_its_declaration() {
    let checked = check(&[
        (
            "exporter.hgl",
            "module a_exporter\nexport const fn scheduled() -> i64 => 1\n",
        ),
        (
            "importer.hgl",
            concat!(
                "module z_importer\n",
                "use a_exporter::{scheduled}\n",
                "const fn apply(f: const fn() -> i64) -> i64 => f()\n",
                "const fn use_import() -> i64 => apply(scheduled)\n",
            ),
        ),
    ]);
    assert_eq!(
        checked.summary(),
        ["exporter.hgl:2:17 name/name.reserved UNIT-10"],
        "{}",
        checked.report
    );
}
