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
