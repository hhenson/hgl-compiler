//! The analyser against the rules on a `ref` result that a source has
//! (FN-16, TIME-7, SOURCE-1). Each test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::module;

/// TIME-7: "In a hook, `return value` and `out = value` take a complete
/// value of the result's type T ... so does a source's `yield`
/// (SOURCE-1) ... A `ref` result takes no value: a `return` to it names
/// an endpoint". FN-16: "A value that names no endpoint rejects there
/// (`type.mismatch`)". SOURCE-1: a source is "a node that produces its
/// output and has no temporal parameters", so a `yield` to its `ref`
/// result names no endpoint, a temporal input of the node, and rejects.
#[test]
fn a_yield_to_a_ref_result_rejects() {
    let checked = module(concat!(
        "fn counted() -> ref i64 {\n",
        "    yield 1s: 1\n",
        "}\n",
        "fn looped() -> ref i64 {\n",
        "    for at in [1, 2] {\n",
        "        yield 1s: at\n",
        "    }\n",
        "}\n",
        "fn plain() -> i64 {\n",
        "    yield 1s: 1\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:15 type/type.mismatch FN-16",
            "m.hgl:7:19 type/type.mismatch FN-16",
        ]
    );
}
