//! Value operator candidates over an unsized `list<T>` given a fixed-size
//! list (TYPE-10), and a sized candidate outranking an unsized one that
//! also admits it (GEN-5, GEN-12).

mod common;

use common::{Checked, body, module};
use hgl_analyser::Detail;
use hgl_analyser::hir::{Callee, Expr, ExprKind, Node, walk_statement_nodes};

/// Each candidate the body of `function` selects, in order, as its name
/// and parameter types; `lifted` before one a lifted call selects.
fn selected(checked: &Checked, function: &str) -> Vec<String> {
    let mut found = Vec::new();
    walk_statement_nodes(&body(checked, function).statements, &mut |node| {
        let Node::Expression(Expr {
            kind:
                ExprKind::Call {
                    callee:
                        Callee::Candidate {
                            module,
                            declaration,
                            ..
                        },
                    lifted,
                    ..
                },
            ..
        }) = node
        else {
            return;
        };
        let unit = &checked.program.modules[*module];
        let Detail::Callable(signature) = &unit.details[*declaration] else {
            return;
        };
        let parameters: Vec<String> = signature
            .parameters
            .iter()
            .map(|parameter| parameter.ty.to_string())
            .collect();
        found.push(format!(
            "{}{}({})",
            if *lifted { "lifted " } else { "" },
            unit.declarations[*declaration].name,
            parameters.join(", ")
        ));
    });
    found
}

/// TYPE-10: "an unsized parameter pattern can accept a fixed size". An
/// operator's candidate over `list<T>`, written, spelled `unbounded`,
/// generic, or over fixed elements, takes a fixed-size list: as an
/// argument, one written in call braces, and a configuration parameter.
#[test]
fn an_unsized_candidate_takes_a_fixed_list() {
    let checked = module(
        "operator total<T>(xs: T) -> i64
impl const fn total(xs: list<i64>) -> i64 {
    var s = 0
    for x in xs {
        s += x
    }
    return s
}
operator first<T>(xs: list<T>) -> T
impl const fn first<T>(xs: list<T>) -> T => xs[0]
operator spelled(xs: list<i64>) -> i64
impl const fn spelled(xs: list<i64, unbounded>) -> i64 => xs[0]
operator nested(rows: list<list<i64, 2>>) -> i64
impl const fn nested(rows: list<list<i64, 2>>) -> i64 => rows[0][1]
operator op_ord(xs: list<i64>) -> i64
impl const fn op_ord(xs: list<i64>) -> i64 => xs[0]
operator op_cfg{cfg: list<i64>}() -> i64
impl const fn op_cfg{cfg: list<i64>}() -> i64 => cfg[0]
const fn fn_cfg{cfg: list<i64>}() -> i64 => cfg[0]
const fn fixed(a: list<i64, 3>) -> i64 => total(a)
const fn growing(a: list<i64>) -> i64 => total(a)
const fn generic(a: list<str, 2>) -> str => first(a)
const fn unbounded(a: list<i64, 2>) -> i64 => spelled(a)
const fn rows(a: list<list<i64, 2>, 3>) -> i64 => nested(a)
const fn braced(a: list<i64, 2>) -> i64 => op_ord{xs: a}()
const fn configured(a: list<i64, 2>) -> i64 => op_cfg{cfg: a}()
const fn fn_configured(a: list<i64, 2>) -> i64 => fn_cfg{cfg: a}()",
    );
    assert!(checked.findings.is_empty(), "{}", checked.report);
    for (function, candidate) in [
        ("fixed", "total(list<i64>)"),
        ("growing", "total(list<i64>)"),
        ("generic", "first(list<T>)"),
        ("unbounded", "spelled(list<i64>)"),
        ("rows", "nested(list<list<i64, 2>>)"),
        ("braced", "op_ord(list<i64>)"),
        ("configured", "op_cfg(list<i64>)"),
    ] {
        assert_eq!(selected(&checked, function), [candidate], "{function}");
    }
}

/// GEN-5, GEN-12, TYPE-10: where a candidate over `list<T, N>` and one
/// over `list<T>` both admit a fixed-size list, the sized one is more
/// specific and is selected, with no tie, in either declaration order. A
/// growing list, or a fixed list of another size, selects the unsized one.
#[test]
fn a_sized_candidate_outranks_an_unsized_one_in_either_order() {
    let pairs = [
        (
            "operator pick<T>(xs: T) -> i64",
            "impl const fn pick(xs: list<i64, 3>) -> i64 => 3",
            "impl const fn pick(xs: list<i64>) -> i64 => 1",
            ["pick(list<i64, 3>)", "pick(list<i64>)", "pick(list<i64>)"],
        ),
        (
            "operator pick<T>(xs: T) -> i64",
            "impl const fn pick<N>(xs: list<i64, N>) -> i64 => N",
            "impl const fn pick(xs: list<i64>) -> i64 => -1",
            [
                "pick(list<i64, N>)",
                "pick(list<i64, N>)",
                "pick(list<i64>)",
            ],
        ),
        (
            "operator pick<T>(xs: list<T>) -> i64",
            "impl const fn pick<T, N>(xs: list<T, N>) -> i64 => N",
            "impl const fn pick<T>(xs: list<T>) -> i64 => -1",
            ["pick(list<T, N>)", "pick(list<T, N>)", "pick(list<T>)"],
        ),
    ];
    for (operator, sized, unsized_list, [three, two, growing]) in pairs {
        for (first, second) in [(sized, unsized_list), (unsized_list, sized)] {
            let checked = module(&format!(
                "{operator}\n{first}\n{second}
const fn three(a: list<i64, 3>) -> i64 => pick(a)
const fn two(a: list<i64, 2>) -> i64 => pick(a)
const fn growing(a: list<i64>) -> i64 => pick(a)"
            ));
            assert!(checked.findings.is_empty(), "{}", checked.report);
            assert_eq!(selected(&checked, "three"), [three], "{first}");
            assert_eq!(selected(&checked, "two"), [two], "{first}");
            assert_eq!(selected(&checked, "growing"), [growing], "{first}");
        }
    }
}

/// GEN-5, TYPE-10: a fixed-size list at an unsized `list<T>` is an exact
/// match, so the list candidate outranks one over `any` or a bare type
/// variable, in either declaration order, as it does for a growing list.
#[test]
fn an_unsized_list_candidate_outranks_any_and_a_type_variable() {
    let wide = [
        "impl const fn pick(xs: any) -> i64 => 1",
        "impl const fn pick<T>(xs: T) -> i64 => 1",
    ];
    let lists = [
        (
            "impl const fn pick<T>(xs: list<T>) -> i64 => 2",
            "pick(list<T>)",
        ),
        (
            "impl const fn pick(xs: list<i64>) -> i64 => 2",
            "pick(list<i64>)",
        ),
    ];
    for wide in wide {
        for (list, candidate) in lists {
            for (first, second) in [(wide, list), (list, wide)] {
                let checked = module(&format!(
                    "operator pick<T>(xs: T) -> i64\n{first}\n{second}
const fn fixed(a: list<i64, 3>) -> i64 => pick(a)
const fn growing(a: list<i64>) -> i64 => pick(a)"
                ));
                assert!(checked.findings.is_empty(), "{}", checked.report);
                assert_eq!(selected(&checked, "fixed"), [candidate], "{first}");
                assert_eq!(selected(&checked, "growing"), [candidate], "{first}");
            }
        }
    }
}

/// TYPE-10, GEN-5: Core's `join` selects its `...list<str>{1}` candidate
/// for a `list<str, 3>` and, lifted, for `map`'s fixed-size result. Core's
/// `convert` to `str` selects its sized list candidates, with no tie.
#[test]
fn cores_join_takes_a_fixed_list_and_a_map_result() {
    let checked = module(
        "const fn fixed(a: list<str, 3>) -> str => join(a)
const fn separated(a: list<str, 3>) -> str => join{separator: \", \"}(a)
const fn growing(a: list<str>) -> str => join(a)
const fn text(x: i64) -> str => convert<str>(x)
fn joined(items: list<i64, 3>) -> str => join(map(text, x: items))
const fn shown(a: list<i64, 3>) -> str => convert<str>(a)
const fn shown_nullable(a: list<i64?, 3>) -> str => convert<str>(a)",
    );
    assert!(checked.findings.is_empty(), "{}", checked.report);
    // The separator, then the pack's one list.
    let join = "join(str, list<str>)";
    assert_eq!(selected(&checked, "fixed"), [join]);
    assert_eq!(selected(&checked, "separated"), [join]);
    assert_eq!(selected(&checked, "growing"), [join]);
    assert!(
        selected(&checked, "joined").contains(&format!("lifted {join}")),
        "{:?}",
        selected(&checked, "joined")
    );
    assert_eq!(selected(&checked, "shown"), ["convert(list<T, N>)"]);
    assert_eq!(
        selected(&checked, "shown_nullable"),
        ["convert(list<T?, N>)"]
    );
}

/// FN-11, TYPE-10: a lifted call of a value candidate whose `list<T>`
/// parameter has a default takes a fixed-size connection.
#[test]
fn a_lifted_candidate_with_a_default_takes_a_fixed_connection() {
    let checked = module(
        "operator with_default(xs: list<i64> = [0]) -> i64
impl const fn with_default(xs: list<i64> = [0]) -> i64 => xs[0]
fn g(xs: list<i64, 3>) -> i64 => with_default(xs)",
    );
    assert!(checked.findings.is_empty(), "{}", checked.report);
    assert_eq!(selected(&checked, "g"), ["lifted with_default(list<i64>)"]);
}

/// TYPE-10, GEN-5, FN-15: a fixed list of another size or element type
/// selects no candidate; an unsized pattern does not reach an element's
/// type, which TYPE-10 does not say; and a `var` parameter still holds
/// one type, with one diagnostic for a function and for a candidate.
#[test]
fn fixed_lists_a_candidate_does_not_take_still_reject() {
    let checked = module(
        "operator two<T>(xs: T) -> i64
impl const fn two(xs: list<i64, 2>) -> i64 => 2
const fn three(a: list<i64, 3>) -> i64 => two(a)
operator ints(xs: list<i64>) -> i64
impl const fn ints(xs: list<i64>) -> i64 => xs[0]
const fn bools(a: list<bool, 2>) -> i64 => ints(a)
operator rows<T>(xs: T) -> i64
impl const fn rows(xs: list<list<i64>>) -> i64 => 1
const fn nested(a: list<list<i64, 2>, 3>) -> i64 => rows(a)
const fn mut_fn(var xs: list<i64>) {
    xs = []
}
operator mut_op(xs: list<i64>)
impl const fn mut_op(var xs: list<i64>) {
    xs = []
}
const fn mutated() {
    var fixed: list<i64, 3> = [1, 2, 3]
    mut_fn(fixed)
    mut_op(fixed)
}",
    );
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:43 operator/operator.no_candidate GEN-5",
            "m.hgl:7:44 operator/operator.no_candidate GEN-5",
            "m.hgl:10:53 operator/operator.no_candidate GEN-5",
            "m.hgl:20:12 type/type.mismatch FN-15",
            "m.hgl:21:12 type/type.mismatch FN-15",
        ],
        "{}",
        checked.report
    );
}

/// GEN-5, FN-17, TYPE-10: beside a callback, a fixed-size list is admitted
/// as a growing one is: the call selects the candidate, and a callback
/// that does not type reports its own error.
#[test]
fn a_fixed_list_beside_a_callback_is_admitted() {
    let checked = module(
        "operator apply_first<T, R>(f: const fn(T) -> R, xs: list<T>) -> R
impl const fn apply_first<T, R>(f: const fn(T) -> R, xs: list<T>) -> R => f(xs[0])
const fn twice(x: i64) -> i64 => x * 2
const fn named(a: list<i64, 2>) -> i64 => apply_first(twice, a)
const fn anonymous(a: list<i64, 2>) -> i64 => apply_first(fn(x) => x + 1, a)
const fn wrong(a: list<i64, 2>) -> i64 => apply_first(fn(x) => x + \"s\", a)
const fn wrong_growing(a: list<i64>) -> i64 => apply_first(fn(x) => x + \"s\", a)",
    );
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:7:66 operator/operator.no_candidate OPS-1",
            "m.hgl:8:71 operator/operator.no_candidate OPS-1",
        ],
        "{}",
        checked.report
    );
    let apply_first = "apply_first(const fn(T) -> R, list<T>)";
    assert_eq!(selected(&checked, "named"), [apply_first]);
    assert!(
        selected(&checked, "anonymous").contains(&apply_first.to_string()),
        "{:?}",
        selected(&checked, "anonymous")
    );
}
