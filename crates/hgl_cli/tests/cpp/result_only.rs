//! GEN-1, GEN-6, GEN-7, GEN-12 and FN-17 on the C++ SDK.

use super::{argument, hgl_in, scratch, sdk_named, text, write};

#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn generic_callbacks_can_bind_no_result() {
    if !sdk_named() {
        return;
    }
    let root = scratch("result_only_callbacks");
    let source = root.join("src");
    write(
        &source,
        "callbacks.hgl",
        r#"module demo.callbacks

fn record(value: i64) {
    when { write(format("{}\n", value), Stream.out) }
}
fn plus_one(value: i64) -> i64 => value + 1
fn unexpected(failure: error) {
    when { write("unexpected error\n", Stream.out) }
}

fn through<R>(f: fn(i64) -> R, value: i64) -> R => f(value)
fn relay<R>(f: fn(i64) -> R, value: i64) -> R => through(f, value)
fn forward<R>(f: fn(i64) -> R, value: i64) -> R => relay<R>(f, value)

operator visit<R>(f: fn(i64) -> R, value: i64) -> R
impl fn visit<R>(f: fn(i64) -> R, value: i64) -> R => f(value)
impl fn visit(f: fn(i64), value: i64) => f(value + 10)

operator fetch<T, R>(value: T) -> R
impl fn fetch(value: i64) => record(value + 20)

fn input() -> i64 { yield 1s: 1 }
export fn execute() {
    let value = input()
    let seen = capture(through, record, value)
    unexpected(seen.error)
    through(fn(x) => record(x + 1), value)
    through(fn(x) { record(x + 2) }, value)
    relay(fn(x) => { record(x + 3) }, value)
    visit(record, value)
    fetch(value)
    forward(record, value + 4)
}

fn valued(value: i64) -> i64 => through(plus_one, value) + through(fn(x) => x + 1, value) + visit(plus_one, value)

fn forwarded(value: i64) -> i64 => forward(plus_one, value)

test results {
    assert eval valued(value: [3]) == [12]
    assert eval forwarded(value: [3]) == [4]
}
"#,
    );
    write(
        &source,
        "execute.hgl",
        r#"application demo.callbacks.execute

use demo.callbacks::{execute}

eval execute()
"#,
    );
    let root = argument(&source);
    let run = hgl_in(&["run", &root, "--target", "cpp"], &[], &[]);
    let stdout = text(&run.stdout);
    assert_eq!(run.status.code(), Some(0), "{stdout}{}", text(&run.stderr));
    assert_eq!(
        stdout,
        "ran demo.callbacks.execute on cpp: its entry has no result\n"
    );
    // The driver keeps the entry's stream writes separate from publications.
    let stderr = text(&run.stderr);
    let mut effects: Vec<_> = stderr.lines().collect();
    effects.sort_unstable();
    assert_eq!(effects, ["1", "11", "2", "21", "3", "4", "5"]);

    let tests = hgl_in(&["test", &root, "--target", "cpp"], &[], &[]);
    let stdout = text(&tests.stdout);
    assert_eq!(
        tests.status.code(),
        Some(0),
        "{stdout}{}",
        text(&tests.stderr)
    );
    assert!(
        stdout.contains("passed   executable  demo.callbacks::results\n"),
        "{stdout}"
    );
}
