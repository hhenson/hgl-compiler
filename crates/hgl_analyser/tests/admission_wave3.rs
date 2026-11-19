//! What a `when` header admits of its own reads, however many handlers
//! stand before it (TIME-3, TIME-4), and where `scheduled()` and the
//! other endpoint operations may be written, in an anonymous function
//! among the places (TIME-5, TIME-9, FN-9, FN-14, SOURCE-1).

mod common;

use common::{Checked, check};

/// Checks one file, `m.hgl`, of module `m`. The `module` line is added,
/// so the text given starts on line 2.
fn module(text: &str) -> Checked {
    check(&[("m.hgl", &format!("module m\n{text}"))])
}

/// A node of `count` handlers, each reading `b` in its header under its
/// own `valid(b)`, and after them one whose header reads `a`, which it
/// does not admit, and one that admits what it reads.
fn many_handlers(count: usize) -> String {
    let mut text = String::from("fn many(a: i64, b: i64) -> i64 {\n");
    for value in 0..count {
        text.push_str(&format!(
            "    when valid(b) && b == {value} {{ return b }}\n"
        ));
    }
    text.push_str("    when valid(b) && a > 0 { return b }\n");
    text.push_str("    when valid(a) && a > 0 { return a }\n");
    text.push_str("}\n");
    text
}

/// TIME-4: "A payload read under manual admission must be dominated by
/// a successful guard", and TIME-3: "`valid(a, b)` requires both named
/// inputs ... A selector sets subscription or admission when it stands
/// alone in the header or appears as a top-level conjunct". So
/// `when valid(b) && a > 0` admits `b` alone, and its read of `a` is
/// unguarded, however many handlers the node has before it.
fn header_reads_are_judged_after(count: usize) {
    let checked = module(&many_handlers(count));
    assert_eq!(
        checked.summary(),
        [format!(
            "m.hgl:{}:22 phase/payload.unguarded TIME-4",
            count + 3
        )]
    );
}

/// See [`header_reads_are_judged_after`]: 1,000 handlers.
#[test]
fn a_header_is_judged_after_a_thousand_handlers() {
    header_reads_are_judged_after(1_000);
}

/// See [`header_reads_are_judged_after`]: 5,000 handlers.
#[test]
fn a_header_is_judged_after_five_thousand_handlers() {
    header_reads_are_judged_after(5_000);
}
