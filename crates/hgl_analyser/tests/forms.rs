//! The analyser against the rules on the form of a declaration: what
//! the grammar admits and a rule rejects. Each test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{body, check, module};
use hgl_analyser::Argument;
use hgl_analyser::hir::{Callee, ExprKind, PackKey, walk_statements};

/// No lines.
const NONE: [&str; 0] = [];

/// FN-18: functions and native declarations take their configuration
/// by name in braces. EXPR-10 also lets a brace name an ordinary or
/// generic value parameter. UNIT-8 allows any order in the braces.
#[test]
fn configuration_parameters_bind_in_their_braces() {
    let checked = module(concat!(
        "const fn mix<const scale: i64 = 2>{offset: i64 = 1, base: i64}(value: i64) -> i64 => scale * value + base + offset\n",
        "native const fn native_mix{const increment_by: i64}(value: i64) -> i64\n",
        "const fn bare() -> i64 => mix{base: 1, value: 2, scale: 3}()\n",
        "const fn typed() -> i64 => mix<4>{base: 5}(6)\n",
        "const fn native_call() -> i64 => native_mix{increment_by: 2}(7)\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
}

/// FN-18: a required configuration parameter must be supplied by name
/// in braces. EXPR-2 still detects duplicate ordinary arguments.
#[test]
fn configuration_parameters_are_neither_positional_nor_parenthesized() {
    let checked = module(concat!(
        "const fn configured{a: i64}(b: i64 = 10) -> i64 => a + b\n",
        "const fn two(x: i64, y: i64 = 5) -> i64 => x + y\n",
        "const fn missing() -> i64 => configured(2)\n",
        "const fn by_position() -> i64 => configured(1, 2)\n",
        "const fn by_name() -> i64 => configured(a: 1)\n",
        "const fn duplicate() -> i64 => two{x: 1}(2)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:30 type/argument.missing FN-18",
            "m.hgl:5:34 type/argument.missing FN-18",
            "m.hgl:5:48 type/argument.unexpected EXPR-2",
            "m.hgl:6:30 type/argument.missing FN-18",
            "m.hgl:6:41 name/argument.unknown FN-18",
            "m.hgl:7:36 name/argument.duplicate EXPR-2",
        ],
        "{}",
        checked.report
    );
}

/// EXPR-10 preserves the positional argument rules of intrinsic forms.
#[test]
fn positional_intrinsics_reject_named_configuration() {
    let checked = module(concat!(
        "const fn tagged() -> result<i64, error> => ok{ignored: 9}(2)\n",
        "fn selected(value: i64) -> i64 {\n",
        "    when {\n",
        "        if (valid{ignored: 9}(value)) { return 1 }\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:47 name/argument.unknown EXPR-10",
            "m.hgl:5:19 name/argument.unknown EXPR-10",
        ],
        "{}",
        checked.report
    );
}

/// FN-18 forbids parameter modes and name collisions in configuration.
/// UNIT-8 checks the ordering of omittable parameters in parentheses.
#[test]
fn configuration_declarations_reject_forbidden_modes_and_names() {
    let checked = module(concat!(
        "const fn marked{ref low: i64, var rising: i64, items: ...i64, op: const fn(i64) -> i64}() {}\n",
        "const fn clash{both: i64}(both: i64) -> i64 => both\n",
        "const fn generic<const size: i64>{size: i64}(value: i64) -> i64 => value\n",
        "const fn order{maybe: i64 = 2}(x: i64 = 1, y: i64) -> i64 => x + y\n",
        "const fn guarded{const threshold: i64}(value: i64) -> i64 requires threshold == 0 => value\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:17 local:function-kind/parameter.configuration FN-18",
            "m.hgl:2:31 local:function-kind/parameter.configuration FN-18",
            "m.hgl:2:55 local:function-kind/parameter.configuration FN-18",
            "m.hgl:2:67 local:function-kind/parameter.configuration FN-18",
            "m.hgl:3:27 name/name.duplicate UNIT-5",
            "m.hgl:4:35 name/name.duplicate UNIT-5",
            "m.hgl:5:44 type/default.order UNIT-8",
            "m.hgl:6:68 type/type.not_a_type UNIT-10",
        ],
        "{}",
        checked.report
    );
}

/// FN-8: "`...T` captures the positional ones ... `...{Fields}` captures
/// the named ones, each under its source name, where `Fields` is a
/// declared type pack."
#[test]
fn pack_parameters_are_accepted_in_their_forms() {
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "fn same<T>(first: T, rest: ...T{1:*}) {}\n",
            "fn each<...Ts>(rest: ...Ts) {}\n",
            "fn named<...Fields>(const size: i64, fields: ...{Fields}{2}) {}\n",
            // UNIT-8: a `const` parameter may have a default; a temporal
            // one may not.
            "fn both<T, ...Fields>(const first: T = seed(), rest: ...T, fields: ...{Fields}) {}\n",
            "operator join<...Ts{2:8}>(parts: ...Ts) -> str\n",
            "native fn collect<T>(items: ...T) -> list<T>\n",
            "const fn seed() -> i64 => 1\n",
        ),
    )]);
    assert_eq!(checked.summary(), NONE);
}

/// FN-8 and GEN-10: capture in written order, keeping the pack slot,
/// positional indexes or named keys, and the resolved argument types.
#[test]
fn calls_record_their_pack_arguments() {
    let checked = module(concat!(
        "const fn same<T>(fixed: T, rest: ...T) -> T => fixed\n",
        "const fn different<...Types>(rest: ...Types) -> i64 => 1\n",
        "const fn both<T, ...Fields>(rest: ...T, named: ...{Fields}) -> i64 => 1\n",
        "const fn no_args() -> i64 => different()\n",
        "const fn positional() -> i64 => same(1, 2, 3)\n",
        "const fn mixed() -> i64 => both(1, 9, right: true, left: \"x\")\n",
        "fn temporal(items: ...i64) -> i64 { when { return 1 } }\n",
        "fn graph() -> i64 => temporal(1, 2)\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    let recorded = |name| {
        let mut keys = Vec::new();
        let mut types = Vec::new();
        let mut ordinary = Vec::new();
        walk_statements(&body(&checked, name).statements, &mut |expr| {
            if let ExprKind::Call {
                callee, arguments, ..
            } = &expr.kind
            {
                keys.extend(arguments.iter().map(|argument| {
                    (
                        argument.parameter.unwrap(),
                        argument.pack.clone(),
                        argument.value.ty.to_string(),
                    )
                }));
                if let Callee::Function {
                    type_packs,
                    generics,
                    ..
                } = callee
                {
                    types.extend(type_packs.iter().map(|pack| {
                        (
                            pack.parameter,
                            pack.types
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>(),
                        )
                    }));
                    ordinary.extend(generics.iter().filter_map(|argument| match argument {
                        Argument::Type(ty) => Some(ty.to_string()),
                        _ => None,
                    }));
                }
            }
        });
        (keys, types, ordinary)
    };
    let (keys, packs, _) = recorded("no_args");
    assert!(keys.is_empty());
    assert_eq!(packs, [(0, Vec::new())]);
    let (keys, _, ordinary) = recorded("positional");
    assert_eq!(ordinary, ["i64"]);
    assert_eq!(
        keys,
        [
            (0, None, "i64".into()),
            (1, Some(PackKey::Position(0)), "i64".into()),
            (1, Some(PackKey::Position(1)), "i64".into()),
        ]
    );
    let (keys, packs, _) = recorded("mixed");
    assert_eq!(packs, [(1, vec!["bool".into(), "str".into()])]);
    assert_eq!(
        keys,
        [
            (0, Some(PackKey::Position(0)), "i64".into()),
            (0, Some(PackKey::Position(1)), "i64".into()),
            (1, Some(PackKey::Name("right".into())), "bool".into()),
            (1, Some(PackKey::Name("left".into())), "str".into()),
        ]
    );
    let (keys, _, _) = recorded("graph");
    assert_eq!(
        keys,
        [
            (0, Some(PackKey::Position(0)), "i64".into()),
            (0, Some(PackKey::Position(1)), "i64".into()),
        ]
    );
}

/// EXPR-2, FN-8 and GEN-10: packs constrain the kind, type and count
/// of captured arguments. #336 specifies the locations for count errors.
#[test]
fn calls_check_their_pack_arguments() {
    let checked = module(concat!(
        "const fn pair(items: ...i64{2}) -> i64 => 1\n",
        "const fn at_least(items: ...i64{1:*}) -> i64 => 1\n",
        "const fn bounded<...Ts{1:2}>(items: ...Ts{0:3}) -> i64 => 1\n",
        "const fn named<...Fields>(items: ...{Fields}) -> i64 => 1\n",
        "const fn wrong_type() -> i64 => pair(true, 1)\n",
        "const fn exact_too_few() -> i64 => pair(1)\n",
        "const fn minimum_too_few() -> i64 => at_least()\n",
        "const fn maximum_too_many() -> i64 => bounded(true, 1, \"x\")\n",
        "const fn wrong_named() -> i64 => pair(1, 2, extra: 3)\n",
        "const fn wrong_positional() -> i64 => named(1)\n",
        "const fn repeated_key() -> i64 => named(key: 1, key: 2)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:38 type/type.mismatch EXPR-5",
            "m.hgl:7:42 type/pack.cardinality GEN-10",
            "m.hgl:8:47 type/pack.cardinality GEN-10",
            "m.hgl:9:56 type/pack.cardinality GEN-10",
            "m.hgl:10:45 name/argument.unknown EXPR-2",
            "m.hgl:11:45 type/argument.unexpected EXPR-2",
            "m.hgl:12:49 name/argument.duplicate EXPR-2",
        ],
        "{}",
        checked.report
    );
}

/// GEN-8, GEN-10 and GEN-14 (see #336): the explicitly written types
/// are the entire positional pack. A named pack is inferred.
#[test]
fn a_written_type_checks_the_argument_in_its_pack_position() {
    let checked = module(concat!(
        "const fn f<...Ts>(items: ...Ts) -> i64 => 1\n",
        "const fn g<...Ts, ...Fields>(items: ...Ts, named: ...{Fields}) -> i64 => 1\n",
        "const fn accepts() -> i64 => g<i64, bool>(1, true, label: \"a\") + f(1, true)\n",
        "const fn rejects() -> i64 => f<i64>(true)\n",
        "const fn too_many() -> i64 => f<i64>(1, true)\n",
        "const fn too_few() -> i64 => f<i64, bool>(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:37 type/type.mismatch EXPR-5",
            "m.hgl:6:41 type/pack.cardinality GEN-10",
            "m.hgl:7:44 type/pack.cardinality GEN-10",
        ],
        "{}",
        checked.report
    );
}

/// GEN-8 and EXPR-9: a type pack must resolve every captured type.
/// An explicit positional type or an already typed named value can
/// give a literal the context it needs.
#[test]
fn a_type_pack_cannot_infer_from_untyped_arguments() {
    let checked = module(concat!(
        "const fn positional<...Ts>(items: ...Ts) -> i64 => 0\n",
        "const fn named<...Ts>(items: ...{Ts}) -> i64 => 0\n",
        "const fn null_pos() -> i64 => positional(null)\n",
        "const fn empty_pos() -> i64 => positional([])\n",
        "const fn result_pos() -> i64 => positional(ok(1))\n",
        "const fn null_named() -> i64 => named(x: null)\n",
        "const fn explicit() -> i64 => positional<i64?, list<i64>, result<i64, error>>(null, [], ok(1))\n",
        "const fn typed(item: i64?) -> i64 => named(x: item)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:31 type/generic.unresolved GEN-8",
            "m.hgl:5:43 type/sequence.context_type EXPR-9",
            "m.hgl:6:33 type/generic.unresolved GEN-8",
            "m.hgl:7:33 type/generic.unresolved GEN-8",
        ],
        "{}",
        checked.report
    );
}

/// GEN-5, GEN-6 and GEN-10: packed candidate selection checks every
/// operand and admits only those counts covered by the candidate.
#[test]
fn operator_calls_check_pack_types_and_counts() {
    let checked = module(concat!(
        "operator first_of<T>(values: ...T{1:*}) -> T\n",
        "impl const fn first_of(values: ...i64{1:*}) -> i64 => 7\n",
        "const fn empty() -> i64 => first_of()\n",
        "const fn wrong() -> i64 => first_of(1, \"a\")\n",
        "const fn fine() -> i64 => first_of(1, 2)\n",
        "operator overlapping(values: ...i64) -> i64\n",
        "impl const fn overlapping(values: ...i64{1:*}) -> i64 => 7\n",
        "impl const fn overlapping(values: ...i64{1}) -> i64 => 9\n",
        "const fn tied() -> i64 => overlapping(1)\n",
        "const fn several() -> i64 => overlapping(1, 2)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:37 type/pack.cardinality GEN-10",
            "m.hgl:5:28 operator/operator.no_candidate GEN-5",
            "m.hgl:10:27 operator/operator.ambiguous GEN-5",
        ],
        "{}",
        checked.report
    );
}

/// FN-8 and GEN-10: a selected candidate receives the pack index,
/// each source key, and the resolved types of each generic pack.
#[test]
fn an_operator_call_records_positional_and_named_captures() {
    let checked = module(concat!(
        "operator mix<...Ts, ...Fields>(items: ...Ts, named: ...{Fields}) -> i64\n",
        "impl const fn mix<...Ts, ...Fields>(items: ...Ts, named: ...{Fields}) -> i64 => 7\n",
        "const fn mixed() -> i64 => mix<f64, bool>(1, true, label: \"a\")\n",
        "const fn inferred() -> i64 => mix(1.0, true, label: \"a\")\n",
        "const fn bad_count() -> i64 => mix<i64>(1, true, label: \"a\")\n",
        "const fn nullable() -> i64 => mix<i64?, bool>(1, true, label: \"a\")\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:6:44 type/pack.cardinality GEN-10"],
        "{}",
        checked.report
    );
    for (name, first_value, first_type) in [
        ("mixed", "f64", "f64"),
        ("inferred", "f64", "f64"),
        ("nullable", "i64", "i64?"),
    ] {
        let mut calls = 0;
        walk_statements(&body(&checked, name).statements, &mut |expr| {
            if let ExprKind::Call {
                callee: Callee::Candidate { type_packs, .. },
                arguments,
                ..
            } = &expr.kind
            {
                let slots: Vec<_> = arguments
                    .iter()
                    .map(|argument| {
                        (
                            argument.parameter,
                            argument.pack.clone(),
                            argument.value.ty.to_string(),
                        )
                    })
                    .collect();
                assert_eq!(
                    slots,
                    [
                        (Some(0), Some(PackKey::Position(0)), first_value.into()),
                        (Some(0), Some(PackKey::Position(1)), "bool".into()),
                        (Some(1), Some(PackKey::Name("label".into())), "str".into()),
                    ]
                );
                let types: Vec<_> = type_packs
                    .iter()
                    .map(|pack| {
                        (
                            pack.parameter,
                            pack.types
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>(),
                        )
                    })
                    .collect();
                assert_eq!(
                    types,
                    [
                        (0, vec![first_type.into(), "bool".into()]),
                        (1, vec!["str".into()])
                    ]
                );
                calls += 1;
            }
        });
        assert_eq!(calls, 1, "{name}");
    }
}

/// FN-8: "Packs have no defaults, cannot be const, and follow all fixed
/// parameters."
#[test]
fn a_pack_has_no_default_is_not_const_and_comes_last() {
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "fn defaulted<T>(rest: ...T = 1) {}\n",
            "fn constant<T>(const rest: ...T) {}\n",
            "fn early<T>(rest: ...T, last: T, const size: i64 = 1) {}\n",
            "operator all<T>(const rest: ...T = 1, last: T) -> T\n",
            "native fn tick<T>(rest: ...T, last: T) -> T\n",
            // A pack named `const` is not a const pack (FN-5).
            "fn named<T>(const: ...T) {}\n",
        ),
    )]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:28 type/pack.default FN-8",
            "m.hgl:3:16 type/pack.const FN-8",
            "m.hgl:4:25 type/pack.order FN-8",
            "m.hgl:4:40 type/pack.order FN-8",
            "m.hgl:5:17 type/pack.const FN-8",
            "m.hgl:5:34 type/pack.default FN-8",
            "m.hgl:5:39 type/pack.order FN-8",
            "m.hgl:6:31 type/pack.order FN-8",
        ]
    );
}

/// FN-8: "`...{Fields}` ... where `Fields` is a declared type pack."
#[test]
fn a_named_pack_names_a_type_pack_of_its_declaration() {
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "struct Quote { bid: f64 }\n",
            // A type parameter that is not a pack; a type; a type pack of
            // another declaration; a pack applied to arguments.
            "fn single<T>(fields: ...{T}) {}\n",
            "fn concrete(fields: ...{Quote}) {}\n",
            "fn builtin(fields: ...{i64}) {}\n",
            "fn other<...Fields>(fields: ...{Others}) {}\n",
            "fn applied<...Fields>(fields: ...{Fields<i64>}) {}\n",
        ),
    )]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:26 type/pack.undeclared FN-8",
            "m.hgl:4:25 type/pack.undeclared FN-8",
            "m.hgl:5:24 type/pack.undeclared FN-8",
            "m.hgl:6:33 name/name.unknown UNIT-10",
            "m.hgl:6:33 type/pack.undeclared FN-8",
            "m.hgl:7:35 type/pack.undeclared FN-8",
        ]
    );
}

/// NATIVE-3: "A temporal implementation `{}` denotes a graph; `{ when; }`
/// denotes a node. Start/stop hooks require when and are unique. A native
/// const implementation stays value-level and admits no temporal
/// lifecycle hooks."
#[test]
fn a_native_implementations_hooks_follow_native_3() {
    let interface = (
        "interface.hgl",
        concat!(
            "module m\n",
            "native fn graph() -> i64\n",
            "native fn node() -> i64\n",
            "native fn full() -> i64\n",
            "native const fn value() -> i64\n",
            "native fn early() -> i64\n",
            "native fn twice() -> i64\n",
            "native const fn hooked() -> i64\n",
        ),
    );
    let accepted = check(&[
        interface,
        (
            "provider.hgl",
            concat!(
                "module m part provider\n",
                "native fn graph() -> i64 {}\n",
                "native fn node() -> i64 { when; }\n",
                "native fn full() -> i64 {\n    inject out\n    start;\n    when;\n    stop;\n}\n",
                "native const fn value() -> i64 {}\n",
                "native fn early() -> i64 { when; }\n",
                "native fn twice() -> i64 { when; }\n",
                "native const fn hooked() -> i64 {}\n",
            ),
        ),
    ]);
    assert_eq!(accepted.summary(), NONE);

    let rejected = check(&[
        interface,
        (
            "provider.hgl",
            concat!(
                "module m part provider\n",
                "native fn graph() -> i64 {}\n",
                "native fn node() -> i64 { when; }\n",
                "native fn full() -> i64 { when; }\n",
                "native const fn value() -> i64 {}\n",
                "native fn early() -> i64 { start; stop; }\n",
                "native fn twice() -> i64 { start; when; start; stop; stop; stop; }\n",
                "native const fn hooked() -> i64 { start; when; }\n",
            ),
        ),
    ]);
    assert_eq!(
        rejected.summary(),
        [
            "provider.hgl:6:28 phase/native.hook_without_when NATIVE-3",
            "provider.hgl:6:35 phase/native.hook_without_when NATIVE-3",
            "provider.hgl:7:41 phase/native.duplicate_hook NATIVE-3",
            "provider.hgl:7:54 phase/native.duplicate_hook NATIVE-3",
            "provider.hgl:7:60 phase/native.duplicate_hook NATIVE-3",
            "provider.hgl:8:35 phase/native.const_hook NATIVE-3",
            "provider.hgl:8:42 phase/native.const_hook NATIVE-3",
        ]
    );
}

/// GEN-6: "Across the candidates of one operator an option's name is
/// one kind: a generic value parameter in every candidate that declares
/// it, or a parameter in every one, so that what `{zero: .error}` names
/// is known before a candidate is selected." The candidate that
/// declares the parameter is the one reported.
#[test]
fn an_options_name_is_one_kind_across_an_operators_candidates() {
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "operator scaled<L, R, O>(lhs: L, rhs: R) -> O\n",
            // `exact` is a parameter here and a generic value parameter
            // below.
            "impl const fn scaled(lhs: f64, rhs: f64, exact: bool = false) -> f64 => lhs\n",
            "impl const fn scaled<const exact: bool = false>(lhs: i64, rhs: i64) -> i64 => lhs\n",
            // A parameter in every candidate that declares it.
            "operator near<L, R>(lhs: L, rhs: R) -> bool\n",
            "impl const fn near(lhs: f64, rhs: f64, within: f64 = 0.0) -> bool => true\n",
            "impl const fn near(lhs: i64, rhs: i64, within: f64 = 0.0) -> bool => true\n",
            // A generic value parameter in every one; the operator's
            // own parameters are no options.
            "operator ratio<L, R, O>(lhs: L, rhs: R) -> O\n",
            "impl const fn ratio<const zero: bool = false>(lhs: f64, rhs: f64) -> f64 => lhs\n",
            "impl const fn ratio<const zero: bool>(lhs: i64, rhs: i64) -> i64 => lhs\n",
            // GEN-13: a plain parameter used as a size is a value, and
            // so a generic value parameter as one written `const` is.
            "operator count<V>(value: V) -> i64\n",
            "impl const fn count<N>(value: list<i64, N>) -> i64 => 1\n",
            "impl const fn count(value: i64, N: i64 = 2) -> i64 => 1\n",
        ),
    )]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:42 operator/impl.option_kind GEN-6",
            "m.hgl:13:33 operator/impl.option_kind GEN-6",
        ]
    );
    assert_eq!(
        checked.findings[0].message,
        "`exact` is a parameter here and a generic value parameter in another candidate of \
         `scaled`: an option is one kind in every candidate"
    );
}
