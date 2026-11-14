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
            "operator add<T>(left: T, right: T) -> T\n",
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
            "fn main<T>(value: T, items: list<T>, const size: i64 = 2) -> T {\n",
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
fn the_whole_program_is_accepted_but_for_its_cycle() {
    let checked = check_broken(&FILES);
    assert_eq!(checked.front_end_errors, 0, "{}", checked.report);
    assert_eq!(
        checked.summary(),
        ["app.hgl:2:5 module/import.cycle UNIT-3"]
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
    assert_eq!(checked.program.modules.len(), 1);
    assert_eq!(checked.program.modules[0].files.len(), 3);
}
