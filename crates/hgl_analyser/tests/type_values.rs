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
struct Plain { value: i64 }
const fn scalar(value: delta<Type>) {}
const fn structure(value: delta<Descriptor>) {}
const fn generic(value: delta<Box<Type>>) {}
const fn bad_list(value: delta<list<Type>>) {}
const fn bad_tuple(value: delta<tuple<i64, Type>>) {}
const fn nullable(value: delta<Type?>) {}
const fn bad_set(value: delta<set<Type>>) {}
const fn dictionary(value: delta<dict<Type, i64>>) {}
const fn atomic_meta(value: delta<atomic<Type>>) {}
const fn result_meta(value: delta<result<Type, error>>) {}
const fn window_meta(value: delta<rolling_window<Type, 2>>) {}
const fn atomic_struct(value: delta<atomic<Descriptor>>) {}
const fn result_struct(value: delta<result<Descriptor, error>>) {}
const fn atomic_list(value: delta<atomic<list<Type>>>) {}
const fn atomic_generic(value: delta<atomic<Box<Type>>>) {}
const fn unrelated(value: tuple<Type, delta<i64>>) {}
const fn ordinary_atomic(value: delta<atomic<i64>>) {}
const fn ordinary_result(value: delta<result<i64, error>>) {}
const fn ordinary_window(value: delta<rolling_window<i64, 2>>) {}
const fn ordinary_struct(value: delta<atomic<Plain>>) {}
"#,
    )]);
    assert_eq!(checked.findings.len(), 15, "{}", checked.report);
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

/// TYPE-18 and FN-3: unevaluated operands keep their enclosing call role.
#[test]
fn a_temporal_call_inside_a_value_type_query_rejects_once() {
    let checked = check(&[(
        "m.hgl",
        r#"
module m
fn temporal(value: i64) -> i64 => value
const fn kind() -> Type => type(temporal(1))
"#,
    )]);
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "function-kind/call.role");
    assert_eq!(checked.findings[0].at, "m.hgl:4:33");
}

/// TYPE-18: a graph query has the connection's payload type and retains no call.
#[test]
fn a_graph_type_query_retains_only_the_temporal_calls_payload_type() {
    let checked = check(&[(
        "m.hgl",
        r#"
module m
fn temporal(value: i64) -> i64 => value
fn kind() -> bool => type(temporal(1)) == type(1)
"#,
    )]);
    assert_eq!(checked.summary(), Vec::<String>::new());
    let module = checked.program.module("m").unwrap();
    let index = module
        .declarations
        .iter()
        .position(|d| d.name == "kind")
        .unwrap();
    let body = module.bodies[index].as_ref().unwrap();
    let mut found_payload = false;
    hgl_analyser::hir::walk_statements(&body.statements, &mut |value| {
        if let ExprKind::Literal(Value::Type(ty)) = &value.kind {
            found_payload |= **ty == Type::Scalar(Scalar::I64);
        }
        if let ExprKind::Call { callee, .. } = &value.kind
            && let Some((module, declaration)) = callee.declaration()
        {
            assert_ne!(
                checked.program.modules[module].declarations[declaration].name,
                "temporal"
            );
        }
    });
    assert!(found_payload);
}

/// TYPE-18 and GEN-8: the early query probe cannot turn an ill-typed
/// tuple into a Type constant merely because its outer tuple type is known.
#[test]
fn an_ill_typed_early_query_is_not_a_type_constant() {
    let checked = check(&[(
        "m.hgl",
        r#"
module m
const fn selected<const K: Type>() -> Type => K
const fn wrong_operand() -> Type => selected<type((1, 1 + true))>()
test valid_tuple { let known = selected<type((1, true))>() }
"#,
    )]);
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "type/generic.argument_constant");
    assert!(checked.findings[0].message.contains("`K`"));
}

/// TYPE-16/18: constructor scans preserve the same original delta operand;
/// a normal collection delta constructor remains admitted.
#[test]
fn a_normalized_type_containing_delta_constructor_is_reported() {
    let checked = check(&[(
        "m.hgl",
        r#"
module m
const fn bad() { let value = delta<atomic<Type>>() }
const fn good() { let value = delta<list<i64>>() }
"#,
    )]);
    assert!(
        checked
            .findings
            .iter()
            .any(|f| f.code == "shape/delta.unsupported_shape")
    );
    assert!(
        checked
            .findings
            .iter()
            .all(|f| f.at.starts_with("m.hgl:3:")),
        "{}",
        checked.report
    );
}

/// TYPE-18, Proposed: storage through a generic field is recognized even
/// after a negative query, without any field directly declared as Type.
#[test]
fn cached_storage_keeps_generic_only_type_fields() {
    let checked = check(&[(
        "m.hgl",
        "module m\nstruct Plain { value: i64 }\n\
         struct Box<T> { value: T }\n",
    )]);
    assert_eq!(checked.summary(), Vec::<String>::new());
    let module = checked.program.module_index("m").unwrap();
    let named = |name: &str, arguments| Type::Named {
        name: name.to_string(),
        module,
        declaration: checked.program.modules[module]
            .declarations
            .iter()
            .position(|declaration| declaration.name == name)
            .unwrap(),
        arguments,
        kind: NamedKind::Struct,
    };
    let meta = || vec![hgl_analyser::Argument::Type(Type::TypeValue)];
    assert!(
        !checked
            .program
            .holds_type_value(&named("Plain", Vec::new()))
    );
    assert!(checked.program.holds_type_value(&named("Box", meta())));
    assert!(!checked.program.holds_type_value(&named(
        "Box",
        vec![hgl_analyser::Argument::Type(Type::Scalar(Scalar::I64))],
    )));
}

/// TYPE-18: the query uses a var's declared type without requiring a value,
/// and leaves ordinary definite-assignment checks intact (EXPR-3).
#[test]
fn cold_type_queries_allow_declared_unassigned_locals() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
const fn value_query() -> Type {
    var count: i64
    let known = type(count)
    count = 1
    return known
}
fn graph_query(value: i64) -> i64 {
    var count: i64
    let known = type(count)
    count = value
    return count
}
test nested_query {
    let known = type({ var count: i64
        count })
    assert known == type(1)
}
"#,
    )]);
    assert_eq!(checked.summary(), Vec::<String>::new());
    let rejected = check(&[(
        "m.hgl",
        "module m\ntest read { var count: i64\n let known = type(count)\n let value = count }\n",
    )]);
    assert_eq!(rejected.findings.len(), 1, "{}", rejected.report);
    assert_eq!(rejected.findings[0].code, "name/binding.unassigned");
}

/// TYPE-16/18, Proposed, and GEN-7: saved delta operands are held to the
/// prohibition after a making call or struct application supplies the type.
#[test]
fn generic_delta_operands_reject_type_containing_instances() {
    let cases = [
        "const fn keep<T>(value: delta<result<T, error>>) -> bool => true\n\
         test bad { assert keep<Type>(ok(type(1))) }\n",
        "fn temporal<T>(value: delta<atomic<T>>) -> bool => true\n\
         fn graph() -> bool => temporal<Type>(type(1))\n",
        "const fn keep<T>(value: delta<atomic<T>>) -> bool => true\n\
         test bad { assert keep<Type>(type(1)) }\n",
        "const fn keep<T>(value: delta<atomic<T>>) -> bool => true\n\
         const fn forward<T>(value: T) -> bool => keep<T>(value)\n\
         test bad { assert forward<Type>(type(1)) }\n",
        "struct Holder<T> { value: delta<T>? }\n\
         test bad { let held = Holder<Type>() }\n",
        "struct Holder<T> { value: delta<T>? }\n\
         const fn bad(value: Holder<Type>) {}\n",
        "const fn local_delta<T>() -> i64 { let d: delta<T>? = null\n return 1 }\n\
         test bad { assert local_delta<Type>() == 1 }\n",
    ];
    for source in cases {
        let checked = check(&[("m.hgl", &format!("module m\n{source}"))]);
        assert!(
            checked.findings.iter().any(|finding| {
                finding.code == "shape/delta.unsupported_shape" && finding.rule == "TYPE-16"
            }),
            "{source}\n{}",
            checked.report
        );
    }
    let ordinary = check(&[(
        "m.hgl",
        "module m\nconst fn keep<T>(value: delta<atomic<T>>) -> bool => true\n\
         struct Holder<T> { value: delta<T>? }\n\
         test good { assert keep<i64>(1)\n let held = Holder<i64>() }\n",
    )]);
    assert_eq!(ordinary.summary(), Vec::<String>::new());
}
