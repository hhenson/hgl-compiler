//! The types a program declares: a struct's parents and fields, the
//! structs that hold themselves, and an enum's numbering. Each test
//! cites its rule.
//!
//! A finding is written `file:line:column category/code rule`; a code
//! local to this compiler has `local:` before it.

mod common;

use common::{Checked, check};
use hgl_analyser::Detail;

/// No findings.
const NONE: [&str; 0] = [];

/// Checks one file, `m.hgl`, of module `m`. The `module` line is added,
/// so the text given starts on line 2.
fn module(text: &str) -> Checked {
    check(&[("m.hgl", &format!("module m\n{text}"))])
}

/// The message of each finding, in order.
fn messages(checked: &Checked) -> Vec<&str> {
    checked
        .findings
        .iter()
        .map(|finding| finding.message.as_str())
        .collect()
}

/// The members of the enum `name` of module `m` as `member=number`, with
/// `?` for a number that is not known.
fn numbering(checked: &Checked, name: &str) -> Vec<String> {
    let module = checked.program.module("m").expect("module m exists");
    let index = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == name)
        .expect("the declaration exists");
    let Detail::Enum(read) = &module.details[index] else {
        panic!("{name} is an enum");
    };
    read.members
        .iter()
        .map(|member| match member.value {
            Some(value) => format!("{}={value}", member.name),
            None => format!("{}=?", member.name),
        })
        .collect()
}

// ---- Parents ----

/// TYPE-3: "Only abstract structs may be parents; abstract values are
/// not constructible. Concrete structs are final."
#[test]
fn a_parent_is_an_abstract_struct() {
    let checked = module(concat!(
        "abstract struct Base { id: i64 }\n",
        "struct Quote { bid: f64 }\n",
        "enum Side { buy, sell }\n",
        "native type Handle\n",
        "struct Good : Base { bid: f64 }\n",
        "abstract struct Middle : Base { }\n",
        "struct Leaf : Middle { ask: f64 }\n",
        // The chapter's deliberate rejection: "a concrete struct used as
        // a parent".
        "struct FromConcrete : Quote { }\n",
        "struct FromEnum : Side { }\n",
        "struct FromNative : Handle { }\n",
        "struct FromParameter<T> : T { value: T }\n",
        "struct Two : Base, Quote { }\n",
        // UNIT-11: left for the prelude, and not known.
        "struct FromPrelude : Missing { }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:23 local:type/struct.parent TYPE-3",
            "m.hgl:10:19 local:type/struct.parent TYPE-3",
            "m.hgl:11:21 local:type/struct.parent TYPE-3",
            "m.hgl:12:27 local:type/struct.parent TYPE-3",
            "m.hgl:13:20 local:type/struct.parent TYPE-3",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`Quote` is a concrete struct and cannot be a parent: only an abstract struct may be",
            "`Side` is an enum and cannot be a parent: only an abstract struct may be",
            "`Handle` is a native type and cannot be a parent: only an abstract struct may be",
            "`T` cannot be a parent: only an abstract struct may be",
            "`Quote` is a concrete struct and cannot be a parent: only an abstract struct may be",
        ]
    );
    assert_eq!(checked.prelude_names("m"), ["Missing"]);
}

/// TYPE-6: "Cycles through inheritance ... are rejected."
#[test]
fn no_struct_inherits_from_itself() {
    let checked = module(concat!(
        "abstract struct A : B { }\n",
        "abstract struct B : A { }\n",
        "abstract struct Own : Own { }\n",
        "abstract struct X : Y { }\n",
        "abstract struct Y : Z { }\n",
        "abstract struct Z : X { }\n",
        // A struct under a cycle is still read.
        "struct Leaf : A { value: i64 }\n",
    ));
    // Once for each group, at the first of its structs.
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:21 local:type/struct.inheritance_cycle TYPE-6",
            "m.hgl:4:23 local:type/struct.inheritance_cycle TYPE-6",
            "m.hgl:5:21 local:type/struct.inheritance_cycle TYPE-6",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`A` and `B` inherit from one another",
            "`Own` is its own parent",
            "`X` and `Y` and `Z` inherit from one another",
        ]
    );
}

// ---- Fields ----

/// TYPE-4: "A typed field introduction fixes its type and optionality.
/// A null default marks it optional. An untyped inherited override
/// changes only its construction default. Required fields may have
/// defaults; optional fields may have non-null defaults."
#[test]
fn a_struct_and_its_parent_agree_on_a_field() {
    let checked = module(concat!(
        "abstract struct Base {\n    id: i64\n    name: str = null\n    size: i64 = 1\n}\n",
        // Overrides of a parent's defaults, and a field of its own.
        "struct Good : Base {\n    id = 7\n    name = \"x\"\n    extra: f64\n}\n",
        // The same field again is the same field.
        "struct Again : Base { id: i64 }\n",
        "struct Retyped : Base { id: str }\n",
        "struct MadeOptional : Base { id: i64 = null }\n",
        "struct MadeRequired : Base { name: str }\n",
        // An override needs a field to override.
        "struct NoParent { id = 7 }\n",
        "struct NoSuchField : Base { missing = 7 }\n",
        // One name, one member.
        "struct Twice {\n    bid: f64\n    bid: f64\n}\n",
        "struct TwiceOver : Base {\n    id = 1\n    id = 2\n}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:13:25 local:type/field.conflict TYPE-4",
            "m.hgl:14:30 local:type/field.conflict TYPE-4",
            "m.hgl:15:30 local:type/field.conflict TYPE-4",
            "m.hgl:16:19 local:type/field.not_inherited TYPE-4",
            "m.hgl:17:29 local:type/field.not_inherited TYPE-4",
            "m.hgl:20:5 local:type/field.duplicate TYPE-4",
            "m.hgl:24:5 local:type/field.duplicate TYPE-4",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`id` is required `str` here and required `i64` in `Base`",
            "`id` is optional `i64` here and required `i64` in `Base`",
            "`name` is required `str` here and optional `str` in `Base`",
            "`id = ...` overrides the default of a parent's field, and `NoParent` has no field \
             `id` from a parent",
            "`missing = ...` overrides the default of a parent's field, and `NoSuchField` has no \
             field `missing` from a parent",
            "`Twice` has two members named `bid`",
            "`TwiceOver` has two members named `id`",
        ]
    );
}

/// TYPE-4: "Parent conflicts in type/optionality are errors." A parent's
/// generic arguments stand in its fields' types (TYPE-3: "invariant by
/// origin and complete canonical arguments").
#[test]
fn parents_agree_on_a_field() {
    let checked = module(concat!(
        "abstract struct Left { id: i64 }\n",
        "abstract struct Right { id: str }\n",
        "abstract struct Same { id: i64 }\n",
        "abstract struct Optional { id: i64 = null }\n",
        "struct Both : Left, Right { }\n",
        "struct Mixed : Left, Optional { }\n",
        "struct Agree : Left, Same { }\n",
        // One field by two paths is one field.
        "abstract struct Root { id: i64 }\n",
        "abstract struct PathOne : Root { }\n",
        "abstract struct PathTwo : Root { }\n",
        "struct Diamond : PathOne, PathTwo { }\n",
        // Unless the paths give it two types.
        "abstract struct Generic<T> { id: T }\n",
        "abstract struct AsInt : Generic<i64> { }\n",
        "abstract struct AsStr : Generic<str> { }\n",
        "struct Split : AsInt, AsStr { }\n",
        "struct Joined : AsInt, Left { }\n",
        // The argument stands for the parameter.
        "struct Carried<T> : Generic<T> { id: T }\n",
        "struct Fixed : Generic<i64> { id: i64 }\n",
        "struct Wrong : Generic<i64> { id: str }\n",
        "struct Deep : Generic<list<i64, 3>> { id: list<i64, 1 + 2> }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:21 local:type/field.conflict TYPE-4",
            "m.hgl:7:22 local:type/field.conflict TYPE-4",
            "m.hgl:16:23 local:type/field.conflict TYPE-4",
            "m.hgl:20:31 local:type/field.conflict TYPE-4",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`Both` has `id` from two parents: required `i64` and required `str`",
            "`Mixed` has `id` from two parents: required `i64` and optional `i64`",
            "`Split` has `id` from two parents: required `i64` and required `str`",
            "`id` is required `str` here and required `i64` in `Generic`",
        ]
    );
}

/// TYPE-4 compares a field's type as TYPE-11 gives it: "Omitted minimum
/// equals maximum", so a window with its minimum written as its maximum
/// is the parent's window with none written.
#[test]
fn a_field_is_compared_by_what_its_type_means() {
    let checked = module(concat!(
        "abstract struct Base {\n    window: rolling<i64, 5>\n    sized: list<f64, 6>\n}\n",
        "struct Same : Base {\n    window: rolling<i64, 5, 5>\n    sized: list<f64, 2 * 3>\n}\n",
        "struct Other : Base { window: rolling<i64, 5, 2> }\n",
        // And so is a minimum that a parent's argument makes its
        // maximum.
        "abstract struct Bounded<const N: i64> { window: rolling<i64, 5, N> }\n",
        "struct Equal : Bounded<5> { window: rolling<i64, 5> }\n",
        "struct Unequal : Bounded<2> { window: rolling<i64, 5> }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:10:23 local:type/field.conflict TYPE-4",
            "m.hgl:13:31 local:type/field.conflict TYPE-4",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`window` is required `rolling<i64, 5, 2>` here and required `rolling<i64, 5>` in \
             `Base`",
            "`window` is required `rolling<i64, 5>` here and required `rolling<i64, 5, 2>` in \
             `Bounded`",
        ]
    );
}

// ---- Structs that hold themselves ----

/// TYPE-6: "Recursive struct edges stay within their defining module,
/// use an optional `atomic<...>` field defaulted to null ... Descendants
/// cannot remove that null default. Cycles through inheritance or
/// collection elements are rejected. Cross-module references to
/// non-recursive public structs remain admitted."
#[test]
fn a_struct_holds_itself_through_an_optional_atomic_field() {
    let checked = module(concat!(
        "struct Node {\n    value: i64\n    next: atomic<Node> = null\n}\n",
        "struct Ping { pong: atomic<Pong> = null }\n",
        "struct Pong { ping: atomic<Ping> = null }\n",
        "struct Boxed<T> { value: T }\n",
        "struct Wrapped { inner: atomic<tuple<Boxed<Wrapped>, i64>> = null }\n",
        // Structs that hold others and not themselves.
        "struct Uses {\n    node: Node\n    nodes: list<Node>\n    boxed: Boxed<Node>\n}\n",
        // Directly, through a collection, through another struct.
        "struct Direct { inner: Direct }\n",
        "struct Tree { children: list<Tree> }\n",
        "struct Keyed { children: dict<str, atomic<Keyed>> = null }\n",
        "struct InBox { inner: Boxed<InBox> }\n",
        // A collection inside an `atomic<...>` is a collection still.
        "struct Forest { trees: atomic<list<Forest>> = null }\n",
        "struct Index { entries: atomic<dict<str, Boxed<Index>>> = null }\n",
        "struct Recent { last: atomic<set<Recent>> = null }\n",
        // An atomic field with no null default, or another default.
        "struct Required { next: atomic<Required> }\n",
        "struct Filled { next: atomic<Filled> = make() }\n",
        // One edge of a pair that is not atomic.
        "struct Left { right: Right }\n",
        "struct Right { left: atomic<Left> = null }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:15:17 local:type/field.recursive TYPE-6",
            "m.hgl:16:15 local:type/field.recursive TYPE-6",
            "m.hgl:17:16 local:type/field.recursive TYPE-6",
            "m.hgl:18:16 local:type/field.recursive TYPE-6",
            "m.hgl:19:17 local:type/field.recursive TYPE-6",
            "m.hgl:20:16 local:type/field.recursive TYPE-6",
            "m.hgl:21:17 local:type/field.recursive TYPE-6",
            "m.hgl:22:19 local:type/field.recursive TYPE-6",
            "m.hgl:23:17 local:type/field.recursive TYPE-6",
            "m.hgl:24:15 local:type/field.recursive TYPE-6",
        ]
    );
    let recursive = "a recursive field is an `atomic<...>` with a default of null";
    assert_eq!(
        messages(&checked),
        [
            format!("`inner` makes `Direct` hold itself: {recursive}"),
            "`children` makes `Tree` hold itself through the elements of a collection".to_string(),
            "`children` makes `Keyed` hold itself through the elements of a collection".to_string(),
            format!("`inner` makes `InBox` hold itself: {recursive}"),
            "`trees` makes `Forest` hold itself through the elements of a collection".to_string(),
            "`entries` makes `Index` hold itself through the elements of a collection".to_string(),
            "`last` makes `Recent` hold itself through the elements of a collection".to_string(),
            format!("`next` makes `Required` hold itself: {recursive}"),
            format!("`next` makes `Filled` hold itself: {recursive}"),
            format!("`right` makes `Left` hold itself: {recursive}"),
        ]
    );
}

/// TYPE-4: a field that reaches a struct by two paths is one field,
/// however many paths there are. Each of these structs inherits from
/// the two before it, so the first one's field reaches the last by 2^60
/// paths; the fields of each are worked out once.
#[test]
fn a_ladder_of_diamonds_is_read_in_step() {
    let mut text = String::from("abstract struct Root { id: i64 }\n");
    text.push_str("abstract struct A0 : Root { }\nabstract struct B0 : Root { }\n");
    for level in 1..=60 {
        let before = level - 1;
        text.push_str(&format!(
            "abstract struct A{level} : A{before}, B{before} {{ }}\n\
             abstract struct B{level} : A{before}, B{before} {{ }}\n"
        ));
    }
    // The field is there at the end of the ladder: this is its type.
    text.push_str("struct Good : A60, B60 { id: i64 }\n");
    text.push_str("struct Bad : A60, B60 { id: str }\n");
    let checked = module(&text);
    assert_eq!(
        checked.summary(),
        ["m.hgl:126:25 local:type/field.conflict TYPE-4"]
    );
    assert_eq!(
        checked.findings[0].message,
        "`id` is required `str` here and required `i64` in `Root`"
    );
}

/// TYPE-6: a struct holds what the fields it has from a parent hold,
/// with the arguments it gives the parent in place of the parent's
/// parameters. The fault is then in those arguments, and is reported
/// where the struct names the parent.
#[test]
fn a_struct_holds_itself_through_what_it_gives_a_parent() {
    let checked = module(concat!(
        "abstract struct Base<T> { next: T }\n",
        "abstract struct Link<T> { next: atomic<T> = null }\n",
        "abstract struct Items<T> { all: list<T> }\n",
        "struct Child : Base<Child> { }\n",
        "struct Good : Link<Good> { }\n",
        "struct Bag : Items<Bag> { }\n",
        "struct Other : Base<i64> { }\n",
        "struct Apart : Base<Good> { }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:16 local:type/field.recursive TYPE-6",
            "m.hgl:7:14 local:type/field.recursive TYPE-6",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`Child` holds itself through `next`, a field of `Base`: a recursive field is an \
             `atomic<...>` with a default of null",
            "`Bag` holds itself through `all`, a field of `Items` through the elements of a \
             collection",
        ]
    );
}

/// TYPE-6: a generic struct holds what its fields hold once its
/// arguments stand for its parameters, so a collection among those
/// fields is a collection on the path: "Cycles through ... collection
/// elements are rejected."
#[test]
fn a_generic_structs_fields_are_followed_with_its_arguments() {
    let checked = module(concat!(
        "struct Boxed<T> { value: T }\n",
        "struct Listed<T> { values: list<T> }\n",
        "struct Outer<T> { inner: Boxed<T> }\n",
        "struct Keyed<K, V> { entries: dict<K, V> }\n",
        // No collection on the way back.
        "struct Plain { next: atomic<Boxed<Plain>> = null }\n",
        "struct Nested { next: atomic<Outer<Boxed<Nested>>> = null }\n",
        // A list in the struct named, and in one it names in turn.
        "struct Chain { next: atomic<Listed<Chain>> = null }\n",
        "struct Deep { next: atomic<Outer<Listed<Deep>>> = null }\n",
        "struct Mapped { next: atomic<Keyed<str, Mapped>> = null }\n",
        // A struct that names itself with ever larger arguments is
        // followed once.
        "struct Grow<T> {\n    next: atomic<Grow<list<T>>> = null\n    value: T\n}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:8:16 local:type/field.recursive TYPE-6",
            "m.hgl:9:15 local:type/field.recursive TYPE-6",
            "m.hgl:10:17 local:type/field.recursive TYPE-6",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`next` makes `Chain` hold itself through the elements of a collection",
            "`next` makes `Deep` hold itself through the elements of a collection",
            "`next` makes `Mapped` hold itself through the elements of a collection",
        ]
    );
}

/// TYPE-6: "Descendants cannot remove that null default." A struct holds
/// itself through a parent's field as through its own.
#[test]
fn a_descendant_keeps_a_recursive_fields_null() {
    let checked = module(concat!(
        "abstract struct Linked { next: atomic<Item> = null }\n",
        "struct Item : Linked { value: i64 }\n",
        "struct Kept : Linked { next = null }\n",
        "struct Removed : Linked { next = make() }\n",
        // Through a parent and not atomic.
        "abstract struct Held { inner: Holder }\n",
        "struct Holder : Held { value: i64 }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:27 local:type/field.recursive TYPE-6",
            "m.hgl:6:24 local:type/field.recursive TYPE-6",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`next` is a recursive field, and its default stays null",
            "`inner` makes `Held` hold itself: a recursive field is an `atomic<...>` with a \
             default of null",
        ]
    );
}

// ---- Enums ----

/// TYPE-7: "Unnumbered numbering starts at zero and increments the
/// previous member; explicit numbers reset the next value." LEX-7: "Enum
/// numbering admits the full signed minimum." LEX-9: a member name is
/// any word of identifier form.
#[test]
fn an_enums_members_are_numbered() {
    let checked = module(concat!(
        "const fn seed() -> i64 => 7\n",
        "enum Plain { a, b, c }\n",
        "enum Reset { a = 5, b, c = 1, d }\n",
        "enum Least { low = -9223372036854775808, next }\n",
        "enum Most { below = 9223372036854775806, top }\n",
        "enum Folded { a = 2 * 3, b = -(1 + 1), c }\n",
        // What this pass does not evaluate leaves a number not known,
        // until one is written.
        "enum Open { a = seed(), b, c = 3, d }\n",
        "enum Words { error, list, time }\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(numbering(&checked, "Plain"), ["a=0", "b=1", "c=2"]);
    assert_eq!(numbering(&checked, "Reset"), ["a=5", "b=6", "c=1", "d=2"]);
    assert_eq!(
        numbering(&checked, "Least"),
        ["low=-9223372036854775808", "next=-9223372036854775807"]
    );
    assert_eq!(
        numbering(&checked, "Most"),
        ["below=9223372036854775806", "top=9223372036854775807"]
    );
    assert_eq!(numbering(&checked, "Folded"), ["a=6", "b=-2", "c=-1"]);
    assert_eq!(numbering(&checked, "Open"), ["a=?", "b=?", "c=3", "d=4"]);
    assert_eq!(
        numbering(&checked, "Words"),
        ["error=0", "list=1", "time=2"]
    );
}

/// TYPE-7: "Values fit signed `i64`; duplicates and overflow reject."
#[test]
fn an_enums_numbers_are_integers_no_two_alike() {
    let checked = module(concat!(
        "enum Explicit { a, b = 0 }\n",
        "enum Implicit { a = 1, b = 0, c }\n",
        "enum Over { top = 9223372036854775807, more, after }\n",
        "enum Names { a, b, a }\n",
        "enum Kinds { a = 1.5, b = \"x\", c = 5m, d = true, e }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:20 local:type/enum.duplicate_value TYPE-7",
            "m.hgl:3:31 local:type/enum.duplicate_value TYPE-7",
            "m.hgl:4:40 local:type/enum.overflow TYPE-7",
            "m.hgl:5:20 local:type/enum.duplicate_member TYPE-7",
            "m.hgl:6:18 local:type/enum.value TYPE-7",
            "m.hgl:6:27 local:type/enum.value TYPE-7",
            "m.hgl:6:36 local:type/enum.value TYPE-7",
            "m.hgl:6:44 local:type/enum.value TYPE-7",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`b` has the number 0, which `a` has",
            "`c` has the number 1, which `a` has",
            "`more` would be numbered one more than the largest i64",
            "this enum has two members named `a`",
            "an enum member's number is an integer, and this is a float",
            "an enum member's number is an integer, and this is a string",
            "an enum member's number is an integer, and this is a duration",
            "an enum member's number is an integer, and this is a bool",
        ]
    );
    // A number that is rejected is not known, and neither is what
    // follows it.
    assert_eq!(
        numbering(&checked, "Over"),
        ["top=9223372036854775807", "more=?", "after=?"]
    );
    assert_eq!(
        numbering(&checked, "Kinds"),
        ["a=?", "b=?", "c=?", "d=?", "e=?"]
    );
}

// ---- Integer literals ----

/// LEX-7: "Minus is an operator, not part of a numeric literal. Required
/// literal range checks use the i64/f64 domains." 2^63 is in the domain
/// only as the operand of a minus.
#[test]
fn two_to_the_sixty_third_is_an_integer_only_under_a_minus() {
    let checked = module(concat!(
        "const fn least() -> i64 => -9223372036854775808\n",
        "const fn most() -> i64 => 9223372036854775807\n",
        "const fn bare() -> i64 => 9223372036854775808\n",
        "const fn grouped() -> i64 => -(9223372036854775808)\n",
        "const fn subtracted() -> i64 => 0 - 9223372036854775808\n",
        "const fn defaulted(value: i64 = 9223372036854775808) -> i64 => value\n",
        "enum Range { low = -9223372036854775808, high = 9223372036854775808 }\n",
        "fn sized(items: list<f64, 9223372036854775808>) {}\n",
        "test literals { assert -9223372036854775808 != 9223372036854775808 }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:27 parse/number.integer_range LEX-7",
            "m.hgl:5:32 parse/number.integer_range LEX-7",
            "m.hgl:6:37 parse/number.integer_range LEX-7",
            "m.hgl:7:33 parse/number.integer_range LEX-7",
            "m.hgl:8:49 parse/number.integer_range LEX-7",
            "m.hgl:9:27 parse/number.integer_range LEX-7",
            "m.hgl:10:48 parse/number.integer_range LEX-7",
        ]
    );
    assert_eq!(
        checked.findings[0].message,
        "integer literal `9223372036854775808` is outside the i64 domain"
    );
    assert_eq!(
        numbering(&checked, "Range"),
        ["low=-9223372036854775808", "high=?"]
    );
}
