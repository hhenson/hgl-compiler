//! Rules of names, types, expressions and conversions: a chained
//! comparison (EXPR-1), `const` as a name (FN-5), what a source returns
//! and where it yields (SOURCE-4), a tolerance and full comparison
//! (core's NUM-7), and what a conversion is (core's OPS-6, OPS-7). Each
//! test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`; a code
//! local to this compiler has `local:` before it.

mod common;

use common::{Checked, body, locals, module};
use hgl_analyser::hir::{Callee, Expr, ExprKind, Form, Stmt};
use hgl_analyser::{DeclarationKind, Role};

/// No findings.
const NONE: [&str; 0] = [];

/// The value the function `name` of module `m` returns: its arrow
/// body's.
fn returned<'a>(checked: &'a Checked, name: &str) -> &'a Expr {
    match body(checked, name).statements.as_slice() {
        [
            Stmt::Return {
                value: Some(value), ..
            },
        ] => value,
        other => panic!("`{name}` returns one value: {other:?}"),
    }
}

// ---- EXPR-1 ----

/// EXPR-1: "Repeated binary forms associate left ... A chained
/// comparison is not implicit mathematical conjunction." `a < b < c` is
/// `(a < b) < c`: a `bool` compared with `c`. Of three `i64`s that is a
/// `bool` beside an `i64`, which no candidate of `<` takes (OPS-2); of
/// three `bool`s it is admitted, a `bool` being ordered.
#[test]
fn a_chained_comparison_is_typed_as_written() {
    let checked = module(concat!(
        "const fn chained(a: i64, b: i64, c: i64) -> bool => a < b < c\n",
        "const fn flags(a: bool, b: bool, c: bool) -> bool => a < b < c\n",
        "const fn equal(a: i64, b: i64, c: bool) -> bool => a < b == c\n",
        "const fn spelled(a: i64, b: i64, c: i64) -> bool => a < b && b < c\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:2:59 operator/operator.no_candidate OPS-2"]
    );
    assert_eq!(
        checked.findings[0].message,
        "`<` takes no `bool` and `i64` to give `bool`"
    );
    // The left operand of the second `<` is the first.
    let ExprKind::Call {
        arguments, form, ..
    } = &returned(&checked, "flags").kind
    else {
        panic!("a call of `<`");
    };
    assert!(matches!(form, Form::Binary(_)));
    let inner = &arguments[0].value;
    assert!(matches!(
        inner.kind,
        ExprKind::Call {
            form: Form::Binary(_),
            ..
        }
    ));
    assert_eq!(inner.ty.to_string(), "bool");
}

// ---- FN-5 ----

/// FN-5: "`const` is a modifier only in a position admitting that
/// modifier. Otherwise it is an ordinary name, including for an
/// operator or function. `const(argument)` is an ordinary call
/// resolved by the usual naming and phase rules; it has no intrinsic
/// function-role selection behavior." The accepted fragments, and a
/// graph that calls a temporal and value pair named `const`, which
/// selects the temporal one (FN-4).
#[test]
fn a_call_of_const_is_an_ordinary_call() {
    let checked = module(concat!(
        "const fn const(value: i64) -> i64 => value\n",
        "const fn example() -> i64 => const(7)\n",
        "fn const(value: i64) -> i64 {\n",
        "    when { return value }\n",
        "}\n",
        "fn wired(x: i64) -> i64 => const(x)\n",
    ));
    assert_eq!(checked.summary(), NONE);
    let role = |name: &str| {
        let ExprKind::Call {
            callee:
                Callee::Function {
                    module,
                    declaration,
                    ..
                },
            ..
        } = &returned(&checked, name).kind
        else {
            panic!("`{name}` calls a function");
        };
        let declared = &checked.program.modules[*module].declarations[*declaration];
        assert_eq!(declared.name, "const");
        declared.kind
    };
    assert_eq!(role("example"), DeclarationKind::Function(Role::Value));
    assert_eq!(role("wired"), DeclarationKind::Function(Role::Temporal));
}

/// FN-4: a pair can be an operand of another pair without repeating
/// the phase reads of nested calls across operators and lexical scopes.
#[test]
fn nested_pairs_have_bounded_reads() {
    let operators = (0..21).fold(String::from("1"), |inner, _| format!("scale(1 + {inner})"));
    let blocks = (0..21).fold(String::from("1"), |inner, level| {
        format!("scale({{ let item_{level} = 1\n item_{level} + {inner} }})")
    });
    let comprehensions = (0..17).fold(String::from("1"), |inner, level| {
        format!("scale([for item_{level} in 0..<1: item_{level} + {inner}][0])")
    });
    let anonymous = (0..21).fold(String::from("(fn(x: i64) -> i64 => x)(1)"), |inner, _| {
        format!("scale({inner})")
    });
    // Use the analyser's stack for the parser limits, as `hgl` does.
    // The default test thread can overflow on these valid nested reads.
    let worker = std::thread::Builder::new()
        .stack_size(hgl_analyser::stack_size(hgl_parser::Limits::default()))
        .spawn(move || {
            for argument in [operators, blocks, comprehensions, anonymous] {
                let source = format!(
                    "const fn scale(value: i64) -> i64 => value * 2\n\
                     fn scale(value: i64) -> i64 => value * 3\n\
                     fn wired(tick: i64) -> i64 => tick + {argument}\n"
                );
                let checked = module(&source);
                assert_eq!(checked.summary(), NONE, "{}", checked.report);
            }
        })
        .expect("a thread starts");
    worker.join().expect("the analyser fits the stack");
}

/// FN-5: "`const(argument)` is an ordinary call resolved by the usual
/// naming and phase rules". With nothing named `const` it names
/// nothing (UNIT-10); a value function does not call a temporal
/// function of that name (FN-3).
#[test]
fn a_call_of_const_is_resolved_by_the_usual_rules() {
    let unbound = module("const fn example() -> i64 => const(7)\n");
    assert_eq!(unbound.summary(), ["m.hgl:2:30 name/name.unknown UNIT-10"]);
    let temporal = module(concat!(
        "fn const(value: i64) -> i64 {\n",
        "    when { return value }\n",
        "}\n",
        "const fn example() -> i64 => const(7)\n",
    ));
    assert_eq!(
        temporal.summary(),
        ["m.hgl:5:30 function-kind/call.role FN-3"]
    );
}

// ---- SOURCE-4 ----

/// SOURCE-4: "A bare return or body completion finishes the source ...
/// A valued return and yield inside a value expression reject." A
/// `yield` stands in the source's statements, and in the blocks of the
/// `if`, `while` and `for` statements and of a block among them; a
/// block or an `if` whose value is used is a value expression.
#[test]
fn a_source_returns_no_value_and_yields_in_statements() {
    let checked = module(concat!(
        "fn kept(const done: bool, const values: list<i64>) -> i64 {\n",
        "    yield 1s: 7\n",
        "    if done { return }\n",
        "    {\n",
        "        yield 1s: 8\n",
        "    }\n",
        "    if done {\n",
        "        yield 1s: 9\n",
        "    } else {\n",
        "        yield 2s: 9\n",
        "    }\n",
        "    for value in values {\n",
        "        yield 1s: value\n",
        "    }\n",
        "}\n",
        "fn valued(const done: bool) -> i64 {\n",
        "    yield 1s: 7\n",
        "    if done { return 9 }\n",
        "}\n",
        "fn in_block() -> i64 {\n",
        "    let x = {\n",
        "        yield 1s: 7\n",
        "        3\n",
        "    }\n",
        "    yield 1s: x\n",
        "}\n",
        "fn in_if(const done: bool) -> i64 {\n",
        "    let x = if done {\n",
        "        yield 1s: 7\n",
        "        3\n",
        "    } else {\n",
        "        4\n",
        "    }\n",
        "    yield 1s: x\n",
        "}\n",
        "fn in_loop(const values: list<i64>) -> i64 {\n",
        "    let x = {\n",
        "        for value in values {\n",
        "            yield 1s: value\n",
        "        }\n",
        "        3\n",
        "    }\n",
        "    yield 1s: x\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:19:22 phase/source.return_value SOURCE-4",
            "m.hgl:23:9 phase/yield.in_value SOURCE-4",
            "m.hgl:30:9 phase/yield.in_value SOURCE-4",
            "m.hgl:40:13 phase/yield.in_value SOURCE-4",
        ]
    );
}

// ---- NUM-7 ----

/// Core's NUM-7: "A tolerance option grants no ordering law ...
/// Approximate equality does not configure `<=>`." No candidate of
/// `compare_` or of an ordering declares `abs` or `rel`, so a tolerance
/// given to one, by symbol or by name, is an option nothing declares
/// (EXPR-10, EXPR-2). Equality takes it (NUM-4). Core declares no
/// operation that orders a domain, so none asks for a total-order
/// policy.
#[test]
fn a_tolerance_configures_no_comparison_but_equality() {
    let checked = module(concat!(
        "const fn close(a: f64, b: f64) -> bool => a =={abs: 0.1} b\n",
        "const fn full(a: f64, b: f64) -> CmpResult => a <=> b\n",
        "const fn loose(a: f64, b: f64) -> CmpResult => a <=>{abs: 0.1} b\n",
        "const fn named(a: f64, b: f64) -> CmpResult => compare_(a, b, rel: 0.1)\n",
        "const fn less(a: f64, b: f64) -> bool => a <{abs: 0.1} b\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:54 name/argument.unknown EXPR-10",
            "m.hgl:5:63 name/argument.unknown EXPR-2",
            "m.hgl:6:46 name/argument.unknown EXPR-10",
        ]
    );
}

// ---- OPS-6, OPS-7 ----

/// Core's OPS-6: "A conversion yields what the third table below says."
/// A conversion that cannot fail is the type asked for; one that can is
/// a `result` of it (OPS-7, OPS-8).
#[test]
fn each_conversion_is_of_the_type_its_table_gives() {
    let checked = module(concat!(
        "const fn uses(n: i64, real: f64, flag: bool, text: str, raw: bytes, day: date, moment: datetime, step: tenor) {\n",
        "    let a = convert<f64>(n)\n",
        "    let b = convert<i64>(real)\n",
        "    let c = convert<bool>(n)\n",
        "    let d = convert<bool>(real)\n",
        "    let e = convert<i64>(flag)\n",
        "    let f = convert<f64>(flag)\n",
        "    let g = convert<str>(n)\n",
        "    let h = convert<str>(real)\n",
        "    let i = convert<str>(flag)\n",
        "    let j = convert<i64>(text)\n",
        "    let k = convert<f64>(text)\n",
        "    let l = convert<bool>(text)\n",
        "    let m = convert<bytes>(text)\n",
        "    let o = convert<str>(raw)\n",
        "    let p = convert<datetime>(day)\n",
        "    let q = convert<date>(moment)\n",
        "    let r = convert<str>(step)\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    assert_eq!(
        locals(&checked, "uses")[8..],
        [
            "a: f64",
            "b: result<i64, error>",
            "c: bool",
            "d: bool",
            "e: i64",
            "f: f64",
            "g: str",
            "h: str",
            "i: str",
            "j: result<i64, error>",
            "k: result<f64, error>",
            "l: result<bool, error>",
            "m: bytes",
            "o: result<str, error>",
            "p: datetime",
            "q: date",
            "r: str",
        ]
    );
}

/// Core's OPS-6: "`convert<list<B>>(values)` is admitted too, where
/// `values` is a `list<A>` and the struct `A` inherits from the abstract
/// struct `B`; it yields the same elements." Core declares no candidate
/// for it, so the analyser admits it: through any number of parents,
/// each with the arguments the struct below gives it (TYPE-3). It
/// cannot fail and takes no option, so it is the list asked for (OPS-8).
///
/// A list of a struct to a list of that struct, of a struct that
/// inherits from no `B`, of a size, or of nullable elements is no such
/// conversion, and neither is a `result` of the list.
#[test]
fn a_list_of_structs_converts_to_a_list_of_their_parent() {
    let checked = module(concat!(
        "abstract struct Shape {}\n",
        "abstract struct Polygon : Shape {}\n",
        "struct Round : Shape { radius: f64 }\n",
        "struct Square : Polygon { side: f64 }\n",
        "struct Loose { x: f64 }\n",
        "abstract struct Tagged<T> : Shape { label: T }\n",
        "struct Tag<T> : Tagged<T> {}\n",
        "const fn widened(values: list<Round>) -> list<Shape> => convert<list<Shape>>(values)\n",
        "const fn deep(values: list<Square>) -> list<Shape> => convert<list<Shape>>(values)\n",
        "const fn parents(values: list<Polygon>) -> list<Shape> => convert<list<Shape>>(values)\n",
        "const fn generic(values: list<Tag<i64>>) -> list<Tagged<i64>> => convert<list<Tagged<i64>>>(values)\n",
        "const fn same(values: list<Round>) -> list<Round> => convert<list<Round>>(values)\n",
        "const fn unrelated(values: list<Loose>) -> list<Shape> => convert<list<Shape>>(values)\n",
        "const fn sized(values: list<Round, 2>) -> list<Shape, 2> => convert<list<Shape, 2>>(values)\n",
        "const fn optional(values: list<Round?>) -> list<Shape?> => convert<list<Shape?>>(values)\n",
        "const fn other(values: list<Tag<i64>>) -> list<Tagged<f64>> => convert<list<Tagged<f64>>>(values)\n",
        "const fn sibling(values: list<Round>) -> list<Polygon> => convert<list<Polygon>>(values)\n",
        "const fn checked(values: list<Round>) -> result<list<Shape>, error> => convert<list<Shape>>(values)\n",
        "const fn forced(values: list<Round>) -> list<Shape> => convert<list<Shape>>(values, invalid: .fail)\n",
        // By the operator's name for its parameter (EXPR-2), and with
        // its other generic arguments written: the value's type, and
        // the result, which is the list asked for.
        "const fn named(values: list<Round>) -> list<Shape> => convert<list<Shape>>(value: values)\n",
        "const fn written(values: list<Round>) -> list<Shape> => convert<list<Shape>, list<Round>, list<Shape>>(values)\n",
        "const fn output(values: list<Round>) -> str => convert<list<Shape>, list<Round>, str>(values)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:13:54 operator/operator.no_candidate GEN-5",
            "m.hgl:14:59 operator/operator.no_candidate GEN-5",
            "m.hgl:15:61 operator/operator.no_candidate GEN-5",
            "m.hgl:16:60 operator/operator.no_candidate GEN-5",
            "m.hgl:17:64 operator/operator.no_candidate GEN-5",
            "m.hgl:18:59 operator/operator.no_candidate GEN-5",
            "m.hgl:19:72 operator/operator.no_candidate GEN-5",
            "m.hgl:20:56 operator/operator.no_candidate GEN-5",
            "m.hgl:23:48 operator/operator.no_candidate GEN-5",
        ]
    );
    for name in ["widened", "deep", "parents", "generic", "named", "written"] {
        let value = returned(&checked, name);
        assert!(
            matches!(value.kind, ExprKind::ParentList(_)),
            "{name}: {value:?}"
        );
    }
    assert_eq!(returned(&checked, "widened").ty.to_string(), "list<Shape>");
    assert_eq!(
        returned(&checked, "generic").ty.to_string(),
        "list<Tagged<i64>>"
    );
}

/// Core's OPS-6: the value a list of structs is converted from may be a
/// block that declares locals, read first as itself and then where it
/// stands, as an argument of a call is. What the typed body keeps is
/// the second reading, with locals of its own: `item` is declared once,
/// both elements read it, and `other`, declared after, is another
/// local.
#[test]
fn a_converted_block_keeps_the_locals_it_declares() {
    let checked = module(concat!(
        "abstract struct Shape {}\n",
        "struct Round : Shape { radius: f64 }\n",
        "const fn widened() -> list<Shape> {\n",
        "    let shapes = convert<list<Shape>>({\n",
        "        let item = Round(radius: 1.0)\n",
        "        [item, item]\n",
        "    })\n",
        "    let other = Round(radius: 2.0)\n",
        "    return shapes\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    let body = body(&checked, "widened");
    let names: Vec<String> = body
        .locals
        .iter()
        .map(|local| format!("{}: {}", local.name, local.ty))
        .collect();
    assert_eq!(
        names,
        ["item: Round", "shapes: list<Shape>", "other: Round"]
    );
    let Stmt::Declare {
        value: Some(shapes),
        ..
    } = &body.statements[0]
    else {
        panic!("`shapes` is declared first: {:?}", body.statements[0]);
    };
    let ExprKind::ParentList(converted) = &shapes.kind else {
        panic!("a list converted: {shapes:?}");
    };
    let ExprKind::Block {
        statements,
        value: Some(last),
    } = &converted.kind
    else {
        panic!("a block: {converted:?}");
    };
    let [Stmt::Declare { local: item, .. }] = statements.as_slice() else {
        panic!("one local declared: {statements:?}");
    };
    assert_eq!(body.locals[*item].name, "item");
    let ExprKind::Sequence(elements) = &last.kind else {
        panic!("a sequence: {last:?}");
    };
    for element in elements {
        assert!(
            matches!(element.kind, ExprKind::Local(local) if local == *item),
            "{element:?}"
        );
    }
}

/// Core's OPS-7: "A conversion is configured as division is (NUM-3,
/// NUM-9). Options select how an `f64` is rounded to an `i64` and what
/// a NaN becomes ... Its error policy is checked wherever it can be:
/// the conversion then returns `result<T, error>` ... A policy under
/// which a failure escapes instead is an option." The accepted
/// fragments, with `price` an `f64`, are of the types the chapter gives.
/// Its deliberate rejection, `invalid: policy` with `policy` no
/// constant, leaves the result type unresolved (GEN-8). A conversion
/// that cannot fail takes no `invalid`, and only an `f64` to an `i64`
/// takes `rounding` and `nan`.
#[test]
fn a_conversions_options_and_the_result_its_policy_returns() {
    let checked = module(concat!(
        "const fn uses(price: f64, policy: InvalidPolicy, n: i64, text: str) {\n",
        "    let a = convert<i64>(price)\n",
        "    let b = convert<i64>(price, rounding: .half_even)\n",
        "    let c = convert<i64>(price, nan: 0)\n",
        "    let d = convert<i64>(price, invalid: .fail)\n",
        "    let e = convert<f64>(\"1.5\")\n",
        "    let f = convert<i64>(price, invalid: .error)\n",
        "    let bad1 = convert<i64>(price, invalid: policy)\n",
        "    let bad2 = convert<f64>(n, invalid: .fail)\n",
        "    let bad3 = convert<i64>(text, rounding: .floor)\n",
        "    let bad4 = convert<i64>(price, nan: 1.5)\n",
        "}\n",
        "const fn plain(price: f64) -> i64 => convert<i64>(price)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:45 type/generic.argument_constant GEN-8",
            "m.hgl:10:16 operator/operator.no_candidate GEN-5",
            "m.hgl:11:16 operator/operator.no_candidate GEN-5",
            "m.hgl:12:16 operator/operator.no_candidate GEN-5",
        ]
    );
    assert_eq!(
        locals(&checked, "uses")[4..10],
        [
            "a: result<i64, error>",
            "b: result<i64, error>",
            "c: result<i64, error>",
            "d: i64",
            "e: result<f64, error>",
            "f: result<i64, error>",
        ]
    );
}

/// FN-4, FN-17 and ERR-9: an unrelated error in a callback still leaves
/// its temporal calls visible to role selection.
#[test]
fn temporal_callback_body_errors_keep_the_selected_roles_diagnostics() {
    let value = "const fn apply(f: const fn(i64) -> i64, x: i64) -> i64 => f(x) * 2";
    let temporal = "fn apply(f: fn(i64) -> i64, x: i64) -> i64 => f(x) * 3";
    for (body, ordinary_error, depth) in [
        (
            "apply(fn(a) => later(a) + product(1), 1)",
            "type/argument.missing",
            1,
        ),
        (
            "apply(fn(a) => later(a) + apply(fn(b) => later(b) + apply(fn(c) => later(c) + product(1), 1), 1), 1)",
            "type/argument.missing",
            3,
        ),
        (
            "apply(fn(a) => later(a) + product(1, \"wrong\"), 1)",
            "type/type.mismatch",
            1,
        ),
    ] {
        for (definitions, role_errors) in [
            (format!("{value}\n{temporal}"), 0),
            (temporal.to_string(), 0),
            (value.to_string(), depth),
        ] {
            let checked = module(&format!(
                "{definitions}\n\
                 const fn product(a: i64, b: i64) -> i64 => a * b\n\
                 fn later(n: i64) -> i64 {{ when {{ return n + 10 }} }}\n\
                 fn graph(tick: i64) -> i64 => tick + {body}\n"
            ));
            let codes: Vec<_> = checked
                .findings
                .iter()
                .map(|finding| finding.code.as_str())
                .collect();
            let mut expected = vec!["function-kind/call.role"; role_errors];
            expected.push(ordinary_error);
            assert_eq!(codes, expected, "{}", checked.report);
        }
    }
}

/// FN-17: a declaration restricted to the value template rejects
/// a temporal function name or a temporal call inside its argument.
#[test]
fn value_only_callback_twins_still_reject_temporal_arguments() {
    let definitions = concat!(
        "const fn named(f: const fn(str) -> i64, x: i64) -> i64 => f(\"a\") * 2\n",
        "const fn generic<T>(f: const fn(T) -> T, x: T) -> T requires T in {i64} => f(x) * 2\n",
        "fn later(n: i64) -> i64 { when { return n + 10 } }\n",
    );
    for (body, expected) in [
        ("named(later, 1)", "type/type.mismatch"),
        ("generic(later, 1)", "type/type.mismatch"),
        ("generic(fn(n) => later(n), 1)", "function-kind/call.role"),
    ] {
        let checked = module(&format!(
            "{definitions}fn graph(tick: i64) -> i64 => tick + {body}\n"
        ));
        assert_eq!(checked.findings.len(), 1, "{}", checked.report);
        assert_eq!(checked.findings[0].code, expected, "{}", checked.report);
    }
}
