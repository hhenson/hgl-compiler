//! The declaration and name-resolution boundary of FN-8 and GEN-17.

mod common;

use common::module;
use hgl_analyser::{Argument, Binding, ConstantValue, Detail, LocalKind, Type};

#[test]
fn a_named_pack_of_function_parameters_and_call_exclusions_are_bound() {
    // GEN-17 and FN-8: fixed inputs come first; the reflected pack and
    // the independent argument pack bind to different generic parameters.
    // An exclusion may supply a single name or a set.
    let checked = module(
        r#"operator pass<R, ...Ps, ...As, const key: str = "key", const given: set<str> = []>(
    f: fn(fixed: i64, ...{Ps}{1:*}) -> R,
    args: ...{As for f without key, given},
) -> R"#,
    );
    assert!(checked.findings.is_empty(), "{}", checked.report);
    let module = checked.program.module("m").unwrap();
    let Detail::Callable(signature) = &module.details[0] else {
        panic!("expected a callable");
    };
    let Type::Function(template) = &signature.parameters[0].ty else {
        panic!("expected a function type");
    };
    assert_eq!(template.parameters.len(), 1);
    assert_eq!(template.parameters[0].name(), Some("fixed"));
    let reflected = template.named_pack.as_ref().expect("a reflected pack");
    assert_eq!(reflected.at, signature.generics[1].at);
    assert_eq!(reflected.bounds, (1, None));
    let call_pack = &signature.parameters[1];
    let dependent = &module.dependent_packs[&call_pack.at];
    assert_eq!(dependent.function, signature.parameters[0].at);
    assert_eq!(
        dependent.without,
        [signature.generics[3].at, signature.generics[4].at]
    );
    assert!(matches!(call_pack.ty, Type::Parameter { at, .. } if at == signature.generics[2].at));
}

#[test]
fn a_pack_member_type_keeps_string_and_position_keys() {
    // GEN-17: names are str and positions are i64. The grammar uses a
    // name for either key. A generic argument with this shape is a type
    // when its base resolves to a type pack.
    let checked = module(
        r#"const name_key: str = "price"
const position_key: i64 = 0
struct Holder<T> { value: T }
operator by_key<...Ps>(f: fn(...{Ps})) -> Holder<Ps[name_key]>
operator by_position<...Ps>(f: fn(...{Ps})) -> list<Ps[position_key]>"#,
    );
    assert!(checked.findings.is_empty(), "{}", checked.report);
    let module = checked.program.module("m").unwrap();
    let Detail::Callable(by_key) = &module.details[3] else {
        panic!("expected a callable");
    };
    let Some(Type::Named { arguments, .. }) = &by_key.result else {
        panic!("expected an applied struct");
    };
    assert!(matches!(
        arguments.as_slice(),
        [Argument::Type(Type::PackMember { pack, key, .. })]
            if *pack == by_key.generics[0].at && matches!(&key.value, ConstantValue::Str(name) if name == "price")
    ));
    let Detail::Callable(by_position) = &module.details[4] else {
        panic!("expected a callable");
    };
    let Some(Type::List { element, .. }) = &by_position.result else {
        panic!("expected a list");
    };
    assert!(matches!(
        element.as_ref(),
        Type::PackMember { pack, key, .. }
            if *pack == by_position.generics[0].at && matches!(key.value, ConstantValue::Int(0))
    ));
}

/// FN-19, GEN-17 and ERR-9: the written result already rejects this
/// making call. Its entry read adds no second report, and the report
/// uses the key supplied by the call.
#[test]
fn a_missing_pack_key_in_a_result_and_body_is_reported_once() {
    let checked = module(concat!(
        "const fn fetch<const key: str, ...As>(args: ...{As}) -> As[key] => args[key]\n",
        "test missing { assert fetch<\"z\">(given: 1) == 1 }\n",
    ));
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "type/pack.name");
    assert_eq!(
        checked.findings[0].message,
        "pack `As` has no member under `\"z\"`"
    );
}

/// FN-19 and GEN-17: a hook's keyed entry reports the concrete position
/// when the making call specializes its generic key.
#[test]
fn a_missing_positional_pack_key_reports_its_value() {
    let checked = module(concat!(
        "fn read<const k: i64>(items: ...i64) -> i64 { when { return items[k] } }\n",
        "fn graph(a: i64, b: i64) -> i64 => read<5>(a, b)\n",
    ));
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "type/pack.name");
    assert_eq!(
        checked.findings[0].message,
        "pack `items` has no member under `5`"
    );
}

/// GEN-17: a module constant keeps its written name in the diagnostic
/// even when the compiler knows the value that failed the lookup.
#[test]
fn a_missing_module_pack_key_reports_its_name() {
    let checked = module(concat!(
        "const absent: str = \"z\"\n",
        "const fn fetch<...As>(args: ...{As}) -> As[absent] => args[absent]\n",
        "test missing { assert fetch(given: 1) == 1 }\n",
    ));
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "type/pack.name");
    assert_eq!(
        checked.findings[0].message,
        "pack `As` has no member under `absent`"
    );
}

#[test]
fn key_membership_reads_the_bound_key_as_a_value() {
    // GEN-17: key membership refers to the compile-time key that each binds.
    let checked = module(
        "operator present<...Ps, ...As>(f: fn(...{Ps}), args: ...{As for f})
    requires each key in arg_keys(Ps) { key in arg_keys(As) }",
    );
    assert!(checked.findings.is_empty(), "{}", checked.report);
    let module = checked.program.module("m").unwrap();
    assert!(module.uses.iter().any(|used| used.name == "key"
        && matches!(
            used.binding,
            Binding::Local {
                kind: LocalKind::PackKey,
                ..
            }
        )));
}

#[test]
fn each_binds_a_key_and_member_operands_keep_their_pack_bindings() {
    // GEN-17: `arg_keys` is a reflection form, and `each` binds the
    // compile-time key shared by the two member operands.
    let checked = module(
        "operator pass<R, ...Ps, ...As>(f: fn(...{Ps}) -> R, args: ...{As for f}) -> R
    requires each key in arg_keys(As) { As[key] == Ps[key] }",
    );
    assert!(checked.findings.is_empty(), "{}", checked.report);
    let module = checked.program.module("m").unwrap();
    assert!(
        !module
            .prelude_names
            .iter()
            .any(|name| name.name == "arg_keys")
    );
    let keys: Vec<_> = module
        .uses
        .iter()
        .filter(|used| used.name == "key")
        .collect();
    let [first, second] = keys.as_slice() else {
        panic!("expected two uses of the same key");
    };
    assert_eq!(first.binding, second.binding);
    assert!(matches!(
        first.binding,
        Binding::Local {
            kind: LocalKind::PackKey,
            ..
        }
    ));
    let written: Vec<_> = module
        .written
        .iter()
        .filter_map(|written| match &written.ty {
            Type::PackMember { name, key, .. } => Some((name.as_str(), &key.value)),
            _ => None,
        })
        .collect();
    assert!(matches!(
        written.as_slice(),
        [("As", ConstantValue::Parameter { at: left, .. }), ("Ps", ConstantValue::Parameter { at: right, .. })]
            if left == right
    ));
}

#[test]
fn duplicate_packs_are_reported_at_the_second_pack_of_each_kind() {
    // FN-8: both kinds are allowed together, but a second of either kind
    // would leave the argument matcher unable to place arguments.
    let checked = module(
        "operator positional(first: ...i64, second: ...i64)
operator named<...As, ...Bs>(first: ...{As}, second: ...{Bs})
operator both<...As>(first: ...i64, second: ...{As})",
    );
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:36 type/pack.duplicate FN-8",
            "m.hgl:3:46 type/pack.duplicate FN-8",
        ],
        "{}",
        checked.report
    );
}

#[test]
fn dependent_pack_names_have_the_kinds_their_places_require() {
    // FN-8: `for` needs a parameter taking a function. `without` needs
    // a generic value parameter of type str or set<str>.
    let checked = module(
        "operator bad_target<...As>(f: i64, args: ...{As for f})
operator bad_key<...Ps, ...As, const key: i64 = 0>(f: fn(...{Ps}), args: ...{As for f without key})",
    );
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:53 type/type.mismatch FN-8",
            "m.hgl:3:95 type/type.mismatch FN-8",
        ],
        "{}",
        checked.report
    );
}
