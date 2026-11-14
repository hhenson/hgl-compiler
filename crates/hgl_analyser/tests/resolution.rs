//! The analyser's resolution pass against the rules it implements: what
//! each name in a signature, body, type or constraint is bound to, and
//! which bindings a scope does not admit. Each test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, check};
use hgl_analyser::Binding;

/// No lines.
const NONE: [&str; 0] = [];

/// Each resolved use of module `path` as `name line:column -> what`,
/// where `what` is the kind of a local binding or the module and kinds
/// of the declarations found.
fn uses(checked: &Checked, path: &str) -> Vec<String> {
    let module = checked.program.module(path).expect("the module exists");
    module
        .uses
        .iter()
        .map(|used| {
            let place = checked
                .sources
                .file(used.at.file)
                .line_col(used.at.span.start);
            let what = match &used.binding {
                Binding::Local { kind, .. } => format!("{kind:?}"),
                Binding::Declarations {
                    module,
                    declarations,
                } => {
                    let module = &checked.program.modules[*module];
                    let kinds: Vec<String> = declarations
                        .iter()
                        .map(|&index| module.declarations[index].kind.describe())
                        .collect();
                    format!("{} {}", module.path, kinds.join(" + "))
                }
            };
            format!("{} {place} -> {what}", used.name)
        })
        .collect()
}

/// A module `m` with one file holding `body`.
fn module(body: &str) -> Checked {
    let text = format!("module m\n{body}");
    check(&[("m.hgl", text.as_str())])
}

// ---- UNIT-10: what a name resolves to ----

/// UNIT-10: "A name resolves to the one binding in scope for it: an
/// earlier local or iteration binding of an enclosing block; a parameter
/// of an enclosing function; a generic parameter of an enclosing
/// declaration; a module declaration; a selective import; or the
/// prelude."
#[test]
fn a_name_resolves_to_the_one_binding_in_scope() {
    let checked = check(&[
        (
            "lib.hgl",
            "module lib\nexport const fn shared() -> i64 => 1\nexport struct Quote { bid: f64 }\n",
        ),
        (
            "m.hgl",
            concat!(
                "module m\n",
                "use lib::{shared}\n",
                "use lib as l\n",
                "const fn pick<T>(first: T, items: list<T>) -> T {\n",
                "    let local = first\n",
                "    var count = shared()\n",
                "    for index, item in items {\n",
                "        count = helper(index) + count\n",
                "        let seen: l::Quote = make(item, local)\n",
                "    }\n",
                "    return first\n",
                "}\n",
                // UNIT-6: a module declaration may be used before it.
                "const fn helper(value: i64) -> i64 => value\n",
            ),
        ),
    ]);
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        uses(&checked, "m"),
        [
            "T 4:25 -> GenericParameter",
            "T 4:40 -> GenericParameter",
            "T 4:47 -> GenericParameter",
            "first 5:17 -> Parameter",
            "shared 6:17 -> lib value function",
            "items 7:24 -> Parameter",
            "count 8:9 -> Var",
            "helper 8:17 -> m value function",
            "index 8:24 -> Iteration",
            "count 8:33 -> Var",
            "Quote 9:22 -> lib struct",
            "item 9:35 -> Iteration",
            "local 9:41 -> Let",
            "first 11:12 -> Parameter",
            "value 13:39 -> Parameter",
        ]
    );
    // UNIT-11: `make` is bound by nothing in the program, so it is left
    // for the prelude and not reported.
    assert_eq!(checked.prelude_names("m"), ["make"]);
}

/// UNIT-10 binds names, and these are not names in scope: a member after
/// `.`, the label of an argument or of an operator's configuration, a
/// field of a struct, a member of an enum.
#[test]
fn members_and_labels_are_not_looked_up() {
    let checked = module(concat!(
        "struct Quote {\n    bid: f64\n    ask: f64 = 0.0\n}\n",
        "enum Side { buy, sell = 2 }\n",
        "const fn spread(quote: Quote, side: Side) -> f64 {\n",
        "    let made = Quote(bid: 1.0, ask: 2.0)\n",
        "    let scaled = quote.ask /{zero: quote.bid} made.bid\n",
        "    if side == Side.buy { return quote.bid.abs() }\n",
        "    return scaled\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(checked.prelude_names("m"), NONE);
    let names: Vec<String> = uses(&checked, "m")
        .iter()
        .map(|line| line.split(' ').next().unwrap().to_string())
        .collect();
    assert_eq!(
        names,
        [
            "Quote", "Side", "Quote", "quote", "quote", "made", "side", "Side", "quote", "scaled"
        ]
    );
}

/// UNIT-11: "Every module binds [the prelude's declarations] without a
/// `use`." Core declares none yet, so a name nothing binds is listed
/// once for its module, at its first use, and not reported.
#[test]
fn a_name_nothing_binds_is_left_for_the_prelude() {
    let checked = module(concat!(
        "fn pass(value: i64) -> i64 {\n",
        "    when modified(value) && valid(value) { return delta_value(value) }\n",
        "}\n",
        "fn again(value: i64) -> i64 {\n",
        "    when { return delta_value(value) }\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        checked.prelude_names("m"),
        ["modified", "valid", "delta_value"]
    );
}

// ---- UNIT-12 and UNIT-5: one binding for a name ----

/// UNIT-12: "A local or iteration binding, parameter, or generic
/// parameter takes no name bound there by an enclosing block or function,
/// a module declaration, a `use` in its file". UNIT-5: "Duplicate ...
/// parameter names ... or local bindings in one scope are rejected."
#[test]
fn a_binding_takes_no_name_already_in_scope() {
    let checked = check(&[
        ("lib.hgl", "module lib\nexport fn shared() {}\n"),
        (
            "m.hgl",
            concat!(
                "module m\n",
                "use lib::{shared}\n",
                "use lib as l\n",
                "const fn seed() -> i64 => 7\n",
                // A parameter named for a declaration; one named twice;
                // one named for a generic parameter.
                "const fn a<T>(seed: i64, twice: i64, twice: i64, T: i64) -> i64 => 1\n",
                "const fn b(base: i64) -> i64 {\n",
                // A local named for a parameter, an import, an alias.
                "    let base = 1\n",
                "    let shared = 2\n",
                "    var l = 3\n",
                "    let outer = 4\n",
                "    if base == 1 {\n",
                // A local of an inner block named for one around it.
                "        let outer = 5\n",
                "        for outer, fine in [1, 2] { }\n",
                "    }\n",
                // Twice in one block.
                "    let outer = 6\n",
                "    return base\n",
                "}\n",
                "fn c<U, U>() {}\n",
            ),
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:15 name/name.duplicate UNIT-12",
            "m.hgl:5:38 name/name.duplicate UNIT-5",
            "m.hgl:5:50 name/name.duplicate UNIT-5",
            "m.hgl:7:9 name/name.duplicate UNIT-12",
            "m.hgl:8:9 name/name.duplicate UNIT-12",
            "m.hgl:9:9 name/name.duplicate UNIT-12",
            "m.hgl:12:13 name/name.duplicate UNIT-12",
            "m.hgl:13:13 name/name.duplicate UNIT-12",
            "m.hgl:15:9 name/name.duplicate UNIT-5",
            "m.hgl:18:9 name/name.duplicate UNIT-5",
        ]
    );
}

/// UNIT-12: "Two blocks, neither enclosing the other, may each bind the
/// same local name", and the same holds for two declarations.
#[test]
fn blocks_that_do_not_enclose_one_another_may_reuse_a_name() {
    let checked = module(concat!(
        "const fn first(value: i64) -> i64 {\n",
        "    if value == 1 {\n        let local = 1\n        return local\n    }\n",
        "    if value == 2 {\n        let local = 2\n        return local\n    }\n",
        "    for local in [1, 2] { }\n",
        "    return value\n",
        "}\n",
        "const fn second<T>(value: T) -> T {\n    let local = value\n    return local\n}\n",
        "const fn third<T>(value: T) -> T => value\n",
    ));
    assert_eq!(checked.summary(), NONE);
}

/// UNIT-12's own examples: "a first parameter named `seed`, a local named
/// `base` in a block body of `choose`, a local of an inner block named
/// for an earlier local of a block around it ... takes a name already in
/// scope".
#[test]
fn the_chapters_rejections_are_rejected() {
    let accepted = module(concat!(
        "const fn seed() -> i64 => 7\n",
        "const fn choose(base: i64, value: i64 = seed()) -> i64 => base + value\n",
        "test defining_closure {\n    assert choose(1) == 8\n}\n",
    ));
    assert_eq!(accepted.summary(), NONE);

    let rejected = module(concat!(
        "const fn seed() -> i64 => 7\n",
        "const fn first(seed: i64) -> i64 => 1\n",
        "const fn choose(base: i64) -> i64 {\n    let base = 1\n    return base\n}\n",
        "const fn inner(base: i64) -> i64 {\n",
        "    let earlier = 1\n",
        "    if base == 1 {\n        let earlier = 2\n    }\n",
        "    return earlier\n",
        "}\n",
        // UNIT-7: "a default of `base` in place of `seed()` reads a
        // formal argument".
        "const fn defaulted(base: i64, value: i64 = base) -> i64 => value\n",
    ));
    assert_eq!(
        rejected.summary(),
        [
            "m.hgl:3:16 name/name.duplicate UNIT-12",
            "m.hgl:5:9 name/name.duplicate UNIT-12",
            "m.hgl:11:13 name/name.duplicate UNIT-12",
            "m.hgl:15:44 name/default.formal_argument UNIT-7",
        ]
    );
}

/// UNIT-12: "A block's scope is the scope around it plus its own
/// bindings; an anonymous function's is the scope around it plus its
/// parameters". The statements under a `switch` case are neither, and
/// the grammar gives a case no block, so what a case declares is
/// declared in the block the `switch` stands in. UNIT-5: "Duplicate ...
/// local bindings in one scope are rejected."
#[test]
fn a_switch_case_declares_in_the_block_around_its_switch() {
    let checked = module(concat!(
        "const fn pick(value: i64) -> i64 {\n",
        "    let before = early\n",
        "    switch value {\n",
        "        case 1:\n",
        "            let chosen = 1\n",
        "            let early = 2\n",
        "        case 2:\n",
        "            let chosen = 2\n",
        "        default:\n",
        "            let other = chosen\n",
        "    }\n",
        "    return other\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:18 name/name.used_before_declaration UNIT-6",
            "m.hgl:9:17 name/name.duplicate UNIT-5",
        ]
    );
    assert_eq!(checked.prelude_names("m"), NONE);
    let tail: Vec<String> = uses(&checked, "m")
        .into_iter()
        .filter(|line| line.starts_with("chosen") || line.starts_with("other"))
        .collect();
    assert_eq!(tail, ["chosen 11:25 -> Let", "other 13:12 -> Let"]);
}

// ---- UNIT-6 and UNIT-7 ----

/// UNIT-6: "Module declarations and shared test helpers may be referenced
/// forward. Ordinary block locals become visible after declaration."
#[test]
fn a_local_is_visible_after_its_declaration() {
    let checked = module(concat!(
        "const fn early() -> i64 {\n",
        "    let first = second\n",
        "    let second = 1\n",
        "    let own = own\n",
        "    if first == 1 { return later }\n",
        "    let later = 2\n",
        "    return forward()\n",
        "}\n",
        "const fn forward() -> i64 => 1\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:17 name/name.used_before_declaration UNIT-6",
            "m.hgl:5:15 name/name.used_before_declaration UNIT-6",
            "m.hgl:6:28 name/name.used_before_declaration UNIT-6",
        ]
    );
    assert_eq!(checked.prelude_names("m"), NONE);
}

/// UNIT-7: "Resolve defaults in the declaration's enclosing binding
/// closure with its generic environment, excluding the callable's formal
/// arguments and the struct's instance fields."
#[test]
fn a_default_reads_no_parameter_and_no_field() {
    let checked = module(concat!(
        "const fn seed() -> i64 => 7\n",
        "const fn sized<const N: i64>(count: i64 = N, base: i64 = seed(), more: i64 = count) -> i64 => 1\n",
        "struct Quote {\n    bid: f64 = 1.0\n    ask: f64 = bid\n    size = seed()\n}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:78 name/default.formal_argument UNIT-7",
            "m.hgl:6:16 name/default.formal_argument UNIT-7",
        ]
    );
}

// ---- FN-14: anonymous functions ----

/// FN-14: "An anonymous function's scope is the scope around it plus its
/// parameters (UNIT-12). From the functions and blocks around it, its
/// body uses only parameters, generic parameters, `let` locals, and
/// iteration bindings. Use of an enclosing `var`, `state`, or `cache`
/// rejects."
#[test]
fn an_anonymous_function_uses_only_what_fn_14_admits() {
    let checked = module(concat!(
        "fn node<T>(value: T, items: list<T>) -> T {\n",
        "    state kept = 0\n",
        "    cache seen = 0\n",
        "    inject out\n",
        "    when {\n",
        "        let fixed = 1\n",
        "        var moving = 2\n",
        "        for item in items {\n",
        // Parameters, a `let`, an iteration binding, its own parameter.
        "            let fine = fn(x) => x + fixed + item + value\n",
        "        }\n",
        "        let bad = fn(x) => x + moving + kept + seen\n",
        "        let worse = fn() => out\n",
        // Its parameters take no name around it.
        "        let clash = fn(fixed, value) => 1\n",
        // One inside another: the outer one's parameter is a parameter.
        "        let nested = fn(a) => fn(b) => a + b + moving\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:12:32 name/name.captured FN-14",
            "m.hgl:12:41 name/name.captured FN-14",
            "m.hgl:12:48 name/name.captured FN-14",
            "m.hgl:13:29 name/name.captured FN-14",
            "m.hgl:14:24 name/name.duplicate UNIT-12",
            "m.hgl:14:31 name/name.duplicate UNIT-12",
            "m.hgl:15:48 name/name.captured FN-14",
        ]
    );
}

// ---- TST-1 and TST-2: the test scope ----

/// TST-1: "A named test sees module-private declarations", and the
/// helpers of the module's test scope. TST-2: "Production cannot refer to
/// test-only declarations."
#[test]
fn production_names_no_test_only_declaration() {
    let checked = check(&[
        (
            "a.hgl",
            concat!(
                "module m\n",
                "const fn private() -> i64 => 1\n",
                "const fn production() -> Fixture => helper()\n",
                "test uses_both { assert helper() == private() }\n",
            ),
        ),
        (
            "b.hgl",
            concat!(
                "module m part tests\n",
                "test {\n",
                "    struct Fixture { x: i64 }\n",
                "    const fn helper() -> i64 => private()\n",
                // A helper's local takes no helper's name; a
                // production local may.
                "    const fn clash() -> i64 {\n        let helper = 1\n        return helper\n    }\n",
                "}\n",
                "const fn free() -> i64 {\n    let helper = 1\n    return helper\n}\n",
            ),
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "a.hgl:3:26 name/name.test_only TST-2",
            "a.hgl:3:37 name/name.test_only TST-2",
            "b.hgl:6:13 name/name.duplicate UNIT-12",
        ]
    );
}

// ---- UNIT-2: aliases ----

/// UNIT-2: "an alias qualifies a public interface with `alias::name`."
#[test]
fn an_alias_qualifies_a_public_declaration() {
    let checked = check(&[
        (
            "lib.hgl",
            concat!(
                "module lib\n",
                "export const fn shared() -> i64 => 1\n",
                "export struct Quote { bid: f64 }\n",
                "const fn hidden() -> i64 => 1\n",
            ),
        ),
        (
            "m.hgl",
            concat!(
                "module m\n",
                "use lib as l\n",
                "use lib::{shared}\n",
                "const fn uses(quote: l::Quote) -> i64 {\n",
                "    let a = l::shared() + shared()\n",
                "    let b = l::hidden()\n",
                "    let c = l::missing()\n",
                // An alias names nothing by itself.
                "    let d = l\n",
                // And only an alias goes before `::`.
                "    let e = shared::thing() + quote::bid + nothing::at_all()\n",
                "    return a\n",
                "}\n",
            ),
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:16 module/import.private UNIT-2",
            "m.hgl:7:16 name/import.unknown_name UNIT-2",
            "m.hgl:8:13 name/alias.unqualified UNIT-2",
            "m.hgl:9:13 name/alias.unknown UNIT-2",
            "m.hgl:9:31 name/alias.unknown UNIT-2",
            "m.hgl:9:44 name/alias.unknown UNIT-2",
        ]
    );
    assert_eq!(
        uses(&checked, "m")[..4],
        [
            "Quote 4:25 -> lib struct",
            "shared 5:16 -> lib value function",
            "shared 5:27 -> lib value function",
            "a 10:12 -> Let",
        ]
    );
    assert_eq!(checked.prelude_names("m"), NONE);
}

// ---- GEN-1 and GEN-14: generics ----

/// GEN-1: "Generic scope covers the whole declaration": the signature,
/// the constraints, the body. An `each` constraint binds its name inside
/// its braces.
#[test]
fn generic_scope_covers_the_whole_declaration() {
    let checked = module(concat!(
        "struct Box<T> { value: T }\n",
        "operator total<T, ...Ts, const N: i64>(first: T, rest: ...Ts) -> Box<T>\n",
        "    requires each U in types(Ts) { comparable(U, T) } && N == 3\n",
        "    properties<i64> { identity = N }\n",
        "impl fn total<T>(first: T, rest: ...T) -> Box<T> => Box<T>(value: first)\n",
        "instantiate total<i64, _>\n",
        // `U` is bound only inside its braces, and takes no name around
        // them.
        "fn outside<T, ...Ts>(value: T) requires each T in types(Ts) { numeric(T) } && comparable(U) {}\n",
        // The generic parameters are in scope of one another, whatever
        // their order: the type of `N` is the `T` after it.
        "fn ordered<const N: T, T>(value: T) {}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:8:46 name/name.duplicate UNIT-12"]
    );
    assert_eq!(
        checked.prelude_names("m"),
        ["types", "comparable", "numeric", "U"]
    );
    let ordered: Vec<String> = uses(&checked, "m")
        .into_iter()
        .filter(|line| line.contains(" 9:"))
        .collect();
    assert_eq!(
        ordered,
        ["T 9:21 -> GenericParameter", "T 9:34 -> GenericParameter"]
    );
    let total: Vec<String> = uses(&checked, "m")
        .into_iter()
        .filter(|line| line.starts_with("total") || line.starts_with("N "))
        .collect();
    assert_eq!(
        total,
        [
            "N 4:58 -> GenericParameter",
            "N 5:34 -> GenericParameter",
            "total 7:13 -> m operator",
        ]
    );
}

/// GEN-14: "directly following (LEX-27) a name resolved to a generic type
/// in an expression, `<` opens generic arguments rather than a
/// comparison ... Reject an invalid application without comparison
/// fallback. Ordinary `<` and `>` do not order type names."
#[test]
fn a_generic_types_name_decides_an_undecided_constructor() {
    let checked = module(concat!(
        "struct Box<T> { value: T }\n",
        "struct Plain { value: i64 }\n",
        "const fn decide(a: i64, b: i64, c: i64) -> bool {\n",
        // Both readings parse. `Box` is a generic type: a constructor.
        "    let made = Box<b>(c)\n",
        // `a` is a parameter: comparisons.
        "    let compared = a<b>(c)\n",
        // `Plain` is a type with no generic parameters: comparisons.
        "    let plain = Plain<b>(c)\n",
        // Nothing binds `unknown`: it is left for the prelude, and so
        // is the decision.
        "    let open = unknown<b>(c)\n",
        // A generic type before a `<` that opens no arguments.
        "    let ordered = Box < a\n",
        "    return compared\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:9:19 type/generic.invalid_application GEN-14"]
    );
    let decisions: Vec<(u32, Option<bool>)> = checked
        .program
        .module("m")
        .unwrap()
        .decisions
        .iter()
        .map(|decision| {
            (
                checked
                    .sources
                    .file(decision.at.file)
                    .line_col(decision.at.span.start)
                    .line,
                decision.constructor,
            )
        })
        .collect();
    assert_eq!(
        decisions,
        [
            (5, Some(true)),
            (6, Some(false)),
            (7, Some(false)),
            (8, None)
        ]
    );
    assert_eq!(checked.prelude_names("m"), ["unknown"]);
}

/// GEN-14: the same holds for a generic type reached as `alias::name`
/// or through a selective import.
#[test]
fn an_imported_generic_type_is_not_ordered_either() {
    let checked = check(&[
        ("lib.hgl", "module lib\nexport struct Box<T> { value: T }\n"),
        (
            "m.hgl",
            concat!(
                "module m\n",
                "use lib as l\n",
                "const fn first(a: i64) -> bool => l::Box < a\n",
                "const fn made(a: i64, b: i64) -> bool => l::Box<a>(b)\n",
            ),
        ),
        (
            "part.hgl",
            concat!(
                "module m part second\n",
                "use lib::{Box}\n",
                "const fn second(a: i64) -> bool => Box < a\n",
            ),
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:38 type/generic.invalid_application GEN-14",
            "part.hgl:3:36 type/generic.invalid_application GEN-14",
        ]
    );
    let decisions: Vec<Option<bool>> = checked
        .program
        .module("m")
        .unwrap()
        .decisions
        .iter()
        .map(|decision| decision.constructor)
        .collect();
    assert_eq!(decisions, [Some(true)]);
}

// ---- Scopes of the other statements ----

/// UNIT-12, TIME-5: `state`, `cache` and `inject` bind in their block as
/// a local does, a `for` pattern binds for its body alone, and a native
/// contract's `inject` names stand in its declaration's scope.
#[test]
fn every_binding_form_binds_in_its_scope() {
    let checked = module(concat!(
        "fn node(value: i64, items: list<i64>) -> i64 {\n",
        "    state kept: i64 = 0\n",
        "    cache seen = kept\n",
        "    inject out, clock\n",
        "    start { kept = 1 }\n",
        "    when value == kept {\n",
        "        for index, item in items { seen = item + index }\n",
        "        return out\n",
        "    }\n",
        "    stop { seen = index }\n",
        "}\n",
        "native fn tick(clock: i64) -> i64\n",
        "native fn tick(clock: i64) -> i64 {\n    inject out, clock\n    when;\n}\n",
        "fn logger(kept: i64) {\n    state kept = 0\n    inject logger\n}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:15:17 name/name.duplicate UNIT-5",
            "m.hgl:19:11 name/name.duplicate UNIT-12",
            "m.hgl:20:12 name/name.duplicate UNIT-12",
        ]
    );
    // `index` is bound for the `for` body alone.
    assert_eq!(checked.prelude_names("m"), ["index"]);
}

// ---- TIME-8: capabilities ----

/// TIME-8: "The injectables are `out`, `logger`, `clock`, and
/// `scheduler`, and the proposed `alarm` and `global_state` below;
/// `inject` names no other."
#[test]
fn inject_names_a_capability() {
    let checked = module(concat!(
        "fn node(value: i64) -> i64 {\n",
        "    inject out, logger, clock\n",
        "    inject scheduler, alarm, global_state\n",
        "    inject banana, value_store\n",
        "    when { return value }\n",
        "}\n",
        "native fn tick() -> i64\n",
        "native fn tick() -> i64 {\n    inject out, engine\n    when;\n}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:12 injectable/inject.unknown_capability TIME-8",
            "m.hgl:5:20 injectable/inject.unknown_capability TIME-8",
            "m.hgl:10:17 injectable/inject.unknown_capability TIME-8",
        ]
    );
    assert!(
        checked.report.contains(
            "`inject` names one of `out`, `logger`, `clock`, `scheduler`, `alarm`, `global_state`"
        ),
        "{}",
        checked.report
    );
}

/// The crate's list is TIME-8's, so a pin that changes the rule's list
/// fails here.
#[test]
fn the_capability_names_are_the_chapters_list() {
    let path = common::spec_root().join("hgl_spec/spec/language/temporal-model.md");
    let chapter = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    // The rule's first sentence, up to where it says the list is closed.
    let start = chapter
        .find("The injectables are")
        .expect("TIME-8 lists them");
    let end = chapter[start..]
        .find("`inject` names no other")
        .expect("TIME-8 closes the list")
        + start;
    // The names are the odd pieces: between a pair of backquotes.
    let listed: Vec<&str> = chapter[start..end].split('`').skip(1).step_by(2).collect();
    assert_eq!(listed, hgl_analyser::INJECTABLES);
}
