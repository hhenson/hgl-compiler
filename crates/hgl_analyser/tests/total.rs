//! The analyser gives a program for any input and never panics.
//!
//! The parser builds a tree for any tokens, so the analyser meets
//! declarations with any part missing. These tests cut a program short
//! at every character and analyse what is left.

mod common;

use common::check_broken;

/// A program that uses every kind of item the analyser reads, and in a
/// body every form that binds a name.
const FILES: [(&str, &str); 3] = [
    (
        "lib.hgl",
        concat!(
            "module lib\n",
            "use app as a\n",
            "export fn shared() {}\n",
            "export const fn shared() -> i64 => 1\n",
            "export struct Quote<T> { bid: T }\n",
            "export enum Side { buy, sell }\n",
            "export native type Handle\n",
            "export operator add<T>(left: T, right: T) -> T\n",
            "impl fn add(left: i64, right: i64) -> i64 => left\n",
            "instantiate add<i64>\n",
            "native fn tick() -> i64\n",
            "native const fn describe(value: i64) -> str\n",
        ),
    ),
    (
        "provider.hgl",
        concat!(
            "module lib part provider\n",
            "native fn tick() -> i64 { inject out; start; when; stop; }\n",
            "native const fn describe(value: i64) -> str {}\n",
        ),
    ),
    (
        "app.hgl",
        concat!(
            "module app\n",
            "use lib::{shared, Quote, add}\n",
            "use lib as l\n",
            "impl fn add(left: f64, right: f64) -> f64 => left\n",
            "impl fn mul(left: f64, right: f64) -> f64 => left\n",
            "fn main<T>(value: T, items: dict<i64, T>, const size: i64 = 2) -> T {\n",
            "    state kept = 0\n",
            "    inject out\n",
            "    when modified(value) {\n",
            "        let local = l::shared()\n",
            "        for index, item in items { kept = index }\n",
            "        let pick = fn(x) => x + local\n",
            "        switch kept {\n",
            "            case 1:\n",
            "                return value\n",
            "            default:\n",
            "                return Quote<T>(bid: value).bid\n",
            "        }\n",
            "    }\n",
            "}\n",
            "test main { assert true }\n",
            "test {\n",
            "    fn helper() {}\n",
            "    struct Fixture { x: i64 }\n",
            "    test inner { assert true }\n",
            "}\n",
        ),
    ),
];

#[test]
fn the_whole_program_is_accepted_but_for_its_cycle_and_an_unknown_operator() {
    let checked = check_broken(&FILES);
    assert_eq!(checked.front_end_errors, 0, "{}", checked.report);
    assert_eq!(
        checked.summary(),
        [
            "app.hgl:2:5 module/import.cycle UNIT-3",
            // GEN-4: nothing declares an operator `mul`.
            "app.hgl:5:9 operator/impl.unknown_operator GEN-4",
        ]
    );
}

/// Each file in turn is cut short at every character, with the others
/// whole.
#[test]
fn every_prefix_of_every_file_is_analysed() {
    let mut runs = 0;
    for (cut, (_, text)) in FILES.iter().enumerate() {
        // `char_indices` gives the byte position of each character, so
        // the cut never falls inside one.
        for (end, _) in text.char_indices() {
            let mut files = FILES;
            files[cut].1 = &text[..end];
            let checked = check_broken(&files);
            // Whatever was reported points inside its file; `check_broken`
            // asserts that. A program came back, holding at least the
            // modules of the files left whole.
            assert!(!checked.program.modules.is_empty());
            runs += 1;
        }
    }
    assert!(runs > 500, "the files were cut {runs} times");
}

/// Declarations with a part missing declare what they can.
#[test]
fn declarations_with_missing_parts_are_analysed() {
    let checked = check_broken(&[
        ("a.hgl", "module\nfn f() {}\n"),
        (
            "b.hgl",
            "module m part\nfn f() {}\nfn () {}\nstruct { x: i64 }\n",
        ),
        (
            "c.hgl",
            "module m\nuse\nuse lib\nuse lib::{}\nuse lib as\nenum { a }\ntest { fn }\n",
        ),
        (
            "d.hgl",
            "module m part\noperator (left: i64)\nnative fn\nnative type\ntest {\n",
        ),
        ("e.hgl", ""),
    ]);
    assert!(checked.front_end_errors > 0);
    // `a.hgl` names no module. `b.hgl` and `d.hgl` have no part name
    // the parser could read, so with `c.hgl` the module has three
    // unnamed files.
    assert_eq!(checked.modules().len(), 1);
    assert_eq!(checked.modules()[0].files.len(), 3);
}

/// The analyser reads the tree by calling itself for what is nested, so
/// its use of the call stack grows with the tree's depth. The parser
/// bounds that depth (`hgl_parser::Limits`), and `hgl_analyser::stack_size`
/// says how much stack source within those bounds needs. The `hgl`
/// command runs on a thread with that much.
#[test]
fn the_analyser_fits_the_stack_it_asks_for() {
    let limits = hgl_parser::Limits::default();
    // An `if` and its block are two levels of nesting, and four of the
    // tree. Blocks nest nearly as deep as nesting may, and inside them
    // the tree goes nearly as deep as it may be: by a chain of
    // operators, by a member of a member, by an `else if` ladder.
    let blocks = limits.nesting / 2 - 5;
    let left = limits.tree_depth - 4 * blocks - 20;
    let chain = vec!["value"; left].join(" + ");
    let members = vec!["next"; left - 4].join(".");
    let mut ladder = String::from("if value == 0 { }");
    for _ in 0..left / 2 - 4 {
        ladder.push_str(" else if value == 1 { }");
    }
    let deepest = [
        format!("let total = {chain}\n"),
        format!("let last = link.{members}\n"),
        format!("{ladder}\n"),
    ];
    for inside in deepest {
        let mut text = String::from(concat!(
            "module m\n",
            "struct Link { next: Link? }\n",
            "const fn deep(value: i64, link: Link) -> i64 {\n",
        ));
        for _ in 0..blocks {
            text.push_str("if value == 1 {\n");
        }
        text.push_str(&inside);
        for _ in 0..blocks {
            text.push_str("}\n");
        }
        text.push_str("return value\n}\n");

        let worker = std::thread::Builder::new()
            .stack_size(hgl_analyser::stack_size(limits))
            .spawn(move || {
                let checked = check_broken(&[("m.hgl", text.as_str())]);
                (checked.front_end_errors, checked.report.clone())
            })
            .expect("a thread starts");
        let (front_end_errors, report) = worker.join().expect("the analyser fits the stack");
        assert_eq!(front_end_errors, 0, "{report}");
    }
}

/// A value read before a generic struct's arguments are found is read
/// again once they are, when its first reading gave it no whole type
/// (GEN-2). One such value inside another would double the work at each
/// level, so the pass counts them for each statement and stops reading
/// again past its bound: the analysis ends.
#[test]
fn values_read_again_are_counted_for_each_statement() {
    let depth = 40;
    let mut text =
        String::from("module m\nstruct Box<T> { value: T }\nconst fn deep() {\n    let nested = ");
    for _ in 0..depth {
        text.push_str("Box(value: [");
    }
    // Entries of two types: with nothing known of their place they give
    // the sequence no type, at every level above.
    text.push_str("1, \"x\"");
    for _ in 0..depth {
        text.push_str("])");
    }
    text.push_str("\n}\n");
    let started = std::time::Instant::now();
    let checked = check_broken(&[("m.hgl", text.as_str())]);
    assert_eq!(checked.front_end_errors, 0, "{}", checked.report);
    // Two to the fortieth readings would not end in the life of the
    // test; a bounded count ends at once.
    assert!(started.elapsed() < std::time::Duration::from_secs(20));
}

/// GEN-5: an argument written as a sequence is tried as each candidate's
/// parameter, and read once more as the selected one's. A call inside
/// such an argument is tried in turn, which would multiply the work at
/// each level; the readings are counted for each statement, and one that
/// needs more than its bound is reported (local `reading.limit`): what is
/// past it is not judged, and is never accepted.
#[test]
fn candidate_trials_past_a_statements_bound_are_reported() {
    let depth = 40;
    let mut text = String::from(concat!(
        "module m\n",
        "operator wrap<T>(values: T) -> i64\n",
        "impl const fn wrap(values: list<i64>) -> i64 => 0\n",
        "impl const fn wrap(values: list<f64>) -> i64 => 0\n",
        "const fn deep() {\n    let nested = ",
    ));
    for _ in 0..depth {
        text.push_str("wrap([");
    }
    text.push('1');
    for _ in 0..depth {
        text.push_str("])");
    }
    text.push_str("\n}\n");
    let started = std::time::Instant::now();
    let checked = check_broken(&[("m.hgl", text.as_str())]);
    assert_eq!(checked.front_end_errors, 0, "{}", checked.report);
    assert_eq!(
        checked.summary(),
        ["m.hgl:6:5 local:type/reading.limit EXPR-5"]
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(20));
}
