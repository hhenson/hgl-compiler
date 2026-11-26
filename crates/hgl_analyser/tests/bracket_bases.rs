//! EXPR-7 bracket reads with known base types.

mod common;

use common::module;

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
