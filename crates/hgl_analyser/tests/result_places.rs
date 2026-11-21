//! Contextual result and nullable reads in calls, comparisons, assertions
//! and constant arguments (ERR-5, ERR-12, ERR-15).

mod common;

use common::{Checked, body, module};
use hgl_analyser::hir::{Callee, ExprKind, Raise, Stmt, Value, walk_statements};
use hgl_analyser::{Argument, Scalar, Type};

const CHECKED: &str = r#"
const fn checked(value: i64) -> result<i64, error> {
    if value < 0 { return err(error(message: "negative", code: "convert.invalid")) }
    return ok(value)
}
"#;

fn with_checked(source: &str) -> Checked {
    module(&format!("{CHECKED}{source}"))
}

/// ERR-12, ERR-15 and GEN-5: operators try payloads where their
/// candidates' conditions rule out the whole arguments.
#[test]
fn constrained_comparisons_admit_payload_operands() {
    let checked = with_checked(
        r#"
const fn nullable(n: i64?) -> bool => n < 1
const fn fallible(x: i64, y: i64) -> bool => checked(x) < checked(y)
const fn fraction(x: f64, y: f64) -> bool => (x /{zero: .error} y) < 1.0
const fn ordered(x: i64, y: i64) -> CmpResult => checked(x) <=> checked(y)
const fn both(x: result<i64?, error>) -> bool => x < 1
operator ordered_before<L, R>(left: L, right: R) -> bool
impl const fn ordered_before<T>(left: T, right: T) -> bool
    requires T in {i64} => true
const fn custom(x: i64, y: i64) -> bool => ordered_before(checked(x), checked(y))
"#,
    );
    assert!(checked.summary().is_empty(), "{}", checked.report);
    for (name, raised, nullable) in [
        ("nullable", 0, 1),
        ("fallible", 2, 0),
        ("fraction", 1, 0),
        ("ordered", 2, 0),
        ("both", 1, 1),
        ("custom", 2, 0),
    ] {
        let mut forms = (0, 0);
        walk_statements(
            &body(&checked, name).statements,
            &mut |expr| match expr.kind {
                ExprKind::Raised {
                    raise: Raise::Execution,
                    ..
                } => forms.0 += 1,
                ExprKind::NullRead {
                    raise: Raise::Execution,
                    ..
                } => forms.1 += 1,
                _ => {}
            },
        );
        assert_eq!(forms, (raised, nullable), "{name}");
    }
}

/// ERR-15 keeps an argument whole when its parameter accepts the
/// result. Only the other generic binding needs the nullable read.
#[test]
fn another_arguments_payload_does_not_unwrap_a_whole_generic_argument() {
    let checked = with_checked(
        r#"
operator take<X, Y>(keep: X, read: Y) -> i64
impl const fn take<T, U>(keep: T, read: U) -> i64
    requires U in {i64} => 0
const fn keep_failed(read: i64?) -> i64 => take(checked(-1), read)
operator take_if<X, Y>(keep: X, read: Y) -> i64
impl const fn take_if<T, U>(keep: T, read: U) -> i64
    requires T in {result<i64, error>, i64} && U in {i64} => 0
const fn keep_allowed(read: i64?) -> i64 => take_if(checked(-1), read)
"#,
    );
    assert!(checked.summary().is_empty(), "{}", checked.report);
    for name in ["keep_failed", "keep_allowed"] {
        let mut calls = 0;
        walk_statements(&body(&checked, name).statements, &mut |expr| {
            if let ExprKind::Call {
                callee: Callee::Candidate { .. },
                arguments,
                ..
            } = &expr.kind
                && arguments.len() == 2
            {
                assert_eq!(arguments[0].value.ty.to_string(), "result<i64, error>");
                assert!(matches!(arguments[0].value.kind, ExprKind::Call { .. }));
                assert!(matches!(
                    arguments[1].value.kind,
                    ExprKind::NullRead {
                        raise: Raise::Execution,
                        ..
                    }
                ));
                calls += 1;
            }
        });
        assert_eq!(calls, 1, "{name}");
    }
}

/// ERR-12 and ERR-15: each generic parameter takes the form its
/// constraint admits from the same fallible nullable argument type.
#[test]
fn constrained_generic_arguments_take_independent_result_forms() {
    let checked = module(
        r#"
operator choose<X, Y, Z>(held: X, maybe: Y, value: Z) -> i64
impl const fn choose<T, U, V>(held: T, maybe: U, value: V) -> i64
    requires T in {result<i64?, error>} && U in {i64?} && V in {i64} => 0
const fn mixed(input: result<i64?, error>) -> i64 => choose(input, input, input)
"#,
    );
    assert!(checked.summary().is_empty(), "{}", checked.report);
    let mut calls = 0;
    walk_statements(&body(&checked, "mixed").statements, &mut |expr| {
        if let ExprKind::Call {
            callee: Callee::Candidate { .. },
            arguments,
            ..
        } = &expr.kind
        {
            let [held, maybe, value] = arguments.as_slice() else {
                panic!("expected three arguments");
            };
            assert_eq!(held.value.ty.to_string(), "result<i64?, error>");
            assert_eq!(maybe.value.ty.to_string(), "i64?");
            assert!(matches!(
                maybe.value.kind,
                ExprKind::Raised {
                    raise: Raise::Execution,
                    ..
                }
            ));
            assert_eq!(value.value.ty, Type::Scalar(Scalar::I64));
            let ExprKind::NullRead { value: read, .. } = &value.value.kind else {
                panic!("expected a read of the nullable success");
            };
            assert!(matches!(
                read.kind,
                ExprKind::Raised {
                    raise: Raise::Execution,
                    ..
                }
            ));
            calls += 1;
        }
    });
    assert_eq!(calls, 1);
}

/// GEN-5: a candidate that takes the integer literal without widening
/// outranks one that widens it, even when another operand takes its value.
#[test]
fn comparison_reads_a_result_before_widening_an_integer_literal() {
    let checked = with_checked(
        "const fn below(v: i64) -> result<bool, error> => ok(checked(v) < 9007199254740993)\n",
    );
    assert!(checked.summary().is_empty(), "{}", checked.report);
    let mut comparisons = 0;
    walk_statements(&body(&checked, "below").statements, &mut |expr| {
        if let ExprKind::Call {
            callee: Callee::Candidate { .. },
            arguments,
            ..
        } = &expr.kind
        {
            assert_eq!(arguments.len(), 2);
            assert!(
                arguments
                    .iter()
                    .all(|arg| arg.value.ty == Type::Scalar(Scalar::I64))
            );
            assert!(matches!(
                arguments[0].value.kind,
                ExprKind::Raised {
                    raise: Raise::Return,
                    ..
                }
            ));
            assert!(matches!(
                arguments[1].value.kind,
                ExprKind::Literal(Value::Int(9007199254740993))
            ));
            comparisons += 1;
        }
    });
    assert_eq!(comparisons, 1);
}

/// TST-4 expects a boolean. A result or nullable operand supplies the
/// boolean, with a failure raised in the test (ERR-5, ERR-12, ERR-15).
#[test]
fn assertions_take_fallible_and_nullable_booleans() {
    let checked = module(
        r#"
const fn checked_bool(n: i64) -> result<bool, error> => ok(n > 0)
const fn maybe_bool(n: i64) -> bool? => n > 0
test asserts {
    assert checked_bool(1)
    assert maybe_bool(1)
}
"#,
    );
    assert!(checked.summary().is_empty(), "{}", checked.report);
    let statements = &checked.modules()[0].test_bodies[0]
        .as_ref()
        .unwrap()
        .statements;
    let [
        Stmt::Assert {
            condition: result, ..
        },
        Stmt::Assert {
            condition: nullable,
            ..
        },
    ] = statements.as_slice()
    else {
        panic!("expected two assertions");
    };
    assert!(matches!(
        result.kind,
        ExprKind::Raised {
            raise: Raise::Test,
            ..
        }
    ));
    assert!(matches!(
        nullable.kind,
        ExprKind::NullRead {
            raise: Raise::Test,
            ..
        }
    ));
}

/// GEN-2 leaves a bare result parameter open at a nullable place.
/// The inputs decide that parameter without an extra nullable read.
#[test]
fn generic_functions_keep_nullable_types_resolved_from_inputs() {
    let checked = module(
        r#"
const fn same<T>(a: T, b: T) -> T => a
const fn first<T>(values: list<T>) -> T => values[0]
const fn s(a: i64?, b: i64?) -> i64? => same(a, b)
const fn head(xs: list<i64?>) -> i64? => first(xs)
"#,
    );
    assert!(checked.summary().is_empty(), "{}", checked.report);
    for name in ["s", "head"] {
        let mut calls = 0;
        walk_statements(&body(&checked, name).statements, &mut |expr| {
            assert!(!matches!(expr.kind, ExprKind::NullRead { .. }));
            if let ExprKind::Call { callee, .. } = &expr.kind {
                let [Argument::Type(ty)] = callee.generics() else {
                    panic!("expected one inferred type argument");
                };
                assert_eq!(ty.to_string(), "i64?");
                calls += 1;
            }
        });
        assert_eq!(calls, 1, "{name}");
    }
}

/// ERR-6 and ERR-15: the dictionary supplies its nullable value type.
/// A plain place raises a failed lookup and reads a successful null.
#[test]
fn nullable_dictionary_values_decide_the_type_of_get() {
    let checked = module(
        r#"
const fn price(book: dict<str, i64?>, key: str) -> i64? => get(book, key)
const fn required(book: dict<str, i64?>, key: str) -> i64 => get(book, key)
"#,
    );
    assert!(checked.summary().is_empty(), "{}", checked.report);
    for (name, reads) in [("price", 0), ("required", 1)] {
        let mut observed = (0, 0);
        walk_statements(
            &body(&checked, name).statements,
            &mut |expr| match &expr.kind {
                ExprKind::Raised {
                    result,
                    raise: Raise::Execution,
                } => {
                    assert_eq!(result.ty.to_string(), "result<i64?, error>");
                    observed.0 += 1;
                }
                ExprKind::NullRead {
                    raise: Raise::Execution,
                    ..
                } => observed.1 += 1,
                _ => {}
            },
        );
        assert_eq!(observed, (1, reads), "{name}");
    }
}

/// ERR-12 and GEN-5 (hhenson/hgl_spec#182): an operator returning T?
/// fits a T place the same way as an ordinary function returning T?.
#[test]
fn a_selected_operator_can_return_a_nullable_value_to_a_plain_place() {
    let checked = module(
        r#"
operator maybe<X>(x: X) -> i64?
impl const fn maybe(x: i64) -> i64? => x
const fn read(x: i64) -> i64 => maybe(x)
"#,
    );
    assert!(checked.summary().is_empty(), "{}", checked.report);
    let mut reads = 0;
    walk_statements(&body(&checked, "read").statements, &mut |expr| {
        if let ExprKind::NullRead {
            value,
            raise: Raise::Execution,
            ..
        } = &expr.kind
        {
            assert!(matches!(
                value.kind,
                ExprKind::Call {
                    callee: Callee::Candidate { .. },
                    ..
                }
            ));
            reads += 1;
        }
    });
    assert_eq!(reads, 1);
}

/// EXPR-5 and ERR-5 give a result with another error type the T form
/// where its T is expected and the enclosing function admits that error.
/// ERR-15 gives the T? form to result<T, error>.
#[test]
fn a_custom_error_result_has_no_nullable_form() {
    let checked = module(
        r#"
const fn other(v: i64) -> result<i64, str> => ok(v)
const fn kept(v: i64) -> result<i64, str> {
    let x: i64? = other(v)
    return ok(0)
}
const fn elsewhere(v: i64) -> i64? => other(v)
const fn admitted(v: i64) -> result<i64, str> {
    let x: i64 = other(v)
    return ok(x)
}
"#,
    );
    let codes: Vec<_> = checked
        .findings
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect();
    assert_eq!(
        codes,
        ["type/type.mismatch", "type/type.mismatch"],
        "{}",
        checked.report
    );
    let mut raises = 0;
    walk_statements(&body(&checked, "admitted").statements, &mut |expr| {
        if matches!(
            expr.kind,
            ExprKind::Raised {
                raise: Raise::Return,
                ..
            }
        ) {
            raises += 1;
        }
    });
    assert_eq!(raises, 1);
}

/// FN-1 and UNIT-13: taking the value of a constant leaves it fixed even
/// when a result or nullable read is present (ERR-12 and ERR-15).
#[test]
fn result_forms_of_constants_are_still_fixed() {
    let checked = with_checked(
        r#"
const MAYBE: i64? = 1
const HELD: result<i64, error> = checked(1)
const fn needs(const k: i64) -> i64 => k
const fn from_nullable() -> i64 => needs(MAYBE)
const fn from_result() -> i64 => needs(HELD)
"#,
    );
    assert!(checked.summary().is_empty(), "{}", checked.report);
}

/// GEN-2, ERR-12 and ERR-15: a generic constructor infers its
/// field's type parameters from the form the field accepts.
#[test]
fn constructors_infer_from_the_field_payload() {
    let checked = module(
        r#"
struct Box<T> { items: list<T> }
const fn make(items: result<list<i64>, error>) -> i64 {
    let boxed = Box(items: items)
    return boxed.items[0]
}
"#,
    );
    assert!(checked.summary().is_empty(), "{}", checked.report);
    let mut constructs = 0;
    walk_statements(&body(&checked, "make").statements, &mut |expr| {
        if let ExprKind::Construct { arguments, .. } = &expr.kind {
            let Type::Named {
                arguments: generics,
                ..
            } = &expr.ty
            else {
                panic!("expected a constructed struct");
            };
            assert_eq!(generics, &[Argument::Type(Type::Scalar(Scalar::I64))]);
            assert!(matches!(
                arguments[0].value.kind,
                ExprKind::Raised {
                    raise: Raise::Execution,
                    ..
                }
            ));
            constructs += 1;
        }
    });
    assert_eq!(constructs, 1);
}
