//! Ordinary Type values, cold queries and constant identity (TYPE-18, GEN-13).

mod common;

use common::check;
use hgl_analyser::hir::{ExprKind, Value};
use hgl_analyser::{Constant, ConstantValue, NamedKind, Scalar, Type};

/// TYPE-18: querying an expression keeps only its type, including in
/// a module constant, a generic instance, and a generic value argument.
#[test]
fn type_queries_are_typed_constants_without_operand_calls() {
    let checked = check(&[(
        "m.hgl",
        r#"
module m
const fn explosive() -> i64 => 1 // 0
const KIND: Type = type(explosive())
const fn kind<T>(value: T) -> Type => type(value)
const fn selected<const K: Type>() -> Type => K
test queries {
    let value = kind(1)
    let given = selected<type(1)>()
    assert type(1) == type(2)
    assert type(1) != type(1.0)
    assert KIND == type(1)
}
"#,
    )]);
    assert_eq!(checked.summary(), Vec::<String>::new());
    let module = checked.program.module("m").unwrap();
    let index = module
        .declarations
        .iter()
        .position(|d| d.name == "KIND")
        .unwrap();
    let constant = module.constants[index].as_ref().unwrap();
    assert!(matches!(&constant.value.kind,
        ExprKind::Literal(Value::Type(ty)) if **ty == Type::Scalar(Scalar::I64)));
}

/// TYPE-18: an input's type is available without reading its payload,
/// including at start and in a when that admits no input.
#[test]
fn a_type_query_reads_no_input_payload() {
    let checked = check(&[(
        "m.hgl",
        r#"
module m
fn probe(value: i64, tick: bool) -> bool {
    state known: Type = type(value)
    start { let at_start = type(value) }
    when valid([]) { return known == type(value) }
}

fn graph(value: i64) -> bool => type(value) == type(1)
"#,
    )]);
    assert_eq!(checked.summary(), Vec::<String>::new());
}

/// TYPE-18, GEN-13: identity uses the declared type and canonical
/// arguments; display names and constant spellings are not identity.
#[test]
fn type_equality_and_keys_use_one_identity() {
    let named = |name: &str, module| Type::Named {
        name: name.to_string(),
        module,
        declaration: 0,
        arguments: Vec::new(),
        kind: NamedKind::Struct,
    };
    let value = |ty| ConstantValue::Type(Box::new(ty));
    let first = value(named("Record", 0));
    let alias = value(named("Alias", 0));
    let other = value(named("Record", 1));
    assert_eq!(first.equals(&alias), Some(true));
    assert_eq!(first.key(), alias.key());
    assert_eq!(first.equals(&other), Some(false));
    assert_ne!(first.key(), other.key());

    let list = |text: &str| Type::List {
        element: Box::new(Type::Scalar(Scalar::I64)),
        size: Some(Constant {
            text: text.to_string(),
            value: ConstantValue::Int(2),
        }),
    };
    let first = value(list("2"));
    let computed = value(list("1 + 1"));
    assert_eq!(first.equals(&computed), Some(true));
    assert_eq!(first.key(), computed.key());
    assert_eq!(list("2").canonical_spelling(), "list<i64, 2>");
    assert_eq!(list("1 + 1").canonical_spelling(), "list<i64, 2>");
}

/// TYPE-16 and TYPE-18, Proposed: an ordinary Type-containing value is
/// allowed, but none of these shapes has a delta.
#[test]
fn a_type_containing_value_has_no_delta() {
    let checked = check(&[(
        "m.hgl",
        r#"
module m
struct Descriptor { tag: Type }
struct Box<T> { value: T }
const fn scalar(value: delta<Type>) {}
const fn structure(value: delta<Descriptor>) {}
const fn generic(value: delta<Box<Type>>) {}
const fn bad_list(value: delta<list<Type>>) {}
const fn bad_tuple(value: delta<tuple<i64, Type>>) {}
const fn nullable(value: delta<Type?>) {}
const fn bad_set(value: delta<set<Type>>) {}
const fn dictionary(value: delta<dict<Type, i64>>) {}
"#,
    )]);
    assert_eq!(checked.findings.len(), 8, "{}", checked.report);
    assert!(
        checked
            .findings
            .iter()
            .all(|f| f.code == "shape/delta.unsupported_shape" && f.rule == "TYPE-16")
    );
}

/// GEN-8, TYPE-18: resolving the type of an inferred local as a generic
/// value argument is deferred to #871; it rejects cleanly in this slice.
#[test]
fn a_local_type_as_a_generic_value_argument_is_reported() {
    let checked = check(&[(
        "m.hgl",
        r#"
module m
const fn selected<const Kind: Type>() -> Type => Kind
test local_type {
    let x = 1
    let given = selected<type(x)>()
}
"#,
    )]);
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "type/generic.argument_constant");
    assert!(checked.findings[0].message.contains("Kind"));
}

/// TYPE-18 and GEN-2: Type is a builtin value type with no parameters.
#[test]
fn type_takes_no_generic_arguments() {
    let checked = check(&[("m.hgl", "module m\nconst fn bad(value: Type<i64>) {}\n")]);
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "type/generic.argument_count");
    assert!(checked.findings[0].message.contains("`Type`"));
}

/// TYPE-18: a discarded assignment does not set an outer graph var's category.
#[test]
fn a_type_query_does_not_assign_an_existing_local() {
    let checked = check(&[(
        "m.hgl",
        r#"
module m
fn graph(value: i64) -> i64 {
    var x: i64
    let kind = type({
        x = value
        x
    })
    x = 1
    return x
}
"#,
    )]);
    assert_eq!(checked.summary(), Vec::<String>::new());
}

/// GEN-13 and TYPE-18: requires reflection retains its existing source
/// diagnostic; the independent body query remains a Type value.
#[test]
fn a_requires_type_reflection_keeps_its_existing_diagnostic() {
    let checked = check(&[(
        "m.hgl",
        r#"
module m
const fn reflected<const N: i64>() -> Type
    requires type(N) in {i64, f64} => type(N)
test query { assert reflected<1>() == type(1) }
"#,
    )]);
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "name/name.unknown");
    assert!(checked.findings[0].message.contains("`type`"));
}
