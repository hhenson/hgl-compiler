//! The analyser against the rules on the form of a declaration: what
//! the grammar admits and a rule rejects. Each test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::check;

/// No lines.
const NONE: [&str; 0] = [];

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
            "fn both<T, ...Fields>(first: T = seed(), rest: ...T, fields: ...{Fields}) {}\n",
            "operator join<...Ts{2:8}>(parts: ...Ts) -> str\n",
            "native fn collect<T>(items: ...T) -> list<T>\n",
            "const fn seed() -> i64 => 1\n",
        ),
    )]);
    assert_eq!(checked.summary(), NONE);
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
