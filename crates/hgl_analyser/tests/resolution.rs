//! The analyser's resolution pass against the rules it implements: what
//! each name in a signature, body, type or constraint is bound to, and
//! which bindings a scope does not admit. Each test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, check, module};
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
                "const fn pick<T>(first: T, items: dict<i64, T>) -> T {\n",
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
    assert_eq!(checked.summary(), ["m.hgl:9:30 name/name.unknown UNIT-10",]);
    assert_eq!(
        uses(&checked, "m"),
        [
            "T 4:25 -> GenericParameter",
            "T 4:45 -> GenericParameter",
            "T 4:52 -> GenericParameter",
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
    // UNIT-10: `make` is bound by nothing, the prelude included, and is
    // reported above. The program keeps it as a name nothing binds.
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
        "    let near = quote.ask =={abs: quote.bid} made.bid\n",
        "    if side == Side.buy { return quote.bid.abs() }\n",
        "    return quote.ask\n",
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
            "Quote", "Side", "Quote", "quote", "quote", "made", "side", "Side", "quote", "quote"
        ]
    );
}

/// UNIT-11: "Every module binds [the prelude's declarations] without a
/// `use`." A name nothing binds, core's declarations included, is
/// rejected (UNIT-10, `name.unknown`), once for its module, at its
/// first use.
/// TIME-9: "The operations on an endpoint are intrinsics of the
/// language ... These reserved intrinsics cannot be shadowed." No
/// declaration binds one, so it is not a name the prelude is waited on
/// for.
#[test]
fn a_name_nothing_binds_is_left_for_the_prelude() {
    let checked = module(concat!(
        "fn pass(value: i64) -> i64 {\n",
        "    when modified(value) && valid(value) { return smooth(delta_value(value)) }\n",
        "}\n",
        "fn again(value: i64) -> i64 {\n",
        "    when { return clamp(smooth(value)) }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:51 name/name.unknown UNIT-10",
            "m.hgl:6:19 name/name.unknown UNIT-10",
            "m.hgl:6:25 name/name.unknown UNIT-10",
        ]
    );
    assert_eq!(checked.prelude_names("m"), ["smooth", "clamp"]);
}

// ---- UNIT-12 and UNIT-5: one binding for a name ----

/// UNIT-12 permits lexical bindings to hide declarations and imports.
/// UNIT-5 and UNIT-12 still reject duplicate lexical bindings.
#[test]
fn a_binding_can_hide_declarations_and_imports_but_cannot_reuse_lexical_names() {
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
                "const fn a<T>(seed: i64, twice: i64, twice: i64, T: i64, last: T) -> i64 => 1\n",
                "const fn b(base: i64, book: dict<i64, i64>) -> i64 {\n",
                // A local named for a parameter, an import, an alias.
                "    let base = 1\n",
                "    let shared = 2\n",
                "    var l = 3\n",
                "    let outer = 4\n",
                "    if base == 1 {\n",
                // A local of an inner block named for one around it.
                "        let outer = 5\n",
                "        for outer, fine in book { }\n",
                "    }\n",
                // Twice in one block.
                "    let outer = 6\n",
                "    return base\n",
                "}\n",
                "fn c<U, U>(value: U) {}\n",
            ),
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:38 name/name.duplicate UNIT-5",
            "m.hgl:5:50 name/name.duplicate UNIT-5",
            "m.hgl:7:9 name/name.duplicate UNIT-12",
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

/// UNIT-12 allows a parameter named for the module's `seed` and rejects
/// duplicate lexical declarations. UNIT-7 keeps formals out of defaults.
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
        "const fn first(seed: i64) -> i64 => seed\n",
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
            // The names resolve, being in scope. A case's local has no
            // value on a path that did not declare it (CTRL-11).
            "m.hgl:11:25 name/binding.unassigned EXPR-3",
            "m.hgl:13:12 name/binding.unassigned EXPR-3",
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
        "struct Quote {\n    bid: f64 = 1.0\n    ask: f64 = bid\n    size: i64 = seed()\n}\n",
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

/// FN-14: an enclosing `var` parameter names a place. A `let` of that
/// parameter can be captured as the value read when the `let` was bound.
#[test]
fn an_anonymous_function_captures_the_snapshot_of_a_var_parameter() {
    let checked = module(concat!(
        "const fn direct(var count: i64) -> i64 => (fn() -> i64 => count)()\n",
        "operator choose(value: i64) -> i64\n",
        "impl const fn choose(var value: i64) -> i64 => (fn() -> i64 => value)()\n",
        "const fn snapshot(var count: i64) -> i64 {\n",
        "    let saved = count\n",
        "    count += 1\n",
        "    return (fn() -> i64 => saved)()\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:59 name/name.captured FN-14",
            "m.hgl:4:64 name/name.captured FN-14",
        ],
        "{}",
        checked.report
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
                "test uses_both { assert helper().x == private() }\n",
            ),
        ),
        (
            "b.hgl",
            concat!(
                "module m part tests\n",
                "test {\n",
                "    struct Fixture { x: i64 }\n",
                "    const fn helper() -> Fixture => Fixture(x: private())\n",
                // A local may hide a test-scope declaration in a test helper.
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
        [
            // UNIT-10: nothing binds `comparable`, `numeric`, or the
            // `U` outside the braces its `each` binds it in. `types`
            // is a form of the language, and is not reported.
            "m.hgl:4:36 name/name.unknown UNIT-10",
            "m.hgl:8:46 name/name.duplicate UNIT-12",
            "m.hgl:8:63 name/name.unknown UNIT-10",
            "m.hgl:8:79 name/name.unknown UNIT-10",
            "m.hgl:8:90 name/name.unknown UNIT-10",
        ]
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
        // Both readings parse. `Box` is a generic type: a constructor,
        // whose fields are then given by name (TYPE-5).
        "    let made = Box<Plain>(c)\n",
        // `a` is a parameter: comparisons.
        "    let compared = a<b>(c)\n",
        // `Plain` is a type with no generic parameters: comparisons.
        "    let plain = Plain<b>(c)\n",
        // Nothing binds `unknown` (UNIT-10): the decision is not made.
        "    let open = unknown<b>(c)\n",
        // A generic type before a `<` that opens no arguments.
        "    let ordered = Box < a\n",
        "    return compared\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:27 type/argument.unexpected TYPE-5",
            "m.hgl:8:16 name/name.unknown UNIT-10",
            "m.hgl:9:19 type/generic.invalid_application GEN-14",
        ]
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
        (
            "lib.hgl",
            concat!(
                "module lib\n",
                "export struct Box<T> { value: T }\n",
                "export struct Unit { value: i64 }\n",
            ),
        ),
        (
            "m.hgl",
            concat!(
                "module m\n",
                "use lib as l\n",
                "const fn first(a: i64) -> bool => l::Box < a\n",
                "const fn made(b: i64) -> l::Box<l::Unit> => l::Box<l::Unit>(b)\n",
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
            // The constructor is decided, and its fields are then given
            // by name (TYPE-5).
            "m.hgl:4:61 type/argument.unexpected TYPE-5",
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
        // CTRL-12: two names iterate a dictionary's keys and values.
        "fn node(value: i64, items: dict<i64, i64>) -> i64 {\n",
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
            "m.hgl:11:19 name/name.unknown UNIT-10",
            "m.hgl:15:17 name/name.duplicate UNIT-5",
            "m.hgl:19:11 name/name.duplicate UNIT-12",
        ]
    );
    // `index` is bound for the `for` body alone.
    assert_eq!(checked.prelude_names("m"), ["index"]);
}

// ---- TIME-8: capabilities ----

/// TIME-8: "The injectables are `out`, `logger`, `clock`, `scheduler`,
/// `alarm`, and `global_state`; `inject` names no other."
#[test]
fn inject_names_a_capability() {
    let checked = module(concat!(
        "fn node(value: i64) -> i64 {\n",
        "    inject out, logger, clock\n",
        "    inject scheduler, global_state\n",
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

/// TST-1 and TST-2: the names of a test's declarations are invisible to
/// every other test and to production, and may not take a visible name.
#[test]
fn test_local_declarations_do_not_escape_their_owner() {
    let checked = module(
        r#"const module_value: i64 = 1

test first {
    const hidden: i64 = 7
    struct Secret { value: i64 }
    enum Mode { private }
    operator rule(value: i64) -> i64
    impl const fn rule(value: i64) -> i64 => value
    assert rule(hidden) == hidden
}

test other {
    assert hidden == 7
    assert rule(1) == 1
    assert Mode.private == Mode.private
    let value: Secret = Secret(value: 7)
}

test collides {
    struct module_value { field: i64 }
    assert true
}

const fn outside() -> i64 => hidden
"#,
    );
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:14:12 name/name.unknown UNIT-10",
            "m.hgl:15:12 name/name.unknown UNIT-10",
            "m.hgl:16:12 name/name.unknown UNIT-10",
            "m.hgl:16:28 name/name.unknown UNIT-10",
            "m.hgl:17:16 name/name.unknown UNIT-10",
            "m.hgl:17:25 name/name.unknown UNIT-10",
            "m.hgl:21:12 name/name.duplicate TST-2",
            "m.hgl:25:30 name/name.unknown UNIT-10",
        ]
    );
}

/// UNIT-10 and UNIT-7 resolve signature types and defaults before the
/// callable binds its formal parameters, including in an anonymous function.
#[test]
fn annotations_and_defaults_see_the_outer_scope() {
    let checked = module(
        r#"const n: i64 = 2
const fn seed() -> i64 => 11
const fn same_name(n: i64, items: list<f64, n>) -> list<f64, n> => items
const fn lambda() -> list<f64, 2> => (fn(n: i64, items: list<f64, n>) => items)(3, [1.0, 2.0])
const fn module_default(data: i64 = seed()) -> i64 => data
const fn prelude_default(answer: str = format("seed")) -> str => answer
const fn missing_formal(parameter: i64 = 1, value: i64 = parameter) -> i64 => value"#,
    );
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "name/default.formal_argument");
    let names = uses(&checked, "m");
    assert!(
        names
            .iter()
            .any(|used| used == "seed 6:37 -> m value function")
    );
    assert!(
        names
            .iter()
            .any(|used| used == "format 7:40 -> core operator")
    );
    assert_eq!(
        names
            .iter()
            .filter(|used| used.starts_with("n ") && used.ends_with("constant"))
            .count(),
        3,
        "{names:?}"
    );
}

/// UNIT-12 warns once at a function-typed binding hiding a function or
/// operator. ERR-10 silences it. Ordinary anonymous formals may hide too.
#[test]
fn function_bindings_call_the_binding_and_report_only_unsuppressed_warnings() {
    let checked = module(
        r#"const fn helper(value: i64) -> i64 => value + 1
const fn typed(helper: const fn(i64) -> i64) -> i64 => helper(7)
const fn module_target() -> i64 => helper(7)
const fn hidden_operator(add_: const fn(i64) -> i64) -> i64 => add_(7)
const fn anonymous_formal() -> i64 => (fn(helper: i64) => helper)(7)
@suppress_warning("name.hides_function")
const fn quiet(helper: const fn(i64) -> i64) -> i64 => helper(7)"#,
    );
    assert!(!checked.rejected, "{}", checked.report);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:16 warning/name.hides_function UNIT-12",
            "m.hgl:5:26 warning/name.hides_function UNIT-12",
        ],
        "{}",
        checked.report
    );

    let rejected = module(
        r#"const fn helper(value: i64) -> i64 => value
const fn call(helper: i64) -> i64 => helper(7)"#,
    );
    assert!(rejected.rejected);
    assert_eq!(rejected.findings.len(), 1, "{}", rejected.report);
    assert!(
        rejected
            .report
            .contains("function or operator is hidden by the binding"),
        "{}",
        rejected.report
    );
}

/// UNIT-2 and UNIT-12 use the ordinary nearest binding for a qualifier.
#[test]
fn a_lexical_binding_hides_an_import_alias_only_in_its_scope() {
    let checked = check(&[
        (
            "lib.hgl",
            "module lib\nexport const fn marker() -> i64 => 7\n",
        ),
        (
            "m.hgl",
            r#"module m
use lib as dep
const fn ordinary() -> i64 => dep::marker()
const fn bad(dep: i64) -> i64 => dep::marker()
const fn value(dep: i64) -> i64 => dep"#,
        ),
    ]);
    assert_eq!(
        checked.summary(),
        ["m.hgl:4:34 name/alias.unknown UNIT-2"],
        "{}",
        checked.report
    );
    assert!(
        checked.report.contains("parameter hides the alias"),
        "{}",
        checked.report
    );
    assert!(
        checked.report.contains("the alias is declared here"),
        "{}",
        checked.report
    );
}

/// UNIT-12 preserves contextual names and guards lexical duplicates.
#[test]
fn activation_and_result_forms_are_reserved_for_lexical_bindings() {
    let checked = module(
        r#"const fn reserved(scheduled: i64, modified: i64, valid: i64, ok: i64) -> i64 => scheduled
const fn local() -> i64 {
    let valid = 1
    return 1
}"#,
    );
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:19 name/name.duplicate UNIT-12",
            "m.hgl:2:35 name/name.duplicate UNIT-12",
            "m.hgl:2:50 name/name.duplicate UNIT-12",
            "m.hgl:2:62 name/name.duplicate ERR-13",
            "m.hgl:4:9 name/name.duplicate UNIT-12",
        ],
        "{}",
        checked.report
    );
}

/// UNIT-12 allows each lexical form to hide a module or prelude name.
#[test]
fn generic_configuration_iteration_and_storage_bindings_hide_module_names() {
    let checked = module(
        r#"const fn helper(value: i64) -> i64 => value + 100
const limit: i64 = 2
const kept: i64 = 1
const seen: i64 = 1
const clock: i64 = 1
const fn generic<const limit: i64>() -> i64 requires limit == 3 => limit
const fn configuration{helper: i64}(value: i64) -> i64 => value + helper
const fn iteration(items: list<i64>) -> i64 {
    var sum = 0
    for helper in items { sum += helper }
    return sum
}
fn node(value: i64) -> i64 {
    state kept: i64 = 0
    cache seen = kept
    inject clock
    when { return value + seen }
}"#,
    );
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
}

/// GEN-18's law predicate reads its own callback formal, and FN-8's
/// pack clause reads that formal and its own generic exclusion key.
#[test]
fn requirements_and_pack_exclusions_use_lexical_bindings() {
    let checked = module(
        r#"const key: str = "outer"
const fn f(a: i64, b: i64) -> i64 => a + b
const fn law<const answered: bool = associative(f)>(f: const fn(i64, i64) -> i64, a: i64, b: i64) -> i64 requires associative(f) => f(a, b)
operator pass<R, ...Ps, ...As, const key: str = "inner">(f: fn(...{Ps}) -> R, args: ...{As for f without key}) -> R"#,
    );
    assert!(!checked.rejected, "{}", checked.report);
    assert_eq!(
        checked
            .findings
            .iter()
            .map(|finding| finding.code.as_str())
            .collect::<Vec<_>>(),
        ["warning/name.hides_function", "warning/name.hides_function"],
        "{}",
        checked.report
    );
    let names = uses(&checked, "m");
    assert!(
        names
            .iter()
            .any(|used| used.starts_with("key 5:") && used.ends_with("GenericParameter")),
        "{names:?}"
    );
    assert_eq!(
        names
            .iter()
            .filter(|used| used.starts_with("f 4:") && used.ends_with("Parameter"))
            .count(),
        3,
        "{names:?}"
    );
    assert_eq!(
        names
            .iter()
            .filter(|used| used.starts_with("f 5:") && used.ends_with("Parameter"))
            .count(),
        1,
        "{names:?}"
    );

    let rejected = module(
        r#"const key: str = "outer"
const fn f(a: i64, b: i64) -> i64 => a + b
const fn bad(f: i64) -> i64 requires associative(f) => f
operator wrong<...Ps, ...As, const key: i64 = 0>(f: fn(...{Ps}), args: ...{As for f without key})"#,
    );
    assert!(rejected.rejected);
    assert_eq!(rejected.findings.len(), 3, "{}", rejected.report);
    assert_eq!(rejected.findings[2].code, "type/type.mismatch");
}

/// UNIT-12 keeps option labels, fields and pack keys out of lexical lookup.
#[test]
fn written_call_labels_members_and_pack_keys_keep_their_own_meanings() {
    let checked = module(
        r#"const fn option() -> i64 => 7
struct Record { option: i64 }
operator choose{option: i64}(value: i64) -> i64
impl const fn choose{option: i64}(value: i64) -> i64 => value + option
const fn take<...As>(values: ...{As}) -> i64 => 1
const fn uses_labels(option: i64) -> i64 {
    let record = Record(option: option)
    return choose{option: record.option}(1) + take(option: option)
}"#,
    );
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
}

/// UNIT-10, FN-14 and EXPR-2 still reject an unbound type argument, a
/// captured mutable binding and an unmatched call label.
#[test]
fn shadowing_changes_no_signature_capture_or_call_label_rules() {
    let checked = module(
        r#"const fn missing_size(n: i64, items: list<f64, n>) -> i64 => n
const fn helper() -> i64 => 1
const fn invalid_capture(value: i64) -> i64 {
    var helper = value
    return (fn() => helper)()
}
const fn label_only(foo: i64) -> i64 => foo
const fn wrong_label(bad: i64) -> i64 => label_only(bad: bad)"#,
    );
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:48 name/name.unknown UNIT-10",
            "m.hgl:6:21 name/name.captured FN-14",
            "m.hgl:9:42 type/argument.missing EXPR-2",
            "m.hgl:9:53 name/argument.unknown EXPR-2",
        ],
        "{}",
        checked.report
    );
}

/// UNIT-7 prohibits names of formals and fields in defaults, including
/// when UNIT-12 allows a formal to take a callable's name.
#[test]
fn a_default_checks_formal_and_field_names_before_outer_lookup() {
    let checked = module(
        r#"struct S {
    format: str = "a"
    b: str = format
}
struct S2 {
    format: str = "a"
    b: str = format("{}", 1)
}
const fn d2(format: str, b: str = format) -> str => b
const fn helper() -> i64 => 1
const fn e(helper: i64, c: i64 = helper) -> i64 => c
const fn g(helper: i64, c: i64 = helper()) -> i64 => c
struct U3 {
    helper: i64 = 5
    c: i64 = helper()
}"#,
    );
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:14 name/default.formal_argument UNIT-7",
            "m.hgl:8:14 name/default.formal_argument UNIT-7",
            "m.hgl:10:35 name/default.formal_argument UNIT-7",
            "m.hgl:12:34 name/default.formal_argument UNIT-7",
            "m.hgl:13:34 name/default.formal_argument UNIT-7",
            "m.hgl:16:14 name/default.formal_argument UNIT-7",
        ],
        "{}",
        checked.report
    );
}

/// UNIT-7 also holds when the formal shares a name with a file import.
#[test]
fn a_default_checks_formal_names_before_resolving_file_imports() {
    let checked = check(&[
        (
            "lib.hgl",
            "module lib\nexport const fn marker() -> i64 => 7\n",
        ),
        (
            "m.hgl",
            r#"module m
use lib::{marker}
use lib as dep
const fn first(marker: i64, value: i64 = marker()) -> i64 => value
const fn second(dep: i64, value: i64 = dep::marker()) -> i64 => value
const fn outer(value: i64 = dep::marker()) -> i64 => value"#,
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:42 name/default.formal_argument UNIT-7",
            "m.hgl:5:40 name/default.formal_argument UNIT-7",
        ],
        "{}",
        checked.report
    );
}
