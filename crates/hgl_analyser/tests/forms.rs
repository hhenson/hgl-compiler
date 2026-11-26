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
            "operator fuse<...Ts{2:8}>(parts: ...Ts) -> str\n",
            "native fn collect<T>(items: ...T) -> list<T>\n",
            "const fn seed() -> i64 => 1\n",
        ),
    )]);
    assert_eq!(checked.summary(), NONE);
}

/// FN-8: the second pack of either kind is reported at its name. A
/// declaration may have one positional and one named pack together.
#[test]
fn a_declaration_accepts_one_pack_of_each_kind() {
    let checked = module(concat!(
        "const fn either(a: ...i64{0:1}, b: ...i64{0:1}) -> i64 => 0\n",
        "const fn named_twice<...Fs, ...Gs>(a: ...{Fs}, b: ...{Gs}) -> i64 => 0\n",
        "const fn both<...Fields>(items: ...i64, named: ...{Fields}) -> i64 => 0\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:33 type/pack.duplicate FN-8",
            "m.hgl:3:48 type/pack.duplicate FN-8",
        ],
        "{}",
        checked.report
    );
}

/// GEN-6: a fixed parameter cannot replace a pack. A pack with
/// cardinality one is legal, including in a candidate with configuration.
#[test]
fn a_candidate_keeps_the_operator_pack_slot() {
    let checked = module(concat!(
        "operator first_of<T>(values: ...T{1:*}) -> T\n",
        "impl const fn first_of(values: i64) -> i64 => values\n",
        "impl const fn first_of(values: ...i64{1}) -> i64 => 0\n",
        "operator configured{offset: i64 = 1}(items: ...i64) -> i64\n",
        "impl const fn configured{extra: i64 = 2, offset: i64}(item: i64) -> i64 => item\n",
        "impl const fn configured{extra: i64 = 2, offset: i64}(items: ...i64{1}) -> i64 => offset\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:24 operator/impl.pack GEN-6",
            "m.hgl:6:55 operator/impl.pack GEN-6",
        ],
        "{}",
        checked.report
    );
}

/// GEN-6: a public const parameter or callback keeps its mode.
#[test]
fn a_candidate_keeps_the_operators_parameter_modifiers() {
    let checked = module(concat!(
        "operator at(arg: i64, const n: i64) -> i64\n",
        "impl const fn at(arg: i64, n: i64) -> i64 => arg + n\n",
        "operator apply(f: const fn(i64) -> i64, x: i64) -> i64\n",
        "impl fn apply(f: fn(i64) -> i64, x: i64) -> i64 => f(x)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:28 operator/impl.parameter GEN-6",
            "m.hgl:5:15 operator/impl.parameter GEN-6",
        ],
        "{}",
        checked.report
    );
}

/// GEN-6: a condition on an option does not restore a missing const
/// modifier. The value and native candidates reject where declared.
#[test]
fn a_candidates_option_condition_does_not_replace_const() {
    let checked = module(concat!(
        "enum Zero { ieee, nan, error }\n",
        "operator ratio<L, R, O>(lhs: L, rhs: R, const zero: Zero = .ieee) -> O\n",
        "native impl const fn ratio(lhs: i64, rhs: i64, zero: Zero) -> f64\n",
        "    requires zero in {.ieee, .nan}\n",
        "impl const fn ratio(lhs: str, rhs: str, zero: Zero) -> str requires zero == .nan => lhs\n",
        "impl const fn ratio(lhs: bool, rhs: bool, zero: Zero) -> bool requires .nan == zero => lhs\n",
        "impl const fn ratio(lhs: f64, rhs: f64, zero: Zero) -> f64\n",
        "    requires zero in {.ieee, .nan} => lhs\n",
        "impl const fn ratio(lhs: f64, rhs: f64, zero: Zero) -> result<f64, error>\n",
        "    requires zero == .error => ok(lhs)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:48 operator/impl.parameter GEN-6",
            "m.hgl:6:41 operator/impl.parameter GEN-6",
            "m.hgl:7:43 operator/impl.parameter GEN-6",
            "m.hgl:8:41 operator/impl.parameter GEN-6",
            "m.hgl:10:41 operator/impl.parameter GEN-6",
        ],
        "{}",
        checked.report
    );
}

/// GEN-6: a candidate can add const, take a public callback as const in
/// value code, add var (FN-15), or take a public ordinary input as any.
#[test]
fn a_candidate_can_add_modifiers_or_take_any() {
    let checked = module(concat!(
        "operator at(arg: i64, n: i64) -> i64\n",
        "impl const fn at(arg: i64, const n: i64) -> i64 => arg + n\n",
        "operator apply(f: fn(i64) -> i64, x: i64) -> i64\n",
        "impl const fn apply(f: const fn(i64) -> i64, x: i64) -> i64 => f(x)\n",
        "operator pick(arg: i64) -> i64\n",
        "impl const fn pick(arg: any) -> i64 => 1\n",
        "operator assign(target: i64, value: i64)\n",
        "impl const fn assign(var target: i64, value: i64) { target = value }\n",
        "const fn inc(value: i64) -> i64 => value + 1\n",
        "const fn call() -> i64 {\n",
        "    var target = at(2, 3)\n",
        "    assign(target, apply(inc, target))\n",
        "    return target + pick(3)\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
}

/// GEN-6: the candidate's declaration reports the result it contradicts
/// even when a generic caller or an explicit argument could select it.
#[test]
fn a_candidate_result_honours_its_operator() {
    let checked = module(concat!(
        "operator fixed<T>(value: T, flag: bool) -> T\n",
        "impl const fn fixed(value: i64, flag: bool) -> f64 => 2.5\n",
        "const fn through<T>(v: T) -> T requires fixed(T, bool) => fixed(v, true)\n",
        "test direct {\n    let value: i64 = 3\n    let x = fixed(value, true)\n}\n",
        "test generic {\n    let value: i64 = through(3)\n    assert value == 2\n}\n",
        "operator fx2<U, T>(flag: U, value: T) -> T\n",
        "impl const fn fx2(flag: bool, value: i64) -> f64 => 2.5\n",
        "test explicit {\n    let value: i64 = 3\n    let x = fx2<bool>(true, value)\n}\n",
        "operator element<T>(values: list<T>) -> T\n",
        "impl const fn element(values: list<str>) -> i64 => 0\n",
        "impl const fn element(values: list<i64>) -> i64 => 0\n",
    ));
    assert_eq!(
        checked
            .summary()
            .into_iter()
            .filter(|report| report.contains("operator/impl.result"))
            .collect::<Vec<_>>(),
        [
            "m.hgl:3:48 operator/impl.result GEN-6",
            "m.hgl:14:46 operator/impl.result GEN-6",
            "m.hgl:20:45 operator/impl.result GEN-6",
        ],
        "{}",
        checked.report
    );
    assert!(checked.rejected);
}

/// GEN-6: a fixed result cannot acquire a failure or a null. An independent
/// output parameter may bind to the complete candidate result.
#[test]
fn candidate_results_refine_their_public_declaration() {
    let checked = module(concat!(
        "operator maybe(x: i64) -> i64\n",
        "impl const fn maybe(x: i64) -> i64? => null\n",
        "operator failed<T>(x: T) -> T\n",
        "impl const fn failed(x: i64) -> result<i64, error> => ok(x)\n",
        "operator present(x: i64) -> i64?\n",
        "impl const fn present(x: i64) -> i64 => x\n",
        "operator success<T>(x: T) -> result<T, error>\n",
        "impl const fn success(x: i64) -> i64 => x\n",
        "operator policy<I, O>(x: I) -> O\n",
        "impl const fn policy(x: i64) -> result<i64, error> => ok(x)\n",
        "operator bound<T>(x: T) -> T\n",
        "impl const fn bound<X>(x: X) -> result<X, error> => ok(x)\n",
        "operator required<T>(x: T) -> T\n",
        "impl const fn required<X>(x: X) -> X? => x\n",
    ));
    let failures: Vec<_> = checked.findings.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(failures, ["operator/impl.result"; 4], "{}", checked.report);
    assert!(checked.findings[0].message.contains("result `i64?`"));
    assert!(
        checked.findings[1]
            .message
            .contains("result `result<i64, error>`")
    );
}

/// GEN-6 and GEN-10: an exact homogeneous candidate pack supplies the
/// public tuple's element types even when it is nested in a result.
#[test]
fn a_homogeneous_candidate_binds_its_operator_tuple_pack() {
    let checked = module(concat!(
        "operator pair<...Ts>(values: ...Ts) -> tuple<Ts>\n",
        "impl const fn pair(values: ...i64{2}) -> tuple<i64, i64> => (1, 2)\n",
        "operator nested<...Ts>(values: ...Ts) -> list<tuple<bool, Ts>>\n",
        "impl const fn nested(values: ...i64{2}) -> list<tuple<bool, i64, i64>> => [(true, 1, 2)]\n",
        "test pair_value { assert pair(10, 20) == (1, 2) }\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
}

/// GEN-6 and GEN-10: a fixed homogeneous pack determines both the
/// result's tuple length and the type of each element.
#[test]
fn a_homogeneous_candidate_rejects_incompatible_tuple_results() {
    let checked = module(concat!(
        "operator short<...Ts>(values: ...Ts) -> tuple<Ts>\n",
        "impl const fn short(values: ...i64{2}) -> tuple<i64> => (1,)\n",
        "operator mixed<...Ts>(values: ...Ts) -> tuple<Ts>\n",
        "impl const fn mixed(values: ...i64{2}) -> tuple<i64, f64> => (1, 2.0)\n",
    ));
    assert_eq!(checked.findings.len(), 2, "{}", checked.report);
    for finding in &checked.findings {
        assert_eq!(finding.code, "operator/impl.result", "{}", checked.report);
        assert!(
            finding.message.ends_with("result `tuple<i64, i64>`"),
            "{}",
            checked.report
        );
    }
}

/// GEN-6: a candidate has a result when its operator has one.
#[test]
fn a_candidate_declares_a_result_exactly_when_its_operator_does() {
    let checked = module(concat!(
        "operator missing<T>(value: T) -> T\n",
        "impl const fn missing(value: i64) {}\n",
        "operator extra<T>(value: T)\n",
        "impl const fn extra(value: i64) -> i64 => value\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:15 operator/impl.result GEN-6",
            "m.hgl:5:36 operator/impl.result GEN-6",
        ],
        "{}",
        checked.report
    );
    assert!(
        checked.findings[0].message.contains("declares no result")
            && checked.findings[0].message.ends_with("result `i64`"),
        "{}",
        checked.report
    );
}

/// GEN-6: a concrete candidate outside the operator's type set rejects
/// at its declaration even without a call. A generic candidate can remain
/// conditional, and a concrete member of the set is admitted.
#[test]
fn a_concrete_candidate_honours_its_operators_requires() {
    let checked = module(concat!(
        "operator admitted<T>(value: T) requires T in {i64, bool}\n",
        "impl const fn admitted(value: str) {}\n",
        "impl const fn admitted(value: bool) {}\n",
        "impl const fn admitted<T>(value: T) requires T in {i64, bool} {}\n",
        "operator items<T>(values: list<T>) requires T in {i64, bool}\n",
        "impl const fn items(values: list<str>) {}\n",
        "impl const fn items(values: list<i64>) {}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:15 operator/impl.requires GEN-6",
            "m.hgl:7:15 operator/impl.requires GEN-6",
        ],
        "{}",
        checked.report
    );
}

/// GEN-6: a condition can test a value the candidate's input type binds.
#[test]
fn a_candidate_honours_requires_on_inferred_values() {
    let checked = module(concat!(
        "operator member<const N: i64>(x: list<i64, N>) requires N in {1, 2}\n",
        "impl const fn member(x: list<i64, 3>) {}\n",
        "impl const fn member(x: list<i64, 2>) {}\n",
        "operator equal<const N: i64>(x: list<i64, N>) requires N == 2\n",
        "impl const fn equal(x: list<i64, 3>) {}\n",
        "enum Zero { ieee, nan }\n",
        "struct Z<const z: Zero> {}\n",
        "operator choose<const z: Zero>(x: Z<z>) requires z == .nan\n",
        "impl const fn choose(x: Z<.ieee>) {}\n",
        "impl const fn choose(x: Z<.nan>) {}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:15 operator/impl.requires GEN-6",
            "m.hgl:6:15 operator/impl.requires GEN-6",
            "m.hgl:10:15 operator/impl.requires GEN-6",
        ],
        "{}",
        checked.report
    );
}

/// GEN-6 and TYPE-10: both growing-list spellings give one type in a
/// result or in a requires set.
#[test]
fn growing_list_spelling_does_not_change_a_candidate_contract() {
    let checked = module(concat!(
        "operator values(value: i64) -> list<i64>\n",
        "impl const fn values(value: i64) -> list<i64, unbounded> => [value]\n",
        "operator allowed<T>(value: T) requires T in {list<i64>}\n",
        "impl const fn allowed(value: list<i64, unbounded>) {}\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
}

/// GEN-6: repeated bindings inside one parameter must agree before its
/// inferred type is used to report a result contradiction.
#[test]
fn a_result_contract_needs_consistent_input_bindings() {
    let checked = module(concat!(
        "operator pick<T>(pair: tuple<T, T>) -> T\n",
        "impl const fn pick(pair: tuple<i64, str>) -> str => \"chosen\"\n",
        "impl const fn pick(pair: tuple<i64, i64>) -> str => \"wrong\"\n",
    ));
    assert_eq!(
        checked
            .summary()
            .into_iter()
            .filter(|report| report.contains("operator/impl.result"))
            .collect::<Vec<_>>(),
        ["m.hgl:4:46 operator/impl.result GEN-6"],
        "{}",
        checked.report
    );
}

/// GEN-6 and GEN-17: corresponding reflected packs preserve a consistent
/// binding from a callback template's parameter and result.
#[test]
fn a_result_contract_checks_repeated_bindings_beside_reflected_packs() {
    let checked = module(concat!(
        "operator packed_bad<T, ...Ps>(f: const fn(T, ...{Ps}) -> T) -> T\n",
        "impl const fn packed_bad<...Qs>(f: const fn(i64, ...{Qs}) -> i64) -> str => \"wrong\"\n",
        "operator packed_ok<T, ...Ps>(f: const fn(T, ...{Ps}) -> T) -> T\n",
        "impl const fn packed_ok<...Qs>(f: const fn(i64, ...{Qs}) -> i64) -> i64 => 1\n",
        "operator packed_conflict<T, ...Ps>(f: const fn(T, ...{Ps}) -> T) -> T\n",
        "impl const fn packed_conflict<...Qs>(f: const fn(i64, ...{Qs}) -> f64) -> str => \"unproven\"\n",
        "operator plain_bad<T>(f: const fn(T) -> T) -> T\n",
        "impl const fn plain_bad(f: const fn(i64) -> i64) -> str => \"wrong\"\n",
        "const item: str = \"item\"\n",
        "const other: str = \"other\"\n",
        "operator packed_member<T, ...Ps>(f: const fn(T, list<Ps[item]>, ...{Ps}) -> T) -> T\n",
        "impl const fn packed_member<...Qs>(f: const fn(i64, list<Qs[item]>, ...{Qs}) -> i64) -> str => \"wrong\"\n",
        "operator packed_key_conflict<T, ...Ps>(f: const fn(T, list<Ps[item]>, ...{Ps}) -> T) -> T\n",
        "impl const fn packed_key_conflict<...Qs>(f: const fn(i64, list<Qs[other]>, ...{Qs}) -> i64) -> str => \"unproven\"\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:70 operator/impl.result GEN-6",
            "m.hgl:9:53 operator/impl.result GEN-6",
            "m.hgl:13:89 operator/impl.result GEN-6",
        ],
        "{}",
        checked.report
    );
}

/// GEN-6: repeats and expressions inside composite constants must
/// agree with the binding before it gives a declared result type.
#[test]
fn a_result_contract_checks_inferred_values_in_composite_constants() {
    for (constant_type, pattern, conflicting, consistent) in [
        ("list<i64>", "[N, N]", "[1, 2]", "[1, 1]"),
        ("tuple<i64, i64>", "(N, N)", "(1, 2)", "(1, 1)"),
        ("list<i64>", "[N, N + 1]", "[1, 4]", "[1, 2]"),
        ("list<i64>", "[N, N * 2]", "[1, 4]", "[1, 2]"),
    ] {
        let checked = module(&format!(
            "struct Box<const X: {constant_type}> {{}}\n\
             operator pick<const N: i64>(boxed: Box<{pattern}>) -> list<i64, N>\n\
             impl const fn pick(boxed: Box<{conflicting}>) -> list<i64, 2> => [1, 2]\n\
             impl const fn pick(boxed: Box<{consistent}>) -> list<i64, 2> => [1, 2]\n"
        ));
        assert_eq!(
            checked
                .summary()
                .into_iter()
                .filter(|report| report.contains("operator/impl.result"))
                .collect::<Vec<_>>(),
            ["m.hgl:5:43 operator/impl.result GEN-6"],
            "{}",
            checked.report
        );
    }
}

/// GEN-6: a value inferred from one input also has to agree with an
/// expression in another input before it gives a declared result type.
#[test]
fn a_result_contract_checks_values_used_across_inputs() {
    let checked = module(concat!(
        "operator pick<const N: i64>(a: list<i64, N>, b: list<i64, N + 1>) -> list<i64, N>\n",
        "impl const fn pick(a: list<i64, 2>, b: list<i64, 4>) -> list<i64, 3> => [1, 2, 3]\n",
        "impl const fn pick(a: list<i64, 2>, b: list<i64, 3>) -> list<i64, 3> => [1, 2, 3]\n",
    ));
    assert_eq!(
        checked
            .summary()
            .into_iter()
            .filter(|report| report.contains("operator/impl.result"))
            .collect::<Vec<_>>(),
        ["m.hgl:4:57 operator/impl.result GEN-6"],
        "{}",
        checked.report
    );
}

/// GEN-6: an unresolved extent leaves just that part of the result open.
#[test]
fn a_result_contract_leaves_unbound_sizes_open() {
    let checked = module(concat!(
        "operator by_argument<const N: i64>(x: list<i64, N>) -> list<i64, N>\n",
        "impl const fn by_argument(x: list<i64>) -> list<i64> => x\n",
        "operator by_result<const N: i64>(x: i64) -> list<i64, N>\n",
        "impl const fn by_result(x: i64) -> list<i64> => [x]\n",
        "operator generic<const N: i64>(x: list<i64, N>) -> list<i64, N>\n",
        "impl const fn generic<const M: i64>(x: list<i64, M>) -> list<i64> => []\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
}

/// GEN-6: an unresolved parameter does not hide a known contradiction
/// in another part of the candidate's result.
#[test]
fn a_result_contract_compares_known_parts_of_a_generic_result() {
    let checked = module(concat!(
        "operator pair<T, U>(x: T) -> tuple<T, U>\n",
        "impl const fn pair(x: i64) -> tuple<str, f64> => (\"a\", 1.5)\n",
        "operator admitted<T, U>(x: T) -> tuple<T, U>\n",
        "impl const fn admitted(x: i64) -> tuple<i64, f64> => (x, 1.5)\n",
        "operator unbound<T, const N: i64>(x: T) -> list<T, N>\n",
        "impl const fn unbound(x: i64) -> list<str, 1> => [\"a\"]\n",
        "operator generic<const N: i64>(x: list<i64, N>) -> list<i64, N>\n",
        "impl const fn generic<const M: i64>(x: list<i64, M>) -> list<str, M> => []\n",
        "operator dictionary<T, U>(x: T) -> dict<T, U>\n",
        "impl const fn dictionary(x: i64) -> dict<str, bool> => [\"a\": true]\n",
        "operator fixed<const N: i64>(x: list<i64, N>) -> list<i64, N>\n",
        "impl const fn fixed(x: list<i64, 2>) -> list<i64> => []\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:31 operator/impl.result GEN-6",
            "m.hgl:7:34 operator/impl.result GEN-6",
            "m.hgl:9:57 operator/impl.result GEN-6",
            "m.hgl:11:37 operator/impl.result GEN-6",
            "m.hgl:13:41 operator/impl.result GEN-6",
        ],
        "{}",
        checked.report
    );
}

/// GEN-6 and GEN-17: the operator's reflected result names the same
/// callback pack as its candidate's. Its known shape is still checked.
#[test]
fn a_result_contract_uses_the_candidates_reflected_pack() {
    let checked = module(concat!(
        "const item: str = \"item\"\n",
        "operator reflected<...Ps, const key: str = \"item\">(f: const fn(...{Ps}) -> i64) -> list<Ps[key]>\n",
        "impl const fn reflected<...Qs, const key: str = \"item\">(f: const fn(...{Qs}) -> i64) -> list<Qs[key]> => []\n",
        "operator concrete<...Ps>(f: const fn(...{Ps}) -> i64) -> list<Ps[item]>\n",
        "impl const fn concrete<...Qs>(f: const fn(...{Qs}) -> i64) -> list<str> => []\n",
        "operator shaped<...Ps>(f: const fn(...{Ps}) -> i64) -> tuple<list<Ps[item]>, i64>\n",
        "impl const fn shaped<...Qs>(f: const fn(...{Qs}) -> i64) -> tuple<list<Qs[item]>, f64> => ([], 1.5)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:63 operator/impl.result GEN-6",
            "m.hgl:8:61 operator/impl.result GEN-6",
        ],
        "{}",
        checked.report
    );
}

/// GEN-6, GEN-17 and TYPE-9: an unbound pack or key leaves its member
/// open, and an atomic type can normalize when its payload is supplied.
#[test]
fn a_result_contract_leaves_unbound_members_and_normalizing_types_open() {
    let checked = module(concat!(
        "const item: str = \"item\"\n",
        "operator from_callback<...Ps>(f: const fn(...{Ps}) -> i64) -> list<Ps[item]>\n",
        "impl const fn from_callback(f: const fn(item: str) -> i64) -> list<str> => []\n",
        "operator by_key<...Ps, const key: str = \"item\">(f: const fn(...{Ps}) -> i64) -> list<Ps[key]>\n",
        "impl const fn by_key<...Qs>(f: const fn(...{Qs}) -> i64) -> list<Qs[item]> => []\n",
        "operator by_sibling<...Ps>(f: const fn(...{Ps}) -> i64) -> tuple<list<Ps[item]>, i64>\n",
        "impl const fn by_sibling(f: const fn(item: str) -> i64) -> tuple<list<str>, f64> => ([], 1.5)\n",
        "operator atomic_open<T>(x: i64) -> atomic<T>\n",
        "impl const fn atomic_open(x: i64) -> i64 => x\n",
        "operator atomic_fixed(x: i64) -> atomic<list<i64>>\n",
        "impl const fn atomic_fixed(x: i64) -> list<i64> => [x]\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:8:60 operator/impl.result GEN-6",
            "m.hgl:12:39 operator/impl.result GEN-6",
        ],
        "{}",
        checked.report
    );
}

/// GEN-6: a contract comparison needs a valid candidate and operator.
/// An invalid binding in a named test is already reported by TST-2.
/// GEN-10 now admits tuple<Ts>; use a value in type position instead so
/// this fixture still has an earlier declaration error (GEN-13).
#[test]
fn a_result_contract_does_not_repeat_an_earlier_declaration_error() {
    let checked = module(concat!(
        "operator tp2<const n: i64>(values: ...i64) -> tuple<n>\n",
        "impl const fn tp2(values: ...i64{2}) -> tuple<i64, i64> => (1, 2)\n",
        "operator list_size<const N: i64>(x: list<i64, N>) -> list<i64, N>\n",
        "impl const fn list_size(x: list<i64, -1>) -> list<i64, 2> => [1, 2]\n",
        "operator fixed<T>(value: T) -> T\n",
        "test local {\n",
        "    operator fixed<T>(value: T) -> list<T>\n",
        "    impl const fn fixed(value: i64) -> list<i64> => [value]\n",
        "}\n",
    ));
    let codes: Vec<_> = checked
        .findings
        .iter()
        .map(|item| item.code.as_str())
        .collect();
    assert_eq!(
        codes,
        [
            "type/type.not_a_type",
            "type/list.size",
            "name/name.duplicate"
        ],
        "{}",
        checked.report
    );
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
        "fn graph_null() -> i64 { return positional(null) }\n",
        "fn graph_result<T>(const item: T) -> i64 { return positional(ok(item)) }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:31 type/generic.unresolved GEN-8",
            "m.hgl:5:43 type/sequence.context_type EXPR-9",
            "m.hgl:6:33 type/generic.unresolved GEN-8",
            "m.hgl:7:33 type/generic.unresolved GEN-8",
            "m.hgl:10:33 type/generic.unresolved GEN-8",
            "m.hgl:11:51 type/generic.unresolved GEN-8",
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

/// FN-8: packs have no defaults, cannot be const, and follow fixed parameters.
#[test]
fn a_pack_has_no_default_is_not_const_and_follows_fixed_parameters() {
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "fn defaulted<T>(rest: ...T = 1) {}\n",
            "fn constant<T>(const rest: ...T) {}\n",
            "fn early<T>(rest: ...T, last: T, const size: i64 = 1) {}\n",
            "operator all<T>(const rest: ...T = 1, last: T) -> T\n",
            "native fn tick<T>(rest: ...T, last: T) -> T\n",
            "fn early_named<...Fs>(rest: ...{Fs}, last: i64) {}\n",
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
            "m.hgl:7:38 type/pack.order FN-8",
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
