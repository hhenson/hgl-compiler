//! The types a program writes: what each means, and what it may be made
//! of.

mod common;

use common::{Checked, check};
use hgl_analyser::{Detail, Type};

/// No findings.
const NONE: [&str; 0] = [];

/// Checks one file, `m.hgl`, of module `m`. The `module` line is added,
/// so the text given starts on line 2.
fn module(text: &str) -> Checked {
    check(&[("m.hgl", &format!("module m\n{text}"))])
}

/// Every type written in module `m`, as HGL writes it, in source order.
fn written(checked: &Checked) -> Vec<String> {
    checked
        .program
        .module("m")
        .expect("module m exists")
        .written
        .iter()
        .map(|written| written.ty.to_string())
        .collect()
}

/// What was read of the declaration `name` of module `m`.
fn detail<'a>(checked: &'a Checked, name: &str) -> &'a Detail {
    let module = checked.program.module("m").expect("module m exists");
    let index = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == name)
        .expect("the declaration exists");
    &module.details[index]
}

/// The signature of the callable `name` of module `m`, as text.
fn signature(checked: &Checked, name: &str) -> String {
    match detail(checked, name) {
        Detail::Callable(signature) => signature.to_string(),
        other => panic!("{name} is {other:?}"),
    }
}

// ---- What a written type means ----

/// The grammar's `type`, TYPE-1 and TYPE-2: every form of type is read,
/// and written back as HGL writes it.
#[test]
fn every_form_of_type_is_read() {
    let checked = module(concat!(
        "struct Quote { bid: f64 }\n",
        "struct Box<T> { value: T }\n",
        "enum Side { buy, sell }\n",
        "native type Handle\n",
        "fn scalars(a: bool, b: i64, c: f64, d: str, e: bytes, f: any, g: date, h: time,\n",
        "    i: datetime, j: duration, k: civil_datetime, l: timezone, m: zoned_datetime,\n",
        "    n: zoned_time) {}\n",
        "fn shapes<T>(a: tuple<i64, str>, b: list<f64>, c: set<str>, d: dict<str, f64>,\n",
        "    e: rolling<f64, 5m>, f: ref<f64>, g: signal, h: atomic<Quote>,\n",
        "    i: result<f64, error>, j: error, k: delta<list<f64>>, l: f64?, m: T)\n",
        "    -> list<dict<str, Box<Quote>>> {}\n",
        "fn named(a: Quote, b: Box<i64>, c: Side, d: Handle) {}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        signature(&checked, "scalars"),
        "(a: bool, b: i64, c: f64, d: str, e: bytes, f: any, g: date, h: time, i: datetime, \
         j: duration, k: civil_datetime, l: timezone, m: zoned_datetime, n: zoned_time)"
    );
    assert_eq!(
        signature(&checked, "shapes"),
        "<T>(a: tuple<i64, str>, b: list<f64>, c: set<str>, d: dict<str, f64>, \
         e: rolling<f64, 5m>, f: ref<f64>, g: signal, h: atomic<Quote>, i: result<f64, error>, \
         j: error, k: delta<list<f64>>, l: f64?, m: T) -> list<dict<str, Box<Quote>>>"
    );
    assert_eq!(
        signature(&checked, "named"),
        "(a: Quote, b: Box<i64>, c: Side, d: Handle)"
    );
}

/// What is read of a declaration: a callable's generic parameters,
/// parameters and result (FN-1, FN-8, GEN-10, GEN-13), and a struct's
/// parents and members (TYPE-3, TYPE-4).
#[test]
fn a_declarations_types_are_recorded() {
    let checked = module(concat!(
        "abstract struct Base<T> { id: T }\n",
        "struct Quote<T> : Base<T> {\n",
        "    bid: f64\n",
        "    ask: f64 = 0.0\n",
        "    id = 7\n",
        "    time: datetime?\n",
        "}\n",
        "const fn scale<const N: i64>(value: f64, factor: f64 = 2.0) -> list<f64, N> => [value]\n",
        "fn sink(value: f64, const depth: i64 = 3) {}\n",
        "operator join<T, ...Rest{1:4}>(first: T, rest: ...Rest, named: ...{Rest}) -> str\n",
        "native fn collect<...Ts>(items: ...Ts) -> list<any>\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        signature(&checked, "scale"),
        "<const N: i64>(value: f64, factor: f64 = ...) -> list<f64, N>"
    );
    // FN-1: "an omitted result arrow means outputless".
    assert_eq!(
        signature(&checked, "sink"),
        "(value: f64, const depth: i64 = ...)"
    );
    assert_eq!(
        signature(&checked, "join"),
        "<T, ...Rest{1:4}>(first: T, rest: ...Rest, named: ...{Rest}) -> str"
    );
    assert_eq!(
        signature(&checked, "collect"),
        "<...Ts>(items: ...Ts) -> list<any>"
    );

    let Detail::Struct(quote) = detail(&checked, "Quote") else {
        panic!("Quote is a struct");
    };
    let parents: Vec<String> = quote.parents.iter().map(Type::to_string).collect();
    assert_eq!(parents, ["Base<T>"]);
    // TYPE-4: `id = 7` has no type; it overrides a parent's default.
    // LEX-9: `time` is a member name here.
    let fields: Vec<String> = quote
        .fields
        .iter()
        .map(|field| {
            format!(
                "{}: {}{}",
                field.name,
                field.ty.as_ref().map_or("-".to_string(), Type::to_string),
                if field.has_default { " = ..." } else { "" }
            )
        })
        .collect();
    assert_eq!(
        fields,
        [
            "bid: f64",
            "ask: f64 = ...",
            "id: - = ...",
            "time: datetime?"
        ]
    );
    let Detail::Struct(base) = detail(&checked, "Base") else {
        panic!("Base is a struct");
    };
    assert!(base.parents.is_empty());
    assert_eq!(base.generics.len(), 1);
    assert_eq!(
        base.fields[0].ty.as_ref().map(Type::to_string).as_deref(),
        Some("T")
    );
}

/// Types are read wherever they are written, each once: in signatures,
/// fields, locals, anonymous functions, constructors, constraints and
/// tests. A type inside another is part of it.
#[test]
fn types_are_read_wherever_they_are_written() {
    let checked = module(concat!(
        "struct Box<T> { value: T }\n",
        "fn node<T>(value: T) -> i64 requires T in {i64, f64} {\n",
        "    state held: list<i64, 4> = []\n",
        "    cache seen: set<str> = []\n",
        "    when {\n",
        "        let boxed: Box<i64> = Box<i64>(value: 1)\n",
        "        var total: i64\n",
        "        let change = delta<list<f64>>(added: [])\n",
        "        let adder = fn(x: i64) -> i64 => x + 1\n",
        "        return 0\n",
        "    }\n",
        "}\n",
        "test reads { let pair: tuple<i64, str> = (1, \"a\") }\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        written(&checked),
        [
            "T",
            "T",
            "i64",
            "T",
            "i64",
            "f64",
            "list<i64, 4>",
            "set<str>",
            "Box<i64>",
            "Box<i64>",
            "i64",
            "delta<list<f64>>",
            "i64",
            "i64",
            "tuple<i64, str>",
        ]
    );
}

/// UNIT-4: "Types have module-qualified nominal identity ... An alias
/// does not create a new identity. Referencing the same public type
/// through two imports preserves one identity and its generic
/// arguments."
#[test]
fn a_type_is_one_type_however_it_is_reached() {
    let checked = check(&[
        (
            "lib.hgl",
            concat!(
                "module lib\n",
                "export struct Quote { bid: f64 }\n",
                "export struct Box<T> { value: T }\n",
            ),
        ),
        (
            "other.hgl",
            "module other\nexport struct Quote { bid: f64 }\n",
        ),
        (
            "m.hgl",
            concat!(
                "module m\n",
                "use lib as l\n",
                "use other as o\n",
                "fn aliased(a: l::Quote, b: l::Box<i64>, c: l::Box<f64>, d: o::Quote) {}\n",
            ),
        ),
        (
            "part.hgl",
            concat!(
                "module m part second\n",
                "use lib::{Quote, Box}\n",
                "fn imported(a: Quote, b: Box<i64>) {}\n",
            ),
        ),
    ]);
    assert_eq!(checked.summary(), NONE);
    let parameters = |name: &str| match detail(&checked, name) {
        Detail::Callable(signature) => signature
            .parameters
            .iter()
            .map(|parameter| parameter.ty.clone())
            .collect::<Vec<Type>>(),
        other => panic!("{name} is {other:?}"),
    };
    let aliased = parameters("aliased");
    let imported = parameters("imported");
    // One identity through an alias and through a selective import.
    assert_eq!(aliased[0], imported[0]);
    assert_eq!(aliased[1], imported[1]);
    // The generic arguments are part of it.
    assert_ne!(aliased[1], aliased[2]);
    // The declaring module is part of it: two modules' `Quote` are two
    // types, though both are written `Quote`.
    assert_ne!(aliased[0], aliased[3]);
    assert_eq!(aliased[0].to_string(), aliased[3].to_string());
}

// ---- Names in type positions ----

/// A name in a type position names a struct, an enum, a native type or
/// a generic parameter that is a type. GEN-13: "A generic parameter
/// denotes a type, type pack, or compile-time ordinary value." GEN-2: "A
/// type pack is a sequence, not a singular source type."
#[test]
fn a_name_in_a_type_position_is_a_type() {
    let checked = module(concat!(
        "struct Quote { bid: f64 }\n",
        "const fn helper() -> i64 => 1\n",
        "operator join(left: str, right: str) -> str\n",
        // A function, an operator, a local.
        "fn f1(value: helper) {}\n",
        "fn f2(value: join) {}\n",
        "const fn f3() -> i64 {\n",
        "    let size = 3\n",
        "    let items: list<size> = []\n",
        "    return 0\n",
        "}\n",
        // A value parameter and a type pack as one type.
        "fn f4<const N: i64>(value: N) {}\n",
        "fn f5<...Ts>(value: Ts, rest: list<Ts>) {}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:14 local:type/type.not_a_type UNIT-10",
            "m.hgl:6:14 local:type/type.not_a_type UNIT-10",
            "m.hgl:9:21 local:type/type.not_a_type UNIT-10",
            "m.hgl:12:28 local:type/type.not_a_type GEN-13",
            "m.hgl:13:21 local:type/type.not_a_type GEN-2",
            "m.hgl:13:36 local:type/type.not_a_type GEN-2",
        ]
    );
    let messages: Vec<&str> = checked
        .findings
        .iter()
        .map(|finding| finding.message.as_str())
        .collect();
    assert_eq!(
        messages,
        [
            "`helper` is a value function, not a type",
            "`join` is an operator, not a type",
            "`size` is a `let`, not a type",
            "`N` is a generic value parameter, not a type",
            "`Ts` is a type pack: a sequence of types, not one type",
            "`Ts` is a type pack: a sequence of types, not one type",
        ]
    );
}

/// The parser reads a lone name among generic arguments as a type
/// (`generic_argument = type | const_expression`). One that names a
/// value, a local or a parameter, is a constant argument; whether it may
/// stand in a constant is for the pass that evaluates constants.
#[test]
fn a_value_name_among_generic_arguments_is_a_constant() {
    let checked = module(concat!(
        "struct Sized<const N: i64> { width: i64 }\n",
        "const fn made(count: i64) -> i64 {\n",
        "    let n = 3\n",
        "    let first: Sized<n> = Sized<n>(width: 0)\n",
        "    let second: Sized<count> = Sized<count>(width: 0)\n",
        "    return 0\n",
        "}\n",
        // A function is no value by its name alone.
        "const fn seed() -> i64 => 7\n",
        "fn named(sized: Sized<seed>) {}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:10:23 local:type/type.not_a_type UNIT-10"]
    );
    let all = written(&checked);
    assert!(all.contains(&"Sized<n>".to_string()), "{all:?}");
    assert!(all.contains(&"Sized<count>".to_string()), "{all:?}");
}

/// UNIT-10: a parameter's type is read in the scope around the
/// parameters, so it does not see them: a name there that only a
/// parameter has is left for the prelude.
#[test]
fn a_parameters_type_does_not_see_the_parameters() {
    let checked = module("fn sized(value: i64, other: value) {}\n");
    assert_eq!(checked.summary(), NONE);
    assert_eq!(checked.prelude_names("m"), ["value"]);
    assert_eq!(signature(&checked, "sized"), "(value: i64, other: _)");
}

/// The places a generic parameter may be named: a type parameter as a
/// type; a type pack after `...`, in `...{ }` (FN-8), and as a generic
/// argument; a value parameter as a size and as a generic argument.
#[test]
fn a_generic_parameter_is_named_where_its_kind_fits() {
    let checked = module(concat!(
        "struct Row<...Ts> { width: i64 }\n",
        "struct Grid<const N: i64, const M: i64> { cells: list<f64, N> }\n",
        "fn types<T>(value: T, items: list<T>) -> T {}\n",
        "fn packs<...Ts>(rest: ...Ts, named: ...{Ts}) -> Row<Ts> {}\n",
        "fn values<const N: i64, const M: i64>(grid: Grid<N, M>, flat: list<f64, N * M>) {}\n",
        // UNIT-11: a name nothing binds is left for the prelude, and its
        // type is not known.
        "fn open(value: Missing, items: list<Missing<i64>>) {}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        signature(&checked, "packs"),
        "<...Ts>(rest: ...Ts, named: ...{Ts}) -> Row<Ts>"
    );
    assert_eq!(
        signature(&checked, "values"),
        "<const N: i64, const M: i64>(grid: Grid<N, M>, flat: list<f64, N * M>)"
    );
    assert_eq!(signature(&checked, "open"), "(value: _, items: list<_>)");
    assert_eq!(checked.prelude_names("m"), ["Missing"]);
}

/// GEN-2: "A generic struct application supplies all arguments ...
/// Partial applications, generic defaults, and `_` struct arguments are
/// not admitted." GEN-10: a cardinality bounds how many a pack takes.
#[test]
fn a_generic_type_is_given_the_arguments_it_declares() {
    let checked = module(concat!(
        "struct Quote { bid: f64 }\n",
        "enum Side { buy, sell }\n",
        "struct Box<T> { value: T }\n",
        "struct Pair<A, B> { first: A\n second: B }\n",
        "struct Row<...Ts> { width: i64 }\n",
        "struct Wide<T, ...Ts{1:2}> { width: i64 }\n",
        // As many as declared; none where none are needed.
        "fn good(a: Box<i64>, b: Pair<i64, str>, c: Row, d: Row<i64, str, f64>,\n",
        "    e: Wide<i64, str>, f: Wide<i64, str, f64>) {}\n",
        // None, too many, too few, and some for a type that takes none.
        "fn bare(a: Box) {}\n",
        "fn many(a: Box<i64, str>) {}\n",
        "fn few(a: Pair<i64>) {}\n",
        "fn plain(a: Quote<i64>, b: Side<i64>) {}\n",
        "fn bounded(a: Wide<i64>, b: Wide<i64, i64, i64, i64>) {}\n",
        "fn parameter<T>(a: T<i64>) {}\n",
        // A parent, a field and a constructor are no different.
        "abstract struct Base<T> { id: T }\n",
        "struct Child : Base { boxes: list<Box> }\n",
        "const fn made() -> Box<i64> => Box<i64, i64>(value: 1)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:11:12 local:type/generic.argument_count GEN-2",
            "m.hgl:12:12 local:type/generic.argument_count GEN-2",
            "m.hgl:13:11 local:type/generic.argument_count GEN-2",
            "m.hgl:14:13 local:type/generic.argument_count GEN-2",
            "m.hgl:14:28 local:type/generic.argument_count GEN-2",
            "m.hgl:15:15 local:type/generic.argument_count GEN-2",
            "m.hgl:15:29 local:type/generic.argument_count GEN-2",
            "m.hgl:16:20 local:type/generic.argument_count GEN-13",
            "m.hgl:18:16 local:type/generic.argument_count GEN-2",
            "m.hgl:18:35 local:type/generic.argument_count GEN-2",
            "m.hgl:19:32 local:type/generic.argument_count GEN-2",
        ]
    );
    let messages: Vec<&str> = checked
        .findings
        .iter()
        .map(|finding| finding.message.as_str())
        .collect();
    assert_eq!(
        messages,
        [
            "`Box` is a generic type and is given no arguments: it takes 1 argument",
            "`Box` takes 1 argument, and 2 are given",
            "`Pair` takes 2 arguments, and 1 is given",
            "`Quote` is not a generic type and takes no generic arguments",
            "`Side` is not a generic type and takes no generic arguments",
            "`Wide` takes from 2 to 3 arguments, and 1 is given",
            "`Wide` takes from 2 to 3 arguments, and 4 are given",
            "`T` is a generic parameter and takes no generic arguments",
            "`Base` is a generic type and is given no arguments: it takes 1 argument",
            "`Box` is a generic type and is given no arguments: it takes 1 argument",
            "`Box` takes 1 argument, and 2 are given",
        ]
    );
}

/// GEN-10: an argument that names a type pack passes on as many types
/// as the pack holds. The count is rejected when no length the pack may
/// have fits; a length that may fit is for the pass that resolves it.
#[test]
fn a_forwarded_pack_counts_for_what_it_may_hold() {
    let checked = module(concat!(
        "struct Row<...Ts{2}> { width: i64 }\n",
        "struct Box<T> { value: T }\n",
        "fn exact<...Us{2}>(row: Row<Us>) {}\n",
        "fn with_one<T, ...Us{1}>(row: Row<T, Us>) {}\n",
        "fn single<...Us{1}>(boxed: Box<Us>) {}\n",
        // Any number may be two.
        "fn open<...Us>(row: Row<Us>) {}\n",
        "fn at_least<...Us{1:*}>(row: Row<Us>) {}\n",
        // No length these may have is two, or one.
        "fn short<...Us{0:1}>(row: Row<Us>) {}\n",
        "fn long<...Us{3:*}>(row: Row<Us>) {}\n",
        "fn over<T, ...Us{2}>(row: Row<T, Us>) {}\n",
        "fn pair<...Us{2:3}>(boxed: Box<Us>) {}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:27 local:type/generic.argument_count GEN-2",
            "m.hgl:10:26 local:type/generic.argument_count GEN-2",
            "m.hgl:11:27 local:type/generic.argument_count GEN-2",
            "m.hgl:12:28 local:type/generic.argument_count GEN-2",
        ]
    );
    let messages: Vec<&str> = checked
        .findings
        .iter()
        .map(|finding| finding.message.as_str())
        .collect();
    assert_eq!(
        messages,
        [
            "`Row` takes 2 arguments, and from 0 to 1 are given",
            "`Row` takes 2 arguments, and at least 3 are given",
            "`Row` takes 2 arguments, and 3 are given",
            "`Box` takes 1 argument, and from 2 to 3 are given",
        ]
    );
}

/// GEN-10: a cardinality is any integer literal. Forwarded packs whose
/// lengths sum past what a `u64` holds are counted without wrapping
/// round: the fewest stays more than any type takes, and the most is no
/// limit.
#[test]
fn forwarded_packs_too_long_to_sum_are_still_counted() {
    let checked = module(concat!(
        "struct Row<...Ts{2}> { width: i64 }\n",
        "struct Any<...Ts> { width: i64 }\n",
        // Three times this is 2^64 + 2.
        "fn exact<...Us{6148914691236517206}>(row: Row<Us, Us, Us>) {}\n",
        "fn at_most<...Us{0:6148914691236517206}>(row: Row<Us, Us, Us>, rest: Any<Us, Us, Us>) {}\n",
        "fn open<...Us{6148914691236517206}>(rest: Any<Us, Us, Us>) {}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:4:43 local:type/generic.argument_count GEN-2"]
    );
    assert_eq!(
        checked.findings[0].message,
        "`Row` takes 2 arguments, and more than can be counted are given"
    );
}

/// TYPE-3: "Generic applications are invariant by origin and complete
/// canonical arguments." Two applications are one type when their
/// arguments are the same: a constant by its value, and one this pass
/// holds no value for by how it is written.
#[test]
fn a_generic_application_is_known_by_its_arguments() {
    let checked = module(concat!(
        "struct Flag<const On: bool> { width: i64 }\n",
        "struct Label<const Text: str> { width: i64 }\n",
        "struct Sized<const N: i64> { width: i64 }\n",
        "fn flags(a: Flag<true>, b: Flag<false>, c: Flag<true>) {}\n",
        "fn labels(a: Label<\"bid\">, b: Label<\"ask\">, c: Label<\"bid\">) {}\n",
        // The spaces inside a string are part of it; those between
        // tokens are not.
        "fn spaced(a: Label<\"bid ask\">, b: Label<\"bid  ask\">, c: Label< \"bid ask\" >,\n",
        "    d: Sized<2*3>, e: Sized<2  *  3>, f: Sized<limit( 1 )>, g: Sized<limit(1)>) {}\n",
        "fn sizes(a: Sized<6>, b: Sized<2 * 3>, c: Sized<7>, d: Sized<limit()>, e: Sized<limit()>,\n",
        "    f: Sized<other()>) {}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    let types = |name: &str| match detail(&checked, name) {
        Detail::Callable(signature) => signature
            .parameters
            .iter()
            .map(|parameter| parameter.ty.clone())
            .collect::<Vec<Type>>(),
        other => panic!("{name} is {other:?}"),
    };
    let flags = types("flags");
    assert_ne!(flags[0], flags[1]);
    assert_eq!(flags[0], flags[2]);
    let labels = types("labels");
    assert_ne!(labels[0], labels[1]);
    assert_eq!(labels[0], labels[2]);
    let spaced = types("spaced");
    assert_ne!(spaced[0], spaced[1]);
    assert_eq!(spaced[0], spaced[2]);
    assert_eq!(spaced[1].to_string(), "Label<\"bid  ask\">");
    assert_eq!(spaced[3], spaced[4]);
    assert_eq!(spaced[4].to_string(), "Sized<2 * 3>");
    // A call is compared as written until constants are evaluated,
    // token for token: the spacing between its tokens is no part of it.
    assert_eq!(spaced[5].to_string(), "Sized<limit( 1 )>");
    assert_eq!(spaced[5], spaced[6]);
    let sizes = types("sizes");
    // By value where it is folded; as written where it is not.
    assert_eq!(sizes[0], sizes[1]);
    assert_ne!(sizes[0], sizes[2]);
    assert_eq!(sizes[3], sizes[4]);
    assert_ne!(sizes[3], sizes[5]);
}

// ---- Ordinary value types ----

/// The grammar gives `value_type` to a set's element, a dictionary's
/// key, a window's value, an atomic payload and a result's payloads, and
/// `rolling`, `ref`, `atomic` and `signal` are not value types. TYPE-12:
/// "`ref<T>` ... is excluded from const values, atomic payloads, keys,
/// and window values."
#[test]
fn a_part_that_is_a_value_is_an_ordinary_value_type() {
    let checked = module(concat!(
        "struct Quote { bid: f64 }\n",
        // What may hold an endpoint or a temporal shape.
        "fn good(a: list<ref<f64>>, b: dict<str, ref<f64>>, c: tuple<ref<f64>, atomic<Quote>>,\n",
        "    d: dict<str, rolling<f64, 5>>, e: ref<list<f64>>, f: delta<ref<f64>>,\n",
        "    g: set<tuple<i64, list<str>>>, h: atomic<dict<str, list<Quote>>>) {}\n",
        "fn sets(a: set<ref<f64>>, b: set<rolling<f64, 5>>, c: set<atomic<Quote>>) {}\n",
        "fn keys(a: dict<ref<str>, f64>, b: dict<atomic<Quote>, f64>) {}\n",
        "fn windows(a: rolling<ref<f64>, 5>, b: rolling<rolling<f64, 5>, 5>) {}\n",
        "fn payloads(a: atomic<ref<f64>>, b: atomic<atomic<Quote>>) {}\n",
        "fn results(a: result<ref<f64>, error>, b: result<f64, rolling<f64, 5>>) {}\n",
        // A value is made of values, however deep.
        "fn deep(a: set<list<tuple<i64, ref<f64>>>>, b: atomic<dict<str, signal>>) {}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:16 local:type/type.not_ordinary TYPE-12",
            "m.hgl:6:34 local:type/type.not_ordinary TYPE-9",
            "m.hgl:6:59 local:type/type.not_ordinary TYPE-9",
            "m.hgl:7:17 local:type/type.not_ordinary TYPE-12",
            "m.hgl:7:41 local:type/type.not_ordinary TYPE-9",
            "m.hgl:8:23 local:type/type.not_ordinary TYPE-12",
            "m.hgl:8:48 local:type/type.not_ordinary TYPE-11",
            "m.hgl:9:23 local:type/type.not_ordinary TYPE-12",
            "m.hgl:9:44 local:type/type.not_ordinary TYPE-9",
            "m.hgl:10:22 local:type/type.not_ordinary TYPE-12",
            "m.hgl:10:55 local:type/type.not_ordinary TYPE-13",
            "m.hgl:11:32 local:type/type.not_ordinary TYPE-12",
            "m.hgl:11:65 local:type/type.not_ordinary TYPE-12",
        ]
    );
    assert_eq!(
        checked.findings[0].message,
        "`ref<f64>` is not an ordinary value type, and a set's element is one"
    );
    assert_eq!(
        checked.findings[6].message,
        "`rolling<f64, 5>` is not an ordinary value type, and a window's value is one"
    );
    assert_eq!(
        checked.findings[12].message,
        "`signal` is not an ordinary value type, and an atomic payload is one"
    );
}

/// The grammar gives `value_type` to a `const` parameter (FN-1), a
/// parameter with a default (UNIT-8), a generic value parameter
/// (GEN-13: "a value parameter of ordinary type V") and the domain of an
/// operator's properties (GEN-9). TYPE-12: `ref` and `signal` are no
/// const value.
#[test]
fn constant_configuration_is_an_ordinary_value_type() {
    let checked = module(concat!(
        "fn good(value: ref<f64>, const depth: i64, const names: list<str> = []) {}\n",
        "const fn defaults(value: f64, factor: f64 = 2.0, window: rolling<f64, 5> = []) -> f64 => 1.0\n",
        "fn constants(const a: ref<f64>, const b: signal, const c: atomic<i64>) {}\n",
        "fn generic<const N: ref<i64>>(value: f64) {}\n",
        "operator add<T>(left: T, right: T) -> T\n",
        "    properties<i64, rolling<i64, 5>> { associative }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:58 local:type/type.not_ordinary UNIT-8",
            "m.hgl:4:23 local:type/type.not_ordinary TYPE-12",
            "m.hgl:4:42 local:type/type.not_ordinary TYPE-12",
            "m.hgl:4:59 local:type/type.not_ordinary FN-1",
            "m.hgl:5:21 local:type/type.not_ordinary TYPE-12",
            "m.hgl:7:21 local:type/type.not_ordinary GEN-9",
        ]
    );
    assert_eq!(
        checked.findings[0].message,
        "`rolling<f64, 5>` is not an ordinary value type, and a parameter with a default is one"
    );
    assert_eq!(
        checked.findings[4].message,
        "`ref<i64>` is not an ordinary value type, and a generic value parameter is one"
    );
}

// ---- Endpoints ----

/// TYPE-12: "`signal` is a complete temporal input constraint with no
/// readable payload; it is not a result, nested shape, field, or const
/// value."
#[test]
fn signal_is_the_whole_type_of_an_input() {
    let checked = module(concat!(
        "fn good(tick: signal, value: f64) -> f64 {}\n",
        "native fn pulse(tick: signal)\n",
        "fn output() -> signal {}\n",
        "struct Field { tick: signal }\n",
        "fn nested(a: list<signal>, b: ref<signal>, c: tuple<i64, signal>, d: dict<str, signal>) {}\n",
        // FN-8: each argument a positional pack captures is an input.
        "fn packed(ticks: ...signal) {}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:16 local:type/signal.position TYPE-12",
            "m.hgl:5:22 local:type/signal.position TYPE-12",
            "m.hgl:6:19 local:type/signal.position TYPE-12",
            "m.hgl:6:35 local:type/signal.position TYPE-12",
            "m.hgl:6:58 local:type/signal.position TYPE-12",
            "m.hgl:6:80 local:type/signal.position TYPE-12",
        ]
    );
    assert_eq!(
        checked.findings[0].message,
        "`signal` is the whole type of an input, and nothing else"
    );
}

/// TYPE-14: "`T?` denotes an ordinary value that is either a T or null,
/// where T is an admitted ordinary value type." TYPE-15: "The suffix
/// applies to the complete preceding type ... applying it to `ref<T>` or
/// `signal` rejects under TYPE-12/14 rather than creating nullable
/// endpoint handles."
#[test]
fn a_nullable_type_is_an_ordinary_value_or_null() {
    let checked = module(concat!(
        "const fn good(a: f64?, b: list<f64?>, c: list<f64>?, d: dict<str, f64?>?) -> f64? => a\n",
        "fn endpoints(a: ref<f64>?, b: signal?, c: list<ref<f64>?>) -> ref<f64>? {}\n",
        // The whole of what `?` is applied to is an ordinary value.
        "fn shapes(a: rolling<f64, 5>?, b: atomic<i64>?, c: list<ref<f64>>?,\n",
        "    d: dict<str, tuple<i64, rolling<f64, 5>>>?) {}\n",
        // Reported once, as what a set's element must be.
        "fn once(a: set<ref<f64>?>) {}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:17 local:type/type.not_ordinary TYPE-14",
            "m.hgl:3:31 local:type/type.not_ordinary TYPE-14",
            "m.hgl:3:48 local:type/type.not_ordinary TYPE-14",
            "m.hgl:3:63 local:type/type.not_ordinary TYPE-14",
            "m.hgl:4:14 local:type/type.not_ordinary TYPE-14",
            "m.hgl:4:35 local:type/type.not_ordinary TYPE-14",
            "m.hgl:4:57 local:type/type.not_ordinary TYPE-14",
            "m.hgl:5:29 local:type/type.not_ordinary TYPE-14",
            "m.hgl:6:16 local:type/type.not_ordinary TYPE-12",
        ]
    );
    assert_eq!(
        checked.findings[0].message,
        "`ref<f64>` is not an ordinary value type, and what `?` is applied to is one"
    );
    assert_eq!(
        checked.findings[4].message,
        "`rolling<f64, 5>` is not an ordinary value type, and what `?` is applied to is one"
    );
    // TYPE-15: "`list<f64?>` has nullable elements; `list<f64>?` is a
    // nullable list value." They are two types.
    assert_eq!(
        signature(&checked, "good"),
        "(a: f64?, b: list<f64?>, c: list<f64>?, d: dict<str, f64?>?) -> f64?"
    );
}

// ---- Sizes ----

/// TYPE-10: "`list<T>` denotes a growing shape; fixed sizes are
/// nonnegative constants. `unbounded` is the size sentinel. Size
/// participates in identity ... A tuple retains positional identity and
/// is not a fixed list alias."
#[test]
fn a_lists_size_is_a_nonnegative_count() {
    let checked = module(concat!(
        "fn good<const N: i64, M>(a: list<f64>, b: list<f64, 3>, c: list<f64, unbounded>,\n",
        "    d: list<f64, N>, e: list<f64, M>, f: list<f64, 0>, g: list<f64, 2 * 3>,\n",
        "    h: list<f64, (1 + 2)>, i: list<f64, 0x10>, j: list<f64, N + 1>, k: tuple<f64, f64>) {}\n",
        "fn negative(a: list<f64, -1>, b: list<f64, 2 - 3>) {}\n",
        "fn kinds(a: list<f64, 5m>, b: list<f64, 1.5>, c: list<f64, \"three\">, d: list<f64, true>) {}\n",
        "fn typed<const D: duration, const F: f64>(a: list<f64, D>, b: list<f64, F>) {}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:26 local:type/list.size TYPE-10",
            "m.hgl:5:44 local:type/list.size TYPE-10",
            "m.hgl:6:23 local:type/list.size TYPE-10",
            "m.hgl:6:41 local:type/list.size TYPE-10",
            "m.hgl:6:60 local:type/list.size TYPE-10",
            "m.hgl:6:83 local:type/list.size TYPE-10",
            "m.hgl:7:56 local:type/list.size TYPE-10",
            "m.hgl:7:73 local:type/list.size TYPE-10",
        ]
    );
    let messages: Vec<&str> = checked
        .findings
        .iter()
        .map(|finding| finding.message.as_str())
        .collect();
    assert_eq!(
        messages,
        [
            "a list's size is not negative",
            "a list's size is not negative",
            "a list's size is a count, and this is a duration",
            "a list's size is a count, and this is a float",
            "a list's size is a count, and this is a string",
            "a list's size is a count, and this is a bool",
            "a list's size is a count, and `D` is a constant of type `duration`",
            "a list's size is a count, and `F` is a constant of type `f64`",
        ]
    );

    // Size is part of what the type is, by its value and not its
    // spelling.
    let Detail::Callable(good) = detail(&checked, "good") else {
        panic!("good is a function");
    };
    let ty = |name: &str| {
        &good
            .parameters
            .iter()
            .find(|parameter| parameter.name == name)
            .expect("the parameter exists")
            .ty
    };
    assert_ne!(ty("a"), ty("b"));
    assert_ne!(ty("b"), ty("c"));
    assert_ne!(ty("b"), ty("f"));
    assert_eq!(ty("b"), ty("h"));
    assert_ne!(ty("d"), ty("e"));
    assert_eq!(ty("g").to_string(), "list<f64, 2 * 3>");
    assert_eq!(ty("i").to_string(), "list<f64, 0x10>");
    // A tuple of two is not a list of two.
    assert_ne!(ty("k").to_string(), "list<f64, 2>");
}

/// GEN-1: "Generic scope covers the whole declaration." A size that
/// names a generic value parameter is checked against that parameter's
/// type wherever the parameter is declared, before the use or after.
#[test]
fn a_size_is_checked_against_a_parameter_declared_after_it() {
    let checked = module(concat!(
        "fn good<const N: list<i64, C>, const C: i64>(value: f64) {}\n",
        "fn later<const N: list<i64, D>, const D: duration>(value: f64) {}\n",
        "fn earlier<const D: duration, const N: list<i64, D>>(value: f64) {}\n",
    ));
    // Each once: reading a type to learn it reports nothing.
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:29 local:type/list.size TYPE-10",
            "m.hgl:4:50 local:type/list.size TYPE-10",
        ]
    );
    assert_eq!(
        signature(&checked, "later"),
        "<const N: list<i64, D>, const D: duration>(value: f64)"
    );
    // And each written type is recorded once.
    assert_eq!(
        written(&checked),
        [
            "list<i64, C>",
            "i64",
            "f64",
            "list<i64, D>",
            "duration",
            "f64",
            "duration",
            "list<i64, D>",
            "f64",
        ]
    );
}

/// TYPE-11: "A rolling maximum/minimum has one constant kind: tick count
/// or duration. Omitted minimum equals maximum."
#[test]
fn a_windows_sizes_are_of_one_kind() {
    let checked = module(concat!(
        "fn good<const N: i64, const D: duration, M>(a: rolling<f64, 5>, b: rolling<f64, 5m>,\n",
        "    c: rolling<f64, 5, 2>, d: rolling<f64, 5m, 30s>, e: rolling<f64, N>,\n",
        "    f: rolling<f64, D, 1s>, g: rolling<f64, M, 2>, h: rolling<f64, N, 1>) {}\n",
        // The specification's illustration, then the other way round.
        "fn invalid(value: rolling<f64, 5m, 3>) { when {} }\n",
        "fn mixed(a: rolling<f64, 5, 1s>, b: rolling<f64, N, 1s>) {}\n",
        "fn mixed_generic<const N: i64, const D: duration>(a: rolling<f64, N, 1s>, b: rolling<f64, D, 1>) {}\n",
        // Neither a tick count nor a duration.
        "fn wrong(a: rolling<f64, 1.5>, b: rolling<f64, \"long\">, c: rolling<f64, 5, null>) {}\n",
        "fn wrong_generic<const F: f64>(a: rolling<f64, F>) {}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:36 type/rolling.size_kind TYPE-11",
            "m.hgl:6:29 type/rolling.size_kind TYPE-11",
            "m.hgl:7:70 type/rolling.size_kind TYPE-11",
            "m.hgl:7:94 type/rolling.size_kind TYPE-11",
            "m.hgl:8:26 type/rolling.size_kind TYPE-11",
            "m.hgl:8:48 type/rolling.size_kind TYPE-11",
            "m.hgl:8:76 type/rolling.size_kind TYPE-11",
            "m.hgl:9:48 type/rolling.size_kind TYPE-11",
        ]
    );
    assert_eq!(
        checked.findings[0].message,
        "a window's sizes are of one kind: the maximum is a duration and the minimum is a tick \
         count"
    );
    assert_eq!(
        checked.findings[4].message,
        "a window's size is a tick count or a duration, and this is a float"
    );
    assert_eq!(
        checked.findings[7].message,
        "a window's size is a tick count or a duration, and `F` is a constant of type `f64`"
    );
    // `N` in `mixed` is left for the prelude: its kind is not known.
    assert_eq!(checked.prelude_names("m"), ["N"]);
}

/// TYPE-11: "Omitted minimum equals maximum ... Size/kind are part of
/// identity." A window whose minimum is written as its maximum is the
/// window with no minimum written.
#[test]
fn an_omitted_minimum_is_the_maximum() {
    let checked = module(concat!(
        "fn windows<const N: i64>(a: rolling<i64, 5>, b: rolling<i64, 5, 5>, c: rolling<i64, 5, 2>,\n",
        "    d: rolling<f64, 5m>, e: rolling<f64, 5m, 300s>, f: rolling<f64, N>, g: rolling<f64, N, N>,\n",
        "    h: rolling<i64, 2 + 3, 5>) {}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    let Detail::Callable(windows) = detail(&checked, "windows") else {
        panic!("windows is a function");
    };
    let ty = |name: &str| {
        &windows
            .parameters
            .iter()
            .find(|parameter| parameter.name == name)
            .expect("the parameter exists")
            .ty
    };
    assert_eq!(ty("a"), ty("b"));
    assert_eq!(ty("b").to_string(), "rolling<i64, 5>");
    assert_ne!(ty("a"), ty("c"));
    assert_eq!(ty("d"), ty("e"));
    assert_eq!(ty("f"), ty("g"));
    assert_eq!(ty("a"), ty("h"));
}

/// TYPE-11: "Tick sizes are positive; duration maximum is positive and
/// minimum may be zero. Minimum cannot exceed maximum."
#[test]
fn a_windows_sizes_are_in_bounds() {
    let checked = module(concat!(
        "fn good(a: rolling<f64, 1>, b: rolling<f64, 5, 5>, c: rolling<f64, 1us>,\n",
        "    d: rolling<f64, 5m, 0s>, e: rolling<f64, 5m, 5m>, f: rolling<f64, 2 * 3, 6>) {}\n",
        "fn ticks(a: rolling<f64, 0>, b: rolling<f64, -5>, c: rolling<f64, 5, 0>, d: rolling<f64, 5, 6>) {}\n",
        "fn durations(a: rolling<f64, 0s>, b: rolling<f64, -5m>, c: rolling<f64, 5m, -1s>,\n",
        "    d: rolling<f64, 5m, 10m>, e: rolling<f64, 1h, 61m>) {}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:26 type/rolling.size_bounds TYPE-11",
            "m.hgl:4:46 type/rolling.size_bounds TYPE-11",
            "m.hgl:4:70 type/rolling.size_bounds TYPE-11",
            "m.hgl:4:93 type/rolling.size_bounds TYPE-11",
            "m.hgl:5:30 type/rolling.size_bounds TYPE-11",
            "m.hgl:5:51 type/rolling.size_bounds TYPE-11",
            "m.hgl:5:77 type/rolling.size_bounds TYPE-11",
            "m.hgl:6:25 type/rolling.size_bounds TYPE-11",
            "m.hgl:6:51 type/rolling.size_bounds TYPE-11",
        ]
    );
    let messages: Vec<&str> = checked
        .findings
        .iter()
        .map(|finding| finding.message.as_str())
        .collect();
    assert_eq!(
        messages,
        [
            "a window holds at least one tick",
            "a window holds at least one tick",
            "a window's minimum is at least one tick",
            "a window's minimum cannot exceed its maximum",
            "a window's duration is more than zero",
            "a window's duration is more than zero",
            "a window's minimum duration is not negative",
            "a window's minimum cannot exceed its maximum",
            "a window's minimum cannot exceed its maximum",
        ]
    );
}

/// LEX-7: "Minus is an operator, not part of a numeric literal ... the
/// full signed minimum" is a minus before 2^63, which is a constant like
/// any other.
#[test]
fn a_minus_before_two_to_the_sixty_third_is_the_least_integer() {
    let checked = module(concat!(
        "fn least(a: list<f64, -9223372036854775808>, b: list<f64, -9223372036854775807 - 1>) {}\n",
        // 2^63 is no `i64` by itself, and what overflows is not a size
        // this pass can judge: both are left for the pass that evaluates
        // constants.
        "fn left(a: list<f64, 9223372036854775807 + 1>, b: list<f64, -(9223372036854775808)>) {}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:23 local:type/list.size TYPE-10",
            "m.hgl:2:59 local:type/list.size TYPE-10",
        ]
    );
}

// ---- Defaults ----

/// UNIT-8: "Value-function parameters and const configuration may have
/// defaults; ordinary temporal parameters may not."
#[test]
fn a_temporal_parameter_has_no_default() {
    let checked = module(concat!(
        "const fn value(first: f64, second: f64 = 2.0) -> f64 => first\n",
        "fn configured(first: f64, const second: f64 = 2.0) -> f64 {}\n",
        "native const fn native_value(first: f64 = 1.0) -> f64\n",
        "fn temporal(first: f64, second: f64 = 2.0) -> f64 {}\n",
        "native fn native_temporal(first: f64 = 1.0) -> f64\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:25 local:function-kind/default.temporal_parameter UNIT-8",
            "m.hgl:6:27 local:function-kind/default.temporal_parameter UNIT-8",
        ]
    );
    assert_eq!(
        checked.findings[0].message,
        "`second` is a temporal parameter and cannot have a default"
    );
}

// ---- What is not known ----

/// ERR-9: "Do not demand cascaded diagnostics after an earlier error
/// prevents checking." A type that is not known is reported once, where
/// it is wrong, and what holds it is still checked.
#[test]
fn an_unknown_type_is_reported_once_and_its_surroundings_are_checked() {
    let checked = module(concat!(
        "const fn helper() -> i64 => 1\n",
        "fn f(a: set<helper>, b: rolling<helper, 5m, 3>, c: list<Missing, -1>) {}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:13 local:type/type.not_a_type UNIT-10",
            "m.hgl:3:33 local:type/type.not_a_type UNIT-10",
            "m.hgl:3:45 type/rolling.size_kind TYPE-11",
            "m.hgl:3:66 local:type/list.size TYPE-10",
        ]
    );
    assert_eq!(
        signature(&checked, "f"),
        "(a: set<_>, b: rolling<_, 5m, 3>, c: list<_, -1>)"
    );
}
