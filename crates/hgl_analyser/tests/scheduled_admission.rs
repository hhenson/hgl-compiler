//! What a `when` header that is `scheduled()` alone admits (TIME-3,
//! TIME-4).

mod common;

use common::{Checked, check};

/// Checks one file, `m.hgl`, of module `m`. The `module` line is added,
/// so the text given starts on line 2.
fn module(text: &str) -> Checked {
    check(&[("m.hgl", &format!("module m\n{text}"))])
}

/// TIME-3: "A header that is `scheduled()` alone is the exception: it
/// requires no input to be valid, as one that writes `valid([])` does".
/// A read of an input there is unguarded, written in parentheses too;
/// TIME-4: a `valid(value)` that holds where it stands guards it, and so
/// does one beside `scheduled()` in the header, which then admits it.
#[test]
fn a_header_that_is_scheduled_alone_admits_no_input() {
    let checked = module(concat!(
        "fn alone(value: f64) -> f64 {\n",
        "    inject scheduler\n",
        "    start { schedule(scheduler, 1s) }\n",
        "    when scheduled() { return value }\n",
        "}\n",
        "fn parenthesised(value: f64) -> f64 {\n",
        "    inject alarm\n",
        "    start { schedule(alarm, 1s) }\n",
        "    when (scheduled()) { return value }\n",
        "}\n",
        "fn guarded(value: f64) -> f64 {\n",
        "    inject scheduler\n",
        "    start { schedule(scheduler, 1s) }\n",
        "    when scheduled() {\n",
        "        if valid(value) { return value }\n",
        "        return 0.0\n",
        "    }\n",
        "}\n",
        "fn admitted(value: f64) -> f64 {\n",
        "    inject scheduler\n",
        "    start { schedule(scheduler, 1s) }\n",
        "    when scheduled() && valid(value) { return value }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:31 phase/payload.unguarded TIME-4",
            "m.hgl:10:33 phase/payload.unguarded TIME-4",
        ]
    );
}

/// TIME-3: "Omitting the validity selector requires all temporal inputs
/// to be valid", and the exception is for `scheduled()` alone: a header
/// that is `modified([])` alone, or that has `scheduled()` beside
/// another conjunct, admits every input.
#[test]
fn other_timer_headers_keep_the_default_admission() {
    let checked = module(concat!(
        "fn empty(value: f64) -> f64 {\n",
        "    inject scheduler\n",
        "    start { schedule(scheduler, 1s) }\n",
        "    when modified([]) { return value }\n",
        "}\n",
        "fn beside(value: f64) -> f64 {\n",
        "    inject scheduler\n",
        "    start { schedule(scheduler, 1s) }\n",
        "    when scheduled() && value > 0.0 { return value }\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), Vec::<String>::new());
}
