//! The analyser against the rules on references, signals and atomics:
//! what a graph gives across a reference boundary and to a `signal`,
//! what a generic is inferred to be from a reference, and what a hook
//! reads of each (TYPE-9, TYPE-12, TIME-10, GEN-11). Each test cites its
//! rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, check};
use hgl_analyser::hir::{Body, Callee, Expr, ExprKind, walk_statements};
use hgl_analyser::{Argument, Scalar, Type};

/// No lines.
const NONE: [&str; 0] = [];

/// Checks one file, `m.hgl`, of module `m`. The `module` line is added,
/// so the text given starts on line 2.
fn module(text: &str) -> Checked {
    check(&[("m.hgl", &format!("module m\n{text}"))])
}

/// The typed body of the function `name` of module `m`.
fn body<'a>(checked: &'a Checked, name: &str) -> &'a Body {
    let module = checked.program.module("m").expect("module m exists");
    let index = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == name)
        .expect("the declaration exists");
    module.bodies[index].as_ref().expect("it has a body")
}

/// Each call in the body of `name` that resolves generic parameters, as
/// `callee<argument, ...>`, in the order the calls are met.
fn instances(checked: &Checked, name: &str) -> Vec<String> {
    let mut found = Vec::new();
    walk_statements(&body(checked, name).statements, &mut |value: &Expr| {
        let ExprKind::Call {
            callee:
                Callee::Function {
                    module,
                    declaration,
                    generics,
                    ..
                },
            ..
        } = &value.kind
        else {
            return;
        };
        if generics.is_empty() {
            return;
        }
        let arguments: Vec<String> = generics
            .iter()
            .map(|argument| match argument {
                Argument::Type(ty) => ty.to_string(),
                Argument::Constant(constant) => constant.text.clone(),
            })
            .collect();
        let callee = &checked.program.modules[*module].declarations[*declaration].name;
        found.push(format!("{callee}<{}>", arguments.join(", ")));
    });
    found
}

/// The nodes the tests below give connections to.
const NODES: &str = concat!(
    "fn pick(use_left: bool, left: ref<i64>, right: ref<i64>) -> ref<i64> {\n",
    "    when {\n",
    "        if use_left { return left }\n",
    "        return right\n",
    "    }\n",
    "}\n",
    "fn counted(tick: signal) -> i64 {\n",
    "    when { return 1 }\n",
    "}\n",
    "fn plain(value: i64) -> i64 {\n",
    "    when { return value }\n",
    "}\n",
);

/// TYPE-12: "`ref<T>` designates an endpoint, not an ordinary payload
/// type", so a graph gives a `ref<T>` input a connection of `T`, which
/// it designates. TIME-10: "Ordinary followers observe the selected
/// target": a `ref<T>` is given to an input of `T`, a lifted value
/// function's among them, and returned where a graph's result is `T`,
/// from a branch of an `if` or a `switch` too. TYPE-14: the `?` of
/// `ref<i64>?` marks an input that may be left out, and is no part of
/// what it designates.
/// TYPE-12: "`signal` is a complete temporal input constraint with no
/// readable payload": any connection is given to one.
#[test]
fn a_graph_gives_a_connection_across_a_reference_and_to_a_signal() {
    let checked = module(&format!(
        "{NODES}{}",
        concat!(
            "struct Quote { bid: i64 }\n",
            "fn quoted(bid: i64) -> Quote {\n",
            "    when { return Quote(bid: bid) }\n",
            "}\n",
            "const fn doubled(value: i64) -> i64 => value * 2\n",
            "fn followed(use_left: bool, left: i64, right: i64) -> i64 => pick(use_left, left, right)\n",
            "fn given(use_left: bool, left: i64, right: i64) -> i64 => plain(pick(use_left, left, right))\n",
            "fn lifted(use_left: bool, left: i64, right: i64) -> i64 => doubled(pick(use_left, left, right))\n",
            "fn designated(left: i64) -> ref<i64> => left\n",
            "fn passed(left: ref<i64>) -> ref<i64> => left\n",
            "fn woken(value: i64) -> i64 => counted(value)\n",
            "fn woken_by_struct(bid: i64) -> i64 => counted(quoted(bid))\n",
            "fn woken_by_reference(left: ref<i64>) -> i64 => counted(left)\n",
            "fn passed_on(tick: signal) -> i64 => counted(tick)\n",
            "fn branched(flag: bool, left: ref<i64>, right: i64) -> i64 => if flag { left } else { right }\n",
            "fn optional(target: ref<i64>?) -> i64 => target\n",
            "fn optional_passed(target: ref<i64>?) -> ref<i64> => target\n",
            "fn switched(flag: bool, left: ref<i64>, right: ref<i64>) -> i64 {\n",
            "    switch flag {\n",
            "        case true: { return left }\n",
            "        default: { return right }\n",
            "    }\n",
            "}\n",
        )
    ));
    assert_eq!(checked.summary(), NONE);
}

/// What composition gives across a reference stays a connection of the
/// type it is: a reference is no `signal`, a `signal` is read by no
/// ordinary input, and an ordinary value is given neither (TYPE-12).
#[test]
fn a_reference_and_a_signal_take_only_what_their_rules_admit() {
    let checked = module(&format!(
        "{NODES}{}",
        concat!(
            "fn wider(left: f64) -> ref<i64> => left\n",
            "fn from_signal(tick: signal) -> i64 => plain(tick)\n",
            "fn as_reference(tick: signal) -> ref<i64> => tick\n",
            "fn constant_designated() -> i64 => pick(true, 1, 2)\n",
            "fn constant_signal() -> i64 => counted(1)\n",
        )
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:14:36 type/type.mismatch EXPR-5",
            "m.hgl:15:46 type/type.mismatch EXPR-5",
            "m.hgl:16:46 type/type.mismatch EXPR-5",
            "m.hgl:17:47 type/type.mismatch EXPR-5",
            "m.hgl:17:50 type/type.mismatch EXPR-5",
            "m.hgl:18:40 type/type.mismatch EXPR-5",
        ]
    );
}

/// GEN-11: "Argument inference removes reference boundaries recursively
/// from an otherwise unbound generic; such an input observes values. An
/// explicit ref pattern binds its underlying type. A stated resolution
/// preserves its reference boundaries. A requested output binds a whole
/// output variable as requested".
#[test]
fn inference_removes_a_reference_from_a_generic_that_is_not_one() {
    let checked = module(&format!(
        "{NODES}{}",
        concat!(
            "fn ticks<T>(value: T) -> i64 {\n",
            "    when { return 1 }\n",
            "}\n",
            "fn designations<T>(value: ref<T>) -> i64 {\n",
            "    when { return 1 }\n",
            "}\n",
            "fn same<T>(value: T) -> T {\n",
            "    when { return value }\n",
            "}\n",
            "fn followed(use_left: bool, left: i64, right: i64) -> i64 => ticks(pick(use_left, left, right))\n",
            "fn listed(left: list<ref<i64>, 2>) -> i64 => ticks(left)\n",
            "fn designated(use_left: bool, left: i64, right: i64) -> i64 => designations(pick(use_left, left, right))\n",
            "fn designated_value(left: i64) -> i64 => designations(left)\n",
            "fn stated(left: ref<i64>) -> i64 => ticks<ref<i64>>(left)\n",
            "fn requested(left: i64) -> ref<i64> => same(left)\n",
        )
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(instances(&checked, "followed"), ["ticks<i64>"]);
    assert_eq!(instances(&checked, "listed"), ["ticks<list<i64, 2>>"]);
    assert_eq!(instances(&checked, "designated"), ["designations<i64>"]);
    assert_eq!(
        instances(&checked, "designated_value"),
        ["designations<i64>"]
    );
    assert_eq!(instances(&checked, "stated"), ["ticks<ref<i64>>"]);
    assert_eq!(instances(&checked, "requested"), ["same<ref<i64>>"]);
}

/// TYPE-12: "`signal` is a complete temporal input constraint with no
/// readable payload ... A `signal` input takes no default but `null`."
/// A hook gives it to an endpoint operation and reads it nowhere else.
#[test]
fn a_signal_has_no_payload_and_no_default_but_null() {
    let checked = module(concat!(
        "fn defaulted(value: i64, tick: signal = 1) -> i64 => value\n",
        "fn nulled(value: i64, tick: signal = null) -> i64 => value\n",
        "fn optional(value: i64, tick: signal?) -> i64 => value\n",
        "fn sampled(value: i64, tick: signal) -> i64 {\n",
        "    when modified(tick) && valid(value) { return value }\n",
        "    when valid(tick) && is_active(tick) { return 0 }\n",
        "}\n",
        "fn read(tick: signal) -> bool {\n",
        "    when {\n",
        "        let held = tick\n",
        "        return true\n",
        "    }\n",
        "    stop {\n",
        "        if valid(tick) { let again = tick }\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:41 type/type.mismatch TYPE-12",
            "m.hgl:11:20 type/type.not_ordinary TYPE-12",
            "m.hgl:15:38 type/type.not_ordinary TYPE-12",
        ]
    );
}

/// TYPE-12: "`ref<T>` designates an endpoint, not an ordinary payload
/// type". In a hook a reference input is the designation: returned, and
/// given to an endpoint operation. A value function's parameters are
/// ordinary value types (FN-1), so one written as a generic parameter,
/// and an operator's value candidate, take none.
#[test]
fn a_hook_returns_a_reference_and_gives_it_to_no_value_function() {
    let checked = module(concat!(
        "const fn keep<T>(value: T) -> T => value\n",
        "fn kept(target: ref<i64>) -> ref<i64> {\n",
        "    when {\n",
        "        let held = target\n",
        "        if valid(target) && modified(target) { return held }\n",
        "    }\n",
        "}\n",
        "fn given(target: ref<i64>) -> ref<i64> {\n",
        "    when { return keep(target) }\n",
        "}\n",
        "fn compared(target: ref<i64>, other: ref<i64>) -> bool {\n",
        "    when { return target == other }\n",
        "}\n",
        "fn added(target: ref<i64>) -> i64 {\n",
        "    when { return target + 1 }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:10:24 type/type.not_ordinary TYPE-12",
            "m.hgl:13:19 type/type.not_ordinary TYPE-12",
            "m.hgl:13:29 type/type.not_ordinary TYPE-12",
            "m.hgl:16:26 operator/operator.no_candidate OPS-1",
        ]
    );
}

/// TYPE-9: "`atomic<V>` stops expansion at a complete ordinary V", and
/// TYPE-16: "an atomic publishes whole". In a hook an atomic input, and
/// `out` (TIME-7), is read as that V: bound to a `let` of V, given to a
/// value function of V, compared, and projected. A graph keeps the
/// atomic's shape: it is no struct's bundle.
#[test]
fn a_hook_reads_an_atomic_input_as_its_whole_value() {
    let checked = module(concat!(
        "struct Quote {\n",
        "    bid: i64\n",
        "    ask: i64\n",
        "}\n",
        "const fn width(quote: Quote) -> i64 => quote.ask - quote.bid\n",
        "fn widths(quote: atomic<Quote>, other: atomic<Quote>) -> i64 {\n",
        "    when {\n",
        "        let held: Quote = quote\n",
        "        if quote == other { return width(held) }\n",
        "        return width(other) + delta_value(quote).ask + quote.bid\n",
        "    }\n",
        "}\n",
        "fn snapshot(bid: i64) -> atomic<Quote> {\n",
        "    inject out\n",
        "    when {\n",
        "        out = Quote(bid: bid, ask: bid)\n",
        "        let whole: Quote = out\n",
        "        out = Quote(bid: whole.bid, ask: width(out))\n",
        "    }\n",
        "}\n",
        "fn reshaped(bid: i64) -> Quote => snapshot(bid)\n",
    ));
    assert_eq!(checked.summary(), ["m.hgl:22:35 type/type.mismatch EXPR-5"]);
}

/// TYPE-14: the `?` of a type that is no ordinary value marks omission,
/// and TIME-10 and GEN-11: a follower observes the type with every
/// reference removed. The analyser admits a follower by
/// `Type::followed`, and a target writes its input by the same method.
#[test]
fn a_follower_observes_the_type_under_its_references_and_omission() {
    let integer = Type::Scalar(Scalar::I64);
    let reference = |ty: Type| Type::Ref(Box::new(ty));
    let optional = |ty: Type| Type::Nullable(Box::new(ty));
    assert_eq!(optional(reference(integer.clone())).followed(), integer);
    assert_eq!(reference(reference(integer.clone())).followed(), integer);
    assert_eq!(
        optional(reference(optional(integer.clone()))).followed(),
        optional(integer.clone())
    );
    assert_eq!(
        optional(integer.clone()).followed(),
        optional(integer.clone())
    );
    assert_eq!(Type::Signal.omission_removed(), &Type::Signal);
    assert_eq!(optional(Type::Signal).omission_removed(), &Type::Signal);
    assert_eq!(
        optional(integer.clone()).omission_removed(),
        &optional(integer)
    );
}
