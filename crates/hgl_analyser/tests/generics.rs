//! What each generic parameter is, a type, a value or a type pack, and
//! each generic argument against the parameter it is given to. Each test
//! cites its rule.
//!
//! A finding is written `file:line:column category/code rule`; a code
//! local to this compiler has `local:` before it.

mod common;

use common::{Checked, check};
use hgl_analyser::{Detail, GenericKind};

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

/// The generic parameters of the declaration `name` of module `m`, each
/// as `name:kind`.
fn kinds(checked: &Checked, name: &str) -> Vec<String> {
    let module = checked.program.module("m").expect("module m exists");
    let index = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == name)
        .expect("the declaration exists");
    let generics = match &module.details[index] {
        Detail::Callable(signature) => &signature.generics,
        Detail::Struct(read) => &read.generics,
        other => panic!("{name} is {other:?}"),
    };
    generics
        .iter()
        .map(|generic| {
            let kind = match &generic.kind {
                GenericKind::Plain => "unsettled".to_string(),
                GenericKind::Type => "type".to_string(),
                GenericKind::Pack { .. } => "pack".to_string(),
                GenericKind::Value(ty) => format!("value {ty}"),
            };
            format!("{}:{kind}", generic.name)
        })
        .collect()
}

/// GEN-13: "A generic parameter denotes a type, type pack, or
/// compile-time ordinary value ... Infer its kind from signature
/// positions and constraints. An explicit `const N: V` declares a value
/// parameter of ordinary type V."
#[test]
fn a_plain_parameter_is_what_its_signature_uses_it_as() {
    let checked = module(concat!(
        "struct Box<T> { value: T }\n",
        "struct Sized<N> { items: list<f64, N> }\n",
        "struct Window<D> { values: rolling<f64, D> }\n",
        "struct Grid<N, M, T> { cells: list<list<T, M>, N> }\n",
        "struct Defaulted<N> { size: i64 = N }\n",
        "abstract struct Base<T> { id: T }\n",
        "struct Child<T> : Base<T> { extra: i64 }\n",
        "fn first<T>(items: list<T>) -> T {}\n",
        "fn result_only<T>() -> T {}\n",
        "fn fixed<N>(items: list<f64, N + 1>) {}\n",
        "fn packed<T>(rest: ...T) {}\n",
        "const fn defaulted<N>(size: i64 = N * 2) -> i64 => size\n",
        // Through another struct's parameter.
        "fn through<T, N>(boxed: Box<T>, sized: Sized<N>) {}\n",
        "fn declared<const N: i64, ...Ts>(items: list<f64, N>, rest: ...Ts) {}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(kinds(&checked, "Box"), ["T:type"]);
    // A value written plain has no declared type.
    assert_eq!(kinds(&checked, "Sized"), ["N:value _"]);
    assert_eq!(kinds(&checked, "Window"), ["D:value _"]);
    assert_eq!(
        kinds(&checked, "Grid"),
        ["N:value _", "M:value _", "T:type"]
    );
    assert_eq!(kinds(&checked, "Defaulted"), ["N:value _"]);
    assert_eq!(kinds(&checked, "Child"), ["T:type"]);
    assert_eq!(kinds(&checked, "first"), ["T:type"]);
    assert_eq!(kinds(&checked, "result_only"), ["T:type"]);
    assert_eq!(kinds(&checked, "fixed"), ["N:value _"]);
    assert_eq!(kinds(&checked, "packed"), ["T:type"]);
    assert_eq!(kinds(&checked, "defaulted"), ["N:value _"]);
    assert_eq!(kinds(&checked, "through"), ["T:type", "N:value _"]);
    assert_eq!(kinds(&checked, "declared"), ["N:value i64", "Ts:pack"]);
}

/// GEN-13: a use as an argument of a generic type says what a parameter
/// is once that type's own parameter is settled, whichever is declared
/// first and however the structs name one another.
#[test]
fn kinds_are_settled_across_structs() {
    let checked = check(&[
        (
            "lib.hgl",
            concat!(
                "module lib\n",
                "export struct Outer<N> { inner: Inner<N> }\n",
                "export struct Inner<N> { items: list<f64, N> }\n",
            ),
        ),
        (
            "m.hgl",
            concat!(
                "module m\n",
                "use lib::{Outer}\n",
                // Each names the other; one field settles both.
                "struct Ping<N> { pong: atomic<Pong<N>> = null }\n",
                "struct Pong<N> {\n    ping: atomic<Ping<N>> = null\n    items: list<f64, N>\n}\n",
                "struct Chain<A> { next: Middle<A> }\n",
                "struct Middle<B> { last: Ping<B> }\n",
                "fn uses<K>(outer: Outer<K>, chain: Chain<3>) {}\n",
            ),
        ),
    ]);
    assert_eq!(checked.summary(), NONE);
    assert_eq!(kinds(&checked, "Ping"), ["N:value _"]);
    assert_eq!(kinds(&checked, "Pong"), ["N:value _"]);
    assert_eq!(kinds(&checked, "Chain"), ["A:value _"]);
    assert_eq!(kinds(&checked, "Middle"), ["B:value _"]);
    assert_eq!(kinds(&checked, "uses"), ["K:value _"]);
}

/// GEN-13: "Reject conflicting kinds or an unresolved kind lacking a
/// determining declaration/constraint."
#[test]
fn a_parameters_uses_agree_and_something_says_what_it_is() {
    let checked = module(concat!(
        "struct Box<T> { value: T }\n",
        "struct Sized<N> { items: list<f64, N> }\n",
        // A type, then a size; a size, then a type; through two structs.
        "fn both<T>(value: T, items: list<f64, T>) {}\n",
        "struct Mixed<N> {\n    items: list<f64, N>\n    value: N\n}\n",
        "fn crossed<X>(boxed: Box<X>, sized: Sized<X>) {}\n",
        // Nothing in the signature names these.
        "fn unused<T>(value: i64) {}\n",
        "struct Phantom<T> { width: i64 }\n",
        // A body is no signature position.
        "const fn body_only<T>(value: i64) -> i64 {\n    let items: list<T> = []\n    return value\n}\n",
        // A constraint may say what it is; that is for the pass that
        // reads constraints.
        "fn constrained<T>(value: i64) requires T in {i64, f64} {}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:39 local:type/generic.kind_conflict GEN-13",
            "m.hgl:7:12 local:type/generic.kind_conflict GEN-13",
            "m.hgl:9:43 local:type/generic.kind_conflict GEN-13",
            "m.hgl:10:11 local:type/generic.kind_unresolved GEN-13",
            "m.hgl:11:16 local:type/generic.kind_unresolved GEN-13",
            "m.hgl:12:20 local:type/generic.kind_unresolved GEN-13",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`T` is used as a type and as a value",
            "`N` is used as a type and as a value",
            "`X` is used as a type and as a value",
            "nothing in the signature of `unused` says whether `T` is a type or a value",
            "nothing in the signature of `Phantom` says whether `T` is a type or a value",
            "nothing in the signature of `body_only` says whether `T` is a type or a value",
        ]
    );
    assert_eq!(kinds(&checked, "both"), ["T:unsettled"]);
    assert_eq!(kinds(&checked, "constrained"), ["T:unsettled"]);
}

/// GEN-13: "an unresolved kind lacking a determining
/// declaration/constraint" rejects. A parameter that is only handed on,
/// to a parameter that nothing settles either, is determined by
/// nothing: structs that only name one another settle no kind.
#[test]
fn a_parameter_that_is_only_handed_on_is_unresolved() {
    let checked = module(concat!(
        "struct Node<T> { next: atomic<Node<T>> = null }\n",
        "struct Ping<T> { pong: atomic<Pong<T>> = null }\n",
        "struct Pong<T> { ping: atomic<Ping<T>> = null }\n",
        // One use elsewhere settles the whole ring.
        "struct Tree<T> {\n    next: atomic<Tree<T>> = null\n    value: T\n}\n",
        // A function given such a struct is not where the fault is.
        "fn uses<K>(node: Node<K>) {}\n",
        // A type nothing declares may yet say what its argument is.
        "struct Open<T> { held: Missing<T> }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:13 local:type/generic.kind_unresolved GEN-13",
            "m.hgl:3:13 local:type/generic.kind_unresolved GEN-13",
            "m.hgl:4:13 local:type/generic.kind_unresolved GEN-13",
        ]
    );
    assert_eq!(kinds(&checked, "Tree"), ["T:type"]);
    assert_eq!(kinds(&checked, "uses"), ["K:unsettled"]);
    assert_eq!(kinds(&checked, "Open"), ["T:unsettled"]);
}

/// GEN-14: "Check each argument against the corresponding parameter
/// kind; shared name syntax is not a second interpretation."
#[test]
fn an_argument_is_of_its_parameters_kind() {
    let checked = module(concat!(
        "struct Box<T> { value: T }\n",
        "struct Sized<const N: i64> { width: i64 }\n",
        "struct Count<N> { items: list<f64, N> }\n",
        "struct Row<T, ...Ts> { first: T }\n",
        "enum Side { buy, sell }\n",
        "fn good<T, const N: i64, M>(a: Box<i64>, b: Sized<3>, c: Sized<2 * 3>, d: Row<i64, str, f64>,\n",
        "    e: Box<Side>, f: Box<T>, g: Sized<N>, h: Sized<M>, i: Count<N + 1>, j: Box<list<T>>) {}\n",
        // A value where a type goes.
        "fn value_for_type(a: Box<3>, b: Box<1 + 2>, c: Row<i64, 3>) {}\n",
        // A type where a value goes.
        "fn type_for_value(a: Sized<i64>, b: Sized<Side>, c: Count<list<i64>>) {}\n",
        // A constructor's arguments are no different.
        "const fn made() -> Box<i64> => Box<7>(value: 1)\n",
        // A name nothing binds may yet be a constant (UNIT-11).
        "fn open(a: Sized<Missing>) {}\n",
        // A type pack passes on types. One that may hold nothing is
        // given to no parameter for certain, and is left for the pass
        // that resolves it.
        "fn packs<...One{1}, ...Any>(a: Sized<One>, b: Sized<Any>) {}\n",
        // A pack of a known length keeps the place of what follows it.
        "struct Pair<T, const N: i64> { value: T }\n",
        "fn placed<...One{1}, ...Two{2}>(a: Pair<One, i64>, b: Pair<One, 3>, c: Row<Two, 7>) {}\n",
        // Each type of a pack is given to a parameter of its own: as
        // many as the pack holds at least, and none when it may hold
        // nothing.
        "fn spans<...Two{2}, ...Some{2:3}, ...None{0}, ...Maybe{0:1}>(a: Pair<Two>, b: Pair<Some>,\n",
        "    c: Sized<None, 3>, d: Sized<Maybe>) {}\n",
        // A parameter settled as a type is a type in a body too.
        "const fn body<T>(value: T) -> i64 {\n    let sized: Sized<T> = make()\n    return 0\n}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:26 local:type/generic.argument_kind GEN-14",
            "m.hgl:9:37 local:type/generic.argument_kind GEN-14",
            "m.hgl:9:57 local:type/generic.argument_kind GEN-14",
            "m.hgl:10:28 local:type/generic.argument_kind GEN-14",
            "m.hgl:10:43 local:type/generic.argument_kind GEN-14",
            "m.hgl:10:59 local:type/generic.argument_kind GEN-14",
            "m.hgl:11:36 local:type/generic.argument_kind GEN-14",
            "m.hgl:13:38 local:type/generic.argument_kind GEN-14",
            "m.hgl:15:46 local:type/generic.argument_kind GEN-14",
            "m.hgl:15:81 local:type/generic.argument_kind GEN-14",
            "m.hgl:16:70 local:type/generic.argument_kind GEN-14",
            "m.hgl:16:84 local:type/generic.argument_kind GEN-14",
            "m.hgl:19:22 local:type/generic.argument_kind GEN-14",
        ]
    );
    assert_eq!(
        messages(&checked),
        [
            "`Box` takes a type for `T`, and `3` is a value",
            "`Box` takes a type for `T`, and `1 + 2` is a value",
            "`Row` takes a type for `Ts`, and `3` is a value",
            "`Sized` takes a value for `N`, and `i64` is a type",
            "`Sized` takes a value for `N`, and `Side` is a type",
            "`Count` takes a value for `N`, and `list<i64>` is a type",
            "`Box` takes a type for `T`, and `7` is a value",
            "`Sized` takes a value for `N`, and `One` is a type",
            "`Pair` takes a value for `N`, and `i64` is a type",
            "`Row` takes a type for `Ts`, and `7` is a value",
            "`Pair` takes a value for `N`, and `Two` passes on types",
            "`Pair` takes a value for `N`, and `Some` passes on types",
            "`Sized` takes a value for `N`, and `T` is a type",
        ]
    );
    assert_eq!(
        kinds(&checked, "good"),
        ["T:type", "N:value i64", "M:value _"]
    );
    assert_eq!(checked.prelude_names("m"), ["Missing", "make"]);
}
