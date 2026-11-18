//! The analyser against FN-15, the `var` parameter. Each test cites
//! the rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, check};
use hgl_analyser::hir::Body;
use hgl_analyser::{Detail, LocalKind};

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

/// The functions the tests call.
const DECLARED: &str = concat!(
    "const fn bump(var count: i64, by: i64) {\n",
    "    count += by\n",
    "}\n",
    "const fn set_first(var items: list<i64>, value: i64) {\n",
    "    items[0] = value\n",
    "}\n",
    "const fn clear(var value: i64?) { value = null }\n",
);

/// FN-15: "A value function, a `const fn` or a native one, may mark a
/// parameter `var` ... A call gives a `var` parameter a place, as the
/// left of an assignment is (EXPR-3): a `var`, a `state`, or a `cache`,
/// or a place inside one ... In the function the parameter is a `var`
/// ... A `var` parameter may be given on to a `var` parameter of a
/// function the body calls ... a call of it stands in a value function,
/// in a runtime hook, or in composition where the place holds an
/// ordinary value."
#[test]
fn a_var_parameter_takes_a_place() {
    let checked = module(&format!(
        "{DECLARED}{}",
        concat!(
            "struct Book {\n",
            "    orders: list<i64>\n",
            "    name: str\n",
            "}\n",
            "native const fn grow<V>(var values: list<V>, item: V)\n",
            "const fn through(var count: i64) { bump(count, 1) }\n",
            "const fn total(values: list<i64>) -> i64 {\n",
            "    var sum: i64 = 0\n",
            "    for value in values {\n",
            "        bump(sum, value)\n",
            "    }\n",
            "    return sum\n",
            "}\n",
            // A field and an element of a `var` are places.
            "const fn nested() -> i64 {\n",
            "    var book: Book = Book(orders: [1, 2], name: \"a\")\n",
            "    set_first(book.orders, 5)\n",
            "    bump(book.orders[1], 3)\n",
            "    grow(book.orders, 4)\n",
            "    return book.orders[0]\n",
            "}\n",
            // A `state` and a `cache` of a node are.
            "fn counted(value: i64) -> i64 {\n",
            "    state seen: i64 = 0\n",
            "    cache held: i64 = 0\n",
            "    when {\n",
            "        bump(seen, 1)\n",
            "        bump(held, value)\n",
            "        return seen\n",
            "    }\n",
            "}\n",
            // A `var` of a test that holds an ordinary value is.
            "test in_composition {\n",
            "    var count: i64 = 1\n",
            "    bump(count, 4)\n",
            "    assert count == 5\n",
            "}\n",
        )
    ));
    assert_eq!(checked.summary(), NONE);
    // The parameter is a `var` of the body, and the other is not.
    let locals: Vec<(String, LocalKind)> = body(&checked, "bump")
        .locals
        .iter()
        .map(|local| (local.name.clone(), local.kind))
        .collect();
    assert_eq!(
        locals,
        [
            ("count".to_string(), LocalKind::Var),
            ("by".to_string(), LocalKind::Parameter)
        ]
    );
    let module = checked.program.module("m").expect("module m exists");
    let bump = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == "bump")
        .expect("it is declared");
    let Detail::Callable(signature) = &module.details[bump] else {
        panic!("`bump` is a function");
    };
    assert_eq!(
        signature
            .parameters
            .iter()
            .map(|parameter| parameter.var)
            .collect::<Vec<_>>(),
        [true, false]
    );
}

/// FN-15: "What is read-only rejects, a `let`, a parameter that is not
/// `var`, a constant, or an iteration binding (`binding.immutable`); so
/// does what is no place, a literal or what a call gives
/// (`argument.place`) ... It is always given: it has no default, and is
/// not left out where its type is written `T?` (UNIT-8)."
#[test]
fn what_is_no_place_is_not_given_to_a_var_parameter() {
    let checked = module(&format!(
        "{DECLARED}{}",
        concat!(
            "const LIMIT: i64 = 3\n",
            "const fn made() -> i64 => 1\n",
            // Line 11.
            "const fn literal() -> i64 {\n",
            "    bump(1, 2)\n",
            "    return 1\n",
            "}\n",
            "const fn called() -> i64 {\n",
            "    bump(made(), 2)\n",
            "    return 1\n",
            "}\n",
            "const fn summed() -> i64 {\n",
            "    var n: i64 = 1\n",
            "    bump(n + 1, 2)\n",
            "    return n\n",
            "}\n",
            "const fn fixed() -> i64 {\n",
            "    let held = 0\n",
            "    bump(held, 2)\n",
            "    return held\n",
            "}\n",
            "const fn parameter(count: i64) -> i64 {\n",
            "    bump(count, 2)\n",
            "    return count\n",
            "}\n",
            "const fn constant() -> i64 {\n",
            "    bump(LIMIT, 2)\n",
            "    return 1\n",
            "}\n",
            "const fn iterated(values: list<i64>) -> i64 {\n",
            "    for value in values { bump(value, 1) }\n",
            "    return 1\n",
            "}\n",
            // A field of a `let` is read-only all the way down.
            "const fn inside(values: list<i64>) -> i64 {\n",
            "    let held = values\n",
            "    bump(held[0], 1)\n",
            "    return 1\n",
            "}\n",
            // UNIT-8: a `var` parameter is given though it is nullable.
            "const fn omitted() -> i64 {\n",
            "    clear()\n",
            "    return 1\n",
            "}\n",
        )
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:12:10 type/argument.place FN-15",
            "m.hgl:16:10 type/argument.place FN-15",
            "m.hgl:21:10 type/argument.place FN-15",
            "m.hgl:26:10 name/binding.immutable FN-15",
            "m.hgl:30:10 name/binding.immutable FN-15",
            "m.hgl:34:10 name/binding.immutable FN-15",
            "m.hgl:38:32 name/binding.immutable FN-15",
            "m.hgl:43:10 name/binding.immutable FN-15",
            "m.hgl:47:5 type/argument.missing EXPR-2",
        ]
    );
}

/// FN-15: "Its type is an ordinary value type, and no capability's
/// (TIME-8) or `endpoint<T>` (TIME-9), which no place holds
/// (`parameter.var`) ... A temporal `fn`, a temporal candidate, and an
/// operator declare none (`parameter.var`)."
#[test]
fn only_a_value_function_or_candidate_declares_a_var_parameter() {
    let checked = module(concat!(
        "fn ticking(var value: i64) -> i64 {\n",
        "    when { return value }\n",
        "}\n",
        "fn wired(var value: i64) -> i64 => value\n",
        "operator joined(var lhs: i64, rhs: i64) -> i64\n",
        "operator plain(lhs: i64, rhs: i64) -> i64\n",
        // Line 8: a value candidate may: the operator marks none, and
        // the candidate says which of its own is `var`.
        "impl const fn plain(var lhs: i64, rhs: i64) -> i64 => lhs\n",
        "native fn outside(var value: i64) -> i64\n",
        "const fn logged(var logger: Logger) -> i64 => 1\n",
        // A value function, and a native one, may.
        "const fn fine(var value: i64) { value = 1 }\n",
        "native const fn grown(var values: list<i64>, item: i64)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:12 function-kind/parameter.var FN-15",
            "m.hgl:5:10 function-kind/parameter.var FN-15",
            "m.hgl:6:17 function-kind/parameter.var FN-15",
            "m.hgl:9:19 function-kind/parameter.var FN-15",
            "m.hgl:10:17 function-kind/parameter.var FN-15",
        ]
    );
}

/// FN-15: "A function that declares one is not lifted (FN-6) ... No
/// argument of such a call is a connection (`connection.as_constant`),
/// and the function is no target of an `eval` (`eval.target`)."
#[test]
fn a_function_with_a_var_parameter_is_not_lifted() {
    let checked = module(&format!(
        "{DECLARED}{}",
        concat!(
            // Line 9: `value` is a connection in a graph.
            "fn wired(value: i64) -> i64 {\n",
            "    var sum: i64 = 0\n",
            "    bump(sum, value)\n",
            "    return value\n",
            "}\n",
            "test evaluated {\n",
            "    eval(clear(value: [1]))\n",
            "}\n",
        )
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:11:15 phase/connection.as_constant FN-15",
            "m.hgl:15:10 type/eval.target FN-15",
        ]
    );
}

/// FN-15: a place "holds an ordinary value of the parameter's type".
/// The call gives the place what the parameter holds when the function
/// returns, so a place of a narrower type takes none: an `i64` place
/// is given to no `i64?` parameter, though an `i64` is a value of an
/// `i64?`, nor a struct's place to a parameter of its abstract parent.
#[test]
fn a_place_is_of_the_var_parameters_type() {
    let checked = module(&format!(
        "{DECLARED}{}",
        concat!(
            "abstract struct Shape {\n",
            "    name: str\n",
            "}\n",
            "struct Circle: Shape {\n",
            "    radius: f64\n",
            "}\n",
            "const fn reshape(var shape: Shape, other: Shape) {{ shape = other }}\n",
            "native const fn grow<V>(var values: list<V>, item: V)\n",
            // Line 17.
            "const fn narrower() -> i64 {{\n",
            "    var n: i64 = 1\n",
            "    clear(n)\n",
            "    return n\n",
            "}}\n",
            "const fn inherited(other: Shape) -> f64 {{\n",
            "    var circle: Circle = Circle(name: \"c\", radius: 1.0)\n",
            "    reshape(circle, other)\n",
            "    return circle.radius\n",
            "}}\n",
            "const fn element() -> i64 {{\n",
            "    var items: list<i64> = [1]\n",
            "    clear(items[0])\n",
            "    return items[0]\n",
            "}}\n",
            // Each of these is of the parameter's type.
            "const fn same(other: Shape) -> i64 {{\n",
            "    var n: i64? = 1\n",
            "    clear(n)\n",
            "    var shape: Shape = other\n",
            "    reshape(shape, other)\n",
            "    var names: list<str> = []\n",
            "    grow(names, \"a\")\n",
            "    var maybe: list<i64?> = []\n",
            "    grow(maybe, 1)\n",
            "    return 1\n",
            "}}\n",
        )
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:19:11 type/type.mismatch FN-15",
            "m.hgl:24:13 type/type.mismatch FN-15",
            "m.hgl:29:11 type/type.mismatch FN-15",
        ]
    );
}

/// FN-15: "A call assigns the place". So a call is held to what an
/// assignment of the place is. NATIVE-6, TYPE-16: a property of a
/// native value or of a delta is read and not assigned, whatever holds
/// the value. CTRL-14: in a graph a `var` that holds an ordinary value
/// is not assigned in a branch selected at runtime. CTRL-15: one
/// declared outside a loop that makes a child for each member is not
/// carried into it.
#[test]
fn a_call_that_assigns_a_place_is_held_to_what_an_assignment_is() {
    let checked = module(concat!(
        "native type Handle {\n",
        "    size: i64\n",
        "}\n",
        "native const fn made() -> Handle\n",
        "const fn bump(var count: i64, by: i64) { count += by }\n",
        "const fn property() -> i64 {\n",
        "    var handle: Handle = made()\n",
        "    bump(handle.size, 1)\n",
        "    return 1\n",
        "}\n",
        "const fn keep(var members: set<i64>) { let one = 1 }\n",
        // Inside a list, and a delta's own property.
        "const fn changed(change: delta<set<i64>>) -> i64 {\n",
        "    var held: delta<set<i64>> = change\n",
        "    var handles: list<Handle> = [made()]\n",
        "    bump(handles[0].size, 1)\n",
        "    keep(held.added)\n",
        "    return 1\n",
        "}\n",
        "fn passes(value: i64) -> i64 {\n",
        "    when { return value }\n",
        "}\n",
        // Line 23: assigned where the graph is built, which is fine.
        "fn built(value: i64) -> i64 {\n",
        "    var n: i64 = 0\n",
        "    bump(n, 1)\n",
        "    return passes(value)\n",
        "}\n",
        "fn branched(rising: bool, value: i64) -> i64 {\n",
        "    var n: i64 = 0\n",
        "    var rows: list<i64> = [0]\n",
        "    if rising {\n",
        "        bump(n, 1)\n",
        "        bump(rows[0], 1)\n",
        "    }\n",
        "    return passes(value)\n",
        "}\n",
        "fn looped(values: set<i64>, value: i64) -> i64 {\n",
        "    var n: i64 = 0\n",
        "    for member in values { bump(n, 1) }\n",
        "    return passes(value)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:10 name/binding.immutable NATIVE-6",
            "m.hgl:16:10 name/binding.immutable NATIVE-6",
            "m.hgl:17:10 name/binding.immutable TYPE-16",
            "m.hgl:32:14 type/binding.category CTRL-14",
            "m.hgl:33:14 type/binding.category CTRL-14",
            "m.hgl:39:33 phase/loop.carried CTRL-15",
        ]
    );
}

/// Core's LIST-1 and LIST-2: `append` and `extend` are core's native
/// functions, bound in every module (UNIT-11), and each takes its list
/// as a `var` parameter (LIST-3, FN-15): the list is a place, of the
/// list type the item is of.
#[test]
fn cores_append_and_extend_take_a_place() {
    let checked = module(concat!(
        "const fn grown() -> list<i64> {\n",
        "    var entries: list<i64> = [1]\n",
        "    append(entries, 2)\n",
        "    extend(entries, [3, 4])\n",
        "    return entries\n",
        "}\n",
        "fn gathered(value: i64) -> i64 {\n",
        "    cache seen: list<i64> = []\n",
        "    when {\n",
        "        append(seen, value)\n",
        "        return seen[0]\n",
        "    }\n",
        "}\n",
        // Line 15: no place, a read-only one, and an item of another
        // type.
        "const fn literal() -> i64 {\n",
        "    append([1], 2)\n",
        "    return 1\n",
        "}\n",
        "const fn fixed() -> i64 {\n",
        "    let entries = [1]\n",
        "    extend(entries, [2])\n",
        "    return 1\n",
        "}\n",
        "const fn mixed() -> i64 {\n",
        "    var entries: list<i64> = [1]\n",
        "    append(entries, \"two\")\n",
        "    return 1\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:16:12 type/argument.place FN-15",
            "m.hgl:21:12 name/binding.immutable FN-15",
            "m.hgl:26:21 type/type.mismatch EXPR-5",
        ]
    );
}

/// FN-15: "So may a value candidate of an operator (GEN-6) ... A
/// temporal `fn`, a temporal candidate, and an operator declare none
/// (`parameter.var`): an operator gives the parameters every candidate
/// takes, and each candidate says which of its own is `var`. The mark
/// takes no part in selecting a candidate (GEN-5): a call of an
/// operator is resolved as any other, and the candidate it resolves to
/// says which of its arguments are places." GEN-16: "A candidate that
/// marks a parameter `var` (FN-15) answers no requirement".
#[test]
fn a_value_candidate_says_which_of_its_arguments_are_places() {
    let checked = module(concat!(
        "operator push<C, V>(values: C, item: V)\n",
        "impl const fn push(var values: list<i64>, item: i64) { append(values, item) }\n",
        "impl const fn push(var values: dict<str, i64>, item: str) { values[item] = 1 }\n",
        "const fn used() -> i64 {\n",
        "    var entries: list<i64> = [1]\n",
        "    push(entries, 2)\n",
        "    var seen: dict<str, i64> = []\n",
        "    push(seen, \"a\")\n",
        "    return entries[1]\n",
        "}\n",
        // Line 12: the candidate is selected by the types of the
        // arguments, and then says its first is a place.
        "const fn refused() -> i64 {\n",
        "    let fixed = [1]\n",
        "    push([1], 2)\n",
        "    push(fixed, 2)\n",
        "    return 1\n",
        "}\n",
        // Line 18: an operator, and a temporal candidate, declare none.
        "operator poke<C>(var values: C)\n",
        "operator tick<T>(value: T) -> T\n",
        "impl fn tick(var value: i64) -> i64 {\n",
        "    when { return value }\n",
        "}\n",
        // Line 23: a requirement supplies types, and no place.
        "const fn filled<C>(values: C) -> i64 requires push(C, i64) { return 1 }\n",
        "const fn asks() -> i64 => filled([1])\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:14:10 type/argument.place FN-15",
            "m.hgl:15:10 name/binding.immutable FN-15",
            "m.hgl:18:18 function-kind/parameter.var FN-15",
            "m.hgl:20:14 function-kind/parameter.var FN-15",
            "m.hgl:24:27 constraint/requires.unmet GEN-3",
        ]
    );
}
