//! The analyser against the rules on an endpoint given to a function
//! (TIME-9, TYPE-12). Each test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, check};
use hgl_analyser::hir::{Body, ExprKind, Stmt};

/// No lines.
const NONE: [&str; 0] = [];

/// A core that declares the endpoint operations as TIME-9 has core
/// declare them, with the one operator the tests' symbols need. Each
/// test gives it, so that what is checked does not turn on which core
/// the compiler is built with.
const CORE: &str = concat!(
    "module core\n",
    "export native const fn valid<T>(input: endpoint<T>) -> bool\n",
    "export native const fn modified<T>(input: endpoint<T>) -> bool\n",
    "export native const fn delta_value<T>(input: endpoint<T>) -> T\n",
    "export native const fn last_modified<T>(input: endpoint<T>) -> datetime?\n",
    "export native const fn invalidate<T>(input: endpoint<T>)\n",
    "export native const fn activate<T>(input: endpoint<T>)\n",
    "export native const fn passivate<T>(input: endpoint<T>)\n",
    "export native const fn is_active<T>(input: endpoint<T>) -> bool\n",
    "operator add_<L, R, O>(lhs: L, rhs: R) -> O\n",
    "native impl const fn add_(lhs: i64, rhs: i64) -> i64\n",
);

/// Checks `core.hgl`, the core above, with one file, `m.hgl`, of module
/// `m`. The `module` line is added, so the text given starts on line 2.
fn module(text: &str) -> Checked {
    check(&[("core.hgl", CORE), ("m.hgl", &format!("module m\n{text}"))])
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

/// The locals of `name`, as `local: type`.
fn locals(checked: &Checked, name: &str) -> Vec<String> {
    body(checked, name)
        .locals
        .iter()
        .map(|local| format!("{}: {}", local.name, local.ty))
        .collect()
}

/// TIME-9: "The operations on an endpoint are core's native value
/// functions with such a parameter: `valid`, `modified`, `delta_value`,
/// `last_modified`, `invalidate`, `activate`, and `passivate` ...
/// `modified` and `valid` are selectors in a `when` header (TIME-3):
/// there they are forms of the language, which set subscription and
/// admission." TIME-4: "A guard is a test of `valid(input)`, on the
/// input by its name". So a node reads the same whether core has
/// declared the functions or not.
#[test]
fn the_endpoint_operations_are_cores_and_the_selectors_the_languages() {
    let checked = module(concat!(
        "fn manual_forward(value: i64) -> i64 {\n",
        "    inject out\n",
        "    when modified() && valid([]) {\n",
        "        if valid(value) { return value }\n",
        "    }\n",
        "}\n",
        "fn summed(a: i64, b: i64) -> i64 {\n",
        "    inject out\n",
        "    when modified(a, b) && valid(a) {\n",
        "        if valid(b) { return a + b }\n",
        "        let last = last_modified(a)\n",
        "        let change = delta_value(a)\n",
        "        let fresh = modified(b)\n",
        "        invalidate(out)\n",
        // `b` is not among those the header requires, and no guard
        // proves it here.
        "        return a + b\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:16:20 phase/payload.unguarded TIME-4"]
    );
    // The names are core's now, and none is left for the prelude.
    assert_eq!(checked.prelude_names("m"), NONE);
    assert_eq!(
        locals(&checked, "summed")[3..],
        ["last: datetime?", "change: i64", "fresh: bool"]
    );
    // The header's selectors are the language's forms.
    let Some(Stmt::Hook {
        condition: Some(condition),
        ..
    }) = body(&checked, "manual_forward")
        .statements
        .iter()
        .find(|statement| matches!(statement, Stmt::Hook { .. }))
    else {
        panic!("`manual_forward` has a `when` with a header");
    };
    let ExprKind::Binary { left, right, .. } = &condition.kind else {
        panic!("the header is a conjunction");
    };
    assert!(matches!(left.kind, ExprKind::Endpoint { inputs: None, .. }));
    assert!(matches!(
        &right.kind,
        ExprKind::Endpoint { inputs: Some(named), .. } if named.is_empty()
    ));
}

/// TIME-9: an endpoint operation written with its generic argument is
/// the form it is without one. TIME-7: "`out` is read in a `when` and
/// in `stop`", so not in `start`, however the read is written. GEN-8:
/// a call may supply its generic arguments; core declares each
/// operation with one, the type its endpoint holds.
#[test]
fn an_endpoint_operation_with_a_generic_argument_is_the_same_form() {
    let checked = module(concat!(
        "fn typed(a: i64) -> i64 {\n",
        "    inject out\n",
        "    start { let first = delta_value<i64>(out) }\n",
        "    when valid([]) {\n",
        "        if valid<i64>(a) { return a }\n",
        "        let texts = valid<str>(a)\n",
        "        let two = valid<i64, i64>(a)\n",
        "        let number = modified<3>(a)\n",
        "        return 0\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:25 phase/out.lifecycle TIME-7",
            "m.hgl:7:27 type/type.mismatch EXPR-5",
            "m.hgl:8:19 type/generic.argument_count GEN-8",
            "m.hgl:9:31 type/generic.argument_kind GEN-14",
        ]
    );
    // The guard written with its argument proves its input as the one
    // without does: `return a` under manual admission is not reported.

    // A header's selector written so sets admission as the one without
    // does (TIME-3): `b` is not among those it names.
    let header = module(concat!(
        "fn selected(a: i64, b: i64) -> i64 {\n",
        "    when valid<i64>(a) { return b }\n",
        "}\n",
    ));
    assert_eq!(
        header.summary(),
        ["m.hgl:3:33 phase/payload.unguarded TIME-4"]
    );
}

/// TIME-9: "A call gives an `endpoint<T>` parameter a temporal input of
/// the enclosing node by its name, or `out`, and nothing else ... Every
/// other function takes the payload". TYPE-12: "`endpoint<T>` is the
/// endpoint of a temporal value of T, for a function that takes the
/// endpoint and not its payload".
#[test]
fn an_endpoint_parameter_takes_an_input_by_its_name() {
    let checked = module(concat!(
        "native const fn peek<T>(input: endpoint<T>) -> T\n",
        "native const fn seen(input: endpoint<i64>) -> bool\n",
        "fn reads(a: i64, const fixed: i64) -> i64 {\n",
        "    state kept: i64 = 0\n",
        "    inject out\n",
        // No input is required to be valid: the endpoint is given, and
        // its payload is not read.
        "    when valid([]) {\n",
        "        let one = peek(a)\n",
        "        let two = seen((a))\n",
        "        let mine = peek(out)\n",
        // What has no endpoint: a state, a `const` parameter, an
        // expression. An input of another type does not fit.
        "        let held = peek(kept)\n",
        "        let constant = seen(fixed)\n",
        "        let sum = seen(1 + 2)\n",
        "        return one\n",
        "    }\n",
        "}\n",
        "fn texts(a: str) -> bool {\n",
        "    when valid([]) { return seen(a) }\n",
        "}\n",
        // A graph and a value function have no input of their own to
        // give.
        "fn graph(a: i64) -> bool => seen(a)\n",
        "const fn value(a: i64) -> bool => seen(a)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:11:25 phase/endpoint.argument TIME-9",
            "m.hgl:12:29 phase/endpoint.argument TIME-9",
            "m.hgl:13:24 phase/endpoint.argument TIME-9",
            "m.hgl:18:34 type/type.mismatch EXPR-5",
            "m.hgl:20:34 phase/endpoint.argument TIME-9",
            "m.hgl:21:40 phase/endpoint.argument TIME-9",
        ]
    );
    assert_eq!(
        locals(&checked, "reads")[4..7],
        ["one: i64", "two: bool", "mine: i64"]
    );
    // `peek(a)`: the endpoint of `a`, and no read of it.
    let reads = body(&checked, "reads");
    let Some(Stmt::Hook { body, .. }) = reads
        .statements
        .iter()
        .find(|statement| matches!(statement, Stmt::Hook { .. }))
    else {
        panic!("`reads` has a `when`");
    };
    let Some(Stmt::Declare {
        value: Some(one), ..
    }) = body.first()
    else {
        panic!("the `when` begins with a `let`");
    };
    let ExprKind::Call { arguments, .. } = &one.kind else {
        panic!("`peek(a)` is a call");
    };
    assert!(matches!(arguments[0].value.kind, ExprKind::EndpointOf(0)));
    assert_eq!(arguments[0].value.ty.to_string(), "endpoint<i64>");
}

/// TIME-9: a node can change the activity of a direct temporal input.
/// Its output and a computed payload have no input subscription to change.
#[test]
fn activity_operations_take_direct_inputs() {
    let checked = module(concat!(
        "fn change(input: i64) -> i64 {\n",
        "    inject out\n",
        "    when {\n",
        "        passivate(input)\n",
        "        activate<i64>(input)\n",
        "        activate(out)\n",
        "        passivate<i64>(out)\n",
        "        activate(input + 1)\n",
        "        return input\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:7:18 phase/endpoint.argument TIME-9",
            "m.hgl:8:24 phase/endpoint.argument TIME-9",
            "m.hgl:9:18 phase/endpoint.argument TIME-9",
        ]
    );
}

/// END-5 and TIME-9: core's activity query takes a direct input in a
/// handler or in stop. As an ordinary boolean guard it proves no validity.
#[test]
fn activity_is_an_observation_of_a_direct_input() {
    let checked = module(concat!(
        "use core as c\n",
        "fn observe(data: i64, const fixed: i64) -> i64 {\n",
        "    inject out\n",
        "    start { let early = is_active(data) }\n",
        "    when valid([]) && is_active(input: data) { return data }\n",
        "    when valid([]) {\n",
        "        let current = c::is_active<i64>(input: (data))\n",
        "        let output = is_active(out)\n",
        "        let constant = is_active(fixed)\n",
        "        let expression = is_active(1)\n",
        "        let wrong = is_active(other: fixed)\n",
        "        return 0\n",
        "    }\n",
        "    stop { let final_activity = is_active(data) }\n",
        "}\n",
        "fn wired(data: i64) -> bool => is_active(data)\n",
        "test no_node { let unused = eval(is_active<i64>(input: [])) }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:25 phase/input.lifecycle TIME-5",
            "m.hgl:6:55 phase/payload.unguarded TIME-4",
            "m.hgl:9:32 phase/endpoint.argument TIME-9",
            "m.hgl:10:34 phase/endpoint.argument TIME-9",
            "m.hgl:11:36 phase/endpoint.argument TIME-9",
            "m.hgl:12:21 type/argument.missing EXPR-2",
            "m.hgl:12:31 name/argument.unknown EXPR-2",
            "m.hgl:17:42 phase/endpoint.argument TIME-9",
            "m.hgl:18:56 phase/endpoint.argument TIME-9",
        ],
        "{}",
        checked.report
    );
    let observed = locals(&checked, "observe");
    assert!(observed.iter().any(|entry| entry == "current: bool"));
    assert!(observed.iter().any(|entry| entry == "final_activity: bool"));
}

/// UNIT-11: a program whose core has no activity query can define an
/// ordinary function with that name. The call reads its payload arguments.
#[test]
fn the_spelling_is_active_alone_does_not_make_an_endpoint_query() {
    let checked = check(&[
        ("core.hgl", "module core\n"),
        (
            "local.hgl",
            concat!(
                "module local\n",
                "const fn is_active(left: i64, right: i64) -> i64 => left\n",
                "fn use_local(data: i64) -> i64 {\n",
                "    when { return is_active(left: data, right: 0) }\n",
                "}\n",
            ),
        ),
    ]);
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
}
