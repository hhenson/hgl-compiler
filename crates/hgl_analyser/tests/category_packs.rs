//! Category constraints and tuple type-pack expansion (GEN-3, GEN-10).

mod common;

use common::{check, check_broken};

fn accepts(source: &str) {
    let checked = check(&[("m.hgl", source)]);
    assert!(!checked.rejected, "{}", checked.report);
}

fn rejects(source: &str, code: &str) {
    let checked = check(&[("m.hgl", source)]);
    assert!(
        checked.findings.iter().any(|finding| finding.code == code),
        "{}",
        checked.report
    );
}

#[test]
fn bare_callback_uses_the_most_specific_declared_membership() {
    // GEN-3 and FN-17: a direct function can declare the same evidence
    // as an operator candidate. GEN-12 compares the nested list shape.
    let source = r#"module m
const entry: str = "x"
operator twice<T>(x: T) -> T
impl fn twice(x: i64) -> i64 => x * 2
fn nested<R, ...Inputs, ...Actual>(f: fn(...{Inputs}) -> R, args: ...{Actual for f}) -> i64
    requires each k in arg_keys(Actual) {
        Actual[k] in {Inputs[k], list<Inputs[k], 2>, list<list<Inputs[k], 2>, 2>}
    }
    => 3
fn single<R, ...Inputs, ...Actual>(f: fn(...{Inputs}) -> R, args: ...{Actual for f}) -> i64
    requires (Actual[entry] in {Inputs[entry], list<Inputs[entry], 2>})
    => 4
fn positional(items: list<list<i64, 2>, 2>) -> i64 => nested(twice, items)
fn named(items: list<i64, 2>) -> i64 => single(twice, x: items)
"#;
    accepts(source);
}

#[test]
fn bare_callback_rejects_incomparable_member_types() {
    // GEN-3 and GEN-12: either alternative fits the dictionary, but
    // each constrains a different part and they give different types.
    rejects(
        r#"module m
fn identity<T>(x: T) -> T => x
fn select<R, ...Inputs, ...Actual>(f: fn(...{Inputs}) -> R, args: ...{Actual for f}) -> i64
    requires each k in arg_keys(Actual) {
        Actual[k] in {dict<Inputs[k], str>, dict<i64, Inputs[k]>}
    }
    => 3
fn ambiguous(items: dict<i64, str>) -> i64 => select(identity, items)
"#,
        "type/generic.unresolved",
    );
}

#[test]
fn bare_map_callback_uses_declared_native_collection_members() {
    // MAP-2 and FN-17: expanding a list or dictionary is a property of
    // the admitted declarations, independent of their target support.
    accepts(
        r#"module m
const fn identity<T>(x: T) -> T => x
fn growing(values: list<i64>) -> list<i64> => map(add_, values, 2)
fn keyed(values: dict<str, i64>) -> dict<str, i64> => map(identity, values)
"#,
    );
}

#[test]
fn bare_callback_checks_every_input_of_a_shared_generic() {
    // GEN-3 and FN-17: the second list gives i64 to the bare function.
    // Writing the callback type explicitly can instead pass it whole.
    let source = r#"module m
const fn first<T>(x: T, y: T) -> T => x
fn mixed(x: list<list<i64, 2>, 2>, y: list<i64, 2>) -> list<list<i64, 2>, 2> =>
"#;
    rejects(
        &format!("{source}    map(first, x, y)"),
        "type/generic.unresolved",
    );
    accepts(&format!("{source}    map(first<list<i64, 2>>, x, y)"));
}

#[test]
fn negative_and_disjoined_memberships_supply_no_bare_callback_evidence() {
    // GEN-3: a test of argument types can hold without binding a type.
    let source = r#"module m
operator keep<T>(x: T) -> i64
impl fn keep(x: list<i64, 2>) -> i64 => 5
fn whole<R, ...Inputs, ...Actual>(f: fn(...{Inputs}) -> R, args: ...{Actual for f}) -> i64
    requires each k in arg_keys(Actual) {
        Actual[k] in {list<Inputs[k], 2>} || !(Actual[k] in {list<Inputs[k], 2>})
    }
    => 3
fn pass_whole(items: list<i64, 2>) -> i64 => whole(keep, items)
"#;
    accepts(source);
}

#[test]
fn bare_callback_trials_keep_the_public_operator_diagnostic() {
    // GEN-3 and GEN-4: a public condition belongs to the operator, even
    // when more than one callback trial independently fails it.
    let checked = check(&[(
        "m.hgl",
        r#"module m
fn identity<T>(x: T) -> T => x
operator choose<M, R, ...Ps, ...As>(mode: M, f: fn(...{Ps}) -> R, args: ...{As for f}) -> i64
    requires M in {i64, str}
impl fn choose<R, ...Ps, ...As>(mode: i64, f: fn(...{Ps}) -> R, args: ...{As for f}) -> i64
    requires each k in arg_keys(As) { As[k] in {Ps[k]} } => 1
impl fn choose<R, ...Ps, ...As>(mode: str, f: fn(...{Ps}) -> R, args: ...{As for f}) -> i64
    requires each k in arg_keys(As) { As[k] in {list<Ps[k], 2>} } => 2
fn invalid(items: list<i64, 2>) -> i64 => choose(true, identity, items)
"#,
    )]);
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(
        checked.findings[0].code, "constraint/requires.unmet",
        "{}",
        checked.report
    );
}

const CATEGORIES: &str = "module m\n\
struct Point { x: f64\n y: f64 }\n\
struct Label { text: str }\n\
operator shown<T>(value: T) -> str\n\
impl const fn shown<T>(value: T) -> str requires T in struct => \"a struct\"\n\
impl const fn shown(value: Point) -> str => \"a point\"\n\
impl const fn shown<...Ts>(value: tuple<Ts>) -> str => \"a tuple\"\n";

#[test]
fn the_complete_categories_illustration_and_its_i64_rejection() {
    // GEN-3, GEN-5, GEN-10: all three candidates are distinct choices.
    accepts(&format!(
        "{CATEGORIES}test checks {{ assert shown(Label(text: \"a\")) == \"a struct\"\n assert shown(Point(x: 1.0, y: 2.0)) == \"a point\"\n assert shown((1, \"a\")) == \"a tuple\" }}"
    ));
    rejects(
        &format!("{CATEGORIES}const fn bad() -> str => shown(1)"),
        "operator/operator.no_candidate",
    );
}

#[test]
fn categories_hold_only_for_the_declared_nominal_kind() {
    // GEN-3: generic structs count, nullable types and tuples do not.
    let declarations = "module m\nstruct Box<T> { value: T }\nenum Tag { a, b }\nconst fn structure<T>(x: T) -> bool requires T in struct => true\nconst fn enumeration<T>(x: T) -> bool requires T in enum => true\n";
    accepts(&format!(
        "{declarations}test checks {{ assert structure(Box(value: 1))\n assert enumeration(Tag.a) }}"
    ));
    for call in [
        "structure(1)",
        "structure(Tag.a)",
        "structure((1, true))",
        "enumeration(Box(value: 1))",
        "enumeration(1)",
    ] {
        rejects(
            &format!("{declarations}const fn bad() -> bool => {call}"),
            "constraint/requires.unmet",
        );
    }
    rejects(
        &format!("{declarations}const fn bad(x: Box<i64>?) -> bool => structure(x)"),
        "constraint/requires.unmet",
    );
}

#[test]
fn category_requirements_are_passed_on_only_when_stated() {
    // GEN-3 and GEN-7: an instance is not a proof for an unconstrained body.
    for category in ["struct", "enum"] {
        let declaration =
            format!("module m\nconst fn inner<T>(x: T) -> bool requires T in {category} => true\n");
        accepts(&format!(
            "{declaration}const fn outer<U>(x: U) -> bool requires U in {category} => inner(x)"
        ));
        rejects(
            &format!("{declaration}const fn outer<U>(x: U) -> bool => inner(x)"),
            "constraint/requires.unmet",
        );
    }
}

#[test]
fn category_facts_respect_negation_and_disjunction() {
    // GEN-3: not being an enum does not imply being a struct.
    let inner = "module m\nconst fn inner<T>(x: T) -> bool requires T in struct => true\n";
    for stated in [
        "U in struct",
        "!(!(U in struct))",
        "(U in struct) || (U in struct)",
    ] {
        accepts(&format!(
            "{inner}const fn outer<U>(x: U) -> bool requires {stated} => inner(x)"
        ));
    }
    for stated in [
        "!(U in enum)",
        "(U in struct) || (U in enum)",
        "!(U in struct)",
    ] {
        rejects(
            &format!("{inner}const fn outer<U>(x: U) -> bool requires {stated} => inner(x)"),
            "constraint/requires.unmet",
        );
    }
}

#[test]
fn a_category_never_holds_for_a_generic_value() {
    // GEN-3: category memberships are of types, not bool constants.
    rejects(
        "module m\nconst fn inner<const flag: bool = true>() -> bool requires flag in struct => true\nconst fn bad() -> bool => inner()",
        "type/type.not_a_type",
    );
}

#[test]
fn tuple_packs_infer_nested_elements_and_expand_results() {
    // GEN-10: the pack is a sequence; T remains one type even if T is a tuple.
    accepts(
        "module m\nconst fn echo<...Ts>(x: tuple<Ts>) -> tuple<Ts> => x\nconst fn nested<...Ts>(x: list<tuple<Ts>>) -> tuple<Ts> => x[0]\nconst fn ordinary<T>(x: tuple<T>) -> tuple<T> => x\ntest checks { let a: tuple<i64, str> = echo((1, \"a\"))\n let b: tuple<i64, str> = nested([(1, \"b\")])\n let c: tuple<tuple<i64, str>> = ordinary(((1, \"c\"),))\n assert a[0] == b[0]\n assert c[0][0] == 1 }",
    );
}

#[test]
fn tuple_packs_keep_written_types_bounds_and_repeated_bindings() {
    // GEN-8 and GEN-10: explicit types and cardinalities constrain captures.
    let declarations = "module m\nconst fn pair<...Ts{2}>(x: tuple<Ts>) -> tuple<Ts> => x\nconst fn twice<...Ts>(x: tuple<Ts>, y: tuple<Ts>) -> bool => true\n";
    accepts(&format!(
        "{declarations}test checks {{ let value: tuple<f64, str> = pair<f64, str>((1, \"a\"))\n assert twice((1, \"a\"), (2, \"b\")) }}"
    ));
    rejects(
        &format!("{declarations}const fn bad() -> bool => twice((1, \"a\"), (true, \"b\"))"),
        "type/type.mismatch",
    );
    rejects(
        &format!("{declarations}const fn bad() -> tuple<i64> => pair((1,))"),
        "type/pack.cardinality",
    );
}

#[test]
fn tuple_packs_share_bindings_with_value_packs() {
    // FN-8 and GEN-10: a captured sequence constrains each packed input.
    let declarations =
        "module m\nconst fn share<...Ts>(pair: tuple<Ts>, xs: ...Ts) -> bool => true\n";
    accepts(&format!(
        "{declarations}test checks {{ assert share((1, \"a\"), 2, \"b\") }}"
    ));
    rejects(
        &format!("{declarations}const fn bad(x: i64) -> bool => share((2.0,), x)"),
        "type/type.mismatch",
    );
}

#[test]
fn tuple_packs_specialize_independently_typed_callbacks() {
    // FN-17 and GEN-10: a function reference takes the fixed callback's tuple.
    accepts(
        "module m\nconst fn echo<...Ts>(x: tuple<Ts>) -> tuple<Ts> => x\nconst fn apply(f: const fn(tuple<i64, str>) -> tuple<i64, str>, x: tuple<i64, str>) -> tuple<i64, str> => f(x)\ntest checks { let answer: tuple<i64, str> = apply(echo, (1, \"a\"))\n assert answer[0] == 1 }",
    );
}

#[test]
fn category_keywords_and_type_packs_are_not_standalone_types() {
    // GEN-3: category keywords are admitted only after membership's `in`.
    for requirement in [
        "T == struct",
        "struct == T",
        "T in {struct}",
        "T in enum == true",
    ] {
        let checked = check_broken(&[(
            "m.hgl",
            &format!("module m\nconst fn bad<T>(x: T) -> bool requires {requirement} => true"),
        )]);
        assert!(checked.front_end_errors > 0, "{}", checked.report);
    }
    rejects(
        "module m\nconst fn bad<...Ts>(x: list<Ts>) -> bool => true",
        "type/type.not_a_type",
    );
}

#[test]
fn tuple_packs_forward_through_generic_bodies() {
    // GEN-10: a generic tuple forwards its pack, not one symbolic element.
    accepts(
        "module m\nconst fn echo<...Ts>(x: tuple<Ts>) -> tuple<Ts> => x\nconst fn forward<...Us>(x: tuple<Us>) -> tuple<Us> => echo(x)\ntest checks { let pair: tuple<i64, str> = forward((1, \"a\"))\n assert pair[0] == 1 }",
    );
}

#[test]
fn forwarded_tuple_requirements_need_the_stated_category_bound() {
    // GEN-3, GEN-7, GEN-10: tuple forwarding is not the deferred reflected
    // callback path. Each admitted element must have the stated category.
    let inner = "module m\nconst fn inner<...Ts>(value: tuple<Ts>) -> bool requires each T in types(Ts) { T in struct } => true\n";
    rejects(
        &format!("{inner}const fn outer<...Us>(value: tuple<Us>) -> bool => inner(value)"),
        "constraint/requires.unmet",
    );
    accepts(&format!(
        "{inner}const fn outer<...Us>(value: tuple<Us>) -> bool requires each U in types(Us) {{ U in struct }} => inner(value)"
    ));
}

#[test]
fn forwarded_tuple_cardinality_uses_the_enclosing_bounds() {
    // GEN-7, GEN-10: a symbolic pack marker is not one captured type.
    let inner = "module m\nconst fn pair<...Ts{2}>(x: tuple<Ts>) -> tuple<Ts> => x\n";
    accepts(&format!(
        "{inner}const fn outer<...Us{{2}}>(x: tuple<Us>) -> tuple<Us> => pair(x)"
    ));
    rejects(
        &format!("{inner}const fn outer<...Us>(x: tuple<Us>) -> tuple<Us> => pair(x)"),
        "type/pack.cardinality",
    );
}

#[test]
fn an_empty_forwarded_pack_contributes_no_category_test() {
    // GEN-10: an empty conjunction is true; the fixed struct remains.
    accepts(
        "module m\nstruct Label { text: str }\nconst fn structures<...Ts>(x: tuple<Ts>) -> bool requires each T in types(Ts) { T in struct } => true\nconst fn outer<...Us{0}>(x: tuple<Label, Us>) -> bool => structures(x)\ntest checks { assert outer((Label(text: \"a\"),)) }",
    );
}

#[test]
fn a_category_alone_settles_an_otherwise_unused_generic_as_a_type() {
    // GEN-3 and GEN-13: the constraint determines T's kind without an input.
    let declaration = "module m\nstruct Label { text: str }\nconst fn category<T>() -> bool requires T in struct => true\n";
    accepts(&format!(
        "{declaration}test checks {{ assert category<Label>() }}"
    ));
    rejects(
        &format!("{declaration}test checks {{ assert category<1>() }}"),
        "type/generic.argument_kind",
    );
    rejects(
        "module m\nconst fn bad() -> bool requires 1 in struct => true",
        "type/type.not_a_type",
    );
}

#[test]
fn categories_accept_indexed_pack_types() {
    // GEN-3 and GEN-17: an indexed member denotes a type, not a runtime value.
    let declaration = "module m\nstruct Label { text: str }\nconst fn structures<...Ts>(values: ...Ts) -> bool requires each k in arg_keys(Ts) { Ts[k] in struct } => true\n";
    accepts(&format!(
        "{declaration}test checks {{ assert structures(Label(text: \"a\"), Label(text: \"b\")) }}"
    ));
    rejects(
        &format!("{declaration}const fn bad() -> bool => structures(1)"),
        "constraint/requires.unmet",
    );
}

#[test]
fn existential_category_facts_need_a_guaranteed_element() {
    // GEN-10: a sole tuple pack is nonempty; a value pack can be empty.
    let tuple = "module m\nconst fn inner<...Ts>(x: tuple<Ts>) -> bool requires any T in types(Ts) { T in struct } => true\n";
    accepts(&format!(
        "{tuple}const fn outer<...Us>(x: tuple<Us>) -> bool requires each U in types(Us) {{ U in struct }} => inner(x)"
    ));
}

#[test]
fn forwarded_value_pack_existence_is_checked_in_the_instance() {
    // GEN-7: a forwarded FN-19 pack is checked in its concrete instance.
    let packed = "module m\nstruct Label { text: str }\nconst fn inner<...Ts>(args: ...Ts) -> bool requires any T in types(Ts) { T in struct } => true\nconst fn outer<...Us>(args: ...Us) -> bool requires each U in types(Us) { U in struct } => inner(...args)\n";
    accepts(packed);
    accepts(&format!(
        "{packed}test checks {{ assert outer(Label(text: \"a\")) }}"
    ));
    rejects(
        &format!("{packed}test checks {{ assert outer() }}"),
        "constraint/requires.unmet",
    );
}

#[test]
fn fixed_tuple_items_do_not_prove_a_pack_has_an_element() {
    // GEN-10: a fixed prefix can supply the entire tuple.
    let prefix = "module m\nconst fn inner<...Ts>(x: tuple<i64, Ts>) -> bool requires any T in types(Ts) { T in struct } => true\n";
    rejects(
        &format!(
            "{prefix}const fn outer<...Us>(x: tuple<i64, Us>) -> bool requires each U in types(Us) {{ U in struct }} => inner(x)"
        ),
        "constraint/requires.unmet",
    );
}

#[test]
fn a_decisive_false_each_also_needs_an_element() {
    // GEN-10: a possibly empty pack cannot disprove a conjunction.
    let inner = "module m\nconst fn inner<...Ts>(x: tuple<i64, Ts>) -> bool requires !(each T in types(Ts) { T in struct }) => true\n";
    rejects(
        &format!(
            "{inner}const fn outer<...Us>(x: tuple<i64, Us>) -> bool requires each U in types(Us) {{ !(U in struct) }} => inner(x)"
        ),
        "constraint/requires.unmet",
    );
    accepts(&format!(
        "{inner}const fn outer<...Us{{1:*}}>(x: tuple<i64, Us>) -> bool requires each U in types(Us) {{ !(U in struct) }} => inner(x)"
    ));
}

#[test]
fn tuple_indices_select_members_not_pack_markers() {
    // GEN-7, GEN-10, EXPR-7: one indexed value is never an entire pack.
    rejects(
        "module m\nconst fn last<T, ...Ts>(value: tuple<Ts, T>) -> T => value[1]",
        "type/tuple.index",
    );
    rejects(
        "module m\nconst fn first_only<...Ts>(value: tuple<Ts>) -> tuple<Ts> => (value[0],)",
        "type/type.mismatch",
    );
    accepts(
        "module m\nconst fn second<...Ts{2}>(value: tuple<Ts>) -> bool { let y = value[1]\n return true }\ntest t { assert second((1, \"a\")) }",
    );
    rejects(
        "module m\nconst fn third<...Ts{2}>(value: tuple<Ts>) -> bool { let y = value[2]\n return true }",
        "type/tuple.index",
    );
    accepts(
        "module m\nconst fn tail<T, ...Ts{1}>(value: tuple<Ts, T>) -> T => value[1]\ntest t { assert tail((1, \"a\")) == \"a\" }",
    );
}

#[test]
fn sole_tuple_pack_nonemptiness_also_proves_cardinality() {
    // GEN-10: the same effective bounds govern witnesses and pack lengths.
    let inner = "module m\nconst fn inner<...Ts{1:*}>(value: tuple<Ts>) -> bool => true\n";
    accepts(&format!(
        "{inner}const fn outer<...Us>(value: tuple<Us>) -> bool => inner(value)"
    ));
    rejects(
        &format!(
            "{inner}const fn outer<...Us>(value: tuple<i64, Us>) -> bool => inner_tail(value)\nconst fn inner_tail<...Vs{{1:*}}>(value: tuple<i64, Vs>) -> bool => true"
        ),
        "type/pack.cardinality",
    );
}

#[test]
fn a_concrete_tuple_component_outranks_a_pack_variable() {
    // GEN-5, GEN-12: ranking uses declared variables, not inferred scalars.
    accepts(
        "module m\noperator shown<T>(value: T) -> str\nimpl const fn shown<...Ts>(value: tuple<Ts>) -> str => \"pack\"\nimpl const fn shown<T>(value: tuple<T, bool>) -> str => \"bool\"\ntest t { assert shown((\"a\", true)) == \"bool\"\n assert shown((1, 2, 3)) == \"pack\" }",
    );
}

#[test]
fn tuple_partition_equations_reach_a_fixed_point() {
    // GEN-3: later independent evidence can solve an earlier partition.
    let declarations = "module m\nconst fn later<...As, ...Bs>(joined: tuple<As, Bs>, left: tuple<As>) -> bool => true\nconst fn earlier<...As, ...Bs>(left: tuple<As>, joined: tuple<As, Bs>) -> bool => true\noperator split<J, L>(joined: J, left: L) -> bool\nimpl const fn split<...As, ...Bs>(joined: tuple<As, Bs>, left: tuple<As>) -> bool => true\n";
    accepts(&format!(
        "{declarations}test t {{ assert later((1, \"a\", true), (1,))\n assert earlier((1,), (1, \"a\", true))\n assert split((1, \"a\", true), (1,)) }}"
    ));
    rejects(
        &format!("{declarations}test t {{ assert later((\"a\", true), (1,)) }}"),
        "type/type.mismatch",
    );
}

#[test]
fn requirements_expand_tuple_operands_from_the_capture() {
    // GEN-3, GEN-10: a captured pack is a sequence even in a requirement.
    let f = "module m\nconst fn f<...Ts>(x: tuple<Ts>) -> bool requires tuple<Ts> == tuple<i64, str> => true\n";
    accepts(&format!("{f}test t {{ assert f((1, \"x\")) }}"));
    rejects(
        &format!("{f}test t {{ assert f((1, 2)) }}"),
        "constraint/requires.unmet",
    );
}

#[test]
fn a_nullable_tuple_element_cannot_wrap_a_pack() {
    // GEN-10: only a direct tuple element expands a sequence of types.
    rejects(
        "module m\nconst fn f<...Ts>(x: tuple<Ts?>) -> bool => true",
        "type/type.not_a_type",
    );
    accepts(
        "module m\nconst fn f<T>(x: tuple<T?>) -> bool => true\ntest t { assert f<i64>((1,)) }",
    );
}

#[test]
fn explicit_callback_pack_conflicts_report_the_two_function_types() {
    // FN-17, GEN-8: an explicit callback pack must match its receiving template.
    let f = "module m\nconst fn echo<...Ts>(x: tuple<Ts>) -> tuple<Ts> => x\nconst fn apply(f: const fn(tuple<i64, i64>) -> tuple<i64, i64>, v: tuple<i64, i64>) -> tuple<i64, i64> => f(v)\n";
    accepts(&format!(
        "{f}test t {{ assert apply(echo<i64, i64>, (1, 2)) == (1, 2)\n assert apply(echo, (1, 2)) == (1, 2) }}"
    ));
    let result = check(&[(
        "m.hgl",
        &format!("{f}test t {{ assert apply(f: echo<f64, f64>, v: (1, 2)) == (1, 2) }}"),
    )]);
    assert!(
        result
            .findings
            .iter()
            .any(|finding| finding.code == "type/type.mismatch"),
        "{}",
        result.report
    );
    assert!(
        result.report.contains("tuple<f64, f64>") && result.report.contains("tuple<i64, i64>"),
        "{}",
        result.report
    );
}

#[test]
fn callback_pack_commitments_cover_singletons_and_named_arguments() {
    // FN-17, GEN-8: fixed and reflected named arguments keep explicit packs.
    let f = "module m\nconst fn echo<...Ts>(x: tuple<Ts>) -> tuple<Ts> => x\nconst fn apply(f: const fn(tuple<i64>) -> tuple<i64>, v: tuple<i64>) -> tuple<i64> => f(v)\nconst fn invoke<...Ps, ...As>(f: const fn(...{Ps}) -> tuple<i64>, args: ...{As for f}) -> tuple<i64> requires each k in arg_keys(As) { As[k] == Ps[k] } => f(...args)\n";
    accepts(&format!(
        "{f}test t {{ assert apply(echo<i64>, (1,)) == (1,)\n assert apply(echo, (2,)) == (2,)\n assert invoke(echo<i64>, x: (3,)) == (3,)\n assert invoke(echo, x: (4,)) == (4,) }}"
    ));
    // #713 tracks a follow-on instance report after this primary
    // failure. Check the argument mismatch, never the number of reports.
    for call in ["apply(echo<f64>, (1,))", "invoke(echo<f64>, x: (1,))"] {
        let source = format!("{f}test t {{ let x = {call} }}");
        let result = check(&[("m.hgl", &source)]);
        let mismatch = result
            .findings
            .iter()
            .find(|finding| finding.code == "type/type.mismatch")
            .unwrap_or_else(|| panic!("{}", result.report));
        assert!(
            mismatch.message.contains("const fn")
                && mismatch.message.contains("tuple<f64>")
                && mismatch.message.contains("tuple<i64>"),
            "{}",
            result.report
        );
    }
}

#[test]
fn guaranteed_tuple_members_inherit_only_universal_categories() {
    // GEN-7 and GEN-10: indexing keeps each source fact at an existing key.
    let base = "module m\nstruct P { x: i64 }\nconst fn inner<T>(x: T) -> bool requires T in struct => true\n";
    for (bounds, index) in [("{1:*}", 0), ("{2:*}", 1), ("", 0)] {
        accepts(&format!(
            "{base}const fn outer<...Us{bounds}>(x: tuple<Us>) -> bool requires each U in types(Us) {{ U in struct }} => inner(x[{index}])"
        ));
        rejects(
            &format!(
                "{base}const fn outer<...Us{bounds}>(x: tuple<Us>) -> bool => inner(x[{index}])"
            ),
            "constraint/requires.unmet",
        );
    }
    accepts(&format!(
        "{base}const fn forward<...Us>(x: tuple<Us>) -> bool requires each U in types(Us) {{ U in struct }} => inner(x[0])\nconst fn outer<...Vs>(x: tuple<Vs>) -> bool requires each V in types(Vs) {{ V in struct }} => forward(x)\ntest t {{ assert outer((P(x: 1),)) }}"
    ));
    rejects(
        &format!(
            "{base}const fn outer<...Us>(x: tuple<Us>) -> bool requires any U in types(Us) {{ U in struct }} => inner(x[0])"
        ),
        "constraint/requires.unmet",
    );
    rejects(
        &format!(
            "{base}const fn outer<...Us>(x: tuple<Us>) -> bool requires each U in types(Us) {{ U in struct || U in enum }} => inner(x[0])"
        ),
        "constraint/requires.unmet",
    );
    rejects(
        &format!(
            "{base}const fn outer<...Us>(x: tuple<i64, Us>) -> bool requires each U in types(Us) {{ U in struct }} => inner(x[1])"
        ),
        "type/tuple.index",
    );
}

#[test]
fn nested_tuple_evidence_is_revisited_inside_the_named_containers() {
    // GEN-3: retain equations, without rereading values or guessing partitions.
    for (ty, value) in [
        ("list<tuple<As, Bs>>", "[(1, \"a\")]"),
        ("dict<str, tuple<As, Bs>>", "[\"k\": (1, \"a\")]"),
        ("tuple<As, Bs>?", "(1, \"a\")"),
        ("tuple<tuple<As, Bs>>", "((1, \"a\"),)"),
    ] {
        // UNIT-8: an optional input cannot precede a required one.
        let left = if ty.ends_with('?') {
            "tuple<As>?"
        } else {
            "tuple<As>"
        };
        let actual = ty.replace("As, Bs", "i64, str");
        let setup = format!("let joined: {actual} = {value}\n");
        let declared = format!(
            "module m\nconst fn f<...As, ...Bs>(joined: {ty}, left: {left}) -> bool => true\n"
        );
        accepts(&format!(
            "{declared}test t {{ {setup}assert f(joined, (1,)) }}"
        ));
        rejects(
            &format!("{declared}test t {{ {setup}assert f(joined, (true,)) }}"),
            "type/type.mismatch",
        );
        let candidate = format!(
            "module m\noperator f<J, L>(joined: J, left: L) -> bool\nimpl const fn f<...As, ...Bs>(joined: {ty}, left: {left}) -> bool => true\n"
        );
        accepts(&format!(
            "{candidate}test t {{ {setup}assert f(joined, (1,)) }}"
        ));
        rejects(
            &format!("{candidate}test t {{ {setup}assert f(joined, (true,)) }}"),
            "operator/operator.no_candidate",
        );
    }
}

#[test]
fn nested_exact_tuples_guarantee_their_pack_member_exists() {
    // GEN-10: denoting tuple<Us> needs a nonempty Us even inside a container.
    accepts(
        "module m\nconst fn in_list<...Us>(values: list<tuple<Us>>) -> bool { let selected = values[0][0]\n return true }\nconst fn in_tuple<...Us>(value: tuple<tuple<Us>, i64>) -> bool { let selected = value[0][0]\n return true }\ntest t { assert in_list([(1, \"a\")])\n assert in_tuple(((1, \"a\"), 2)) }",
    );
    rejects(
        "module m\nconst fn no_proof<...Us>(value: tuple<tuple<i64, Us>, str>) -> bool { let selected = value[0][1]\n return true }",
        "type/tuple.index",
    );
}

#[test]
fn fixed_tuple_arity_outranks_a_pack_in_either_declaration_order() {
    // GEN-5: the fixed-length tuple outranks a pack that admits it.
    for candidates in [
        "impl const fn p<...Ts>(x: tuple<Ts>) -> bool => true\nimpl const fn p<A, B>(x: tuple<A, B>) -> bool => true\n",
        "impl const fn p<A, B>(x: tuple<A, B>) -> bool => true\nimpl const fn p<...Ts>(x: tuple<Ts>) -> bool => true\n",
    ] {
        accepts(&format!(
            "module m\noperator p<T>(x: T) -> bool\n{candidates}test t {{ assert p((1, \"a\")) }}"
        ));
    }
}

#[test]
fn nested_evidence_does_not_guess_an_unconstrained_partition() {
    // GEN-3: a fixed point needs independent evidence, not an arbitrary split.
    rejects(
        "module m\nconst fn f<...As, ...Bs>(joined: list<tuple<As, Bs>>) -> bool => true\ntest t { assert f([(1, \"a\")]) }",
        "type/generic.unresolved",
    );
}
