//! Named test scopes, ordinary helper checking and eval/run admission (TST-1 to TST-5).
mod common;

use common::check;
use hgl_analyser::hir::{Callee, Expr, ExprKind, Stmt};
use hgl_analyser::{DeclarationKind, Phase, Role};

const NONE: [&str; 0] = [];

fn initializer(statement: &Stmt) -> &Expr {
    match statement {
        Stmt::Declare {
            value: Some(value), ..
        } => value,
        _ => panic!("a local with an initializer"),
    }
}

fn called(expr: &Expr) -> usize {
    match &expr.kind {
        ExprKind::Call {
            callee: Callee::Function { declaration, .. },
            ..
        } => *declaration,
        _ => panic!("a call of a function"),
    }
}

/// TST-1/FN-4: the test owns a normal function pair, and node and value helper bodies
/// see their sibling helpers without acquiring the test's statement scope.
#[test]
fn a_test_checks_its_helpers_as_functions_and_selects_their_roles() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
test local_pair {
    fn twice(value: i64) -> i64 {
        inject clock
        state ticks: i64 = 0
        when {
            ticks += 1
            return add_two(value) + ticks
        }
    }
    const fn twice(value: i64) -> i64 => add_two(value)
    const fn add_two(value: i64) -> i64 => value + 2
    let direct = twice(2)
    let seen = eval twice(value: [1, _, 3])
    assert direct == 4
    assert seen == [4, _, 7]
}
"#,
    )]);
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    let module = checked.program.module("m").unwrap();
    assert_eq!(module.declarations.len(), 3);
    assert!(module.lookup("twice").next().is_none());
    assert!(
        module
            .declarations
            .iter()
            .all(|entry| entry.test_owner == Some(0) && entry.test_only)
    );
    assert_eq!(module.phases[0], Some(Phase::Node));
    let body = module.test_bodies[0].as_ref().unwrap();
    assert_eq!(body.statements.len(), 4);
    assert_eq!(called(initializer(&body.statements[0])), 1);
    let ExprKind::Eval { call, .. } = &initializer(&body.statements[1]).kind else {
        panic!("a test eval");
    };
    assert_eq!(called(call), 0);
}

/// TST-1: private helper identities survive repeated names across named tests
/// and coexist with module-wide helpers contributed by unnamed contexts.
#[test]
fn same_names_in_separate_tests_select_separate_helpers() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
test {
    const fn shared() -> i64 => 1
    test first {
        const fn local() -> i64 => shared()
        let selected = local()
        assert selected == 1
    }
}
test second {
    const fn local() -> bool => true
    let selected = local()
    assert selected
}
"#,
    )]);
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    let module = checked.program.module("m").unwrap();
    assert_eq!(module.lookup("shared").next().unwrap().test_owner, None);
    assert!(module.lookup("local").next().is_none());
    for (index, body) in module.test_bodies.iter().enumerate() {
        let body = body.as_ref().unwrap();
        let helper = &module.declarations[called(initializer(&body.statements[0]))];
        assert_eq!(helper.test_owner, Some(index));
        assert_eq!(helper.kind, DeclarationKind::Function(Role::Value));
    }
}

/// UNIT-5 applies to duplicates inside one test, while TST-2 prevents a
/// helper from using a visible module, test-context, import or prelude name.
#[test]
fn a_helper_cannot_duplicate_its_role_or_a_visible_name() {
    let checked = check(&[
        (
            "exported.hgl",
            "module foreign\nexport const fn brought() -> i64 => 2\n",
        ),
        (
            "m.hgl",
            r#"module m
use foreign::{brought}
const fn top() -> i64 => 1
test {
    const fn shared() -> i64 => 2
}
test rejected {
    const fn top() -> i64 => 3
    const fn shared() -> i64 => 3
    const fn brought() -> i64 => 3
    const fn append() -> i64 => 3
    const fn again() -> i64 => 3
    const fn again() -> i64 => 3
    assert true
}
"#,
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:8:14 name/name.duplicate TST-2",
            "m.hgl:9:14 name/name.duplicate TST-2",
            "m.hgl:10:14 name/name.duplicate TST-2",
            "m.hgl:11:14 name/name.duplicate TST-2",
            "m.hgl:13:14 name/name.duplicate UNIT-5",
        ],
        "{}",
        checked.report
    );
}

/// UNIT-2: an import in another part is outside the test's visible scope.
#[test]
fn a_helper_can_share_an_import_name_from_another_part() {
    let checked = check(&[
        (
            "foreign.hgl",
            "module foreign\nexport const fn shared() -> i64 => 1\n",
        ),
        (
            "m.hgl",
            "module m\nuse foreign::{shared}\nconst fn imported() -> i64 => shared()\n",
        ),
        (
            "mpart.hgl",
            "module m part other\ntest example {\n    const fn shared() -> i64 => 2\n    assert shared() == 2\n}\n",
        ),
    ]);
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
}

/// TST-1: neither production, unnamed-context helpers, nor other tests see a
/// named test's own helpers.
#[test]
fn a_helper_is_unknown_outside_its_own_test() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
const fn production() -> i64 => local()
test {
    const fn shared() -> i64 => local()
}
test owns {
    const fn local() -> i64 => 1
    assert local() == 1
}
test other {
    assert local() == 1
}
"#,
    )]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:33 name/name.unknown UNIT-10",
            "m.hgl:4:33 name/name.unknown UNIT-10",
            "m.hgl:11:12 name/name.unknown UNIT-10",
        ],
        "{}",
        checked.report
    );
}

/// TST-3: a temporal helper requires eval/run even in the test that owns it.
#[test]
fn a_test_does_not_call_its_node_directly() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
test example {
    fn node(value: i64) -> i64 { when { return value } }
    let direct = node(2)
}
"#,
    )]);
    assert_eq!(
        checked.summary(),
        ["m.hgl:4:18 function-kind/call.role TST-3"],
        "{}",
        checked.report
    );
}

/// TST-5/TST-17: an integration test runs its own node, and declarations
/// before its statements do not prevent its leading configuration injection.
#[test]
fn an_integration_test_runs_its_own_helper() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
integration_test configured {
    fn pass(value: i64) -> i64 { when { return value } }
    inject run_config{length: 1s}
    let seen = run pass(value: [7])
    assert seen == [7]
}
"#,
    )]);
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    let module = checked.program.module("m").unwrap();
    let body = module.test_bodies[0].as_ref().unwrap();
    let Stmt::Inject { local, config, .. } = &body.statements[0] else {
        panic!("the first statement is the test's inject");
    };
    assert_eq!(config[0].field, "length");
    let ExprKind::Eval {
        call,
        real_time: true,
        inherited: Some(config),
        ..
    } = &initializer(&body.statements[1]).kind
    else {
        panic!("a run with inherited configuration");
    };
    assert_eq!(config, local);
    assert_eq!(module.declarations[called(call)].test_owner, Some(0));
}
