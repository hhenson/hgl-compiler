//! EXPR-7 bracket reads with known base types.

mod common;

use common::{check, module};

/// EXPR-7 admits an element of a list or tuple and a dictionary value.
/// The base of each other read below is known where the body is checked.
#[test]
fn known_noncollections_reject_bracket_reads() {
    let checked = module(concat!(
        "struct Point { x: i64 }\n",
        "const fn on_bool(value: bool) -> i64 => value[0]\n",
        "const fn on_int(value: i64) -> i64 => value[0]\n",
        "const fn on_float(value: f64) -> i64 => value[0]\n",
        "const fn on_text(value: str) -> i64 => value[0]\n",
        "const fn on_struct(value: Point) -> i64 => value[0]\n",
        "const fn on_set(value: set<i64>) -> i64 => value[0]\n",
        "fn on_input(value: bool) -> i64 { when { return value[0] } }\n",
        "const fn on_list(value: list<i64>) -> i64 => value[0]\n",
        "const fn on_tuple(value: tuple<i64, str>) -> i64 => value[0]\n",
        "const fn on_dict(value: dict<str, i64>) -> i64 => value[\"key\"]\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:41 type/type.mismatch EXPR-7",
            "m.hgl:4:39 type/type.mismatch EXPR-7",
            "m.hgl:5:41 type/type.mismatch EXPR-7",
            "m.hgl:6:40 type/type.mismatch EXPR-7",
            "m.hgl:7:44 type/type.mismatch EXPR-7",
            "m.hgl:8:44 type/type.mismatch EXPR-7",
            "m.hgl:9:49 type/type.mismatch EXPR-7",
        ]
    );
}

/// EXPR-7 and GEN-7: concrete generic instantiations reject invalid bracket
/// bases at the making call (direct and forwarded), while valid list, tuple
/// and dictionary instantiations pass.
#[test]
fn generic_bracket_bases_check_direct_and_forwarded_instances() {
    let rejected = module(concat!(
        "struct Point { x: i64 }\n",
        "const fn direct<T>(value: T) -> i64 => value[0]\n",
        "const fn forwarded<U>(value: U) -> i64 => direct(value)\n",
        "fn node_direct<T>(value: T) -> i64 { when { return value[0] } }\n",
        "fn graph_forwarded<U>(value: U) -> i64 => node_direct(value)\n",
        "test calls {\n",
        "    direct(true)\n",
        "    direct(1)\n",
        "    direct(1.5)\n",
        "    direct(\"text\")\n",
        "    direct(Point(x: 1))\n",
        "    let items: set<i64> = [1]\n",
        "    direct(items)\n",
        "    forwarded(true)\n",
        "    forwarded(items)\n",
        "    assert eval node_direct(value: [true]) == [1]\n",
        "    assert eval graph_forwarded(value: [true]) == [1]\n",
        "}\n",
    ));
    assert_eq!(
        rejected.summary(),
        [
            "m.hgl:8:5 type/type.mismatch EXPR-7",
            "m.hgl:9:5 type/type.mismatch EXPR-7",
            "m.hgl:10:5 type/type.mismatch EXPR-7",
            "m.hgl:11:5 type/type.mismatch EXPR-7",
            "m.hgl:12:5 type/type.mismatch EXPR-7",
            "m.hgl:14:5 type/type.mismatch EXPR-7",
            "m.hgl:15:5 type/type.mismatch EXPR-7",
            "m.hgl:16:5 type/type.mismatch EXPR-7",
            "m.hgl:17:12 type/type.mismatch EXPR-7",
            "m.hgl:18:12 type/type.mismatch EXPR-7",
        ],
        "{}",
        rejected.report
    );
    for expected in [
        "this call makes the instance `direct<bool>`",
        "this call makes the instance `forwarded<bool>`",
        "this call makes the instance `node_direct<bool>`",
        "this call makes the instance `graph_forwarded<bool>`",
    ] {
        assert!(
            rejected.report.contains(expected),
            "missing `{expected}` in:\n{}",
            rejected.report
        );
    }

    let valid = module(concat!(
        "const fn at_zero<T>(value: T) -> i64 => value[0]\n",
        "const fn forwarded_zero<U>(value: U) -> i64 => at_zero(value)\n",
        "const fn at_key<T>(value: T) -> i64 => value[\"k\"]\n",
        "const fn forwarded_key<U>(value: U) -> i64 => at_key(value)\n",
        "test calls {\n",
        "    assert at_zero([7]) == 7\n",
        "    assert forwarded_zero([8]) == 8\n",
        "    assert at_zero((9, \"x\")) == 9\n",
        "    assert forwarded_zero((10, \"y\")) == 10\n",
        "    let book: dict<str, i64> = [\"k\": 11]\n",
        "    assert at_key(book) == 11\n",
        "    assert forwarded_key(book) == 11\n",
        "}\n",
    ));
    assert_eq!(valid.summary(), Vec::<String>::new(), "{}", valid.report);
}

/// EXPR-7, GEN-7, TYPE-9, ERR-12, ERR-15: wrapped generic bracket bases
/// (`T?`, `result<T?, error>`, `atomic<T>` in graphs, nested `list<T?>`,
/// nullable struct fields, nullable `var` places, and function references)
/// are rechecked in each concrete instance.
#[test]
fn wrapped_generic_bracket_bases_check_concrete_instances() {
    let rejected = module(concat!(
        "struct Point { x: i64 }\n",
        "struct Box<T> { item: T? }\n",
        "const fn opt_idx<T>(value: T?) -> i64 => value[0]\n",
        "const fn opt_forwarded<U>(value: U?) -> i64 => opt_idx(value)\n",
        "const fn res_opt_idx<T>(value: result<T?, error>) -> i64 => value[0]\n",
        "const fn nested_opt_idx<T>(value: list<T?>) -> i64 => value[0][0]\n",
        "const fn field_opt_idx<T>(box: Box<T>) -> i64 => box.item[0]\n",
        "const fn place_opt_idx<T>(value: T?) -> T? {\n",
        "    var kept = value\n",
        "    kept[0] = 1\n",
        "    return kept\n",
        "}\n",
        "const fn apply_opt(f: const fn(bool?) -> i64, x: bool?) -> i64 => f(x)\n",
        "fn node_opt<T>(value: T?) -> i64 { when { return value[0] } }\n",
        "fn graph_opt<T>(value: T?) -> i64 => value[0]\n",
        "fn graph_atomic<T>(value: atomic<T>) -> i64 => value[0]\n",
        "test calls {\n",
        "    let flag: bool? = true\n",
        "    opt_idx(flag)\n",
        "    opt_forwarded(flag)\n",
        "    let wrapped: result<bool?, error> = ok(true)\n",
        "    res_opt_idx(wrapped)\n",
        "    let flags: list<bool?> = [true]\n",
        "    nested_opt_idx(flags)\n",
        "    field_opt_idx(Box<bool>(item: true))\n",
        "    place_opt_idx(flag)\n",
        "    apply_opt(opt_idx<bool>, flag)\n",
        "    assert eval node_opt(value: [true]) == [1]\n",
        "    assert eval graph_opt(value: [true]) == [1]\n",
        "    assert eval graph_atomic<Point>(value: [Point(x: 1)]) == [1]\n",
        "}\n",
    ));
    assert_eq!(
        rejected.summary(),
        [
            "m.hgl:20:5 type/type.mismatch EXPR-7",
            "m.hgl:21:5 type/type.mismatch EXPR-7",
            "m.hgl:23:5 type/type.mismatch EXPR-7",
            "m.hgl:25:5 type/type.mismatch EXPR-7",
            "m.hgl:26:5 type/type.mismatch EXPR-7",
            "m.hgl:27:5 type/type.mismatch EXPR-7",
            "m.hgl:28:15 type/type.mismatch EXPR-7",
            "m.hgl:29:12 type/type.mismatch EXPR-7",
            "m.hgl:30:12 type/type.mismatch EXPR-7",
            "m.hgl:31:12 type/type.mismatch EXPR-7",
        ],
        "{}",
        rejected.report
    );

    let valid = module(concat!(
        "const fn opt_zero<T>(value: T?) -> i64 => value[0]\n",
        "const fn opt_forwarded<U>(value: U?) -> i64 => opt_zero(value)\n",
        "const fn opt_key<T>(value: T?) -> i64 => value[\"k\"]\n",
        "const fn res_opt_zero<T>(value: result<T?, error>) -> i64 => value[0]\n",
        "fn node_opt<T>(value: T?) -> i64 { when { return value[0] } }\n",
        "test calls {\n",
        "    let xs: list<i64>? = [7]\n",
        "    assert opt_zero(xs) == 7\n",
        "    assert opt_forwarded(xs) == 7\n",
        "    let pair: tuple<i64, str>? = (8, \"ok\")\n",
        "    assert opt_zero(pair) == 8\n",
        "    let book: dict<str, i64>? = [\"k\": 9]\n",
        "    assert opt_key(book) == 9\n",
        "    let wrapped: result<list<i64>?, error> = ok([10])\n",
        "    assert res_opt_zero(wrapped) == 10\n",
        "    assert eval node_opt(value: [[11]]) == [11]\n",
        "}\n",
    ));
    assert_eq!(valid.summary(), Vec::<String>::new(), "{}", valid.report);
}

/// EXPR-7, GEN-7, FN-7: generic bracket bases inside direct anonymous calls
/// and standalone anonymous functions are rechecked in concrete instances,
/// while nongeneric, uninstantiated, unrelated/concrete, and no-index controls
/// behave identically without spurious or duplicate diagnostics.
#[test]
fn anonymous_generic_bracket_bases_check_concrete_instances() {
    let rejected = module(concat!(
        "const fn nongeneric_close(src: bool?) -> i64 => (fn(i: i64) => src[i])(0)\n",
        "const fn nongeneric_unused(src: bool?) -> i64 {\n",
        "    let f = fn(i: i64) => src[i]\n",
        "    return 0\n",
        "}\n",
        "const fn close_over<T>(src: T?) -> i64 => (fn(i: i64) => src[i])(0)\n",
        "const fn passed_arg<T>(src: T?) -> i64 => (fn(p) => p[0])(src)\n",
        "const fn forwarded_close<U>(src: U?) -> i64 => close_over(src)\n",
        "const fn nested_close<T>(src: T?) -> i64 => (fn(i: i64) => (fn(j: i64) => src[i + j])(0))(0)\n",
        "const fn unused_closure<T>(src: T?) -> i64 {\n",
        "    let f = fn(i: i64) => src[i]\n",
        "    return 0\n",
        "}\n",
        "const fn typed_param<T>(x: T) -> i64 {\n",
        "    let f = fn(p: T?) => p[0]\n",
        "    return 0\n",
        "}\n",
        "const fn place_close<T>(src: T?) -> T? => (fn(i: i64) -> T? {\n",
        "    var kept = src\n",
        "    kept[i] = 1\n",
        "    return kept\n",
        "})(0)\n",
        "fn node_close<T>(src: T?) -> i64 { when { return (fn(i: i64) => src[i])(0) } }\n",
        "fn graph_close<T>(src: T?) -> i64 => (fn(i: i64) => src[i])(0)\n",
        "fn graph_passed<T>(src: T?) -> i64 => (fn(p) => p[0])(src)\n",
        "fn graph_nested<T>(src: T?) -> i64 => (fn(i: i64) => (fn(j: i64) => src[i + j])(0))(0)\n",
        "fn graph_forwarded_close<U>(src: U?) -> i64 => graph_close(src)\n",
        "const fn static_bad<T>(x: T) -> i64 => (fn(b: bool) => b[0])(true)\n",
        "const fn never_called<T>(src: T?) -> i64 => (fn(i: i64) => src[i])(0)\n",
        "test probe {\n",
        "    let flag: bool? = true\n",
        "    close_over(flag)\n",
        "    passed_arg(flag)\n",
        "    forwarded_close(flag)\n",
        "    nested_close(flag)\n",
        "    unused_closure(flag)\n",
        "    typed_param(true)\n",
        "    place_close(flag)\n",
        "    assert eval node_close(src: [true]) == [1]\n",
        "    assert eval graph_close(src: [true]) == [1]\n",
        "    assert eval graph_passed(src: [true]) == [1]\n",
        "    assert eval graph_nested(src: [true]) == [1]\n",
        "    assert eval graph_forwarded_close(src: [true]) == [1]\n",
        "    static_bad(1)\n",
        "    static_bad(\"two\")\n",
        "}\n",
    ));
    assert_eq!(
        rejected.summary(),
        [
            "m.hgl:2:64 type/type.mismatch EXPR-7",
            "m.hgl:4:27 type/type.mismatch EXPR-7",
            "m.hgl:29:56 type/type.mismatch EXPR-7",
            "m.hgl:33:5 type/type.mismatch EXPR-7",
            "m.hgl:34:5 type/type.mismatch EXPR-7",
            "m.hgl:35:5 type/type.mismatch EXPR-7",
            "m.hgl:36:5 type/type.mismatch EXPR-7",
            "m.hgl:37:5 type/type.mismatch EXPR-7",
            "m.hgl:38:5 type/type.mismatch EXPR-7",
            "m.hgl:39:5 type/type.mismatch EXPR-7",
            "m.hgl:40:12 type/type.mismatch EXPR-7",
            "m.hgl:41:12 type/type.mismatch EXPR-7",
            "m.hgl:42:12 type/type.mismatch EXPR-7",
            "m.hgl:43:12 type/type.mismatch EXPR-7",
            "m.hgl:44:12 type/type.mismatch EXPR-7",
        ],
        "{}",
        rejected.report
    );
    for expected in [
        "this call makes the instance `close_over<bool>`",
        "this call makes the instance `passed_arg<bool>`",
        "this call makes the instance `forwarded_close<bool>`",
        "this call makes the instance `nested_close<bool>`",
        "this call makes the instance `unused_closure<bool>`",
        "this call makes the instance `typed_param<bool>`",
        "this call makes the instance `place_close<bool>`",
        "this call makes the instance `node_close<bool>`",
        "this call makes the instance `graph_close<bool>`",
        "this call makes the instance `graph_passed<bool>`",
        "this call makes the instance `graph_nested<bool>`",
        "this call makes the instance `graph_forwarded_close<bool>`",
        "src[i]",
    ] {
        assert!(
            rejected.report.contains(expected),
            "missing `{expected}` in:\n{}",
            rejected.report
        );
    }

    let valid = module(concat!(
        "const fn close_over<T>(src: T?) -> i64 => (fn(i: i64) => src[i])(0)\n",
        "const fn passed_arg<T>(src: T?) -> i64 => (fn(p) => p[0])(src)\n",
        "const fn forwarded_close<U>(src: U?) -> i64 => close_over(src)\n",
        "const fn nested_close<T>(src: T?) -> i64 => (fn(i: i64) => (fn(j: i64) => src[i + j])(0))(0)\n",
        "const fn unused_closure<T>(src: T?) -> i64 {\n",
        "    let f = fn(i: i64) => src[i]\n",
        "    return 0\n",
        "}\n",
        "const fn unused_concrete<T>(x: T, xs: list<i64>) -> i64 {\n",
        "    let f = fn(i: i64) => xs[i]\n",
        "    return 0\n",
        "}\n",
        "const fn unused_untyped<T>(x: T) -> i64 {\n",
        "    let f = fn(p) => p[0]\n",
        "    return 0\n",
        "}\n",
        "const fn unrelated_concrete<T>(x: T, xs: list<i64>) -> i64 => (fn(i: i64) => xs[i])(0)\n",
        "const fn apply_one(f: const fn(i64) -> i64, x: i64) -> i64 => f(x)\n",
        "const fn retained_concrete<T>(x: T, xs: list<i64>) -> i64 => apply_one(fn(i: i64) => xs[i], 0)\n",
        "const fn no_index<T>(src: T?) -> i64 => (fn(i: i64) => i)(0)\n",
        "const fn close_tuple<T>(src: T?) -> i64 => (fn(_unused: i64) => src[0])(0)\n",
        "const fn close_key<T>(src: T?) -> i64 => (fn(k: str) => src[k])(\"k\")\n",
        "fn node_close<T>(src: T?) -> i64 { when { return (fn(i: i64) => src[i])(0) } }\n",
        "fn graph_close<T>(src: T?) -> i64 => (fn(i: i64) => src[i])(0)\n",
        "fn graph_passed<T>(src: T?) -> i64 => (fn(p) => p[0])(src)\n",
        "fn graph_nested<T>(src: T?) -> i64 => (fn(i: i64) => (fn(j: i64) => src[i + j])(0))(0)\n",
        "fn graph_forwarded_close<U>(src: U?) -> i64 => graph_close(src)\n",
        "const fn uninstantiated<T>(src: T?) -> i64 => (fn(i: i64) => src[i])(0)\n",
        "test probe {\n",
        "    let xs: list<i64>? = [7]\n",
        "    close_over(xs)\n",
        "    passed_arg(xs)\n",
        "    forwarded_close(xs)\n",
        "    nested_close(xs)\n",
        "    unused_closure(xs)\n",
        "    let flag: bool? = true\n",
        "    unused_concrete(flag, [1])\n",
        "    unused_untyped(flag)\n",
        "    unrelated_concrete(flag, [1])\n",
        "    retained_concrete(flag, [1])\n",
        "    no_index(flag)\n",
        "    let pair: tuple<i64, str>? = (8, \"ok\")\n",
        "    close_tuple(pair)\n",
        "    let book: dict<str, i64>? = [\"k\": 9]\n",
        "    close_key(book)\n",
        "    assert eval node_close(src: [[11]]) == [11]\n",
        "    assert eval graph_close(src: [[11]]) == [11]\n",
        "    assert eval graph_passed(src: [[11]]) == [11]\n",
        "    assert eval graph_nested(src: [[11]]) == [11]\n",
        "    assert eval graph_forwarded_close(src: [[11]]) == [11]\n",
        "}\n",
    ));
    assert_eq!(valid.summary(), Vec::<String>::new(), "{}", valid.report);
}

/// UNIT-1, EXPR-7, GEN-7: multi-file module parts recheck concrete generic
/// bracket bases (including direct anonymous calls and live connection
/// markers) across files without cross-file span collisions.
#[test]
fn multi_file_part_generic_bracket_bases_check_concrete_instances() {
    let rejected = check(&[
        (
            "m.hgl",
            concat!(
                "module m\n",
                "const fn concrete_anon<T>(x: T, xs: list<i64>) -> i64 => (fn(i: i64) => xs[i])(0)\n",
                "fn main_graph_passed<T>(src: T?) -> i64 => (fn(p) => p[0])(src)\n",
                "test from_main {\n",
                "    let flag: bool? = true\n",
                "    concrete_anon(flag, [1])\n",
                "    part_close(flag)\n",
                "    part_typed_param(true)\n",
                "    assert eval part_graph_close(src: [true]) == [1]\n",
                "}\n",
            ),
        ),
        (
            "extra.hgl",
            concat!(
                "module m part extra\n",
                "const fn part_close<T>(src: T?) -> i64 => (fn(i: i64) => src[i])(0)\n",
                "const fn part_typed_param<T>(x: T) -> i64 {\n",
                "    let f = fn(p: T?) => p[0]\n",
                "    return 0\n",
                "}\n",
                "fn part_graph_close<T>(src: T?) -> i64 => (fn(i: i64) => src[i])(0)\n",
                "test from_part {\n",
                "    assert eval main_graph_passed(src: [true]) == [1]\n",
                "}\n",
            ),
        ),
    ]);
    assert_eq!(
        rejected.summary(),
        [
            "extra.hgl:9:12 type/type.mismatch EXPR-7",
            "m.hgl:7:5 type/type.mismatch EXPR-7",
            "m.hgl:8:5 type/type.mismatch EXPR-7",
            "m.hgl:9:12 type/type.mismatch EXPR-7",
        ],
        "{}",
        rejected.report
    );

    let valid = check(&[
        (
            "m.hgl",
            concat!(
                "module m\n",
                "const fn concrete_anon<T>(x: T, xs: list<i64>) -> i64 => (fn(i: i64) => xs[i])(0)\n",
                "fn main_graph_passed<T>(src: T?) -> i64 => (fn(p) => p[0])(src)\n",
                "test from_main {\n",
                "    let xs: list<i64>? = [7]\n",
                "    concrete_anon(true, [1])\n",
                "    part_close(xs)\n",
                "    assert eval part_graph_close(src: [[11]]) == [11]\n",
                "}\n",
            ),
        ),
        (
            "extra.hgl",
            concat!(
                "module m part extra\n",
                "const fn part_close<T>(src: T?) -> i64 => (fn(i: i64) => src[i])(0)\n",
                "fn part_graph_close<T>(src: T?) -> i64 => (fn(i: i64) => src[i])(0)\n",
                "test from_part {\n",
                "    assert eval main_graph_passed(src: [[13]]) == [13]\n",
                "}\n",
            ),
        ),
    ]);
    assert_eq!(valid.summary(), Vec::<String>::new(), "{}", valid.report);
}
