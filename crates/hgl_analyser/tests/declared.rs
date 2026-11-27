//! The types a program declares: a struct's parents and fields, the
//! structs that hold themselves, and an enum's numbering. Each test
//! cites its rule.
//!
//! A finding is written `file:line:column category/code rule`; a code
//! local to this compiler has `local:` before it.

mod common;

use common::{Checked, check, module};
use hgl_analyser::Detail;
use hgl_analyser::hir::ExprKind;

/// No findings.
const NONE: [&str; 0] = [];

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
        // UNIT-10: nothing binds it, and it is not known.
        "struct FromPrelude : Missing { }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:23 type/struct.parent TYPE-3",
            "m.hgl:10:19 type/struct.parent TYPE-3",
            "m.hgl:11:21 type/struct.parent TYPE-3",
            "m.hgl:12:27 type/struct.parent TYPE-3",
            "m.hgl:13:20 type/struct.parent TYPE-3",
            "m.hgl:14:22 name/name.unknown UNIT-10",
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
            "nothing binds `Missing` here",
        ]
    );
    assert_eq!(checked.prelude_names("m"), ["Missing"]);
}

/// TYPE-3: "Only abstract structs may be parents, each listed once."
#[test]
fn a_parent_is_listed_once() {
    let checked = module(concat!(
        "abstract struct Base { id: i64 }\n",
        "abstract struct Generic<T> { value: T }\n",
        "struct Twice : Base, Base { }\n",
        // With other arguments it is the same parent still.
        "struct Both : Generic<i64>, Generic<str> { }\n",
        "struct Fine : Base, Generic<i64> { }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:22 type/struct.duplicate_parent TYPE-3",
            "m.hgl:5:29 type/struct.duplicate_parent TYPE-3",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`Base` is listed twice among the parents",
            "`Generic` is listed twice among the parents",
        ]
    );
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
            "m.hgl:2:21 type/struct.inheritance_cycle TYPE-6",
            "m.hgl:4:23 type/struct.inheritance_cycle TYPE-6",
            "m.hgl:5:21 type/struct.inheritance_cycle TYPE-6",
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
/// A field is optional when its type is nullable ... An untyped
/// inherited override changes only the construction default of a field
/// a parent has. Required fields may have defaults; optional fields may
/// have non-null defaults."
#[test]
fn a_struct_and_its_parent_agree_on_a_field() {
    let checked = module(concat!(
        "abstract struct Base {\n    id: i64\n    name: str? = null\n    size: i64 = 1\n}\n",
        // Overrides of a parent's defaults, and a field of its own.
        "struct Good : Base {\n    id = 7\n    name = \"x\"\n    extra: f64\n}\n",
        // The same field again is the same field.
        "struct Again : Base { id: i64 }\n",
        "struct Retyped : Base { id: str }\n",
        "struct MadeOptional : Base { id: i64? = null }\n",
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
            "m.hgl:13:25 type/field.conflict TYPE-4",
            "m.hgl:14:30 type/field.conflict TYPE-4",
            "m.hgl:15:30 type/field.conflict TYPE-4",
            "m.hgl:16:19 type/field.not_inherited TYPE-4",
            "m.hgl:17:29 type/field.not_inherited TYPE-4",
            "m.hgl:20:5 type/field.duplicate TYPE-4",
            "m.hgl:24:5 type/field.duplicate TYPE-4",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`id` is `str` here and `i64` in `Base`",
            "`id` is `i64?` here and `i64` in `Base`",
            "`name` is `str` here and `str?` in `Base`",
            "`id = ...` overrides the default of a parent's field, and `NoParent` has no field \
             `id` from a parent",
            "`missing = ...` overrides the default of a parent's field, and `NoSuchField` has no \
             field `missing` from a parent",
            "`Twice` has two members named `bid`",
            "`TwiceOver` has two members named `id`",
        ]
    );
}

/// TYPE-4: "A typed field introduction fixes its type", and UNIT-8:
/// "Defaults remain required-constant/cold expressions checked against
/// the expected type." A field's default is a value of the field's
/// type, and `name = value` one of the type the struct has that field
/// with. Each is kept, typed, for the stages after, and each field a
/// struct has says which default a construction that leaves it out
/// takes (TYPE-5: "its own or one a struct below gives it").
#[test]
fn a_fields_default_is_a_value_of_its_type() {
    let checked = module(concat!(
        "enum Side { buy, sell }\n",
        "abstract struct Base<T> {\n    id: i64 = 1\n    held: T\n    rate: f64 = 2\n}\n",
        // An integer literal is an `f64` there; `.member` takes the
        // field's type; an override is read against the parent's field,
        // the parent's arguments in place.
        "struct Order : Base<str> {\n    id = 7\n    held = \"none\"\n    side: Side = .buy\n",
        "    note: str? = null\n    size: f64? = 3\n}\n",
        // Of another type than the field's.
        "struct Wrong : Base<str> {\n    id = \"x\"\n    held = 5\n    flag: bool = 1\n}\n",
        // A generic parameter's type is a construction's to say.
        "struct Open<T> { value: T = 1 }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:16:10 type/type.mismatch EXPR-5",
            "m.hgl:17:12 type/type.mismatch EXPR-5",
            "m.hgl:18:18 type/type.mismatch EXPR-5",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "this is `str`, and the field `id` is `i64`",
            "this is `i64`, and the field `held` is `str`",
            "this is `i64`, and the field `flag` is `bool`",
        ]
    );
    let module = checked.program.module("m").expect("module m exists");
    let position = |name: &str| {
        module
            .declarations
            .iter()
            .position(|declaration| declaration.name == name)
            .expect("the struct is declared")
    };
    // `Order` writes five fields, each with a default.
    let order = position("Order");
    let types: Vec<String> = module.defaults[order]
        .iter()
        .map(|default| {
            default
                .as_ref()
                .map_or("none".to_string(), |value| value.value.ty.to_string())
        })
        .collect();
    assert_eq!(types, ["i64", "str", "Side", "str?", "f64"]);
    assert!(
        matches!(
            module.defaults[order][4]
                .as_ref()
                .map(|value| &value.value.kind),
            Some(ExprKind::Widen(_))
        ),
        "the `3` of `size: f64? = 3` is widened"
    );
    // `Base` writes three, the second with none.
    let base = position("Base");
    let written: Vec<bool> = module.defaults[base].iter().map(Option::is_some).collect();
    assert_eq!(written, [true, false, true]);
    // What a construction of `Order` takes for each field it has, the
    // parent's first: `Order`'s own for `id` and `held`, and `Base`'s
    // for `rate`.
    let Some(Detail::Struct(read)) = module.details.get(order) else {
        panic!("`Order` is a struct");
    };
    let here = checked
        .program
        .modules
        .iter()
        .position(|module| module.path == "m")
        .expect("module m exists");
    // Each as the field's name, the struct that writes the default and
    // the default's position among that struct's fields.
    let taken: Vec<(&str, usize, usize)> = read
        .members
        .iter()
        .map(|member| {
            let default = member.default.expect("each field has a default");
            assert_eq!(default.module, here);
            (member.name.as_str(), default.declaration, default.field)
        })
        .collect();
    assert_eq!(
        taken,
        [
            ("id", order, 0),
            ("held", order, 1),
            ("rate", base, 2),
            ("side", order, 2),
            ("note", order, 3),
            ("size", order, 4),
        ]
    );
}

/// TYPE-4: "A field a struct has from two parents ... has one type and
/// optionality ... a conflict is an error." A parent's
/// generic arguments stand in its fields' types (TYPE-3: "invariant by
/// origin and complete canonical arguments").
#[test]
fn parents_agree_on_a_field() {
    let checked = module(concat!(
        "abstract struct Left { id: i64 }\n",
        "abstract struct Right { id: str }\n",
        "abstract struct Same { id: i64 }\n",
        "abstract struct Optional { id: i64? = null }\n",
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
            "m.hgl:6:21 type/field.conflict TYPE-4",
            "m.hgl:7:22 type/field.conflict TYPE-4",
            "m.hgl:16:23 type/field.conflict TYPE-4",
            "m.hgl:20:31 type/field.conflict TYPE-4",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`Both` has `id` from two parents: `i64` and `str`",
            "`Mixed` has `id` from two parents: `i64` and `i64?`",
            "`Split` has `id` from two parents: `i64` and `str`",
            "`id` is `str` here and `i64` in `Generic`",
        ]
    );
}

/// TYPE-4: "A field is optional when its type is nullable, `T?`, or an
/// `atomic<...>` of a nullable type, `atomic<T?>`, and only an optional
/// field's default may be null."
#[test]
fn only_an_optional_fields_default_is_null() {
    let checked = module(concat!(
        "abstract struct Base {\n    id: i64\n    name: str?\n    last: atomic<str?>\n}\n",
        "struct Good {\n    name: str? = null\n    last: atomic<str?> = null\n    other: str? = \"x\"\n}\n",
        // The chapter's deliberate rejection: "`size: i64 = null`, a
        // null default on a field that is not optional".
        "struct Sized { size: i64 = null }\n",
        "struct Whole { last: atomic<str> = null }\n",
        // Nullable inside the type is not the field's type.
        "struct Inside { items: list<i64?> = null }\n",
        // `(null)` is `null` (EXPR-1).
        "struct Grouped { size: i64 = (null) }\n",
        // `name = null` gives a parent's field its default.
        "struct Kept : Base {\n    name = null\n    last = null\n}\n",
        "struct Emptied : Base { id = null }\n",
        // A parent's argument stands for its parameter.
        "abstract struct Generic<T> { value: T }\n",
        "struct Nullable : Generic<i64?> { value = null }\n",
        "struct Required : Generic<i64> { value = null }\n",
        "struct Open<T> { value: T = null }\n",
        // UNIT-10: nothing binds it, and it is not known.
        "struct Later { value: Missing = null }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:12:28 type/field.null_default TYPE-4",
            "m.hgl:13:36 type/field.null_default TYPE-4",
            "m.hgl:14:37 type/field.null_default TYPE-4",
            "m.hgl:15:30 type/field.null_default TYPE-4",
            "m.hgl:20:30 type/field.null_default TYPE-4",
            "m.hgl:23:42 type/field.null_default TYPE-4",
            "m.hgl:24:29 type/field.null_default TYPE-4",
            "m.hgl:25:23 name/name.unknown UNIT-10",
        ]
    );
    assert_eq!(
        messages(&checked)[0],
        "`size` is `i64`, which is not optional, and its default is null: an optional field's \
         type is nullable, `T?`, or an `atomic<T?>`"
    );
    assert_eq!(checked.prelude_names("m"), ["Missing"]);
}

/// TYPE-4 compares a field's type as TYPE-11 gives it: "Omitted minimum
/// equals maximum", so a window with its minimum written as its maximum
/// is the parent's window with none written.
#[test]
fn a_field_is_compared_by_what_its_type_means() {
    let checked = module(concat!(
        "abstract struct Base {\n    window: rolling_window<i64, 5>\n    sized: list<f64, 6>\n}\n",
        "struct Same : Base {\n    window: rolling_window<i64, 5, 5>\n    sized: list<f64, 2 * 3>\n}\n",
        "struct Other : Base { window: rolling_window<i64, 5, 2> }\n",
        // A size this pass cannot evaluate is no difference: `N + 1`
        // with 2 for `N` is left for the pass that evaluates constants.
        "abstract struct Grown<const N: i64> { items: list<i64, N + 1> }\n",
        "struct Three : Grown<2> { items: list<i64, 3> }\n",
        // And so is a minimum that a parent's argument makes its
        // maximum.
        "abstract struct Bounded<const N: i64> { window: rolling_window<i64, 5, N> }\n",
        "struct Equal : Bounded<5> { window: rolling_window<i64, 5> }\n",
        "struct Unequal : Bounded<2> { window: rolling_window<i64, 5> }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:10:23 type/field.conflict TYPE-4",
            "m.hgl:15:31 type/field.conflict TYPE-4",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`window` is `rolling_window<i64, 5, 2>` here and `rolling_window<i64, 5>` in \
             `Base`",
            "`window` is `rolling_window<i64, 5>` here and `rolling_window<i64, 5, 2>` in \
             `Bounded`",
        ]
    );
}

/// A chain of parents is as long as the source makes it. The fields of
/// each struct are worked out from its parents' without one call inside
/// another for each link, so the chain's length does not grow the call
/// stack: this one is read on a thread with a small stack. One megabyte
/// is small for the purpose: twenty thousand calls inside one another
/// would need many times that. A quarter of that left no room, in an
/// unoptimised build on Windows, for the frames every analysis has.
#[test]
fn a_long_chain_of_parents_is_read_on_a_small_stack() {
    const LINKS: usize = 20_000;
    let mut text = String::new();
    // Each parent is declared after the struct that names it.
    for link in 0..LINKS {
        text.push_str(&format!("abstract struct A{link} : A{} {{ }}\n", link + 1));
    }
    text.push_str(&format!("abstract struct A{LINKS} {{ id: i64 }}\n"));
    // The field is there at the near end of the chain.
    text.push_str("struct Near : A0 { id: str }\n");
    // The syntax tree is not shared between threads, so the thread
    // gives back only what is compared.
    let found = std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(move || {
            let checked = module(&text);
            (checked.summary(), checked.findings[0].message.clone())
        })
        .expect("the thread starts")
        .join()
        .expect("the analysis does not overflow its stack");
    assert_eq!(
        found.0,
        [format!("m.hgl:{}:20 type/field.conflict TYPE-4", LINKS + 3)]
    );
    assert_eq!(
        found.1,
        format!("`id` is `str` here and `i64` in `A{LINKS}`")
    );
}

/// A chain of generic parents that each give the next `tuple<T, T>`
/// doubles the inherited field's type at every link: forty links would
/// make a type of 2^40 parts. A type that would weigh too much with its
/// arguments in place is not known, and nothing is built for it.
#[test]
fn a_type_that_grows_at_every_parent_is_not_built() {
    let mut text = String::from("abstract struct A0<T> { value: T }\n");
    for link in 1..=40 {
        text.push_str(&format!(
            "abstract struct A{link}<T> : A{}<tuple<T, T>> {{ }}\n",
            link - 1
        ));
    }
    // Near the top the field's type is still small, and still checked.
    text.push_str("struct Near : A1<i64> { value: tuple<i64, str> }\n");
    text.push_str("struct Far : A40<i64> { value: str }\n");
    let checked = module(&text);
    assert_eq!(
        checked.summary(),
        ["m.hgl:43:25 type/field.conflict TYPE-4"]
    );
    assert_eq!(
        checked.findings[0].message,
        "`value` is `tuple<i64, str>` here and `tuple<i64, i64>` in `A0`"
    );
}

/// GEN-2: "A type pack is a sequence, not a singular source type." A
/// parent's pack takes a sequence of arguments, which this pass does not
/// put in place: a field whose type names the pack is not compared, and
/// the parameters before the pack are.
#[test]
fn a_parents_pack_is_not_one_argument() {
    let checked = module(concat!(
        "struct Row<...Us> { width: i64 }\n",
        "abstract struct Base<T, ...Ts> {\n    first: T\n    rest: Row<Ts>\n}\n",
        "struct Child : Base<i64, str, f64> {\n    first: i64\n    rest: Row<str, f64>\n}\n",
        "struct Wrong : Base<i64, str> { first: str }\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:11:33 type/field.conflict TYPE-4"]
    );
}

/// A constant this pass holds no value for is not compared: `1.0` and
/// `1.00` are one float. And where a struct passes a type pack on to its
/// parent, which parameter each argument is given is not worked out, so
/// the parent's fields that name a parameter are not compared.
#[test]
fn what_is_not_worked_out_is_no_conflict() {
    let checked = module(concat!(
        "struct Box<const N: f64> { value: i64 }\n",
        "abstract struct Floats { item: Box<1.0> }\n",
        "struct Same : Floats { item: Box<1.00> }\n",
        "abstract struct Pair<A, B> {\n    first: A\n    second: B\n}\n",
        "struct Forwarded<...Ts{0}> : Pair<Ts, i64, str> {\n    first: i64\n    second: str\n}\n",
        "struct Plain : Pair<i64, str> { first: str }\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:13:33 type/field.conflict TYPE-4"]
    );
}

/// TYPE-4: a field is compared with each parent's field of its name. One
/// whose type is not known hides no conflict with another.
#[test]
fn a_field_is_compared_with_every_parents() {
    let checked = module(concat!(
        "abstract struct Open { value: Missing }\n",
        "abstract struct Known { value: i64 }\n",
        "struct Child : Open, Known { value: str }\n",
        "struct Fine : Open, Known { value: i64 }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            // UNIT-10: nothing binds `Missing`.
            "m.hgl:2:31 name/name.unknown UNIT-10",
            "m.hgl:4:30 type/field.conflict TYPE-4",
        ]
    );
    assert_eq!(
        checked.findings[1].message,
        "`value` is `str` here and `i64` in `Known`"
    );
}

/// ERR-9: "Do not demand cascaded diagnostics after an earlier error
/// prevents checking." A part of a field's type that is not known is no
/// difference from a parent's; what is known beside it still is.
#[test]
fn an_unknown_part_of_a_type_is_no_conflict() {
    let checked = module(concat!(
        "const fn helper() -> i64 => 1\n",
        "abstract struct Base {\n    items: list<helper>\n    pair: tuple<helper, i64>\n}\n",
        "struct Child : Base {\n    items: list<i64>\n    pair: tuple<str, i64>\n}\n",
        "struct Other : Base { pair: tuple<str, str> }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:17 type/type.not_a_type UNIT-10",
            "m.hgl:5:17 type/type.not_a_type UNIT-10",
            "m.hgl:11:23 type/field.conflict TYPE-4",
        ]
    );
    assert_eq!(
        checked.findings[2].message,
        "`pair` is `tuple<str, str>` here and `tuple<_, i64>` in `Base`"
    );
}

// ---- Structs that hold themselves ----

/// TYPE-6: "So that the recursion can end, a recursive field is
/// optional (TYPE-4), `Node?` or `atomic<Node?>` ... Cycles through
/// inheritance are rejected, and so are cycles through collection
/// elements outside an `atomic<...>`. Cross-module references to
/// non-recursive public structs remain admitted."
#[test]
fn a_struct_holds_itself_through_an_optional_atomic_field() {
    let checked = module(concat!(
        "struct Node {\n    value: i64\n    next: atomic<Node?> = null\n}\n",
        "struct Ping { pong: atomic<Pong?> = null }\n",
        "struct Pong { ping: atomic<Ping?> = null }\n",
        "struct Boxed<T> { value: T }\n",
        "struct Wrapped { inner: atomic<tuple<Boxed<Wrapped>, i64>?> = null }\n",
        // Structs that hold others and not themselves.
        "struct Uses {\n    node: Node\n    nodes: list<Node>\n    boxed: Boxed<Node>\n}\n",
        // Directly, through a collection, through another struct.
        "struct Direct { inner: Direct }\n",
        "struct Tree { children: list<Tree> }\n",
        "struct Keyed { children: dict<str, Keyed>? = null }\n",
        "struct InBox { inner: Boxed<InBox> }\n",
        // A collection inside the `atomic<...>` is part of one
        // snapshot.
        "struct Forest { trees: atomic<list<Forest>?> = null }\n",
        "struct Index { entries: atomic<dict<str, Boxed<Index>>?> = null }\n",
        "struct Recent { last: atomic<set<Recent>?> = null }\n",
        // An atomic field that is not optional: what it holds is not
        // nullable.
        "struct Required { next: atomic<Required> }\n",
        "struct Filled { next: atomic<Filled> = make() }\n",
        // One edge of a pair that is not atomic.
        "struct Left { right: Right }\n",
        "struct Right { left: atomic<Left?> = null }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:15:17 type/field.recursive TYPE-6",
            "m.hgl:16:15 type/field.recursive TYPE-6",
            "m.hgl:17:16 type/field.recursive TYPE-6",
            "m.hgl:18:16 type/field.recursive TYPE-6",
            "m.hgl:22:19 type/field.recursive TYPE-6",
            "m.hgl:23:17 type/field.recursive TYPE-6",
            "m.hgl:23:40 name/name.unknown UNIT-10",
            "m.hgl:24:15 type/field.recursive TYPE-6",
        ]
    );
    let recursive = "a recursive field is optional, its type nullable or an `atomic<...>` of a nullable type, or holds its struct only as elements of a collection that may be empty";
    let collection = "through the elements of a collection outside an `atomic<...>`";
    assert_eq!(
        messages(&checked),
        [
            format!("`inner` makes `Direct` hold itself: {recursive}"),
            format!("`children` makes `Tree` hold itself {collection}"),
            format!("`children` makes `Keyed` hold itself {collection}"),
            format!("`inner` makes `InBox` hold itself: {recursive}"),
            format!("`next` makes `Required` hold itself: {recursive}"),
            format!("`next` makes `Filled` hold itself: {recursive}"),
            // UNIT-10: the default is a call of a name nothing binds.
            "nothing binds `make` here".to_string(),
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
        ["m.hgl:126:25 type/field.conflict TYPE-4"]
    );
    assert_eq!(
        checked.findings[0].message,
        "`id` is `str` here and `i64` in `Root`"
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
        "abstract struct Link<T> { next: atomic<T?> = null }\n",
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
            "m.hgl:5:16 type/field.recursive TYPE-6",
            "m.hgl:7:14 type/field.recursive TYPE-6",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`Child` holds itself through `next`, a field of `Base`: a recursive field is optional, its type nullable or an `atomic<...>` of a nullable type, or holds its struct only as elements of a collection that may be empty",
            "`Bag` holds itself through `all`, a field of `Items` through the elements of a \
             collection outside an `atomic<...>`",
        ]
    );
}

/// TYPE-6: a generic struct holds what its fields hold once its
/// arguments stand for its parameters, so a collection among those
/// fields is a collection on the path. Inside an `atomic<...>` field it
/// is part of one snapshot and admitted; outside one it is a cycle
/// through collection elements.
#[test]
fn a_generic_structs_fields_are_followed_with_its_arguments() {
    let checked = module(concat!(
        "struct Boxed<T> { value: T }\n",
        "struct Listed<T> { values: list<T> }\n",
        "struct Outer<T> { inner: Boxed<T> }\n",
        "struct Plain { next: atomic<Boxed<Plain>?> = null }\n",
        "struct Chain { next: atomic<Listed<Chain>?> = null }\n",
        "struct Deep { next: atomic<Outer<Listed<Deep>>?> = null }\n",
        // No `atomic<...>` round the path: a list two structs down.
        "struct Loose { items: Outer<Listed<Loose>> }\n",
        "struct Direct { inner: Boxed<Direct> }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:8:16 type/field.recursive TYPE-6",
            "m.hgl:9:17 type/field.recursive TYPE-6",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`items` makes `Loose` hold itself through the elements of a collection outside an \
             `atomic<...>`",
            "`inner` makes `Direct` hold itself: a recursive field is optional, its type nullable or an `atomic<...>` of a nullable type, or holds its struct only as elements of a collection that may be empty",
        ]
    );
}

/// TYPE-6: "Such a field gives a struct of its cycle each generic
/// parameter whole, `Node<T>`, and no type built from one,
/// `Node<list<T>>`."
#[test]
fn a_recursive_field_passes_each_parameter_on_whole() {
    let checked = module(concat!(
        "struct Node<T> {\n    next: atomic<Node<T>?> = null\n    value: T\n}\n",
        "struct Fixed<T> {\n    next: atomic<Fixed<i64>?> = null\n    value: T\n}\n",
        "struct Swap<A, B> {\n    other: atomic<Swap<B, A>?> = null\n    first: A\n    second: B\n}\n",
        // A new type at every step.
        "struct Grow<T> {\n    next: atomic<Grow<list<T>>?> = null\n    value: T\n}\n",
        // Through another struct of the cycle.
        "struct Ping<T> {\n    pong: atomic<Pong<tuple<T, T>>?> = null\n    value: T\n}\n",
        "struct Pong<U> {\n    ping: atomic<Ping<U>?> = null\n    value: U\n}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:16:5 type/field.recursive TYPE-6",
            "m.hgl:20:5 type/field.recursive TYPE-6",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`next` makes `Grow` hold itself: `Grow<list<T>>` is given a type built from a \
             generic parameter, a new type at every step",
            "`pong` makes `Ping` hold itself: `Pong<tuple<T, T>>` is given a type built from a \
             generic parameter, a new type at every step",
        ]
    );
}

/// TYPE-6: "A field whose type is an abstract struct holds every struct
/// that inherits from it."
#[test]
fn a_field_of_an_abstract_type_holds_each_descendant() {
    let checked = module(concat!(
        "abstract struct Shape { }\n",
        "struct Circle : Shape { radius: f64 }\n",
        // Holds a struct that cannot hold it back.
        "struct Ring : Shape { inner: Circle }\n",
        // A `Shape` may be a `Group`.
        "struct Group : Shape { first: Shape }\n",
        "struct Bunch : Shape { members: atomic<list<Shape>?> = null }\n",
        // Through a struct outside the family.
        "struct Frame { shape: Shape }\n",
        "struct Framed : Shape { frame: Frame }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:24 type/field.recursive TYPE-6",
            "m.hgl:7:16 type/field.recursive TYPE-6",
            "m.hgl:8:25 type/field.recursive TYPE-6",
        ]
    );
    assert_eq!(
        checked.findings[0].message,
        "`first` makes `Group` hold itself: a recursive field is optional, its type nullable or \
         an `atomic<...>` of a nullable type, or holds its struct only as elements of a \
         collection that may be empty"
    );
}

/// TYPE-6: a struct that inherits from `Base<i64>` is a `Base<i64>` and
/// no `Base<str>`, so a field of type `Base<str>` does not hold it. One
/// that gives its parent a parameter of its own may be any application
/// that parameter can stand for.
#[test]
fn a_descendant_is_held_by_the_application_it_inherits_from() {
    let checked = module(concat!(
        "abstract struct Base<T> { value: T }\n",
        "struct Other : Base<i64> { other: Base<str> }\n",
        "struct Same : Base<i64> { same: Base<i64> }\n",
        "struct Open<U> : Base<U> { open: Base<str> }\n",
        "abstract struct Sized<const N: i64> { width: i64 }\n",
        "struct Three : Sized<3> { four: Sized<4> }\n",
        "struct Again : Sized<3> { three: Sized<1 + 2> }\n",
        // Through a parent between: what `Mid` is given is what it
        // gives `Base`.
        "abstract struct Mid<U> : Base<U> { }\n",
        "struct Far : Mid<i64> { far: Base<str> }\n",
        "struct Near : Mid<i64> { near: Base<i64> }\n",
        // A parameter inside a type is matched there.
        "struct Listed<U> : Base<list<U>> { other: Base<str> }\n",
        "struct Inner<U> : Base<list<U>> { inner: Base<list<str>> }\n",
        // A parameter written twice stands for one thing.
        "abstract struct Pair<A, B> { both: tuple<A, B> }\n",
        "struct Twin<U> : Pair<U, U> { other: Pair<i64, str> }\n",
        "struct Both<U> : Pair<U, U> { same: Pair<i64, i64> }\n",
        // What a parent between is given is worked out from it.
        "abstract struct Rows<U> : Base<list<U>> { }\n",
        "struct Apart : Rows<i64> { apart: Base<list<str>> }\n",
        "struct Close : Rows<i64> { close: Base<list<i64>> }\n",
        // TYPE-11: "Omitted minimum equals maximum."
        "struct Windowed : Base<rolling_window<i64, 5, 2>> { next: Base<rolling_window<i64, 5>> }\n",
        "struct Whole : Base<rolling_window<i64, 5>> { whole: Base<rolling_window<i64, 5, 5>> }\n",
        // LEX-20: a float is the number it denotes, however written.
        "struct Mark<const N: f64> { width: i64 }\n",
        "struct Marker : Base<Mark<1.0>> { marked: Base<Mark<1.00>> }\n",
        "struct Unmarked : Base<Mark<1.0>> { other: Base<Mark<2.0>> }\n",
        // LEX-2: and a string is its contents.
        "struct Named<const S: str> { width: i64 }\n",
        "struct Spelt : Base<Named<\"A\">> { spelt: Base<Named<\"\\u{41}\">> }\n",
        "struct Unspelt : Base<Named<\"A\">> { other: Base<Named<\"B\">> }\n",
        // GEN-10: a pack passed on stands for the arguments between
        // those before it and those after, as many as it admits.
        "struct Row<...Ts> { width: i64 }\n",
        "struct Packed<...Us> : Base<Row<Us, i64>> { packed: Base<Row<str, f64, i64>> }\n",
        "struct Ended<...Us> : Base<Row<Us, i64>> { other: Base<Row<str, f64>> }\n",
        "struct Counted<...Us{2}> : Base<Row<Us>> { other: Base<Row<str>> }\n",
        "abstract struct Wide<...Ts> { width: i64 }\n",
        "struct Two<...Us{2}> : Wide<Us> { two: Wide<i64, str> }\n",
        "struct Triple<...Us{2}> : Wide<Us> { other: Wide<i64, str, f64> }\n",
        // EXPR-1: parentheses select grouping, and a literal is the
        // same inside them.
        "struct Flag<const B: bool> { width: i64 }\n",
        "struct Grouped : Base<Flag<true>> { grouped: Base<Flag<(true)>> }\n",
        "struct Ungrouped : Base<Flag<true>> { other: Base<Flag<(false)>> }\n",
        "struct Signed : Base<Mark<-1.0>> { signed: Base<Mark<-(1.00)>> }\n",
        "struct Unsigned : Base<Mark<-1.0>> { other: Base<Mark<1.0>> }\n",
        // What a parameter given twice stands for is compared in the
        // same way: by what the types come to (TYPE-11), a parameter
        // of the struct that has the field being anything.
        "struct Twice<U> : Pair<U, U> { twice: Pair<rolling_window<i64, 5>, rolling_window<i64, 5, 5>> }\n",
        "struct Apiece<U> : Pair<U, U> { other: Pair<rolling_window<i64, 5>, rolling_window<i64, 5, 2>> }\n",
        "abstract struct Duo<A, B> { both: tuple<A, B> }\n",
        "struct Loose<U, V> : Duo<U, U> { loose: Duo<list<V>, list<i64>> }\n",
        "struct Shut<U, V> : Duo<U, U> { other: Duo<list<V>, set<i64>> }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:27 type/field.recursive TYPE-6",
            "m.hgl:5:28 type/field.recursive TYPE-6",
            "m.hgl:8:27 type/field.recursive TYPE-6",
            "m.hgl:11:26 type/field.recursive TYPE-6",
            "m.hgl:13:35 type/field.recursive TYPE-6",
            "m.hgl:16:31 type/field.recursive TYPE-6",
            "m.hgl:19:28 type/field.recursive TYPE-6",
            "m.hgl:21:47 type/field.recursive TYPE-6",
            "m.hgl:23:35 type/field.recursive TYPE-6",
            "m.hgl:26:35 type/field.recursive TYPE-6",
            "m.hgl:29:45 type/field.recursive TYPE-6",
            "m.hgl:33:35 type/field.recursive TYPE-6",
            "m.hgl:36:37 type/field.recursive TYPE-6",
            "m.hgl:38:36 type/field.recursive TYPE-6",
            "m.hgl:40:32 type/field.recursive TYPE-6",
            "m.hgl:43:34 type/field.recursive TYPE-6",
        ]
    );
}

/// TYPE-6: "Recursive fields stay within their defining module." A
/// struct is reached back from another module through an abstract
/// struct declared there.
#[test]
fn a_recursive_field_stays_within_its_module() {
    let checked = check(&[
        ("lib.hgl", "module lib\nexport abstract struct Shape { }\n"),
        (
            "m.hgl",
            concat!(
                "module m\n",
                "use lib::{Shape}\n",
                "struct Node : Shape { next: atomic<Shape?> = null }\n",
                "struct Leaf : Shape { size: i64 }\n",
                // An abstract struct of its own module is no way out.
                "abstract struct Local { }\n",
                "struct Chain : Local { next: atomic<Local?> = null }\n",
            ),
        ),
    ]);
    assert_eq!(
        checked.summary(),
        ["m.hgl:3:23 type/field.recursive TYPE-6"]
    );
    assert_eq!(
        checked.findings[0].message,
        "`next` makes `Node` hold itself through `Shape` of module `lib`: a recursive field \
         stays within its module"
    );
}

/// GEN-10: a pack a struct passes on to a parent between is placed
/// there too, where only some of that parent's arguments are known.
#[test]
fn a_pack_is_placed_among_arguments_partly_known() {
    let checked = module(concat!(
        "abstract struct Base<T> { value: T }\n",
        "abstract struct Middle<A, B, C, D> : Base<list<C>> { rest: tuple<A, B, D> }\n",
        // The two types of `Us` are `A` and `B`, so `C` is `str`.
        "struct Child<...Us{2}> : Middle<Us, str, i64> { next: Base<list<str>> }\n",
        "struct Other<...Us{2}> : Middle<Us, i64, str> { next: Base<list<str>> }\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:4:49 type/field.recursive TYPE-6"]
    );
}

/// TYPE-6: a struct that reaches an abstract struct by two lines of
/// parents is a descendant of it by either, each with the arguments
/// that line gives.
#[test]
fn a_descendant_is_found_by_each_line_of_parents() {
    let checked = module(concat!(
        "abstract struct Base<...Ts> { width: i64 }\n",
        "abstract struct Left<X> : Base<list<X>> { }\n",
        "abstract struct Right<Y> : Base<list<Y>> { }\n",
        "abstract struct Middle<X, Y> : Left<X>, Right<Y> { }\n",
        "struct ByLeft : Middle<i64, str> { next: Base<list<i64>> }\n",
        "struct ByRight : Middle<str, i64> { next: Base<list<i64>> }\n",
        "struct ByNeither : Middle<str, str> { next: Base<list<i64>> }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:36 type/field.recursive TYPE-6",
            "m.hgl:7:37 type/field.recursive TYPE-6",
        ]
    );
}

/// TYPE-6: "So that the recursion can end, a recursive field is
/// optional (TYPE-4), `Node?` or `atomic<Node?>`." It needs no default.
#[test]
fn a_recursive_field_may_be_nullable() {
    let checked = module(concat!(
        "struct Link { next: Link? }\n",
        "struct Paired { other: tuple<Paired, i64>? }\n",
        "struct Left { right: Right? }\n",
        "struct Right { left: atomic<Left?> = null }\n",
        // A collection outside an `atomic<...>` is on the path still.
        "struct Listed { items: list<Listed>? }\n",
        // Nullable inside the field's type is not the field's type.
        "struct Inner { items: tuple<Inner?, i64> }\n",
        // An `atomic<...>` of a nullable type is optional, and so is
        // one that is itself followed by `?` (TYPE-14).
        "struct Snapshot { next: atomic<Snapshot?> }\n",
        "struct Chain { next: atomic<Chain>? }\n",
        "struct Forest { trees: atomic<list<Forest>>? = null }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:17 type/field.recursive TYPE-6",
            "m.hgl:7:16 type/field.recursive TYPE-6",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`items` makes `Listed` hold itself through the elements of a collection outside an \
             `atomic<...>`",
            "`items` makes `Inner` hold itself: a recursive field is optional, its type nullable or an `atomic<...>` of a nullable type, or holds its struct only as elements of a collection that may be empty",
        ]
    );
}

/// TYPE-6: "Descendants cannot remove a null default it has." A struct holds
/// itself through a parent's field as through its own.
#[test]
fn a_descendant_keeps_a_recursive_fields_null() {
    let checked = module(concat!(
        "abstract struct Linked { next: atomic<Linked?> = null }\n",
        "struct Item : Linked { value: i64 }\n",
        "struct Kept : Linked { next = null }\n",
        "struct Removed : Linked { next = make() }\n",
        // Through a parent and not atomic.
        "abstract struct Held { inner: Holder }\n",
        "struct Holder : Held { value: i64 }\n",
        // A nullable recursive field has no null default to keep, and
        // its default is still one that ends the recursion.
        "abstract struct Chain { next: Chain? }\n",
        "struct Ended : Chain { next = make() }\n",
        // A generic parent's field is recursive in the struct that
        // gives it itself, and in no other.
        "abstract struct Base<T> { next: atomic<T?> = null }\n",
        "struct Node : Base<Node> { }\n",
        "struct Value : Base<i64> { next = 0 }\n",
        "struct Loop : Base<Loop> { next = make() }\n",
        // The default in force is the last override on the way down.
        "abstract struct Middle<T> : Base<T> { next = make() }\n",
        "struct Below : Middle<Below> { }\n",
        // Declaring the field again with a null default restores it,
        // and with another removes it.
        "struct Restored : Middle<Restored> { next: atomic<Restored?> = null }\n",
        "struct Redeclared : Linked { next: atomic<Linked?> = make() }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:27 type/field.recursive TYPE-6",
            "m.hgl:5:34 name/name.unknown UNIT-10",
            "m.hgl:6:24 type/field.recursive TYPE-6",
            "m.hgl:9:24 type/field.recursive TYPE-6",
            "m.hgl:9:31 name/name.unknown UNIT-10",
            "m.hgl:13:28 type/field.recursive TYPE-6",
            "m.hgl:13:35 name/name.unknown UNIT-10",
            "m.hgl:14:46 name/name.unknown UNIT-10",
            "m.hgl:15:16 type/field.recursive TYPE-6",
            "m.hgl:17:30 type/field.recursive TYPE-6",
            "m.hgl:17:54 name/name.unknown UNIT-10",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            // UNIT-10: each default here is a call of `make`, a name
            // nothing binds, which is reported where it is written.
            "`next` is a recursive field, and its default stays null",
            "nothing binds `make` here",
            "`inner` makes `Holder` hold itself: a recursive field is optional, its type nullable or an `atomic<...>` of a nullable type, or holds its struct only as elements of a collection that may be empty",
            "`next` is a recursive field, and its default is `null` or the empty collection `[]`",
            "nothing binds `make` here",
            "`next` is a recursive field, and its default stays null",
            "nothing binds `make` here",
            "nothing binds `make` here",
            "`Below` holds itself through `next`, a field of `Base`: a struct between has \
             removed its null default",
            "`next` is a recursive field, and its default stays null",
            "nothing binds `make` here",
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
            "m.hgl:2:20 type/enum.duplicate_value TYPE-7",
            "m.hgl:3:31 type/enum.duplicate_value TYPE-7",
            "m.hgl:4:40 type/enum.overflow TYPE-7",
            "m.hgl:5:20 type/enum.duplicate_member TYPE-7",
            "m.hgl:6:18 type/enum.value TYPE-7",
            "m.hgl:6:27 type/enum.value TYPE-7",
            "m.hgl:6:36 type/enum.value TYPE-7",
            "m.hgl:6:44 type/enum.value TYPE-7",
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

/// TYPE-6: "inside an `atomic<...>`, it holds the struct only as elements
/// of a collection that may be empty, an unsized list, a list of size
/// zero, a set, or a dictionary ... A list of any other size and a tuple
/// do not end it. A recursive field's default, its own or a
/// descendant's, is such a value: `null`, or the empty collection `[]`.
/// Descendants cannot remove a null default it has, not for an empty
/// collection either."
#[test]
fn a_recursive_field_may_end_in_an_empty_collection() {
    let checked = module(concat!(
        "struct Tree { children: atomic<list<Tree>> = [] }\n",
        // The type lets the recursion end; the default is not needed.
        "struct Bare { children: atomic<list<Bare>> }\n",
        "struct Index { entries: atomic<dict<str, Index>> = [] }\n",
        "struct Zero { none: atomic<list<Zero, 0>> }\n",
        // A list of another size, and a tuple, always hold elements.
        "struct Pair { both: atomic<list<Pair, 2>> }\n",
        "struct Twin { pair: atomic<tuple<Twin, Twin>> }\n",
        // Every way to the struct is through such a collection.
        "struct Mixed { both: atomic<tuple<Mixed, list<Mixed>>> }\n",
        // A default holds no struct of the cycle.
        "struct Full { children: atomic<list<Full>> = [make()] }\n",
        // Outside an `atomic<...>` the cycle still rejects.
        "struct Loose { children: list<Loose> = [] }\n",
        // A descendant gives the field an empty default, and no other.
        "abstract struct Open { children: atomic<list<Open>> }\n",
        "struct Filled : Open { children = [] }\n",
        "struct Wrong : Open { children = make() }\n",
        // A null default stays null, for an empty collection too.
        "abstract struct Parent { children: atomic<list<Parent>?> = null }\n",
        "struct Emptied : Parent { children = [] }\n",
        // A field recursive only in a struct below has its default
        // judged there, and reported where it is written.
        "abstract struct Stem { leaves: atomic<list<Leaf>> = [make()] }\n",
        "struct Leaf : Stem { }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:15 type/field.recursive TYPE-6",
            "m.hgl:7:15 type/field.recursive TYPE-6",
            "m.hgl:8:16 type/field.recursive TYPE-6",
            "m.hgl:9:15 type/field.recursive TYPE-6",
            "m.hgl:9:47 name/name.unknown UNIT-10",
            "m.hgl:10:16 type/field.recursive TYPE-6",
            "m.hgl:13:23 type/field.recursive TYPE-6",
            "m.hgl:13:34 name/name.unknown UNIT-10",
            "m.hgl:15:27 type/field.recursive TYPE-6",
            "m.hgl:16:24 type/field.recursive TYPE-6",
            "m.hgl:16:54 name/name.unknown UNIT-10",
        ]
    );
    let ends = "a recursive field is optional, its type nullable or an `atomic<...>` of a \
                nullable type, or holds its struct only as elements of a collection that \
                may be empty";
    assert_eq!(
        messages(&checked),
        [
            format!("`both` makes `Pair` hold itself: {ends}"),
            format!("`pair` makes `Twin` hold itself: {ends}"),
            format!("`both` makes `Mixed` hold itself: {ends}"),
            // UNIT-10: each default here is a call of `make`, a name
            // nothing binds, which is reported where it is written.
            "`children` is a recursive field, and its default is `null` or the empty \
             collection `[]`"
                .to_string(),
            "nothing binds `make` here".to_string(),
            "`children` makes `Loose` hold itself through the elements of a collection \
             outside an `atomic<...>`"
                .to_string(),
            "`children` is a recursive field, and its default is `null` or the empty \
             collection `[]`"
                .to_string(),
            "nothing binds `make` here".to_string(),
            "`children` is a recursive field, and its default stays null".to_string(),
            "`leaves` is a recursive field, and its default is `null` or the empty \
             collection `[]`"
                .to_string(),
            "nothing binds `make` here".to_string(),
        ]
    );
}

/// TYPE-6: the empty set or dictionary ends a recursive field's default,
/// whether its empty literal uses a bracket or explicit EXPR-9 braces.
#[test]
fn empty_braced_default_ends_recursive_collection_fields() {
    let checked = common::module(concat!(
        "struct Group { members: atomic<set<Group>> = ({,}) }\n",
        "struct Index { children: atomic<dict<str, Index>> = {:} }\n",
    ));
    assert!(checked.findings.is_empty(), "{}", checked.report);
    let wrong = common::module(concat!(
        "struct Index { entries: atomic<dict<str, Index>> = {,} }\n",
        "abstract struct Group { members: atomic<set<Group>> = {,} }\n",
        "struct Child : Group { members = {:} }\n",
    ));
    let codes: Vec<_> = wrong
        .findings
        .iter()
        .map(|finding| finding.code.as_str())
        .collect();
    assert_eq!(codes, ["type/field.recursive", "type/field.recursive"]);
}
