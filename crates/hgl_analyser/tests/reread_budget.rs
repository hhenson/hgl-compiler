//! A construct is judged the same however much of its function was read
//! again before it. The analyser reads some expressions a second time:
//! one that takes its type from its place (EXPR-5) is tried as each
//! candidate's parameter (GEN-5), a value is read whole or as its delta
//! (TIME-7), a sequence's entries are read to learn their type (EXPR-9),
//! a generic call's arguments to learn its parameters (GEN-2, GEN-13).
//! Each test reads one construct after none, 999, 1,000 and 2,000
//! statements of its function that are read again, and asks for the
//! same diagnostics each time.

mod common;

use common::check;

/// What is read before the construct: `count` statements, each of which
/// reads an expression again, the `null` of `maybe == null` taking its
/// type from the other operand (EXPR-5).
fn fillers(count: usize, indent: &str) -> String {
    (0..count)
        .map(|index| format!("{indent}let filler{index} = maybe == null\n"))
        .collect()
}

/// The findings for `template`, a module `m` whose line `@FILL@` is
/// replaced by `count` fillers at its indentation, each as
/// `line:column code rule`, with the line counted as in `template`.
fn judged(template: &str, count: usize) -> Vec<String> {
    let lines: Vec<&str> = template.lines().collect();
    let fill = lines
        .iter()
        .position(|line| line.trim() == "@FILL@")
        .expect("the template has a line `@FILL@`");
    let indent = &lines[fill][..lines[fill].len() - lines[fill].trim_start().len()];
    let mut text = String::from("module m\n");
    for (index, line) in lines.iter().enumerate() {
        if index == fill {
            text.push_str(&fillers(count, indent));
        } else {
            text.push_str(line);
            text.push('\n');
        }
    }
    // Line numbers of `text`: the module line, then the template's.
    let fill_line = fill + 2;
    let checked = check(&[("m.hgl", &text)]);
    checked
        .findings
        .iter()
        .map(|finding| {
            let mut parts = finding.at.splitn(3, ':');
            let (_, line, column) = (parts.next(), parts.next(), parts.next());
            let line: usize = line.and_then(|line| line.parse().ok()).unwrap_or(0);
            // A line after the fillers is one of the template's.
            let line = if line >= fill_line + count {
                line + 1 - count
            } else {
                line
            };
            format!(
                "{line}:{} {} {}",
                column.unwrap_or(""),
                finding.code,
                finding.rule
            )
        })
        .collect()
}

/// Asserts that `template` is judged as `expected` after each count of
/// earlier statements that are read again.
fn judged_alike(template: &str, expected: &[&str]) {
    for count in [0, 999, 1_000, 2_000] {
        assert_eq!(
            judged(template, count),
            expected,
            "after {count} statements read again"
        );
    }
}

/// GEN-5: "No match rejects". `.nan` takes its type from its place
/// (TYPE-7), so `+` tries it as each candidate's parameter: no
/// candidate takes a `bool` and it (`symbols.rs`, the operand read
/// again and the candidates tried, `candidates.rs`).
#[test]
fn a_symbol_with_an_operand_of_its_place_is_judged_after_any_count() {
    judged_alike(
        concat!(
            "const fn symbol(maybe: i64?) -> bool {\n",
            "    @FILL@\n",
            "    let wrong = true + .nan\n",
            "    return true\n",
            "}\n",
        ),
        &["4:22 operator/operator.no_candidate OPS-1"],
    );
}

/// GEN-5 for an operator called by its name (`operators.rs`): the
/// argument `.nan` is tried as each candidate's parameter.
#[test]
fn a_named_operator_call_is_judged_after_any_count() {
    judged_alike(
        concat!(
            "const fn named(maybe: i64?) -> bool {\n",
            "    @FILL@\n",
            "    let wrong = add_(true, .nan)\n",
            "    return true\n",
            "}\n",
        ),
        &["4:17 operator/operator.no_candidate GEN-5"],
    );
}

/// GEN-7: "A generic body is checked under its stated requirements": a
/// call the requirement does not answer is selected among the
/// candidates, and `T` and `.nan` select none (`requirements.rs`).
#[test]
fn a_call_in_a_generic_body_is_judged_after_any_count() {
    judged_alike(
        concat!(
            "const fn generic<T>(value: T, maybe: i64?) -> bool requires eq_(T, T) {\n",
            "    @FILL@\n",
            "    let wrong = value == .nan\n",
            "    return true\n",
            "}\n",
        ),
        &["4:23 operator/operator.no_candidate OPS-2"],
    );
}

/// TST-6: a trace is compared "with a trace written out": the sequence
/// before an eval is read as a trace of the eval's cells, learnt from a
/// reading of the eval that is taken back (`evals.rs`).
#[test]
fn a_sequence_compared_with_an_eval_is_judged_after_any_count() {
    judged_alike(
        concat!(
            "fn doubled(value: i64) -> i64 {\n",
            "    when { return value * 2 }\n",
            "}\n",
            "test doubling {\n",
            "    let maybe: i64? = null\n",
            "    @FILL@\n",
            "    assert [2, 4] == eval(doubled(value: [1, 2]))\n",
            "}\n",
        ),
        &[],
    );
}

/// EXPR-9: a sequence's entries say what it holds. With a block among
/// them the others are read first to learn their type, and each is then
/// read as an element of it: the `[]` of `{ [] }` is a `list<i64>`
/// (`expressions.rs`).
#[test]
fn a_sequence_with_a_block_is_judged_after_any_count() {
    judged_alike(
        concat!(
            "const fn lists(maybe: i64?) -> i64 {\n",
            "    @FILL@\n",
            "    let held = [{ [1] }, { [] }]\n",
            "    return 0\n",
            "}\n",
        ),
        &[],
    );
}

/// GEN-2: a generic call's arguments say what `T` is. With a block among
/// them each is read to learn it and that reading taken back, and every
/// argument is then read where it is written (`inference.rs`).
#[test]
fn a_generic_call_with_a_block_is_judged_after_any_count() {
    judged_alike(
        concat!(
            "const fn first<T>(a: T, b: T) -> T => a\n",
            "const fn called(maybe: i64?) -> i64 {\n",
            "    @FILL@\n",
            "    let one = first({ 1 }, 2)\n",
            "    return one\n",
            "}\n",
        ),
        &[],
    );
}

/// TYPE-10, EXPR-9: a sequence written out given to a `list<T, 2>` is
/// held to its size once `T` is known: three entries are no
/// `list<i64, 2>` (`inference.rs`).
#[test]
fn a_sequence_given_to_a_generic_fixed_list_is_judged_after_any_count() {
    judged_alike(
        concat!(
            "const fn head<T>(items: list<T, 2>) -> T => items[0]\n",
            "const fn called(maybe: i64?) -> i64 {\n",
            "    @FILL@\n",
            "    let one = head([1, 2, 3])\n",
            "    return one\n",
            "}\n",
        ),
        &["5:20 type/type.mismatch TYPE-10"],
    );
}

/// GEN-13: what a call gives a generic value parameter says what a type
/// parameter its type names is, "before any argument is held to its
/// place": `fallback: 1` makes `T` an `i64`, and the `null` given to `T`
/// is then no value of it (`inference.rs`).
#[test]
fn a_generic_value_argument_is_judged_after_any_count() {
    judged_alike(
        concat!(
            "const fn pick<T, const fallback: T>(value: T) -> T => fallback\n",
            "const fn called(maybe: i64?) -> i64 {\n",
            "    @FILL@\n",
            "    let one = pick(null, fallback: 1)\n",
            "    return one\n",
            "}\n",
        ),
        &["5:20 type/type.mismatch TYPE-14"],
    );
}

/// TIME-7: "In a hook, `return value` ... take[s] a complete value of
/// the result's type T, published whole, or a `delta<T>`": `(first,
/// null)` is no `tuple<f64, f64>` and is its delta (`node.rs`).
#[test]
fn a_published_delta_is_judged_after_any_count() {
    judged_alike(
        concat!(
            "fn pairs(first: f64, maybe: i64?) -> tuple<f64, f64> {\n",
            "    when {\n",
            "        @FILL@\n",
            "        return (first, null)\n",
            "    }\n",
            "}\n",
        ),
        &[],
    );
}

/// A construct whose judging reads its expressions again more often
/// than this compiler does for one statement is reported (local
/// `reading.limit`), and is never left unjudged: a comparison of lists
/// nested eight deep tries each level as each candidate's parameter.
/// The statement after it is judged as it would be alone.
#[test]
fn a_construct_past_its_allowance_is_reported_and_the_next_is_judged() {
    let mut nested = String::from("maybe == null");
    for _ in 0..8 {
        nested = format!("[{nested}] == [true]");
    }
    let text = format!(
        "module m\nconst fn nested(maybe: i64?) -> i64 {{\n    let held = {nested}\n    \
         let wrong = true + .nan\n    return 0\n}}\n"
    );
    let checked = check(&[("m.hgl", &text)]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:5 local:type/reading.limit EXPR-5",
            "m.hgl:4:22 operator/operator.no_candidate OPS-1",
        ]
    );
}
