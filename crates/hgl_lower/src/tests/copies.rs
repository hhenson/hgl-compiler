//! Borrowed reads, owned values and mutated places as lowering sees them.

use hgl_analyser::hir::{Body, ExprKind, Stmt};

use crate::copies::{Copies, changed_in_place, changes, fresh, is_place, place_of};

use super::{body, program, returned};

fn local(body: &Body, name: &str) -> usize {
    body.locals
        .iter()
        .position(|local| local.name == name)
        .unwrap_or_else(|| panic!("fixture local {name}"))
}

/// Compiler #151, EXPR-3, FN-13: a `let` is a reference when its source
/// stays stable in scope. Growth before the binding leaves it a loan.
#[test]
fn a_let_refers_to_an_unchanged_place() {
    let program = program(&[(
        "m.hgl",
        r#"module m
const fn read() -> list<i64> {
    var rows = [[1]]
    append(rows[0], 2)
    let view = rows[0]
    return view
}
"#,
    )]);
    let body = body(&program, "m", "read");
    let copies = Copies::of(&program, body);
    let rows = local(body, "rows");
    assert!(changes(&program, &body.locals, &body.statements, rows));
    assert!(!copies.copied(local(body, "view")));
    assert_eq!(
        copies.places(&program, &body.locals, returned(body)),
        [rows]
    );
}

/// Compiler #151, EXPR-3, FN-13, FN-15: growth takes the nested place.
/// If the growth is in the `let`'s scope, the `let` holds its own copy.
#[test]
fn a_let_copies_a_place_that_changes_in_scope() {
    let program = program(&[(
        "m.hgl",
        r#"module m
const fn snapshot_then_grow() -> list<i64> {
    var rows = [[1]]
    let snapshot = rows[0]
    append(rows[0], 2)
    return snapshot
}
"#,
    )]);
    let body = body(&program, "m", "snapshot_then_grow");
    let [_, _, Stmt::Expr(growth), _] = body.statements.as_slice() else {
        panic!(
            "expected declarations, growth, and return: {:?}",
            body.statements
        );
    };
    let ExprKind::Call { arguments, .. } = &growth.kind else {
        panic!("append is a call");
    };
    let rows = local(body, "rows");
    let growing = &arguments[0].value;
    assert_eq!(place_of(&body.locals, growing), Some(rows));
    assert!(changed_in_place(&program, &growing.ty, 0));

    let copies = Copies::of(&program, body);
    assert!(!copies.copied(rows));
    assert!(copies.copied(local(body, "snapshot")));
    let snapshot = returned(body);
    assert!(copies.places(&program, &body.locals, snapshot).is_empty());
    assert!(!copies.elsewhere(&program, &body.locals, snapshot));
}

/// Compiler #151, FN-13: an ordinary parameter and a `let` of it borrow
/// the caller's value. Retention sees that provenance. A value created
/// by the body has no such external owner.
#[test]
fn retention_distinguishes_a_borrowed_parameter_from_a_fresh_value() {
    let program = program(&[(
        "m.hgl",
        r#"module m
const fn retained(items: list<i64>) -> list<i64> {
    let alias = items
    return alias
}
const fn made_here() -> list<i64> {
    let own = [1, 2]
    return own
}
"#,
    )]);
    let borrowed = body(&program, "m", "retained");
    let [
        Stmt::Declare {
            value: Some(items), ..
        },
        ..,
    ] = borrowed.statements.as_slice()
    else {
        panic!("expected the parameter read");
    };
    let copies = Copies::of(&program, borrowed);
    assert!(!is_place(&borrowed.locals, local(borrowed, "items")));
    assert!(!copies.copied(local(borrowed, "alias")));
    for value in [items, returned(borrowed)] {
        assert!(copies.places(&program, &borrowed.locals, value).is_empty());
        assert!(copies.elsewhere(&program, &borrowed.locals, value));
        assert!(!fresh(&program, value));
    }

    let owned = body(&program, "m", "made_here");
    let [
        Stmt::Declare {
            value: Some(literal),
            ..
        },
        ..,
    ] = owned.statements.as_slice()
    else {
        panic!("expected the fresh list");
    };
    let copies = Copies::of(&program, owned);
    assert!(fresh(&program, literal));
    assert!(!copies.copied(local(owned, "own")));
    assert!(!copies.elsewhere(&program, &owned.locals, returned(owned)));
}

/// Compiler #151, FN-13, FN-15: the ordinary argument needs a snapshot
/// only where it reads from the same place the call will change.
#[test]
fn a_var_call_can_distinguish_overlapping_and_independent_reads() {
    let program = program(&[(
        "m.hgl",
        r#"module m
const fn grow(var rows: list<list<i64>>, value: list<i64>) {
    append(rows, value)
}
const fn use_grow() {
    var rows = [[1]]
    var other = [[2]]
    grow(rows, rows[0])
    grow(rows, other[0])
}
"#,
    )]);
    let callee = body(&program, "m", "grow");
    assert!(is_place(&callee.locals, local(callee, "rows")));
    assert!(!is_place(&callee.locals, local(callee, "value")));

    let caller = body(&program, "m", "use_grow");
    let copies = Copies::of(&program, caller);
    let rows = local(caller, "rows");
    let other = local(caller, "other");
    let [_, _, Stmt::Expr(overlap), Stmt::Expr(independent)] = caller.statements.as_slice() else {
        panic!("expected two calls of grow");
    };
    for (call, read_root) in [(overlap, rows), (independent, other)] {
        let ExprKind::Call { arguments, .. } = &call.kind else {
            panic!("expected a call of grow");
        };
        assert_eq!(place_of(&caller.locals, &arguments[0].value), Some(rows));
        assert_eq!(
            copies.places(&program, &caller.locals, &arguments[1].value),
            [read_root]
        );
    }
}

/// Compiler #151, EXPR-3: mutation after a block does not force a copy
/// of a `let` whose scope ended with that block.
#[test]
fn a_change_after_a_lets_scope_keeps_the_reference() {
    let program = program(&[(
        "m.hgl",
        r#"module m
const fn read_then_grow() -> i64 {
    var rows = [[1]]
    let first = {
        let view = rows[0]
        view[0]
    }
    append(rows[0], 2)
    return first
}
"#,
    )]);
    let body = body(&program, "m", "read_then_grow");
    let rows = local(body, "rows");
    let view = local(body, "view");
    assert!(changes(&program, &body.locals, &body.statements, rows));

    let copies = Copies::of(&program, body);
    assert!(!copies.copied(view));
    let [
        _,
        Stmt::Declare {
            value: Some(block), ..
        },
        ..,
    ] = body.statements.as_slice()
    else {
        panic!("expected the block initializer");
    };
    let ExprKind::Block {
        value: Some(read), ..
    } = &block.kind
    else {
        panic!("expected a block with a final read");
    };
    assert_eq!(copies.places(&program, &body.locals, read), [rows]);
}
