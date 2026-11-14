//! The analyser's first pass against the rules it implements: modules,
//! declarations, imports. Each test cites the rule it checks.
//!
//! A program is given as `(file name, text)` pairs. A finding is written
//! `file:line:column category/code rule`.

mod common;

use common::{check, check_broken, check_with, span_of};
use hgl_analyser::{DeclarationKind, ImportKind, Role};

/// The names module `path` declares, each with what it is, in the
/// module's own order.
fn declared(checked: &common::Checked, path: &str) -> Vec<String> {
    checked
        .program
        .module(path)
        .expect("the module exists")
        .declarations
        .iter()
        .map(|declaration| {
            format!(
                "{}{}{} {}",
                if declaration.exported { "export " } else { "" },
                if declaration.test_only { "test " } else { "" },
                declaration.kind.describe(),
                declaration.name
            )
        })
        .collect()
}

// ---- Modules and parts ----

/// UNIT-1: "An assembled module has one declaration scope". The files of
/// a module may be given in any order; the unnamed one is first and the
/// parts follow by name.
#[test]
fn files_of_one_module_share_one_scope() {
    let files = [
        ("z.hgl", "module pricing part curves\nfn curve() {}\n"),
        ("a.hgl", "module pricing part bumps\nfn bump() {}\n"),
        ("m.hgl", "module pricing\nfn price() {}\n"),
        ("o.hgl", "module other\nfn curve() {}\n"),
    ];
    let checked = check(&files);
    assert_eq!(checked.summary(), [] as [&str; 0]);

    let paths: Vec<&str> = checked
        .program
        .modules
        .iter()
        .map(|module| module.path.as_str())
        .collect();
    assert_eq!(paths, ["other", "pricing"], "modules are in path order");

    let pricing = checked.program.module("pricing").unwrap();
    let parts: Vec<Option<&str>> = pricing
        .files
        .iter()
        .map(|file| file.part.as_deref())
        .collect();
    assert_eq!(parts, [None, Some("bumps"), Some("curves")]);
    assert_eq!(
        declared(&checked, "pricing"),
        [
            "temporal function price",
            "temporal function bump",
            "temporal function curve"
        ]
    );
    for name in ["price", "bump", "curve"] {
        assert_eq!(pricing.lookup(name).count(), 1, "{name}");
    }
    assert_eq!(pricing.lookup("missing").count(), 0);
    assert!(checked.program.module("pricing.curves").is_none());
}

/// UNIT-1: "at most one unnamed file".
#[test]
fn a_second_unnamed_file_is_rejected() {
    let checked = check(&[
        ("a.hgl", "module pricing\n"),
        ("b.hgl", "module pricing\n"),
        ("c.hgl", "module pricing part curves\n"),
    ]);
    assert_eq!(
        checked.summary(),
        ["b.hgl:1:8 module/module.duplicate_unnamed UNIT-1"]
    );
    assert!(
        checked.report.contains("the other is here"),
        "{}",
        checked.report
    );
}

/// UNIT-1: "unique part names".
#[test]
fn a_repeated_part_name_is_rejected() {
    let checked = check(&[
        ("a.hgl", "module pricing part curves\n"),
        ("b.hgl", "module pricing part curves\n"),
        ("c.hgl", "module other part curves\n"),
    ]);
    assert_eq!(
        checked.summary(),
        ["b.hgl:1:21 module/module.duplicate_part UNIT-1"]
    );
}

/// UNIT-1: "assembly order does not change meaning". Whatever order the
/// files are given in, the program and what is reported are the same.
#[test]
fn the_order_of_the_files_changes_nothing() {
    let files = [
        ("a.hgl", "module m part a\nfn f() {}\nfn g() {}\n"),
        ("b.hgl", "module m part b\nfn f() {}\nstruct g { x: i64 }\n"),
        ("c.hgl", "module m\nuse n::{missing}\nfn h() {}\n"),
        ("n.hgl", "module n\nuse m::{h}\n"),
    ];
    let expected = check(&files);
    assert_eq!(
        expected.summary(),
        [
            "b.hgl:2:4 name/name.duplicate UNIT-5",
            "b.hgl:3:8 name/name.duplicate UNIT-12",
            "c.hgl:2:5 module/import.cycle UNIT-3",
            "c.hgl:2:9 name/import.unknown_name UNIT-2",
            "n.hgl:2:9 module/import.private UNIT-2",
        ]
    );
    // Every rotation and the reverse.
    let mut orders: Vec<Vec<(&str, &str)>> = (1..files.len())
        .map(|shift| {
            let mut order = files.to_vec();
            order.rotate_left(shift);
            order
        })
        .collect();
    orders.push(files.iter().rev().copied().collect());
    for order in orders {
        let checked = check(&order);
        assert_eq!(checked.summary(), expected.summary());
        assert_eq!(declared(&checked, "m"), declared(&expected, "m"));
    }
}

/// UNIT-1: a file that names no module is in none. The parser reports
/// it; the analyser goes on with the rest.
#[test]
fn a_file_with_no_module_line_is_left_out() {
    let checked = check_broken(&[
        ("a.hgl", "fn stray() {}\n"),
        ("b.hgl", "module m\nfn f() {}\n"),
    ]);
    assert_eq!(checked.front_end_errors, 1);
    assert_eq!(checked.summary(), [] as [&str; 0]);
    assert_eq!(checked.program.modules.len(), 1);
    assert_eq!(declared(&checked, "m"), ["temporal function f"]);
}

// ---- Declarations ----

/// UNIT-5: "Duplicate ordinary functions of the same name/role ... are
/// rejected". FN-4: "A temporal/value pair may share a name."
#[test]
fn two_functions_of_one_role_are_duplicates_and_a_pair_is_not() {
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "fn scale(value: f64) -> f64 { return value }\n",
            "const fn scale(value: f64) -> f64 => value\n",
            "fn scale(value: i64) -> i64 { return value }\n",
            "const fn scale(value: i64) -> i64 => value\n",
        ),
    )]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:4 name/name.duplicate UNIT-5",
            "m.hgl:5:10 name/name.duplicate UNIT-5",
        ]
    );
    let module = checked.program.module("m").unwrap();
    let roles: Vec<DeclarationKind> = module.lookup("scale").map(|found| found.kind).collect();
    assert_eq!(
        roles,
        [
            DeclarationKind::Function(Role::Temporal),
            DeclarationKind::Function(Role::Value)
        ]
    );
    assert!(
        checked.report.contains("first declared here"),
        "{}",
        checked.report
    );
}

/// UNIT-12: "No binding takes a name already in scope". Only a temporal
/// and value function share one (FN-4): a type, an operator or a native
/// type with a function's name is a duplicate.
#[test]
fn declarations_of_different_kinds_do_not_share_a_name() {
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "fn quote() {}\n",
            "struct quote { bid: f64 }\n",
            "enum side { buy, sell }\n",
            "const fn side() -> i64 => 1\n",
            "operator add(left: i64, right: i64) -> i64\n",
            "fn add() {}\n",
            "native type handle\n",
            "enum handle { open }\n",
        ),
    )]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:8 name/name.duplicate UNIT-12",
            "m.hgl:5:10 name/name.duplicate UNIT-12",
            "m.hgl:7:4 name/name.duplicate UNIT-12",
            "m.hgl:9:6 name/name.duplicate UNIT-12",
        ]
    );
}

/// UNIT-1, UNIT-5: one scope across parts. Of two declarations in
/// different parts, the one in the later part by name is reported,
/// whichever file was given first.
#[test]
fn a_duplicate_across_parts_is_reported_in_the_later_part() {
    let early = ("x.hgl", "module m part alpha\nfn f() {}\n");
    let late = ("a.hgl", "module m part beta\nfn f() {}\n");
    let unnamed = ("q.hgl", "module m\nfn f() {}\n");
    for order in [[early, late, unnamed], [unnamed, late, early]] {
        assert_eq!(
            check(&order).summary(),
            [
                "a.hgl:2:4 name/name.duplicate UNIT-5",
                "x.hgl:2:4 name/name.duplicate UNIT-5",
            ]
        );
    }
}

/// UNIT-12: "An `impl fn` adds a candidate to its operator (GEN-4), not a
/// name", and "A member name is not a binding and may match any name."
#[test]
fn implementations_and_members_take_no_name() {
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "operator add<T>(left: T, right: T) -> T\n",
            "impl fn add(left: i64, right: i64) -> i64 => left\n",
            "impl fn add(left: f64, right: f64) -> f64 => left\n",
            "struct pair {\n    add: i64\n    pair: i64\n}\n",
            "enum side { add, pair, side }\n",
        ),
    )]);
    assert_eq!(checked.summary(), [] as [&str; 0]);
    assert_eq!(
        checked.program.module("m").unwrap().lookup("add").count(),
        1
    );
}

// ---- Imports ----

/// UNIT-2: "A selective import binds public declarations; an alias
/// qualifies a public interface". UNIT-3: "Ordinary functions and types
/// are private unless exported. Operators are public by definition."
#[test]
fn an_import_binds_public_declarations() {
    let library = (
        "lib.hgl",
        concat!(
            "module lib\n",
            "export fn shared() {}\n",
            "export const fn value() -> i64 => 1\n",
            "export struct Quote { bid: f64 }\n",
            "export enum Side { buy, sell }\n",
            "export native type Handle\n",
            "operator add(left: i64, right: i64) -> i64\n",
            // Of a pair, the exported one is what an import binds.
            "export fn pair() {}\n",
            "const fn pair() -> i64 => 1\n",
            "fn hidden() {}\n",
            "struct Secret { x: i64 }\n",
            "test { fn helper() {} }\n",
        ),
    );
    let checked = check(&[
        library,
        (
            "app.hgl",
            concat!(
                "module app\n",
                "use lib::{shared, value, Quote, Side, Handle, add, pair}\n",
                "use lib as l\n",
            ),
        ),
    ]);
    assert_eq!(checked.summary(), [] as [&str; 0]);
    let app = checked.program.module("app").unwrap();
    let lib = checked.program.module_index("lib");
    let imports = &app.files[0].imports;
    assert_eq!(imports.len(), 8);
    assert!(imports.iter().all(|import| import.module == lib));
    assert!(imports.iter().all(|import| import.from == "lib"));
    assert_eq!(imports[7].name, "l");
    assert_eq!(imports[7].kind, ImportKind::Alias);
    assert!(
        imports[..7]
            .iter()
            .all(|import| import.kind == ImportKind::Selective)
    );

    let rejected = check(&[
        library,
        (
            "app.hgl",
            "module app\nuse lib::{hidden, Secret, helper, missing, shared}\n",
        ),
    ]);
    assert_eq!(
        rejected.summary(),
        [
            "app.hgl:2:11 module/import.private UNIT-2",
            "app.hgl:2:19 module/import.private UNIT-2",
            "app.hgl:2:27 module/import.private UNIT-2",
            "app.hgl:2:35 name/import.unknown_name UNIT-2",
        ]
    );
    assert!(rejected.report.contains("(UNIT-3)"), "{}", rejected.report);
    assert!(rejected.report.contains("(TST-2)"), "{}", rejected.report);
}

/// UNIT-2: a module is found by its `module` line among the files given.
/// One that none declares is reported once for its `use` line.
#[test]
fn an_unknown_module_is_reported_once_for_its_line() {
    let checked = check(&[(
        "app.hgl",
        "module app\nuse lib.missing::{a, b, c}\nuse lib.other as other\n",
    )]);
    assert_eq!(
        checked.summary(),
        [
            "app.hgl:2:5 module/import.unknown_module UNIT-2",
            "app.hgl:3:5 module/import.unknown_module UNIT-2",
        ]
    );
}

/// UNIT-2: "Imports precede ordinary declarations".
#[test]
fn an_import_after_a_declaration_is_rejected() {
    let checked = check(&[
        ("lib.hgl", "module lib\nexport fn shared() {}\n"),
        (
            "app.hgl",
            "module app\nuse lib as l\nfn f() {}\nuse lib::{shared}\ntest t { assert true }\nuse lib as m\n",
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "app.hgl:4:1 module/import.misplaced UNIT-2",
            "app.hgl:6:1 module/import.misplaced UNIT-2",
        ]
    );
    // It is bound all the same.
    let app = checked.program.module("app").unwrap();
    assert_eq!(app.files[0].imports.len(), 3);
}

/// UNIT-12: a `use` binds no name already bound in its file. UNIT-2:
/// imports "bind in their own file", so two files of a module may import
/// one name.
#[test]
fn a_name_is_imported_once_in_a_file() {
    let checked = check(&[
        (
            "lib.hgl",
            "module lib\nexport fn a() {}\nexport fn b() {}\n",
        ),
        ("other.hgl", "module other\nexport fn a() {}\n"),
        (
            "app.hgl",
            "module app\nuse lib::{a, b, a}\nuse other::{a}\nuse other as b\nuse lib as b\n",
        ),
        ("part.hgl", "module app part second\nuse lib::{a, b}\n"),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "app.hgl:2:17 name/name.duplicate UNIT-12",
            "app.hgl:3:13 name/name.duplicate UNIT-12",
            "app.hgl:4:14 name/name.duplicate UNIT-12",
            "app.hgl:5:12 name/name.duplicate UNIT-12",
        ]
    );
}

/// UNIT-12: "A declaration takes no name its module imports", in any of
/// its files; the declaration is the one reported.
#[test]
fn a_declaration_takes_no_imported_name() {
    let checked = check(&[
        (
            "lib.hgl",
            "module lib\nexport fn shared() {}\nexport fn other() {}\n",
        ),
        (
            "app.hgl",
            "module app\nuse lib::{shared}\nuse lib as l\nfn shared() {}\nstruct l { x: i64 }\n",
        ),
        (
            "part.hgl",
            "module app part second\nuse lib::{other}\nfn fine() {}\n",
        ),
        (
            "third.hgl",
            "module app part third\nconst fn other() -> i64 => 1\n",
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "app.hgl:4:4 name/name.duplicate UNIT-12",
            "app.hgl:5:8 name/name.duplicate UNIT-12",
            "third.hgl:2:10 name/name.duplicate UNIT-12",
        ]
    );
    assert!(
        checked.report.contains("imported here"),
        "{}",
        checked.report
    );
}

/// UNIT-3: "Import cycles are rejected." A group of modules that import
/// one another is reported once, in its first module by path.
#[test]
fn an_import_cycle_is_rejected() {
    let a = (
        "a.hgl",
        "module a\nuse b as b\nuse c::{h}\nexport fn f() {}\n",
    );
    let b = ("b.hgl", "module b\nuse c::{h}\nexport fn g() {}\n");
    let c = ("c.hgl", "module c\nuse a::{f}\nexport fn h() {}\n");
    let d = ("d.hgl", "module d\nuse a::{f}\nuse d as me\n");
    for order in [[a, b, c, d], [d, c, b, a]] {
        let checked = check(&order);
        assert_eq!(
            checked.summary(),
            [
                "a.hgl:2:5 module/import.cycle UNIT-3",
                "d.hgl:3:5 module/import.cycle UNIT-3",
            ]
        );
        assert!(
            checked
                .report
                .contains("modules `a`, `b` and `c` import one another"),
            "{}",
            checked.report
        );
        assert!(
            checked.report.contains("module `d` imports itself"),
            "{}",
            checked.report
        );
    }

    // A chain is no cycle.
    let chain = check(&[
        ("a.hgl", "module a\nuse b::{g}\n"),
        ("b.hgl", "module b\nuse c::{h}\nexport fn g() {}\n"),
        ("c.hgl", "module c\nexport fn h() {}\n"),
    ]);
    assert_eq!(chain.summary(), [] as [&str; 0]);
}

// ---- Tests and test contexts ----

/// TST-1: "Unnamed contexts contribute private fn/const-fn helpers,
/// unexported struct/enum types, and named tests to one module-wide test
/// scope across all parts, independent of order."
#[test]
fn test_contexts_form_one_test_scope() {
    let checked = check(&[
        (
            "a.hgl",
            concat!(
                "module m\n",
                "fn production() {}\n",
                "test {\n",
                "    fn helper() {}\n",
                "    struct Fixture { x: i64 }\n",
                "    test inner { assert true }\n",
                "}\n",
                "test outer { assert true }\n",
            ),
        ),
        (
            "b.hgl",
            "module m part b\ntest {\n    const fn helper() -> i64 => 1\n    enum Kind { a }\n}\n",
        ),
    ]);
    assert_eq!(checked.summary(), [] as [&str; 0]);
    assert_eq!(
        declared(&checked, "m"),
        [
            "temporal function production",
            "test temporal function helper",
            "test struct Fixture",
            "test value function helper",
            "test enum Kind",
        ]
    );
    let module = checked.program.module("m").unwrap();
    assert_eq!(module.lookup("helper").count(), 2);
    let tests: Vec<&str> = module.tests.iter().map(|test| test.name.as_str()).collect();
    assert_eq!(tests, ["inner", "outer"]);
}

/// TST-2: "A test-only helper or type takes no name of a production
/// declaration or import (UNIT-12). Duplicate test names or roles
/// reject." The helper is the one reported, wherever it stands.
#[test]
fn a_test_helper_needs_its_own_name() {
    let checked = check(&[
        ("lib.hgl", "module lib\nexport fn shared() {}\n"),
        (
            "a.hgl",
            concat!(
                "module m\n",
                "use lib::{shared}\n",
                "test {\n",
                "    fn choose() {}\n",
                "    struct shared { x: i64 }\n",
                "    fn helper() {}\n",
                "}\n",
                "fn choose() {}\n",
                "fn temporal() {}\n",
                "test {\n",
                "    fn helper() {}\n",
                // Two helpers may be a temporal and value pair (FN-4).
                "    const fn helper() -> i64 => 1\n",
                // A helper and a declaration of the module may not.
                "    const fn temporal() -> i64 => 1\n",
                "}\n",
            ),
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "a.hgl:4:8 name/name.duplicate TST-2",
            "a.hgl:5:12 name/name.duplicate TST-2",
            "a.hgl:11:8 name/name.duplicate TST-2",
            "a.hgl:13:14 name/name.duplicate TST-2",
        ]
    );
    assert!(
        checked.report.contains("needs a name of its own"),
        "{}",
        checked.report
    );
}

/// TST-2: "Duplicate test names ... reject", across files and contexts.
/// UNIT-6: "A test is not a callable value", so its name is in no scope
/// and may match a declaration's.
#[test]
fn test_names_are_unique_in_a_module() {
    let checked = check(&[
        (
            "a.hgl",
            concat!(
                "module m\n",
                "fn survives() {}\n",
                "test survives { assert true }\n",
                "test { test survives { assert true } }\n",
            ),
        ),
        ("b.hgl", "module m part b\ntest survives { assert true }\n"),
        ("c.hgl", "module n\ntest survives { assert true }\n"),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "a.hgl:4:13 name/test.duplicate_name TST-2",
            "b.hgl:2:6 name/test.duplicate_name TST-2",
        ]
    );
}

/// TST-1: "exports, and operator implementations are not admitted here."
/// UNIT-6: "Test-only declarations cannot be exported".
#[test]
fn a_test_context_exports_nothing_and_implements_no_operator() {
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "operator add(left: i64, right: i64) -> i64\n",
            "test {\n",
            "    export fn helper() {}\n",
            "    export struct Fixture { x: i64 }\n",
            "    export enum Kind { a }\n",
            "    impl fn add(left: i64, right: i64) -> i64 => left\n",
            "}\n",
        ),
    )]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:5 module/test.export TST-1",
            "m.hgl:5:5 module/test.export TST-1",
            "m.hgl:6:5 module/test.export TST-1",
            "m.hgl:7:13 operator/test.implementation TST-1",
        ]
    );
    // None is exported for all that.
    let module = checked.program.module("m").unwrap();
    assert!(
        module
            .declarations
            .iter()
            .filter(|declaration| declaration.test_only)
            .all(|declaration| !declaration.is_public())
    );
}

// ---- Operators ----

/// GEN-4: "An `impl fn` binds that operator through local declaration,
/// selective import, or the prelude."
#[test]
fn an_implementation_finds_its_operator() {
    let checked = check(&[
        (
            "lib.hgl",
            "module lib\noperator mul(left: i64, right: i64) -> i64\nexport fn plain() {}\n",
        ),
        (
            "app.hgl",
            concat!(
                "module app\n",
                "use lib::{mul}\n",
                "operator add(left: i64, right: i64) -> i64\n",
                "impl fn add(left: i64, right: i64) -> i64 => left\n",
                "impl fn mul(left: i64, right: i64) -> i64 => left\n",
                "impl fn div(left: i64, right: i64) -> i64 => left\n",
            ),
        ),
    ]);
    assert_eq!(checked.summary(), [] as [&str; 0]);
    let app = checked.program.module("app").unwrap();
    let operators: Vec<Option<(&str, &str)>> = app
        .declarations
        .iter()
        .filter_map(|declaration| match declaration.kind {
            DeclarationKind::Implementation(operator) => Some(operator.map(|operator| {
                let module = &checked.program.modules[operator.module];
                (
                    module.path.as_str(),
                    module.declarations[operator.declaration].name.as_str(),
                )
            })),
            _ => None,
        })
        .collect();
    assert_eq!(
        operators,
        [Some(("app", "add")), Some(("lib", "mul")), None]
    );
    // UNIT-11: nothing in the program binds `div`, so it is left for the
    // prelude and not reported.
    assert_eq!(checked.prelude_names("app"), ["div"]);
}

/// GEN-4: "No matching operator is a declaration error. A plain fn is not
/// a candidate." UNIT-2: an import binds in its own file.
#[test]
fn an_implementation_of_something_else_is_rejected() {
    let checked = check(&[
        (
            "lib.hgl",
            "module lib\noperator mul(left: i64, right: i64) -> i64\nexport fn plain() {}\n",
        ),
        (
            "app.hgl",
            concat!(
                "module app\n",
                "use lib::{plain, mul}\n",
                "use lib as l\n",
                "fn local() {}\n",
                "impl fn local() {}\n",
                "impl fn plain() {}\n",
                "impl fn l() {}\n",
            ),
        ),
        (
            "part.hgl",
            // `mul` is imported in the other file, not this one.
            "module app part second\nimpl fn mul(left: i64, right: i64) -> i64 => left\n",
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "app.hgl:5:9 operator/impl.unknown_operator GEN-4",
            "app.hgl:6:9 operator/impl.unknown_operator GEN-4",
            "app.hgl:7:9 operator/impl.unknown_operator GEN-4",
        ]
    );
    assert_eq!(checked.prelude_names("app"), ["mul"]);
}

// ---- Native declarations ----

/// NATIVE-2: "A selected implementation part completes exactly one shared
/// declaration." NATIVE-1: "Native temporal and native const roles remain
/// distinct". NATIVE-3: "A missing body is a declaration".
#[test]
fn a_native_implementation_completes_its_declaration() {
    let checked = check(&[
        (
            "interface.hgl",
            concat!(
                "module examples.native\n",
                "native fn accumulate(value: i64) -> i64\n",
                "native const fn describe(value: i64) -> str\n",
                "native fn describe(value: i64) -> str\n",
            ),
        ),
        (
            "provider.hgl",
            concat!(
                "module examples.native part provider\n",
                "native fn accumulate(value: i64) -> i64 {\n    inject out\n    when;\n}\n",
                "native const fn describe(value: i64) -> str {}\n",
            ),
        ),
        (
            "other.hgl",
            concat!(
                "module examples.native part other\n",
                "native fn accumulate(value: i64) -> i64 { when; }\n",
            ),
        ),
    ]);
    assert_eq!(checked.summary(), [] as [&str; 0]);
    let module = checked.program.module("examples.native").unwrap();
    assert_eq!(module.lookup("accumulate").count(), 1);
    assert_eq!(module.lookup("describe").count(), 2);
}

/// NATIVE-2: "Duplicate declarations or selected implementations are
/// errors, not overloads."
#[test]
fn native_duplicates_and_stray_implementations_are_rejected() {
    let checked = check(&[
        (
            "interface.hgl",
            concat!(
                "module m\n",
                "native fn tick() -> i64\n",
                "native fn tick() -> i64\n",
                "fn plain() {}\n",
                "struct Shape { x: i64 }\n",
            ),
        ),
        (
            "provider.hgl",
            concat!(
                "module m part provider\n",
                "native fn tick() -> i64 { when; }\n",
                "native fn tick() -> i64 {}\n",
                "native fn plain() {}\n",
                "native fn Shape() {}\n",
                "native fn stray() {}\n",
                "native const fn tick() -> i64 {}\n",
            ),
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "interface.hgl:3:11 name/name.duplicate NATIVE-2",
            "provider.hgl:3:11 name/name.duplicate NATIVE-2",
            "provider.hgl:4:11 name/name.duplicate NATIVE-2",
            "provider.hgl:5:11 name/name.duplicate NATIVE-2",
            "provider.hgl:6:11 name/native.missing_declaration NATIVE-2",
            "provider.hgl:7:17 name/native.missing_declaration NATIVE-2",
        ]
    );
}

// ---- Leaving declarations out ----

/// TST-13: "Exclude all rejection owners from executable source." A
/// declaration, import, test or helper named in a file's `omit` list is
/// analysed as if it were not written.
#[test]
fn an_omitted_item_is_not_declared() {
    let lib = ("lib.hgl", "module lib\nexport fn shared() {}\n");
    let text = concat!(
        "module m\n",
        "use lib::{missing}\n",
        "fn f() {}\n",
        "fn f() { return 1 }\n",
        "test t { assert true }\n",
        "test t { assert false }\n",
        "test {\n",
        "    fn helper() {}\n",
        "    fn helper() { return 1 }\n",
        "}\n",
    );
    let whole = check(&[lib, ("m.hgl", text)]);
    assert_eq!(
        whole.summary(),
        [
            "m.hgl:2:11 name/import.unknown_name UNIT-2",
            "m.hgl:4:4 name/name.duplicate UNIT-5",
            "m.hgl:6:6 name/test.duplicate_name TST-2",
            "m.hgl:9:8 name/name.duplicate TST-2",
        ]
    );

    let without = check_with(&[lib, ("m.hgl", text)], |name, text| {
        if name != "m.hgl" {
            return Vec::new();
        }
        vec![
            span_of(text, "use lib::{missing}", "}"),
            span_of(text, "fn f() { return 1 }", "}"),
            span_of(text, "test t { assert false }", "}"),
            span_of(text, "fn helper() { return 1 }", "}"),
        ]
    });
    assert_eq!(without.front_end_errors, 0);
    assert_eq!(without.summary(), [] as [&str; 0]);
    let module = without.program.module("m").unwrap();
    assert_eq!(module.declarations.len(), 2);
    assert_eq!(module.tests.len(), 1);
    assert!(module.files[0].imports.is_empty());
}
