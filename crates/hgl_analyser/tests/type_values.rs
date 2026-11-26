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

/// GEN-8, GEN-13, TYPE-18: local bindings, same-module calls and constructors,
/// written struct type applications, and module Type constants derived from
/// them are accepted as Type-valued generic arguments without evaluating the
/// operand or mutating local flow.
#[test]
fn late_type_queries_work_as_generic_value_arguments() {
    let checked = check(&[
        (
            "defs.hgl",
            r#"
module defs
export struct Quote { value: i64 }
export const fn one() -> i64 => 1
export const FROM_FN: Type = type(one())
export const FROM_STRUCT: Type = type(Quote(value: 1))
"#,
        ),
        (
            "m.hgl",
            r#"
module m
use defs::{FROM_FN, FROM_STRUCT}

struct Quote { value: i64 }
struct Tagged<const Kind: Type> { value: i64 }
struct Defaulted<const Kind: Type = type(one())> { value: i64 }
const INT_KIND: Type = type(1)
struct OnlyInt<const Kind: Type>
    requires Kind == INT_KIND
{
    value: i64
}

const fn one() -> i64 => 1
const LOCAL_FROM_FN: Type = type(one())
const LOCAL_FROM_STRUCT: Type = type(Quote(value: 1))
const fn selected<const Kind: Type>() -> Type => Kind
const fn wrap<T>(x: T) -> Type => selected<type(x)>()
const fn returns_tagged() -> Tagged<type(one())> => Tagged<type(one())>(value: 7)
const fn returns_defaulted() -> Defaulted => Defaulted(value: 9)
const fn uses_imported() -> Tagged<FROM_FN> => Tagged<type(1)>(value: 3)

test late_queries {
    let x = 1
    let local: i64 = 1
    var unassigned: i64
    let a = selected<type(x)>()
    let b = selected<type(local)>()
    let c = selected<type(unassigned)>()
    let d = selected<type(one())>()
    let e = selected<type(Quote(value: 1))>()
    let f = selected<LOCAL_FROM_FN>()
    let g = selected<LOCAL_FROM_STRUCT>()
    let h = selected<FROM_FN>()
    let i = selected<FROM_STRUCT>()
    let j = selected(Kind: type(x))
    let k: Tagged<type(x)> = Tagged<type(1)>(value: 7)
    let l: Tagged<type(one())>? = null
    let m = Tagged<type(x)>(value: 7)
    let n: Tagged<type(local)> = m
    let o = OnlyInt<type(x)>(value: 1)
    let p = wrap(x)
    let q = returns_tagged()
    let r = returns_defaulted()
    let s = uses_imported()
    unassigned = 2
    assert unassigned == 2
}
"#,
        ),
    ]);
    assert_eq!(
        checked.summary(),
        Vec::<String>::new(),
        "{}",
        checked.report
    );
}

/// TYPE-18, GEN-8: cold queries in a chain of written results have the
/// operand's type once each declaration they refer to has its signature.
#[test]
fn declaration_query_dependencies_resolve_to_a_fixed_point() {
    use std::fmt::Write;
    let mut program = String::from(
        "module m\nstruct Tagged<const K: Type> { n: i64 }\nconst fn base() -> str => \"x\"\n",
    );
    let mut previous = String::from("base");
    for index in 0..24 {
        let ty = format!("Tagged<type({previous}())>");
        let name = format!("f{index}");
        writeln!(program, "const fn {name}() -> {ty} => {ty}(n: {index})").unwrap();
        previous = name;
    }
    let checked = check(&[("m.hgl", &program)]);
    assert!(checked.findings.is_empty(), "{}", checked.report);

    let cycle = check(&[(
        "m.hgl",
        r#"module m
struct Token<const K: Type> { n: i64 }
const fn a() -> Token<type(b())> => Token<type(b())>(n: 1)
const fn b() -> Token<type(a())> => Token<type(a())>(n: 2)
"#,
    )]);
    assert!(!cycle.findings.is_empty());
}

/// TYPE-18 and GEN-13: a field and its initializer agree when both describe
/// the same symbolic type after substitution into the surrounding generic.
#[test]
fn symbolic_field_tag_agrees_with_constructor_argument() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
struct Token<const K: Type> { n: i64 }
const fn empty<T>() -> list<T> => []
struct Frame<T> {
    element: T
    token: Token<type(empty<T>())>
}
const fn make<T>(x: T) -> Frame<T> =>
    Frame<T>(element: x, token: Token<type(empty<T>())>(n: 2))
test concrete { assert make(1).token.n == 2 }
"#,
    )]);
    assert!(checked.findings.is_empty(), "{}", checked.report);
}

/// TYPE-18, GEN-8: a bare result that depends on overload selection in
/// a generic template must remain a Type through all three argument forms.
#[test]
fn reflected_generic_values_wait_for_the_concrete_callback_result() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
operator inspect<A, R>(value: A) -> R
impl const fn inspect(value: bool) -> i64 => 1
impl const fn inspect(value: str) -> str => value
const fn selected<const K: Type>() -> Type => K
const fn infer<T>(x: T) -> tuple<Type, Type, Type> requires T in {bool, str} =>
    (selected<type(inspect(x))>(), selected<(type(inspect(x)))>(), selected(K: type(inspect(x))))
test both {
    assert infer(true) == (type(1), type(1), type(1))
    assert infer("s") == (type("s"), type("s"), type("s"))
}
"#,
    )]);
    assert!(checked.findings.is_empty(), "{}", checked.report);
}

/// TYPE-18, GEN-13: distinct Type arguments on a generic struct remain
/// distinct types, and struct `requires` clauses are enforced once deferred
/// queries resolve.
#[test]
fn late_type_valued_struct_arguments_enforce_identity_and_requires() {
    let checked = check(&[(
        "m.hgl",
        r#"
module m
struct Tagged<const Kind: Type> { value: i64 }
const INT_KIND: Type = type(1)
struct OnlyInt<const Kind: Type>
    requires Kind == INT_KIND
{
    value: i64
}
const fn one() -> i64 => 1
const fn mismatch_and_requires() {
    let x = 1
    let y = 1.0
    let wrong: Tagged<type(y)> = Tagged<type(x)>(value: 7)
    let unmet = OnlyInt<type(y)>(value: 7)
    let ok_val: Tagged<type(one())> = Tagged<type(x)>(value: 7)
}
"#,
    )]);
    assert_eq!(checked.findings.len(), 2, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "type/type.mismatch");
    assert_eq!(checked.findings[1].code, "constraint/requires.unmet");
}

/// TYPE-18, EXPR-3: a cold `type(u)` generic argument never assigns or reads
/// an unassigned local; a later ordinary read still rejects.
#[test]
fn late_type_query_generic_argument_preserves_unassigned_flow() {
    let checked = check(&[(
        "m.hgl",
        r#"
module m
struct Tagged<const Kind: Type> { value: i64 }
const fn selected<const Kind: Type>() -> Type => Kind
const fn still_unassigned() -> i64 {
    var u: i64
    let a = selected<type(u)>()
    let b = Tagged<type({
        u = 1
        u
    })>(value: 7)
    return u
}
"#,
    )]);
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "name/binding.unassigned");
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

/// TYPE-18 and GEN-8: ill-typed operands inside `type(...)` generic arguments
/// preserve their operand diagnostics instead of collapsing into
/// `generic.argument_constant`, in calls, structs, and signatures.
#[test]
fn ill_typed_generic_type_queries_preserve_operand_diagnostics() {
    let checked = check(&[(
        "m.hgl",
        r#"
module m
struct Tagged<const Kind: Type> { value: i64 }
const fn selected<const K: Type>() -> Type => K
const fn wrong_tuple() -> Type => selected<type((1, 1 + true))>()
const fn wrong_call() -> Type => selected<type(1 + true)>()
const fn wrong_named() -> Type => selected(K: type(1 + true))
const fn wrong_struct() -> Tagged<type(1 + true)> => Tagged<type(1 + true)>(value: 7)
test valid_tuple { let known = selected<type((1, true))>() }
"#,
    )]);
    assert_eq!(checked.findings.len(), 5, "{}", checked.report);
    assert!(
        checked
            .findings
            .iter()
            .all(|finding| finding.code == "operator/operator.no_candidate"),
        "{}",
        checked.report
    );
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
            .unwrap()
    );
    assert!(
        checked
            .program
            .holds_type_value(&named("Box", meta()))
            .unwrap()
    );
    assert!(
        !checked
            .program
            .holds_type_value(&named(
                "Box",
                vec![hgl_analyser::Argument::Type(Type::Scalar(Scalar::I64))],
            ))
            .unwrap()
    );
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

/// TYPE-18 and LEX-10: Type remains an ordinary value/member spelling,
/// while the same spelling in an annotation is the descriptor type.
#[test]
fn contextual_type_name_preserves_values_members_and_annotations() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
const fn Type(Type: i64) -> i64 => Type + 1
struct Record { Type: Type }
enum Kind { Type }
test contextual_names {
    let answer = Type(41)
    let Type = answer
    let record = Record(Type: type(1))
    let descriptor: Type = type(Type)
    assert Type == 42
    assert record.Type == descriptor
    assert Kind.Type == Kind.Type
}
"#,
    )]);
    assert_eq!(checked.summary(), Vec::<String>::new());
}

/// GEN-14 and TYPE-18: the target slot, rather than a value named Type,
/// decides whether this spelling supplies the descriptor type or a size.
#[test]
fn contextual_type_arguments_follow_known_parameter_kinds() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
const Type: i64 = 2
struct Box<T> { value: T }
const fn sizes<const Type: i64>(value: Box<Type>, items: list<i64, Type>) -> Type => type(value.value)
const fn count<const N: i64>() -> i64 => N
test known_kinds {
    let value: Box<Type> = Box<Type>(value: type(1))
    assert sizes<2>(value, [1, 2]) == type(type(1))
    assert count<Type>() == 2
}
"#,
    )]);
    assert_eq!(checked.summary(), Vec::<String>::new());
}

/// GEN-14: a call-supplied kind first uses ordinary value lookup, including
/// imported constants; with no such value it supplies the descriptor type.
#[test]
fn contextual_type_arguments_infer_value_before_descriptor_kind() {
    use hgl_analyser::Argument;
    use hgl_analyser::hir::{Callee, walk_statements};

    for (prefix, extra, expected) in [
        ("const Type: i64 = 7\n", None, Some(7)),
        (
            "use other::{Type}\n",
            Some(("other.hgl", "module other\nexport const Type: i64 = 11\n")),
            Some(11),
        ),
        ("", None, None),
    ] {
        let source = format!(
            "module m\n{prefix}const fn unused<X>() -> i64 => 1\n\
             const fn caller() -> i64 => unused<Type>()\n"
        );
        let mut files = vec![("m.hgl", source.as_str())];
        files.extend(extra);
        let checked = check(&files);
        assert_eq!(checked.summary(), Vec::<String>::new(), "{source}");
        let mut found = None;
        walk_statements(&common::body(&checked, "caller").statements, &mut |value| {
            if let ExprKind::Call {
                callee: Callee::Function { generics, .. },
                ..
            } = &value.kind
            {
                found = Some(generics.clone());
            }
        });
        let generics = found.expect("caller retains the generic call");
        match expected {
            Some(expected) => assert!(
                matches!(generics.as_slice(), [Argument::Constant(value)]
                    if matches!(value.value, ConstantValue::Int(given) if given == expected)),
                "{source}\n{generics:?}"
            ),
            None => assert!(
                matches!(generics.as_slice(), [Argument::Type(Type::TypeValue)]),
                "{source}\n{generics:?}"
            ),
        }
    }
}

/// TYPE-18: declarations that introduce a type reject at their reserved name,
/// rather than turning Type into a global keyword that blocks values/members.
#[test]
fn contextual_type_name_rejects_type_declarations_at_the_name() {
    for declaration in [
        "struct Type { value: i64 }",
        "enum Type { Item }",
        "native type Type",
        "const fn packed<...Type>(values: ...Type) {}",
    ] {
        let source = format!("module m\n{declaration}\n");
        let checked = check(&[("m.hgl", &source)]);
        let at = format!("m.hgl:2:{}", declaration.find("Type").unwrap() + 1);
        assert!(
            checked.findings.iter().any(|finding| {
                finding.code == "name/name.reserved"
                    && finding.rule == "TYPE-18"
                    && finding.at == at
            }),
            "{source}\n{}",
            checked.report
        );
    }
}

/// GEN-13 and TYPE-18: an unmarked Type parameter may settle as a value,
/// including at a making call; a type instance rejects at the declaration.
#[test]
fn contextual_type_parameter_reservation_follows_its_kind() {
    let accepted = check(&[(
        "m.hgl",
        r#"module m
struct Count<Type> { items: list<i64, Type> }
const fn sized<Type>(items: list<i64, Type>) -> i64 => Type
const fn unused<Type>() -> i64 => 1
test values {
    let count = Count<2>(items: [1, 2])
    assert sized<2>([1, 2]) == 2
    assert unused<2>() == 1
}
"#,
    )]);
    assert_eq!(accepted.summary(), Vec::<String>::new());
    for argument in ["i64", "Type"] {
        let source = format!(
            "module m\nconst fn unused<Type>() -> i64 => 1\n\
             test type_instance {{ assert unused<{argument}>() == 1 }}\n"
        );
        let rejected = check(&[("m.hgl", &source)]);
        assert!(
            rejected.findings.iter().any(|finding| {
                finding.code == "name/name.reserved"
                    && finding.rule == "TYPE-18"
                    && finding.at == "m.hgl:2:17"
            }),
            "{source}\n{}",
            rejected.report
        );
    }
}

/// GEN-14 and FN-14: a descriptor argument ignores a same-spelled value's
/// capture restriction; choosing the value keeps ordinary lookup diagnostics.
#[test]
fn contextual_type_slots_keep_value_only_lookup_diagnostics_separate() {
    let accepted = check(&[(
        "m.hgl",
        r#"module m
struct Box<T> { value: T }
const fn outer(var Type: i64) -> Type {
    let query = fn() -> Type => type(Box<Type>(value: type(1)))
    return query()
}
"#,
    )]);
    assert_eq!(accepted.summary(), Vec::<String>::new());
    let rejected = check(&[(
        "m.hgl",
        r#"module m
const fn count<const N: i64>() -> i64 => N
const fn outer(var Type: i64) -> i64 => (fn() -> i64 => count<Type>())()
"#,
    )]);
    assert!(
        rejected
            .findings
            .iter()
            .any(|finding| { finding.code == "name/name.captured" && finding.rule == "FN-14" }),
        "{}",
        rejected.report
    );
    let callable = check(&[(
        "m.hgl",
        "module m\nconst fn Type() -> i64 => 1\n\
         const fn unused<X>() -> i64 => 1\n\
         const fn caller() -> i64 => unused<Type>()\n",
    )]);
    assert!(
        callable.findings.iter().any(|finding| {
            finding.code == "type/generic.argument_constant" && finding.rule == "GEN-8"
        }),
        "{}",
        callable.report
    );
}

/// GEN-14: the descriptor is a type, never an implicit value for a known
/// const slot. Ordinary integer constants named Type still fill that slot.
#[test]
fn contextual_type_descriptor_rejects_known_value_slots() {
    for use_site in [
        "const fn take(value: Sized<Type>) {}",
        "test t { let value: Sized<Type>? = null\n assert value == null }",
        "test t { let value = Sized<Type>(value: 1) }",
    ] {
        let source = format!("module m\nstruct Sized<const N: i64> {{ value: i64 }}\n{use_site}\n");
        let checked = check(&[("m.hgl", &source)]);
        assert!(
            checked.findings.iter().any(|finding| {
                finding.code == "type/generic.argument_kind" && finding.rule == "GEN-14"
            }),
            "{source}\n{}",
            checked.report
        );
    }
    let accepted = check(&[(
        "m.hgl",
        "module m\nconst Type: i64 = 2\nstruct Sized<const N: i64> { value: i64 }\n\
         const fn take(value: Sized<Type>) -> i64 => value.value\n\
         test t { let value: Sized<Type> = Sized<Type>(value: 1)\n\
         let absent: Sized<Type>? = null\n assert take(value) == 1\n assert absent == null }\n",
    )]);
    assert_eq!(accepted.summary(), Vec::<String>::new());
}

/// FMT-5 and TYPE-18: enum identity stays qualified, while diagnostic
/// spelling uses the enum's declared name, including inside compound values.
#[test]
fn canonical_type_text_separates_enum_spelling_from_identity() {
    let checked = check(&[(
        "m.hgl",
        "module m\nenum Mode { on, off }\n\
         struct Moded<const M: Mode> { value: i64 }\n\
         struct ModeList<const M: list<Mode>> { value: i64 }\n\
         const fn observe(first: Moded<Mode.on>, second: ModeList<[Mode.on]>) {}\n",
    )]);
    assert_eq!(checked.summary(), Vec::<String>::new());
    let module = checked.program.module("m").unwrap();
    let declaration = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == "observe")
        .unwrap();
    let hgl_analyser::Detail::Callable(signature) = &module.details[declaration] else {
        panic!("observe has a signature");
    };
    for (parameter, expected) in signature
        .parameters
        .iter()
        .zip(["Moded<Mode.on>", "ModeList<[Mode.on]>"])
    {
        let before = parameter.ty.identity();
        assert_eq!(parameter.ty.canonical_spelling(), expected);
        assert_eq!(parameter.ty.identity(), before);
        assert!(before.unwrap().contains("m::Mode.on"));
    }
}

/// TYPE-18 and GEN-7: query results use the instance-selected candidate,
/// and the cold operand makes no runtime call in that instance's body.
#[test]
fn a_generic_type_query_uses_the_instance_selected_result() {
    use hgl_analyser::hir::{Callee, walk_statements};
    let checked = check(&[(
        "m.hgl",
        r#"module m
operator items<T, C>(value: T) -> C
impl const fn items<T>(value: T) -> list<i64, 2> => [1, 2]
impl const fn items(value: i64) -> list<i64, 3> => [1, 2, 3]
const fn kind<T>(value: T) -> Type => type(items(value))
const fn concrete() -> Type => kind(1)
const fn generic() -> Type => kind(true)
"#,
    )]);
    assert_eq!(checked.summary(), Vec::<String>::new());
    for (caller, expected) in [("concrete", "list<i64, 3>"), ("generic", "list<i64, 2>")] {
        let mut called = None;
        walk_statements(&common::body(&checked, caller).statements, &mut |value| {
            if let ExprKind::Call {
                callee:
                    Callee::Function {
                        module,
                        declaration,
                        generics,
                        requirements,
                        ..
                    },
                ..
            } = &value.kind
            {
                called = Some((
                    *module,
                    *declaration,
                    generics.clone(),
                    requirements.clone(),
                ));
            }
        });
        let (module, declaration, generics, requirements) = called.unwrap();
        let (_, body) = checked
            .program
            .instance(module, declaration, &generics, &requirements)
            .unwrap();
        let mut queried = Vec::new();
        let mut calls = 0;
        walk_statements(&body.statements, &mut |value| match &value.kind {
            ExprKind::Literal(Value::Type(ty)) => queried.push(ty.canonical_spelling()),
            ExprKind::Call { .. } => calls += 1,
            _ => {}
        });
        assert_eq!(queried, [expected]);
        assert_eq!(calls, 0);
    }
}

/// TYPE-18 still checks an unevaluated operand after GEN-7 selection changes
/// its result. A query does not hide an invalid operation on that result.
#[test]
fn an_instance_type_query_keeps_selected_operand_diagnostics() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
operator items<T, C>(value: T) -> C
impl const fn items<T>(value: T) -> list<i64, 2> => [1, 2]
impl const fn items(value: bool) -> set<i64> => [3]
const fn kind<T>(value: T) -> Type => type(items(value)[0])
const fn concrete() -> Type => kind(true)
"#,
    )]);
    assert!(
        checked
            .findings
            .iter()
            .any(|finding| { finding.code == "type/type.mismatch" && finding.rule == "EXPR-7" }),
        "{}",
        checked.report
    );
    assert!(checked.report.contains("kind<bool>"), "{}", checked.report);
}

/// TYPE-18: retyping the cold operand makes no recursive body instance.
#[test]
fn recursive_calls_inside_type_queries_remain_cold() {
    let checked = check(&[(
        "m.hgl",
        "module m\nconst fn grow<T>(value: T) -> Type => type(grow((value, value)))\n\
         const fn concrete() -> Type => grow(1)\n",
    )]);
    assert_eq!(checked.summary(), Vec::<String>::new());
}

/// GEN-14, TYPE-18: a callable's declared type/pack slot chooses the
/// descriptor spelling even when the ordinary value Type is in scope.
#[test]
fn callable_type_slots_choose_the_descriptor_over_a_value_name() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
const Type: i64 = 3
const fn same<T>(value: T) -> T => value
operator identity<T>(value: T) -> T
impl const fn identity<T>(value: T) -> T => value
const fn packed<...Ts>(value: tuple<Ts>) -> tuple<Ts> => value
const fn count<const N: i64>() -> i64 => N
const fn unsettled<N>() -> i64 => 1
test obs {
    assert same<Type>(type(1)) == type(1)
    assert identity<Type>(type(1)) == type(1)
    assert packed<Type, Type>((type(1), type(true)))[0] == type(1)
    assert count<Type>() == 3
    assert unsettled<Type>() == 1
}
"#,
    )]);
    assert_eq!(checked.summary(), Vec::<String>::new());
}

/// GEN-14: a known callable value slot cannot treat the descriptor as a
/// value. A constant named Type is covered separately by the positive case.
#[test]
fn callable_value_slots_reject_an_unbound_descriptor() {
    for declaration in [
        "const fn count<const N: i64>() -> i64 => N",
        "operator count<const N: i64>() -> i64\nimpl const fn count<const N: i64>() -> i64 => N",
    ] {
        let source = format!("module m\n{declaration}\ntest obs {{ assert count<Type>() == 1 }}\n");
        let checked = check(&[("m.hgl", &source)]);
        assert!(
            checked
                .findings
                .iter()
                .any(|finding| finding.code == "type/generic.argument_kind"
                    && finding.rule == "GEN-14"),
            "{}",
            checked.report
        );
    }
}

/// TYPE-18, UNIT-7: an omitted default selects under the making call's
/// generic arguments, with no operand call or formal parameter in its HIR.
#[test]
fn omitted_type_query_defaults_use_the_concrete_instance() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
operator sample<T, R>(x: T) -> R
impl const fn sample<T>(x: T) -> list<str, 2> => ["a", "b"]
impl const fn sample(x: list<str>) -> tuple<bool, bool> => (true, false)
const fn make<T>() -> list<T> => []
const fn original<T>(x: T, tag: Type = type(sample(make<T>()))) -> Type => tag
"#,
    )]);
    assert_eq!(checked.summary(), Vec::<String>::new());
    let module = checked.program.module("m").unwrap();
    let declaration = module
        .declarations
        .iter()
        .position(|decl| decl.name == "original")
        .unwrap();
    let hgl_analyser::Detail::Callable(signature) = &module.details[declaration] else {
        panic!("original has a signature");
    };
    let default = module.defaults[declaration][1].as_ref().unwrap();
    for (argument, expected) in [
        (Scalar::Bool, "list<str, 2>"),
        (Scalar::Str, "tuple<bool, bool>"),
    ] {
        let mut diagnostics = hgl_diagnostics::Diagnostics::new();
        let specialized = checked.program.specialize_parameter_default(
            default,
            (
                checked
                    .program
                    .modules
                    .iter()
                    .position(|unit| std::ptr::eq(unit, module))
                    .unwrap(),
                declaration,
            ),
            &[hgl_analyser::Argument::Type(Type::Scalar(argument))],
            &[],
            &signature.parameters[1],
            &mut diagnostics,
        );
        assert!(!diagnostics.has_errors(), "{:?}", diagnostics.as_slice());
        assert!(
            matches!(&specialized.value.kind, ExprKind::Literal(Value::Type(ty)) if ty.canonical_spelling() == expected),
            "{specialized:?}"
        );
        assert!(specialized.locals.is_empty());
    }
}

/// TYPE-18 checks a cold default's operand after instance selection,
/// just as it checks the same query in a function body (EXPR-7).
#[test]
fn omitted_type_query_defaults_keep_selected_operand_errors() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
operator items<T, R>(value: T) -> R
impl const fn items<T>(value: T) -> list<i64> => [1]
impl const fn items(value: list<str>) -> set<i64> => [2]
const fn make<T>() -> list<T> => []
const fn original<T>(value: T, tag: Type = type(items(make<T>())[0])) -> Type => tag
const fn concrete() -> Type => original("one")
"#,
    )]);
    assert!(
        checked
            .findings
            .iter()
            .any(|finding| finding.code == "type/type.mismatch" && finding.rule == "EXPR-7"),
        "{}",
        checked.report
    );
}

/// GEN-14, FN-4/6: the selected role supplies its own generic kinds,
/// independently of which declaration of a pair is written first.
#[test]
fn contextual_type_arguments_follow_the_selected_pair_role() {
    for reverse in [false, true] {
        for inverse in [false, true] {
            let definitions = if inverse {
                [
                    "const fn pick<const N: i64>(tag: Type, v: i64) -> i64 => N",
                    "fn pick<T>(const tag: T, v: i64) -> i64 => 11",
                ]
            } else {
                [
                    "const fn pick<T>(v: T) -> i64 => 11",
                    "fn pick<const N: i64>(v: i64) -> i64 => N",
                ]
            };
            let definitions = if reverse {
                [definitions[1], definitions[0]]
            } else {
                definitions
            };
            let uses = if inverse {
                "fn flow(v: i64) -> i64 => pick<Type>(type(1), v)\ntest obs { assert pick<Type>(type(1), 7) == 3\n assert eval flow(v: [7]) == [11] }"
            } else {
                "fn flow(v: i64) -> i64 => pick<Type>(v)\ntest obs { assert pick<Type>(type(1)) == 11\n assert eval flow(v: [7]) == [3] }"
            };
            let source = format!(
                "module m\nconst Type: i64 = 3\n{}\n{}\n{uses}\n",
                definitions[0], definitions[1]
            );
            let checked = check(&[("m.hgl", &source)]);
            assert_eq!(
                checked.summary(),
                Vec::<String>::new(),
                "{source}\n{}",
                checked.report
            );
        }
    }
}

/// TYPE-18, GEN-10, UNIT-7: a default receives the same concrete type
/// pack as a body query, and different pack instances stay independent.
#[test]
fn omitted_query_defaults_keep_the_bound_type_pack() {
    use hgl_analyser::hir::{Callee, walk_statements};
    let checked = check(&[(
        "m.hgl",
        r#"module m
const fn make<T>() -> list<T> => []
const fn inspect<...Ts>(v: tuple<Ts>, tag: Type = type(make<tuple<Ts>>())) -> str => format("{}", tag)
const fn pair() -> str => inspect((1, true))
const fn single() -> str => inspect(("one",))
"#,
    )]);
    assert_eq!(checked.summary(), Vec::<String>::new());
    for (caller, expected) in [
        ("pair", "list<tuple<i64, bool>>"),
        ("single", "list<tuple<str>>"),
    ] {
        let mut callee = None;
        walk_statements(&common::body(&checked, caller).statements, &mut |value| {
            if let ExprKind::Call {
                callee: call @ Callee::Function { .. },
                ..
            } = &value.kind
            {
                callee = Some(call.clone());
            }
        });
        let callee = callee.unwrap();
        let (module, declaration) = callee.declaration().unwrap();
        let hgl_analyser::Detail::Callable(signature) =
            &checked.program.modules[module].details[declaration]
        else {
            unreachable!()
        };
        let mut diagnostics = hgl_diagnostics::Diagnostics::new();
        let specialized = checked.program.specialize_parameter_default(
            checked.program.modules[module].defaults[declaration][1]
                .as_ref()
                .unwrap(),
            (module, declaration),
            callee.generics(),
            callee.type_packs(),
            &signature.parameters[1],
            &mut diagnostics,
        );
        assert!(!diagnostics.has_errors(), "{:?}", diagnostics.as_slice());
        assert!(
            matches!(&specialized.value.kind, ExprKind::Literal(Value::Type(ty)) if ty.canonical_spelling() == expected),
            "{specialized:?}"
        );
        assert!(specialized.locals.is_empty());
    }
}

/// GEN-14/GEN-8: a callable named Type is ignored in the pair's type
/// slot and still rejects as a nonconstant in the selected value slot.
#[test]
fn paired_type_slots_separate_value_lookup_diagnostics() {
    for reverse in [false, true] {
        let declarations = [
            "const fn pick<T>(v: T) -> i64 => 11",
            "fn pick<const N: i64>(v: i64) -> i64 => N",
        ];
        let source = format!(
            "module m\nconst fn Type() -> i64 => 3\n{}\n{}\ntest obs {{ assert pick<Type>(type(1)) == 11 }}\n",
            declarations[usize::from(reverse)],
            declarations[usize::from(!reverse)]
        );
        let checked = check(&[("m.hgl", &source)]);
        assert_eq!(
            checked.summary(),
            Vec::<String>::new(),
            "{}",
            checked.report
        );
        let rejected = check(&[(
            "m.hgl",
            &format!("{source}\nfn flow(v: i64) -> i64 => pick<Type>(v)\n"),
        )]);
        assert!(
            rejected
                .findings
                .iter()
                .any(|finding| finding.code == "type/generic.argument_constant"
                    && finding.rule == "GEN-8"),
            "{}",
            rejected.report
        );
    }
}

/// TYPE-16/18, GEN-17: generic keys are retained until a struct application
/// selects a member. Forwarded packs and arithmetic keep the same selection.
#[test]
fn storage_queries_resolve_generic_pack_keys_at_the_application() {
    let checked = check(&[(
        "m.hgl",
        r#"module m
struct Pick<const key: i64, ...Ts> { value: Ts[key] }
struct Forward<const key: i64, ...Ts> { value: Pick<key, Ts> }
struct Offset<const key: i64, ...Ts> { value: Pick<key + 1, Ts> }
struct Empty<...Ts> { value: i64 }
struct Recursive<const key: i64, ...Ts> { value: Ts[key]
 next: atomic<list<Recursive<1, Ts>>> }
const ZERO: i64 = 0
struct Tagged<const tag: i64, ...Ts{1}> { value: Ts[ZERO]
 next: atomic<list<Tagged<tag + 1, Ts>>> }
"#,
    )]);
    assert_eq!(
        checked.summary(),
        Vec::<String>::new(),
        "{}",
        checked.report
    );
    let module = checked.program.module_index("m").unwrap();
    let named = |name: &str, arguments| Type::Named {
        name: name.to_string(),
        module,
        declaration: checked.program.modules[module]
            .declarations
            .iter()
            .position(|decl| decl.name == name)
            .unwrap(),
        arguments,
        kind: NamedKind::Struct,
    };
    let applied = |name: &str, key: i64, first, second| {
        named(
            name,
            vec![
                hgl_analyser::Argument::Constant(Constant {
                    text: key.to_string(),
                    value: ConstantValue::Int(key),
                }),
                hgl_analyser::Argument::Type(first),
                hgl_analyser::Argument::Type(second),
            ],
        )
    };
    for (name, key, expected) in [
        ("Pick", 0, false),
        ("Pick", 1, true),
        ("Forward", 0, false),
        ("Forward", 1, true),
        ("Offset", -1, false),
        ("Offset", 0, true),
        ("Recursive", 0, true),
        ("Tagged", 0, false),
    ] {
        let ty = applied(name, key, Type::Scalar(Scalar::I64), Type::TypeValue);
        assert_eq!(
            checked.program.holds_type_value(&ty),
            Ok(expected),
            "{ty:?}"
        );
    }
    let empty = named("Empty", vec![hgl_analyser::Argument::Type(Type::TypeValue)]);
    assert_eq!(
        checked
            .program
            .holds_type_value(&applied("Pick", 0, empty, Type::TypeValue)),
        Ok(false)
    );
}

/// TYPE-16/18, Proposed: a selected Type rejects as a delta operand;
/// an unselected Type argument does not make the stored ordinary field Type.
#[test]
fn indexed_storage_rejects_only_the_type_containing_deltas() {
    let definitions = "module m\nstruct Pick<const key: i64, ...Ts> { value: Ts[key] }\nstruct Forward<const key: i64, ...Ts> { value: Pick<key, Ts> }\n";
    for ty in [
        "Pick<0, Type>",
        "Pick<1, i64, Type>",
        "Forward<1, i64, Type>",
    ] {
        let checked = check(&[(
            "m.hgl",
            &format!("{definitions}\nconst fn bad(value: delta<{ty}>) {{}}\n"),
        )]);
        assert!(
            checked
                .findings
                .iter()
                .any(|finding| finding.code == "shape/delta.unsupported_shape"
                    && finding.rule == "TYPE-16"),
            "{ty}\n{}",
            checked.report
        );
    }
    let good = check(&[(
        "m.hgl",
        &format!("{definitions}\nconst fn good(value: delta<Forward<0, i64, Type>>) {{}}\n"),
    )]);
    assert_eq!(good.summary(), Vec::<String>::new(), "{}", good.report);
}

/// GEN-7/13: a generic value key must also trigger the existing making-
/// instance delta walk, even when no type parameter appears in the operand.
#[test]
fn making_instances_check_generic_value_storage_keys() {
    for key in [0, 1] {
        let definitions = "module m\nstruct Pick<const key: i64, ...Ts> { value: Ts[key] }\n";
        for source in [
            format!(
                "{definitions}const fn later<const key: i64>() {{ let value: delta<Pick<key, i64, Type>>? = null }}\nconst fn call() {{ later<{key}>() }}\n"
            ),
            format!(
                "{definitions}struct Holder<const key: i64> {{ value: delta<Pick<key, i64, Type>> }}\nconst fn consume(value: Holder<{key}>) {{}}\n"
            ),
        ] {
            let checked = check(&[("m.hgl", &source)]);
            if key == 0 {
                assert_eq!(
                    checked.summary(),
                    Vec::<String>::new(),
                    "{}",
                    checked.report
                );
            } else {
                assert!(
                    checked
                        .findings
                        .iter()
                        .any(|finding| finding.code == "shape/delta.unsupported_shape"),
                    "{}",
                    checked.report
                );
            }
        }
    }
}

/// GEN-13: a changing storage application is bounded by the existing
/// instance budget; reaching it is an error, not a false absence of Type.
#[test]
fn indexed_storage_reports_a_bound_instead_of_silently_dropping_a_dependency() {
    let checked = check(&[(
        "m.hgl",
        "module m\nstruct Grow<const key: i64, ...Ts> { value: Ts[key]
 next: atomic<list<Grow<key + 1, Ts>>> }\n",
    )]);
    assert_eq!(
        checked.summary(),
        Vec::<String>::new(),
        "{}",
        checked.report
    );
    let module = checked.program.module_index("m").unwrap();
    let ty = Type::Named {
        name: "Grow".to_string(),
        module,
        declaration: checked.program.modules[module]
            .declarations
            .iter()
            .position(|decl| decl.name == "Grow")
            .unwrap(),
        arguments: vec![
            hgl_analyser::Argument::Constant(Constant {
                text: "1".to_string(),
                value: ConstantValue::Int(1),
            }),
            hgl_analyser::Argument::Type(Type::TypeValue),
            hgl_analyser::Argument::Type(Type::Scalar(Scalar::I64)),
        ],
        kind: NamedKind::Struct,
    };
    assert_eq!(
        checked.program.holds_type_value(&ty),
        Err(hgl_analyser::TypeStorageLimit)
    );
    let rejected = check(&[(
        "m.hgl",
        "module m\nstruct Grow<const key: i64, ...Ts> { value: Ts[key]\n next: atomic<list<Grow<key + 1, Ts>>> }\nconst fn slow(value: delta<Grow<1, Type, i64>>) {}\n",
    )]);
    assert!(
        rejected
            .findings
            .iter()
            .any(|finding| finding.code == "type/generic.instance_limit"
                && finding.rule == "GEN-13"
                && finding.message.contains("checking stored generic types")),
        "{}",
        rejected.report
    );
}

/// GEN-7 and TYPE-18: overload selection can determine a Type argument in
/// a generic declaration after its caller supplies the type arguments.
#[test]
fn generic_signature_and_fields_use_the_selected_operand_type() {
    let declarations = r#"
module m
struct Tag<const K: Type> { n: i64 }
const fn values<T>() -> list<T> => []
operator probe<I, O>(input: I) -> O
impl const fn probe(input: list<i64>) -> bool => true
impl const fn probe(input: list<str>) -> str => ""
abstract struct Parent<T> requires T in {i64, str} {
    value: T
    tag: Tag<type(probe(values<T>()))>
}
struct Child<T> : Parent<T> {}
const fn make<T>(x: T) -> Tag<(type(probe(values<T>())))> requires T in {i64, str} =>
    Tag<type(probe(values<T>()))>(n: 7)
const fn take<T>(x: T, tag: Tag<type(probe(values<T>()))>) -> i64
    requires T in {i64, str} => tag.n
"#;
    let source = format!(
        "{declarations}
test identities {{
    let a: Tag<type(true)> = make(3)
    let b: Tag<type(\"\")> = make(\"s\")
    let child = Child<i64>(value: 4, tag: a)
    assert take(3, child.tag) + take(\"s\", b) == 14
}}"
    );
    let good = check(&[("m.hgl", &source)]);
    assert!(good.findings.is_empty(), "{}", good.report);
    let source = source.replace("take(3, child.tag)", "take(3, b)");
    let bad = check(&[("m.hgl", &source)]);
    assert_eq!(bad.findings.len(), 1, "{}", bad.report);
    assert_eq!(bad.findings[0].code, "type/type.mismatch");
}

/// A declaration cycle that happens to pass a generic argument cannot supply
/// a Type identity. Only an operator waiting for selection is deferred.
#[test]
fn generic_declaration_cycles_are_not_deferred_to_a_making_call() {
    let checked = check(&[(
        "m.hgl",
        r#"
module m
struct Tag<const K: Type> { value: i64 }
const fn cyclic<T>() -> Tag<type(cyclic<T>())> =>
    Tag<type(cyclic<T>())>(value: 2)
"#,
    )]);
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "type/generic.argument_constant");
}

/// TYPE-18, GEN-2: a generic body can inherit a deferred fit through a
/// field without writing type(...) itself. Decide each fit at its making call.
#[test]
fn field_tag_fits_are_checked_at_concrete_calls() {
    let definitions = r#"module m
const INTEGER: Type = type(1)
const fn values<T>() -> list<T> => []
operator select<A,R>(value: A) -> R
impl const fn select<A>(value: A) -> i64 => 1
impl const fn select(value: list<str>) -> bool => true
struct Stamp<const Kind: Type> { id: i64 }
struct Frame<T> { item: T
    tag: Stamp<type(select(values<T>()))> }
const fn read(stamp: Stamp<INTEGER>) -> i64 => stamp.id
const fn returned<T>(f: Frame<T>) -> Stamp<INTEGER> => f.tag
const fn assigned<T>(f: Frame<T>) -> i64 {
    var stamp: Stamp<INTEGER> = Stamp<INTEGER>(id: 0)
    stamp = f.tag
    return stamp.id
}
const fn passed<T>(f: Frame<T>) -> i64 => read(f.tag)
const fn forwarded<T>(f: Frame<T>) -> i64 => returned(f).id
"#;
    for (item, tag, valid) in [("2", "1", true), (r#""s""#, "true", false)] {
        let source = format!(
            "{definitions}
test fits {{
    let f = Frame(item: {item}, tag: Stamp<type({tag})>(id: 17))
    assert returned(f).id == 17
    assert assigned(f) == 17
    assert passed(f) == 17
    assert forwarded(f) == 17
}}
"
        );
        let checked = check(&[("m.hgl", &source)]);
        if valid {
            assert_eq!(
                checked.summary(),
                Vec::<String>::new(),
                "{}",
                checked.report
            );
        } else {
            assert_eq!(checked.findings.len(), 4, "{}", checked.report);
            assert!(
                checked
                    .findings
                    .iter()
                    .all(|d| d.code == "type/type.mismatch"),
                "{}",
                checked.report
            );
        }
    }
}

/// UNIT-8, TYPE-18: when a generic call omits a parameter, either the
/// supplied default or its receiving type can depend on a selected field tag.
#[test]
fn omitted_defaults_fit_the_concrete_field_identity_on_both_sides() {
    let definitions = r#"module m
const INTEGER: Type = type(1)
const fn rows<T>() -> list<T> => []
operator pick<A,R>(value: A) -> R
impl const fn pick<A>(value: A) -> i64 => 1
impl const fn pick(value: list<str>) -> bool => true
struct Stamp<const Kind: Type> { id: i64 }
struct Frame<T> { item: T?
    tag: Stamp<type(pick(rows<T>()))> }
const fn make<T>() -> Frame<T> => Frame<T>(tag: Stamp<type(pick(rows<T>()))>(id: 17))
const fn read<T>(x: T, v: Stamp<INTEGER> = make<T>().tag) -> i64 => v.id
const fn place<T>(x: T, v: Stamp<type(pick(rows<T>()))> = Stamp<INTEGER>(id: 17)) -> i64 => v.id
const fn forward<T>(x: T) -> i64 => read(x)
const fn fixed(stamp: Stamp<INTEGER>) -> i64 => stamp.id
const fn invoke(f: const fn(i64) -> i64) -> i64 => f(1)
const fn nested<T>(x: T, v: i64 = invoke(fn(n: i64) => fixed(make<T>().tag))) -> i64 => v
"#;
    for (call, valid) in [
        ("forward(2)", true),
        (r#"forward("s")"#, false),
        ("nested(2)", true),
        (r#"nested("s")"#, false),
        (r#"nested("s", 17)"#, true),
        ("place(2)", true),
        (r#"place("s")"#, false),
        (r#"read("s", Stamp<INTEGER>(id: 17))"#, true),
        (r#"place("s", Stamp<type(true)>(id: 17))"#, true),
        (r#"read("s", make<str>().tag)"#, false),
    ] {
        let source = format!("{definitions}\ntest use_it {{ assert {call} == 17 }}\n");
        let checked = check(&[("m.hgl", &source)]);
        if valid {
            assert_eq!(
                checked.summary(),
                Vec::<String>::new(),
                "{call}\n{}",
                checked.report
            );
        } else {
            assert_eq!(checked.findings.len(), 1, "{call}\n{}", checked.report);
            assert_eq!(checked.findings[0].code, "type/type.mismatch");
        }
    }
}

/// GEN-6, UNIT-8, TYPE-18: copying the operator's default to an
/// implementation does not turn its generic helper into a concrete root.
#[test]
fn inherited_defaults_follow_only_the_selected_operator_call() {
    let definitions = r#"module m
const INTEGER: Type = type(1)
const fn rows<T>() -> list<T> => []
operator pick<A,R>(value: A) -> R
impl const fn pick<A>(value: A) -> i64 => 1
impl const fn pick(value: list<str>) -> bool => true
struct Stamp<const Kind: Type> { id: i64 }
struct Frame<T> { item: T?
    tag: Stamp<type(pick(rows<T>()))> }
const fn make<T>() -> Frame<T> => Frame<T>(tag: Stamp<type(pick(rows<T>()))>(id: 17))
operator configured<T>{v: Stamp<INTEGER> = make<T>().tag}(x: T) -> i64
impl const fn configured{v: Stamp<INTEGER>}(x: i64) -> i64 => v.id
impl const fn configured{v: Stamp<INTEGER>}(x: str) -> i64 => v.id
"#;
    for (call, valid) in [
        ("17", true),
        ("configured(1)", true),
        (r#"configured{v: Stamp<INTEGER>(id: 17)}("s")"#, true),
        (r#"configured("s")"#, false),
    ] {
        let source = format!("{definitions}\ntest use_it {{ assert {call} == 17 }}\n");
        let checked = check(&[("m.hgl", &source)]);
        if valid {
            assert_eq!(
                checked.summary(),
                Vec::<String>::new(),
                "{call}\n{}",
                checked.report
            );
        } else {
            assert_eq!(checked.findings.len(), 1, "{call}\n{}", checked.report);
            assert_eq!(checked.findings[0].code, "type/type.mismatch");
        }
    }
}

/// GEN-3, TYPE-18: concrete outer arguments reach requirements on nested
/// struct fields and parents, including tags determined by a cold overload.
#[test]
fn nested_requirements_receive_the_concrete_type_value() {
    let library = r#"module library
export const INTEGER: Type = type(1)
export const PAIR: tuple<Type, Type> = (INTEGER, type(true))
export const fn rows<T>() -> list<T> => []
export operator select<A,R>(value: A) -> R
impl const fn select<A>(value: A) -> i64 => 1
impl const fn select(value: list<str>) -> bool => true
export abstract struct Base<const K: Type> requires K == INTEGER { id: i64? }
export struct Only<const K: Type> requires K == INTEGER { id: i64? }
export struct Pair<const K: tuple<Type, Type>> requires K == PAIR { id: i64? }
"#;
    for (declaration, required) in [
        (
            "struct Outer<T> { item:T?\n nested:lib::Only<type(lib::rows<T>()[0])>? }",
            "Only",
        ),
        (
            "struct Outer<T> : lib::Base<type(lib::select(lib::rows<T>()))> { item:T? }",
            "Base",
        ),
        (
            "struct Relay<const K:Type> { nested:lib::Only<K>? }\nstruct Outer<T> { item:T?\n nested:Relay<type(lib::select(lib::rows<T>()))>? }",
            "Only",
        ),
        (
            "struct Outer<T> { item:T?\n nested:lib::Pair<(type(lib::select(lib::rows<T>())), type(true))>? }",
            "Pair",
        ),
    ] {
        for (argument, valid) in [("i64", true), ("str", false)] {
            let source = format!(
                "module m\nuse library as lib\n{declaration}\ntest make {{ let x = Outer<{argument}>() }}\n"
            );
            let checked = check(&[("library.hgl", library), ("m.hgl", &source)]);
            if valid {
                assert_eq!(
                    checked.summary(),
                    Vec::<String>::new(),
                    "{}",
                    checked.report
                );
            } else {
                assert_eq!(checked.findings.len(), 1, "{}", checked.report);
                assert_eq!(checked.findings[0].code, "constraint/requires.unmet");
                assert!(
                    checked
                        .report
                        .contains(&format!("the `requires` of `{required}`")),
                    "{}",
                    checked.report
                );
            }
        }
    }
}

#[test]
fn requirements_follow_constants_inside_a_nominal_type_descriptor() {
    let definitions = r#"module m
struct Tagged<const Pair: tuple<Type, i64>> { n: i64 }
const WANTED: Type = type(Tagged<(type(1), 5)>(n: 0))
const fn rows<T>() -> list<T> => []
operator select<A, R>(value: A) -> R
impl const fn select<A>(value: A) -> i64 => 1
impl const fn select(value: list<str>) -> bool => true
struct Only<const Key: Type> requires Key == WANTED { n: i64? }
struct Route<T, const N: i64> {
    anchor: T?
    nested: Only<type(Tagged<(type(select(rows<T>())), N*2+1)>(n: 0))>?
}
"#;
    for (arguments, valid) in [("i64, 2", true), ("str, 2", false), ("i64, 3", false)] {
        let source = format!("{definitions}test make {{ let x = Route<{arguments}>() }}\n");
        let checked = check(&[("m.hgl", &source)]);
        if valid {
            assert!(checked.findings.is_empty(), "{}", checked.report);
        } else {
            assert_eq!(checked.findings.len(), 1, "{}", checked.report);
            assert_eq!(checked.findings[0].code, "constraint/requires.unmet");
            assert!(checked.report.contains("the `requires` of `Only`"));
        }
    }
}

#[test]
fn a_body_checks_descriptor_requirements_with_its_selected_cold_result() {
    let definitions = r#"module m
struct Tag<const N: i64> { n: i64 }
struct Stamp<const K: Type> { n: i64 }
operator read<A, R>(value: A) -> R
impl const fn read<A>(value: A) -> i64 => 1
impl const fn read(value: Tag<7>) -> bool => true
const fn fixed<A>(value: A) -> i64 => 1
const REQUIRED: Type = type(Stamp<type(true)>(n: 0))
struct Only<const K: Type> requires K == REQUIRED { n: i64? }
const fn place<const N: i64>(arg: Tag<N>) -> i64 {
    let alias = arg
    let checked = Only<type(Stamp<type(read(alias))>(n: 0))>()
    return 1
}
"#;
    for (operand, initializer) in [
        ("read(alias)", "let alias = arg"),
        (
            "alias",
            "let selected = read(arg)\n    let alias = selected",
        ),
    ] {
        let definitions = definitions
            .replace("let alias = arg", initializer)
            .replace("type(read(alias))", &format!("type({operand})"));
        for (call, valid) in [
            ("", true),
            ("test fits { assert place<7>(Tag<7>(n: 9)) == 1 }", true),
            ("test fails { assert place<3>(Tag<3>(n: 9)) == 1 }", false),
        ] {
            let source = format!("{definitions}{call}\n");
            let checked = check(&[("m.hgl", &source)]);
            if valid {
                assert!(checked.findings.is_empty(), "{}", checked.report);
            } else {
                assert_eq!(checked.findings.len(), 1, "{}", checked.report);
                assert_eq!(checked.findings[0].code, "constraint/requires.unmet");
                assert!(
                    checked
                        .report
                        .contains("this call makes the instance `place<3>`")
                );
            }
        }
    }
    // Both results have a fixed i64 type, even in a declaration with no callers.
    for initializer in [
        "let selected = read(arg)\n    let alias = fixed(selected)",
        "let selected = read(arg)\n    let alias = { let ignored = selected\n 1 }",
    ] {
        let source = definitions
            .replace("let alias = arg", initializer)
            .replace("type(read(alias))", "type(alias)");
        let checked = check(&[("m.hgl", &source)]);
        assert_eq!(checked.findings.len(), 1, "{}", checked.report);
        assert_eq!(checked.findings[0].code, "constraint/requires.unmet");
    }
}
