//! A list's or a window's size that a generic value parameter is given
//! (TYPE-10, TYPE-11): a size written as a constant is held to the rules
//! where it is written, and one that names a generic value parameter
//! where the parameter is given one, by a struct application or a call.
//! Each test cites its rule, and uses the core the compiler is built
//! with.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, module};

/// The message of each finding, in order.
fn messages(checked: &Checked) -> Vec<&str> {
    checked
        .findings
        .iter()
        .map(|finding| finding.message.as_str())
        .collect()
}

/// TYPE-10: "fixed sizes are nonnegative integer constants. A generic
/// value parameter declared with a type other than `i64` is no size; one
/// declared with none is a size". TYPE-4: a struct's own field has the
/// type "with the arguments the struct is given", and a field from a
/// parent "the type that parent's arguments give it". So an application
/// that gives a size a negative count, or no count, is `list.size` where
/// it is written: in a parameter's type, in a construction, in a field's
/// type and in a parent; with the argument written, worked out from a
/// size expression, taken from a default, and given through a parent or
/// a field's own application. An application that gives a count is
/// accepted.
#[test]
fn a_struct_application_that_gives_a_list_no_count_is_rejected() {
    let checked = module(concat!(
        "struct Fixed<const N: i64> {\n",
        "    items: list<i64, N>\n",
        "    n: i64 = 0\n",
        "}\n",
        "const fn written(f: Fixed<-1>) -> i64 => f.n\n",
        "const fn built() -> i64 => Fixed<-1>(items: []).n\n",
        "struct Less<const N: i64> { items: list<i64, N - 3> }\n",
        "const fn less(f: Less<1>) -> i64 => 0\n",
        "struct Defaulted<const N: i64 = -2> { items: list<i64, N> }\n",
        "const fn defaulted(f: Defaulted) -> i64 => 0\n",
        "abstract struct Base<const N: i64> { items: list<i64, N> }\n",
        "struct Child<const M: i64> : Base<M - 5> { n: i64 = 0 }\n",
        "const fn child(c: Child<1>) -> i64 => c.n\n",
        "struct Plain : Base<-1> { n: i64 = 0 }\n",
        "struct Outer<const N: i64> { inner: Fixed<N - 3> }\n",
        "const fn outer(o: Outer<1>) -> i64 => 0\n",
        "struct Holder { f: Fixed<-1> }\n",
        "struct Untyped<N> { items: list<i64, N> }\n",
        "const fn untyped(u: Untyped<\"x\">) -> i64 => 0\n",
        "const fn counted(f: Fixed<0>, g: Less<3>, u: Untyped<2>) -> i64 => 0\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:21 type/list.size TYPE-10",
            "m.hgl:7:28 type/list.size TYPE-10",
            "m.hgl:7:45 type/type.mismatch TYPE-10",
            "m.hgl:9:18 type/list.size TYPE-10",
            "m.hgl:11:23 type/list.size TYPE-10",
            "m.hgl:14:19 type/list.size TYPE-10",
            "m.hgl:15:16 type/list.size TYPE-10",
            "m.hgl:17:19 type/list.size TYPE-10",
            "m.hgl:18:20 type/list.size TYPE-10",
            "m.hgl:20:21 type/list.size TYPE-10",
        ]
    );
    let messages = messages(&checked);
    assert_eq!(
        messages[0],
        "`Fixed<-1>` makes `items` a `list<i64, -1>`: a list's size is not negative"
    );
    assert_eq!(
        messages[7],
        "`Outer<1>` makes `inner.items` a `list<i64, -2>`: a list's size is not negative"
    );
    assert_eq!(
        messages[9],
        "`Untyped<\"x\">` makes `items` a `list<i64, \"x\">`: a list's size is a count, and \
         this is a string"
    );
}

/// TYPE-11: "Tick sizes are positive; duration maximum is positive and
/// minimum may be zero ... Minimum cannot exceed maximum", and "A size
/// that is a generic value parameter has its declared type's kind ...
/// One declared with no type has its kind once resolved". So an
/// application that gives a window's size one of these is
/// `rolling_window.size_bounds`, and one that gives it no tick count or
/// duration `rolling_window.size_kind`, where it is written. One that
/// gives sizes the rules admit is accepted.
#[test]
fn a_struct_application_that_gives_a_window_a_size_it_cannot_have_is_rejected() {
    let checked = module(concat!(
        "struct Ticks<const N: i64> { w: rolling_window<f64, N> }\n",
        "const fn ticks(w: Ticks<0>) -> i64 => 0\n",
        "struct Aged<const D: duration> { w: rolling_window<f64, D> }\n",
        "const fn aged(w: Aged<0s>) -> i64 => 0\n",
        "struct Bounded<const N: i64, const M: i64> { w: rolling_window<f64, N, M> }\n",
        "const fn bounded(w: Bounded<3, 5>) -> i64 => 0\n",
        "struct Kinded<N> { w: rolling_window<f64, N> }\n",
        "const fn kinded(w: Kinded<\"x\">) -> i64 => 0\n",
        "struct Floor<const M: duration> { w: rolling_window<f64, 5m, M> }\n",
        "const fn floor(w: Floor<-1s>) -> i64 => 0\n",
        "const fn admitted(a: Ticks<1>, b: Aged<5m>, c: Bounded<5, 3>, d: Floor<0s>, e: Kinded<2s>) -> i64 => 0\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:19 type/rolling_window.size_bounds TYPE-11",
            "m.hgl:5:18 type/rolling_window.size_bounds TYPE-11",
            "m.hgl:7:21 type/rolling_window.size_bounds TYPE-11",
            "m.hgl:9:20 type/rolling_window.size_kind TYPE-11",
            "m.hgl:11:19 type/rolling_window.size_bounds TYPE-11",
        ]
    );
    let messages = messages(&checked);
    assert_eq!(
        messages[0],
        "`Ticks<0>` makes `w` a `rolling_window<f64, 0>`: a window holds at least one tick"
    );
    assert_eq!(
        messages[2],
        "`Bounded<3, 5>` makes `w` a `rolling_window<f64, 3, 5>`: a window's minimum cannot \
         exceed its maximum"
    );
}

/// TYPE-10 as above, for a call of a generic function: GEN-2, "A call
/// resolves a generic of its callee from what it writes after the name
/// (GEN-8) ... A generic value parameter may have a default ... which it
/// takes where a call ... neither gives it nor resolves it". A call that
/// gives a size of its callee's parameter or result a negative count is
/// `list.size` at the call: written, and taken from a default. A value
/// given to a parameter of that size is held to it as well
/// (`type.mismatch`).
#[test]
fn a_call_that_gives_a_list_a_negative_size_is_rejected() {
    let checked = module(concat!(
        "const fn sized<const N: i64>(values: list<i64, N>) -> i64 => 0\n",
        "const fn make<const N: i64>() -> list<i64, N> => []\n",
        "const fn by_default<const N: i64 = -4>() -> list<i64, N> => []\n",
        "const fn uses() -> i64 {\n",
        "    let a = sized<-1>([])\n",
        "    let b = make<-1>()\n",
        "    let c = by_default()\n",
        "    let d = sized<0>([])\n",
        "    let e = make<2>()\n",
        "    return 0\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:13 type/list.size TYPE-10",
            "m.hgl:6:23 type/type.mismatch TYPE-10",
            "m.hgl:7:13 type/list.size TYPE-10",
            "m.hgl:8:13 type/list.size TYPE-10",
        ]
    );
    let messages = messages(&checked);
    assert_eq!(
        messages[0],
        "this call of `sized` makes its parameter `values` a `list<i64, -1>`: a list's size \
         is not negative"
    );
    assert_eq!(
        messages[3],
        "this call of `by_default` makes its result a `list<i64, -4>`: a list's size is not \
         negative"
    );
}

/// A struct that holds itself with arguments that grow at each level
/// gives no size that is known below the type written, and holding it
/// ends: `Grow<N + N>` in `Grow<const N: i64>`, `Twice<tuple<T, T>>` in
/// `Twice<T>`, and `Pairs<[N, N]>` in `Pairs<N>`, where each writes it and
/// where each is applied with known arguments. TYPE-10 as above for
/// `Grow<-1>`.
#[test]
fn a_struct_that_holds_itself_with_growing_arguments_is_held_in_bounds() {
    let checked = module(concat!(
        "struct Grow<const N: i64> {\n",
        "    next: atomic<list<Grow<N + N>>>\n",
        "    items: list<i64, N>\n",
        "}\n",
        "struct Twice<T> {\n",
        "    next: atomic<list<Twice<tuple<T, T>>>>\n",
        "    value: T\n",
        "}\n",
        "const fn grown(g: Grow<1>, t: Twice<i64>) -> i64 => 0\n",
        "const fn shrunk(g: Grow<-1>) -> i64 => 0\n",
        "struct Pairs<N> {\n",
        "    next: atomic<list<Pairs<[N, N]>>>\n",
        "}\n",
        "const fn paired(p: Pairs<0>) -> i64 => 0\n",
    ));
    // TYPE-6 rejects `Twice`, which holds itself; the instances of
    // `Grow` end at an `N + N` out of range (GEN-8), and those of `Pairs`
    // do not end within the bound (GEN-13).
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:7:5 type/field.recursive TYPE-6",
            "m.hgl:10:19 type/generic.argument_constant GEN-8",
            "m.hgl:11:20 type/generic.argument_constant GEN-8",
            "m.hgl:11:20 type/list.size TYPE-10",
            "m.hgl:15:20 type/generic.instance_limit GEN-13",
        ]
    );
}
