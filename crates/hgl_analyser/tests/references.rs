//! The analyser against the rules on references, signals and atomics:
//! `ref` as a mode of a node's parameter and of its result (FN-16), what
//! a graph gives a `ref` parameter and a `signal`, what a generic is
//! inferred to be at one (GEN-11, GEN-12), and what a hook reads of each
//! (TYPE-9, TYPE-12, TIME-10). Each test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, body, module};
use hgl_analyser::hir::{Callee, Expr, ExprKind, Stmt, walk_statements};
use hgl_analyser::{Argument, Detail, Scalar, Type};

/// No lines.
const NONE: [&str; 0] = [];

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

/// The signature of the function `name`, as HGL writes it.
fn signature(checked: &Checked, name: &str) -> String {
    let module = &checked.modules()[0];
    let index = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == name)
        .expect("the function is declared");
    match &module.details[index] {
        Detail::Callable(signature) => signature.to_string(),
        other => panic!("{name} is no callable: {other:?}"),
    }
}

/// Every value a `return` in the body of `name` is given, as the kind
/// of expression it is.
fn returned(checked: &Checked, name: &str) -> Vec<String> {
    fn visit(statements: &[Stmt], found: &mut Vec<String>) {
        for statement in statements {
            match statement {
                Stmt::Return {
                    value: Some(value), ..
                } => found.push(match &value.kind {
                    ExprKind::EndpointOf(_) => format!("endpoint of {}", value.ty),
                    ExprKind::ChildEndpointOf(_) => format!("child endpoint of {}", value.ty),
                    _ => format!("value of {}", value.ty),
                }),
                Stmt::Hook { body, .. } => visit(body, found),
                Stmt::Expr(Expr {
                    kind: ExprKind::If { then, otherwise, .. },
                    ..
                }) => {
                    for branch in [Some(&**then), otherwise.as_deref()].into_iter().flatten() {
                        if let ExprKind::Block { statements, .. } = &branch.kind {
                            visit(statements, found);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut found = Vec::new();
    visit(&body(checked, name).statements, &mut found);
    found
}

/// The nodes the tests below give connections to.
const NODES: &str = concat!(
    "fn pick(use_left: bool, ref left: i64, ref right: i64) -> ref i64 {\n",
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

/// FN-16: "The mark is a mode, as `var` is (FN-15), and no part of the
/// type (TYPE-12): a caller and the type checker see the T after it. A
/// connection of T is given to a `ref` parameter, and a `ref` result is
/// a connection of T to whoever reads it." So a graph gives a `ref`
/// parameter a connection of its type, a lifted value function and an
/// ordinary input take a `ref` result as one of its type, and a graph
/// returns one, from a branch of an `if` or a `switch` too. TYPE-12:
/// "`signal` is a complete temporal input constraint with no readable
/// payload": any connection is given to one.
#[test]
fn a_graph_gives_a_connection_to_a_ref_parameter_and_takes_a_ref_result() {
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
            "fn constant(right: i64) -> i64 => pick(true, 1, right)\n",
            "fn woken(value: i64) -> i64 => counted(value)\n",
            "fn woken_by_struct(bid: i64) -> i64 => counted(quoted(bid))\n",
            "fn woken_by_reference(use_left: bool, left: i64) -> i64 => counted(pick(use_left, left, left))\n",
            "fn passed_on(tick: signal) -> i64 => counted(tick)\n",
            "fn branched(flag: bool, left: i64, right: i64) -> i64 =>\n",
            "    if flag { pick(flag, left, right) } else { right }\n",
            "fn switched(flag: bool, left: i64, right: i64) -> i64 {\n",
            "    switch flag {\n",
            "        case true: { return pick(flag, left, right) }\n",
            "        default: { return right }\n",
            "    }\n",
            "}\n",
        )
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        signature(&checked, "pick"),
        "(use_left: bool, ref left: i64, ref right: i64) -> ref i64"
    );
}

/// FN-16: a caller sees the T after the mark, so what a graph gives a
/// `ref` parameter, and what a `ref` result is given to, is held to that
/// type as any connection is (EXPR-5): a `ref` mark is no `signal`, and
/// a `signal` is given to no `ref` parameter (TYPE-12).
#[test]
fn a_ref_parameter_and_a_signal_take_only_what_their_types_admit() {
    let checked = module(&format!(
        "{NODES}{}",
        concat!(
            "fn wider(use_left: bool, left: f64, right: i64) -> i64 => pick(use_left, left, right)\n",
            "fn from_signal(tick: signal) -> i64 => plain(tick)\n",
            "fn designated(tick: signal) -> i64 => pick(true, tick, 1)\n",
            "fn named(use_left: bool) -> str => pick(use_left, 1, 2)\n",
        )
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:13:76 type/type.mismatch EXPR-5",
            "m.hgl:14:46 type/type.mismatch EXPR-5",
            "m.hgl:15:49 type/type.mismatch EXPR-5",
            "m.hgl:16:36 type/type.mismatch EXPR-5",
        ]
    );
}

/// FN-16: "A node (FN-2, NATIVE-3) may mark a temporal parameter `ref`
/// ... A value function, a graph, and an operator mark none, and a mark
/// there rejects: an operator gives the parameters every candidate
/// takes, and a candidate that is a node marks its own ... Proposed: ...
/// a mark on what is no node with `ref.position`." The language's own
/// rejections: `const fn scale(ref value: f64) -> f64` (FN-1), and
/// `ref idx: i64` on a graph.
#[test]
fn only_a_node_marks_a_parameter_or_its_result_ref() {
    let checked = module(concat!(
        "const fn scale(ref value: f64) -> f64 => value\n",
        "const fn gives(value: f64) -> ref f64 => value\n",
        "fn routed(ref idx: i64, items: list<f64, 3>) -> f64 => items[0]\n",
        "fn designated(idx: i64) -> ref i64 => idx\n",
        "operator watch<T>(ref value: T) -> i64\n",
        "impl const fn watch(ref value: i64) -> i64 => 1\n",
        "impl fn watch(ref value: f64) -> i64 {\n",
        "    when { return 1 }\n",
        "}\n",
        "native const fn measured(ref value: f64) -> f64\n",
        "native fn wired(ref value: f64) -> ref f64\n",
        "native fn wired(ref value: f64) -> ref f64 {}\n",
        "native fn nodal(ref value: f64) -> ref f64\n",
        "native fn nodal(ref value: f64) -> ref f64 { when; }\n",
        "fn held(ref value: f64) -> ref f64 {\n",
        "    when { return value }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:1:16 function-kind/ref.position FN-16",
            "m.hgl:2:30 function-kind/ref.position FN-16",
            "m.hgl:3:11 function-kind/ref.position FN-16",
            "m.hgl:4:28 function-kind/ref.position FN-16",
            "m.hgl:5:19 function-kind/ref.position FN-16",
            "m.hgl:6:21 function-kind/ref.position FN-16",
            "m.hgl:10:26 function-kind/ref.position FN-16",
            "m.hgl:12:17 function-kind/ref.position FN-16",
            "m.hgl:12:36 function-kind/ref.position FN-16",
        ]
    );
    assert_eq!(
        checked.findings[0].message,
        "`scale` is a value function, and only a node marks a parameter or its result `ref`"
    );
}

/// FN-16: "A `ref` parameter and its children, `xs[i]` and `quote.bid`,
/// are endpoints, not values: the endpoint operations take them
/// (TIME-9), `delta_value` aside ... A read of the value or of the
/// `delta_value` of one rejects when the program is compiled. Proposed:
/// that read rejects with the code `ref.read`". EXPR-7: "A structural
/// projection of a `ref` parameter selects its child and reads no
/// payload ... an atomic projection of one, `quote.bid` of `ref quote:
/// atomic<Quote>`, reads its payload and rejects". CTRL-8: "Iteration
/// reads what it traverses, so over a `ref` parameter it rejects".
/// TIME-10, proposed: `valid`, `modified` and `last_modified` of one ask
/// of its designation.
#[test]
fn a_ref_parameter_is_an_endpoint_and_no_value() {
    let checked = module(concat!(
        "struct Quote {\n",
        "    bid: i64\n",
        "    ask: i64\n",
        "}\n",
        "const fn doubled(value: i64) -> i64 => value * 2\n",
        "fn observed(ref price: i64, ref xs: list<i64, 3>, ref quote: Quote, idx: i64) -> bool {\n",
        "    when valid([]) {\n",
        "        let seen = last_modified(price)\n",
        "        if valid(xs[idx]) && modified(quote.bid) && all_valid(xs) { return is_active(price) }\n",
        "        return valid(price) && modified(price) && valid(xs[0]) && seen != null\n",
        "    }\n",
        "}\n",
        "fn read(ref price: i64, ref xs: list<i64, 3>, ref snap: atomic<Quote>) -> i64 {\n",
        "    when {\n",
        "        let held = price\n",
        "        var total: i64 = xs[0] + doubled(price)\n",
        "        for x in xs { total += x }\n",
        "        return delta_value(price) + delta_value(xs[1]) + snap.bid + total\n",
        "    }\n",
        "    stop {\n",
        "        if valid(price) { let last = price }\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:15:20 phase/ref.read FN-16",
            "m.hgl:16:26 phase/ref.read FN-16",
            "m.hgl:16:42 phase/ref.read FN-16",
            "m.hgl:17:18 phase/ref.read FN-16",
            "m.hgl:18:16 phase/ref.read FN-16",
            "m.hgl:18:37 phase/ref.read FN-16",
            "m.hgl:18:57 phase/ref.read FN-16",
            "m.hgl:21:38 phase/ref.read FN-16",
        ]
    );
    assert_eq!(
        checked.findings[0].message,
        "`price` is a `ref` parameter, an endpoint whose value is not read"
    );
}

/// FN-16: "A `return` to a `ref` result is given an expression that
/// names an endpoint: a temporal input of the node, `ref` or not, or a
/// child of one, `items[idx]` ... No payload is read, so nothing needs
/// admitting (TIME-1), and a child is an endpoint whether or not it is
/// valid. A value that names no endpoint rejects there
/// (`type.mismatch`). A parameter that is not `ref` may be designated
/// too, and a body may both read and designate it." TIME-7: "A `ref`
/// result takes no value". The language's rejection: `return items[0] *
/// 2.0` to `fn first(items: list<f64, 3>) -> ref f64`.
#[test]
fn a_ref_result_is_given_an_endpoint_it_designates() {
    let checked = module(concat!(
        "fn route<T>(idx: i64, items: list<T, 3>) -> ref T {\n",
        "    when modified(idx) { return items[idx] }\n",
        "}\n",
        "fn passed(flag: bool, ref low: f64, high: f64) -> ref f64 {\n",
        "    when valid(flag) {\n",
        "        if high > 0.0 { return high }\n",
        "        return low\n",
        "    }\n",
        "}\n",
        "fn first(items: list<f64, 3>) -> ref f64 {\n",
        "    when all_valid(items) { return items[0] * 2.0 }\n",
        "}\n",
        "fn constant(flag: bool) -> ref f64 {\n",
        "    when { return 1.0 }\n",
        "}\n",
        "fn wider(value: i64) -> ref f64 {\n",
        "    when { return value }\n",
        "}\n",
        "fn state_given(value: f64) -> ref f64 {\n",
        "    state last: f64 = 0.0\n",
        "    when { return last }\n",
        "}\n",
        "fn quiet(value: f64) -> ref f64 {\n",
        "    when { return }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:11:36 type/type.mismatch FN-16",
            "m.hgl:14:19 type/type.mismatch FN-16",
            "m.hgl:17:19 type/type.mismatch FN-16",
            "m.hgl:21:19 type/type.mismatch FN-16",
        ]
    );
    assert_eq!(returned(&checked, "route"), ["child endpoint of T"]);
    assert_eq!(
        returned(&checked, "passed"),
        ["endpoint of f64", "endpoint of f64"]
    );
    assert_eq!(
        checked.findings[0].message,
        "a `ref` result takes an endpoint: a temporal input of `first`, or a child of one"
    );
}

/// FN-16, proposed: "`out = items[idx]` designates as `return items[idx]`
/// does, and is rejected where the `return` would be; otherwise `out` of
/// a `ref` result is as a `ref` parameter is: the endpoint operations
/// take it, `delta_value` aside, and a read of its value rejects."
#[test]
fn out_of_a_ref_result_designates_and_is_no_value() {
    let checked = module(concat!(
        "fn route(idx: i64, items: list<f64, 3>) -> ref f64 {\n",
        "    inject out\n",
        "    when modified(idx) {\n",
        "        out = items[idx]\n",
        "        if valid(out) && modified(out) { out = items[0] }\n",
        "    }\n",
        "}\n",
        "fn read(idx: i64, items: list<f64, 3>) -> ref f64 {\n",
        "    inject out\n",
        "    when {\n",
        "        let held = out\n",
        "        out = 2.0\n",
        "        let change = delta_value(out)\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:11:20 phase/ref.read FN-16",
            "m.hgl:12:15 type/type.mismatch FN-16",
            "m.hgl:13:34 phase/ref.read FN-16",
        ]
    );
}

/// GEN-11: "No type holds a reference (TYPE-12), so argument inference
/// removes none: a generic binds from the type of the connection it is
/// given, at a `ref` parameter as at any other (FN-16). `ref low: T`
/// given a connection of `f64` binds T to `f64`, and so does `value: T`
/// given a `ref` result of `f64`". GEN-12: "a `ref` mark adds no
/// priority": the candidate that is more specific is selected, marked or
/// not.
#[test]
fn a_generic_binds_from_the_connection_at_a_ref_parameter_as_anywhere() {
    let checked = module(&format!(
        "{NODES}{}",
        concat!(
            "fn ticks<T>(value: T) -> i64 {\n",
            "    when { return 1 }\n",
            "}\n",
            "fn designations<T>(ref value: T) -> i64 {\n",
            "    when { return 1 }\n",
            "}\n",
            "fn same<T>(value: T) -> T {\n",
            "    when { return value }\n",
            "}\n",
            "operator watch<T>(value: T) -> i64\n",
            "impl fn watch<T>(ref value: T) -> i64 {\n",
            "    when { return 1 }\n",
            "}\n",
            "impl fn watch(value: i64) -> i64 {\n",
            "    when { return 2 }\n",
            "}\n",
            "fn followed(use_left: bool, left: i64, right: i64) -> i64 => ticks(pick(use_left, left, right))\n",
            "fn listed(left: list<i64, 2>) -> i64 => designations(left)\n",
            "fn designated(use_left: bool, left: i64, right: i64) -> i64 => designations(pick(use_left, left, right))\n",
            "fn designated_value(left: i64) -> i64 => designations(left)\n",
            "fn stated(left: i64) -> i64 => designations<i64>(left)\n",
            "fn requested(left: i64) -> i64 => designations(same(left))\n",
            "fn watched(left: i64) -> i64 => watch(left)\n",
        )
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(instances(&checked, "followed"), ["ticks<i64>"]);
    assert_eq!(instances(&checked, "listed"), ["designations<list<i64, 2>>"]);
    assert_eq!(instances(&checked, "designated"), ["designations<i64>"]);
    assert_eq!(
        instances(&checked, "designated_value"),
        ["designations<i64>"]
    );
    assert_eq!(instances(&checked, "stated"), ["designations<i64>"]);
    assert_eq!(
        instances(&checked, "requested"),
        ["same<i64>", "designations<i64>"]
    );
    let mut candidate = None;
    walk_statements(&body(&checked, "watched").statements, &mut |value| {
        if let ExprKind::Call {
            callee: Callee::Candidate { module, declaration, .. },
            ..
        } = &value.kind
        {
            candidate = Some((*module, *declaration));
        }
    });
    let (module, declaration) = candidate.expect("`watch` selects a candidate");
    // The candidate of `i64`, which is not marked, over the generic one,
    // which is.
    match &checked.program.modules[module].details[declaration] {
        Detail::Callable(selected) => assert_eq!(selected.to_string(), "(value: i64) -> i64"),
        other => panic!("no callable: {other:?}"),
    }
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

/// TYPE-9: "`atomic<V>` stops expansion at a complete ordinary V", and
/// TYPE-16: "an atomic publishes whole". In a hook an atomic input, and
/// `out` (TIME-7), is read as that V: bound to a `let` of V, given to a
/// value function of V, compared, and projected; one that may be left
/// out (TYPE-14: the `?` marks omission) the same, where it is valid. A
/// graph keeps the atomic's shape: it is no struct's bundle.
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
        "fn optional(value: i64, quote: atomic<Quote>?) -> i64 {\n",
        "    when valid(value) {\n",
        "        if valid(quote) {\n",
        "            let whole: Quote = quote\n",
        "            return width(whole) + quote.bid\n",
        "        }\n",
        "        return value\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), ["m.hgl:22:35 type/type.mismatch EXPR-5"]);
}

/// TYPE-14: the `?` of a type that is no ordinary value marks omission,
/// and is no part of what the input is given or publishes. The analyser
/// admits a connection by `Type::omission_removed`, and a target writes
/// the input by the same method.
#[test]
fn the_question_mark_of_an_input_that_may_be_left_out_is_removed() {
    let integer = Type::Scalar(Scalar::I64);
    let optional = |ty: Type| Type::Nullable(Box::new(ty));
    assert_eq!(Type::Signal.omission_removed(), &Type::Signal);
    assert_eq!(optional(Type::Signal).omission_removed(), &Type::Signal);
    let atomic = Type::Atomic(Box::new(Type::Tuple(vec![integer.clone()])));
    assert_eq!(optional(atomic.clone()).omission_removed(), &atomic);
    assert_eq!(
        optional(integer.clone()).omission_removed(),
        &optional(integer)
    );
}
