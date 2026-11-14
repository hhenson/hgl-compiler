//! The runner against the rules of source-error expectations, TST-12 to
//! TST-15. Each test cites the rule it checks.
//!
//! What is tested is the runner: which line an annotation applies to,
//! what owns it, and how errors and annotations are matched. Most cases
//! are lexical and syntax errors, which need no other declaration; the
//! analyser's are in `a_name_or_module_rejection_is_a_case`.

mod common;

use common::{run_all, run_selected};
use hgl_runner::{CaseKind, Outcome, Selection};

/// No lines.
const NONE: [&str; 0] = [];

// ---- TST-12: annotations and owners ----

/// TST-12: an annotation "applies to the next physical source line that
/// is not itself an annotation, so several may precede one line", and
/// "The enclosing named test/declaration/import owns the whole rejection
/// case".
#[test]
fn an_annotation_applies_to_the_next_line_and_its_item_owns_the_case() {
    let ran = run_all(&[(
        "cases.hgl",
        concat!(
            "module examples.rejection\n",
            // An import owns a case.
            "# expect-error(parse, \"syntax.expected_token\")\n",
            "use lib::{}\n",
            "\n",
            // A declaration, with the annotation directly before it.
            "# expect-error(parse, \"string.unknown_escape\")\n",
            "const fn one() -> str => \"a\\q\"\n",
            "\n",
            // Two errors on one line take two annotations.
            "    # expect-error(parse, \"string.unknown_escape\")\n",
            "\t#expect-error( parse ,\t\"string.unclosed\" )\t\n",
            "const fn two() -> str => \"a\\q\" + \"b\n",
            "\n",
            // The annotation stands inside its owner, before the line.
            "const fn three() -> str {\n",
            "    let fine = \"a\"\n",
            "    # expect-error(parse, \"string.unknown_escape\")\n",
            "    return \"a\\q\"\n",
            "}\n",
            "\n",
            // A named test owns a case, and so does a helper in a context.
            "test rejected {\n",
            "    # expect-error(parse, \"string.unknown_escape\")\n",
            "    assert \"a\\q\" == \"a\"\n",
            "}\n",
            "test {\n",
            "    # expect-error(parse, \"string.unknown_escape\")\n",
            "    const fn helper() -> str => \"a\\q\"\n",
            "}\n",
        ),
    )]);
    assert_eq!(ran.stopped, NONE);
    assert_eq!(
        ran.cases,
        [
            "passed rejection cases.hgl:3 use lib",
            "passed rejection cases.hgl:6 const fn one",
            "passed rejection cases.hgl:10 const fn two",
            "passed rejection cases.hgl:12 const fn three",
            "passed rejection examples.rejection::rejected",
            "passed rejection cases.hgl:24 const fn helper",
        ]
    );
    assert!(ran.report.succeeded());
}

/// TST-12: "A line comment that begins `expect-error` after `#` and any
/// spaces or tabs, without that form, is invalid metadata (TST-14)", and
/// an annotation is "alone on its line". TST-14: "Invalid metadata ...
/// prevent execution."
#[test]
fn invalid_metadata_stops_the_run() {
    let ran = run_all(&[(
        "cases.hgl",
        concat!(
            "module m\n",
            "# expect-error(parse \"string.unclosed\")\n",
            "fn a() {}\n",
            "# expect-error(parse, \"no.such_code\")\n",
            "fn b() {}\n",
            "fn c() {} # expect-error(parse, \"string.unclosed\")\n",
            "# expect-error (parse, \"string.unclosed\")\n",
            "fn d() {}\n",
            // An ordinary comment that mentions the word is no annotation.
            "# see expect-error(parse, \"string.unclosed\") above\n",
            "test survives { assert true }\n",
        ),
    )]);
    assert_eq!(
        ran.stopped,
        [
            "cases.hgl:2:1 invalid `expect-error` annotation: a comma follows the category (TST-12)",
            "cases.hgl:4:1 invalid `expect-error` annotation: the diagnostic catalogue has no \
             `no.such_code` in category `parse` (TST-12)",
            "cases.hgl:6:11 an `expect-error` annotation stands alone on its line (TST-12)",
            "cases.hgl:7:1 invalid `expect-error` annotation: `(` directly follows `expect-error` \
             (TST-12)",
        ]
    );
    assert_eq!(ran.cases, NONE, "no case is run");
    assert!(ran.report.prevented());
    assert!(!ran.report.succeeded());
}

/// TST-12: "Unnamed context/module headers cannot own a case." Nor can a
/// line of no item at all.
#[test]
fn an_annotation_with_no_owner_stops_the_run() {
    let ran = run_all(&[(
        "cases.hgl",
        concat!(
            "# expect-error(parse, \"syntax.expected_token\")\n",
            "module m\n",
            "# expect-error(parse, \"syntax.expected_token\")\n",
            "\n",
            "fn a() {}\n",
            "# expect-error(parse, \"syntax.expected_token\")\n",
            "# an ordinary comment is a line, and no item\n",
            "fn b() {}\n",
            "# expect-error(parse, \"syntax.expected_token\")\n",
            "test {\n",
            "    fn helper() {}\n",
            "# expect-error(parse, \"syntax.expected_token\")\n",
            "}\n",
            "# expect-error(parse, \"syntax.expected_token\")",
        ),
    )]);
    assert_eq!(
        ran.stopped,
        [
            "cases.hgl:14:1 an `expect-error` annotation applies to the next line, and none \
             follows (TST-12)",
            "cases.hgl:1:1 this `expect-error` annotation applies to a line of no import, \
             declaration or named test (TST-12)",
            "cases.hgl:3:1 this `expect-error` annotation applies to a line of no import, \
             declaration or named test (TST-12)",
            "cases.hgl:6:1 this `expect-error` annotation applies to a line of no import, \
             declaration or named test (TST-12)",
            "cases.hgl:9:1 this `expect-error` annotation applies to a line of no import, \
             declaration or named test (TST-12)",
            "cases.hgl:12:1 this `expect-error` annotation applies to a line of no import, \
             declaration or named test (TST-12)",
        ]
    );
    assert_eq!(ran.cases, NONE);
}

/// TST-12: "reliable source boundaries are required. Unnamed
/// context/module headers cannot own a case." An item on a line with
/// the `test {` or the `}` of its context shares that line with the
/// context, so an error there is not surely its own and it owns no case.
#[test]
fn an_item_on_a_line_with_its_contexts_braces_owns_no_case() {
    let ran = run_all(&[(
        "cases.hgl",
        concat!(
            "module m\n",
            "# expect-error(parse, \"string.unknown_escape\")\n",
            "test { const fn one() -> str => \"a\\q\" }\n",
            "test {\n",
            "    # expect-error(parse, \"string.unknown_escape\")\n",
            "    const fn two() -> str => \"a\\q\" }\n",
            "test {\n",
            // On lines of its own, the helper owns its case.
            "    # expect-error(parse, \"string.unknown_escape\")\n",
            "    const fn three() -> str => \"a\\q\"\n",
            "}\n",
        ),
    )]);
    assert_eq!(
        ran.stopped,
        [
            "cases.hgl:2:1 this `expect-error` annotation applies to a line of no import, \
             declaration or named test (TST-12)",
            "cases.hgl:5:5 this `expect-error` annotation applies to a line of no import, \
             declaration or named test (TST-12)",
            "cases.hgl:3:35 parse/string.unknown_escape",
            "cases.hgl:6:32 parse/string.unknown_escape",
        ]
    );
    assert_eq!(ran.cases, NONE);
}

/// TST-12: "reliable source boundaries are required", and TST-14:
/// "ambiguous boundaries ... prevent execution." Two items on one line
/// share it: an error there could be either's, so neither owns a case
/// and the errors on the line are the surviving source's. The same
/// holds for an item on the `module` line.
#[test]
fn items_that_share_a_line_own_no_case() {
    let ran = run_all(&[(
        "cases.hgl",
        concat!(
            "module m\n",
            "# expect-error(parse, \"syntax.expected_token\")\n",
            "# expect-error(parse, \"string.unknown_escape\")\n",
            "fn removed() {} const fn surviving() -> str => \"bad\\q\"\n",
        ),
    )]);
    assert_eq!(
        ran.stopped,
        [
            "cases.hgl:2:1 this `expect-error` annotation applies to a line of no import, \
             declaration or named test (TST-12)",
            "cases.hgl:3:1 this `expect-error` annotation applies to a line of no import, \
             declaration or named test (TST-12)",
            "cases.hgl:4:17 parse/syntax.expected_token",
            "cases.hgl:4:52 parse/string.unknown_escape",
        ]
    );
    assert_eq!(ran.cases, NONE);

    let ran = run_all(&[(
        "cases.hgl",
        concat!(
            "# expect-error(parse, \"syntax.expected_token\")\n",
            "module m fn first() {}\n",
            "# expect-error(parse, \"string.unknown_escape\")\n",
            "const fn alone() -> str => \"bad\\q\"\n",
        ),
    )]);
    assert_eq!(
        ran.stopped,
        [
            "cases.hgl:1:1 this `expect-error` annotation applies to a line of no import, \
             declaration or named test (TST-12)",
            "cases.hgl:2:10 parse/syntax.expected_token",
        ]
    );
    assert_eq!(ran.cases, NONE);
}

/// TST-13: "Exclude all rejection owners from executable source", and
/// TST-14: "surviving-source errors prevent execution." An error is an
/// owner's only when it is inside the owner. Text the parser could place
/// nowhere, after an item on its line, is not: with the item left out it
/// is still there.
#[test]
fn an_error_beside_an_owner_is_the_surviving_sources() {
    let ran = run_all(&[(
        "cases.hgl",
        concat!(
            "module m\n",
            "# expect-error(parse, \"syntax.expected_token\")\n",
            "fn removed() {} 42\n",
        ),
    )]);
    assert_eq!(ran.stopped, ["cases.hgl:3:17 parse/syntax.expected_token"]);
    assert_eq!(ran.cases, NONE);

    // With no space between them the error still starts after the item.
    let ran = run_all(&[(
        "cases.hgl",
        concat!(
            "module m\n",
            "# expect-error(parse, \"syntax.expected_token\")\n",
            "fn removed() {}42\n",
        ),
    )]);
    assert_eq!(ran.stopped, ["cases.hgl:3:16 parse/syntax.expected_token"]);
    assert_eq!(ran.cases, NONE);

    // Nor is a documentation comment that documents nothing, after the
    // file's last item: the item is whole, and the comment is beside it.
    let ran = run_all(&[(
        "cases.hgl",
        concat!(
            "module m\n",
            "# expect-error(parse, \"string.unknown_escape\")\n",
            "const fn last() -> str => \"a\\q\" /** Documents nothing. */\n",
        ),
    )]);
    assert_eq!(ran.stopped.len(), 1, "{:?}", ran.stopped);
    assert!(
        ran.stopped[0].starts_with("cases.hgl:3:33 parse/doc."),
        "{:?}",
        ran.stopped
    );
    assert_eq!(ran.cases, NONE);

    // Something missing after an item's last token is the item's: the
    // parser reports it at the item's end.
    let ran = run_all(&[(
        "cases.hgl",
        concat!(
            "module m\n",
            "# expect-error(parse, \"syntax.expected_token\")\n",
            "fn cut_short() ->\n",
            "\n",
            "test survives { assert true }\n",
        ),
    )]);
    assert_eq!(ran.stopped, NONE);
    assert_eq!(
        ran.cases,
        [
            "passed rejection cases.hgl:3 fn cut_short",
            "not-run executable m::survives",
        ]
    );
}

/// TST-12: "Unnamed context/module headers cannot own a case." A context
/// the end of the file cuts short is what that error is for, whether or
/// not a line break ends the file: when none does, the file ends on the
/// last line of the context's last item, and the error is still not that
/// item's.
#[test]
fn an_open_contexts_error_is_not_its_last_items() {
    let ran = run_all(&[(
        "cases.hgl",
        concat!(
            "module m\n",
            "test {\n",
            "# expect-error(parse, \"syntax.expected_token\")\n",
            "fn helper() {}",
        ),
    )]);
    assert_eq!(ran.stopped, ["cases.hgl:4:15 parse/syntax.expected_token"]);
    assert_eq!(ran.cases, NONE);

    // A top-level item that the end of the file cuts short owns that
    // error, with or without a line break after it.
    for ending in ["", "\n"] {
        let text = format!(
            "module m\nfn open() {{\n# expect-error(parse, \"syntax.expected_token\")\nstep(){ending}"
        );
        let ran = run_all(&[("cases.hgl", text.as_str())]);
        assert_eq!(ran.stopped, NONE, "{ending:?}");
        // With a line break the error is on the line after `step()`,
        // and the annotation applies to the line of `step()`.
        let expected: &[&str] = if ending.is_empty() {
            &["passed rejection cases.hgl:2 fn open"]
        } else {
            &[
                "failed rejection cases.hgl:2 fn open",
                "  missing parse/syntax.expected_token cases.hgl:4",
                "  unexpected parse/syntax.expected_token cases.hgl:5:1",
            ]
        };
        assert_eq!(ran.cases, expected, "{ending:?}");
    }
}

// ---- TST-13: owners left out, and matching ----

/// TST-13: "Exclude all rejection owners from executable source. Check
/// each selected owner independently alongside the surviving module".
#[test]
fn each_owner_is_checked_alone_beside_the_surviving_source() {
    let ran = run_all(&[
        (
            "cases.hgl",
            concat!(
                "module examples.rejection\n",
                "fn scale() {}\n",
                // Two owners with one name: each is checked without the
                // other, so neither is a duplicate of the other.
                "# expect-error(parse, \"string.unknown_escape\")\n",
                "const fn twin() -> str => \"a\\q\"\n",
                "# expect-error(parse, \"string.unknown_escape\")\n",
                "const fn twin() -> str => \"b\\q\"\n",
                "test survives { assert true }\n",
            ),
        ),
        (
            "part.hgl",
            concat!(
                "module examples.rejection part more\n",
                "# expect-error(parse, \"string.unclosed\")\n",
                "const fn other() -> str => \"a\n",
            ),
        ),
    ]);
    assert_eq!(ran.stopped, NONE);
    assert_eq!(
        ran.cases,
        [
            "passed rejection cases.hgl:4 const fn twin",
            "passed rejection cases.hgl:6 const fn twin",
            "passed rejection part.hgl:3 const fn other",
            "not-run executable examples.rejection::survives",
        ]
    );
}

/// TST-13: "Every primary diagnostic must match one annotation exactly by
/// category, code, file, and next-line location; every annotation must
/// match once." TST-14: "A rejection mismatch fails that case while other
/// valid cases continue."
#[test]
fn errors_and_annotations_match_one_to_one() {
    let ran = run_all(&[(
        "cases.hgl",
        concat!(
            "module m\n",
            // The wrong code.
            "# expect-error(parse, \"string.unclosed\")\n",
            "const fn code() -> str => \"a\\q\"\n",
            // The wrong line: the error is on the line after.
            "# expect-error(parse, \"string.unknown_escape\")\n",
            "const fn line() -> str =>\n",
            "    \"a\\q\"\n",
            // One error, expected twice.
            "# expect-error(parse, \"string.unknown_escape\")\n",
            "# expect-error(parse, \"string.unknown_escape\")\n",
            "const fn twice() -> str => \"a\\q\"\n",
            // Two errors, one expected.
            "# expect-error(parse, \"string.unknown_escape\")\n",
            "const fn more() -> str => \"a\\q\" + \"b\\q\"\n",
            // Nothing wrong with it.
            "# expect-error(parse, \"string.unknown_escape\")\n",
            "const fn accepted() -> str => \"a\"\n",
            // And one that is right, after the others.
            "# expect-error(parse, \"string.unknown_escape\")\n",
            "const fn right() -> str => \"a\\q\"\n",
        ),
    )]);
    assert_eq!(ran.stopped, NONE);
    assert_eq!(
        ran.cases,
        [
            "failed rejection cases.hgl:3 const fn code",
            "  missing parse/string.unclosed cases.hgl:3",
            "  unexpected parse/string.unknown_escape cases.hgl:3:29",
            "failed rejection cases.hgl:5 const fn line",
            "  missing parse/string.unknown_escape cases.hgl:5",
            "  unexpected parse/string.unknown_escape cases.hgl:6:7",
            "failed rejection cases.hgl:9 const fn twice",
            "  missing parse/string.unknown_escape cases.hgl:9",
            "failed rejection cases.hgl:11 const fn more",
            "  unexpected parse/string.unknown_escape cases.hgl:11:37",
            "failed rejection cases.hgl:13 const fn accepted",
            "  missing parse/string.unknown_escape cases.hgl:13",
            "passed rejection cases.hgl:15 const fn right",
        ]
    );
    assert!(!ran.report.succeeded());
    assert_eq!(
        ran.report
            .count(CaseKind::Rejection, |outcome| *outcome == Outcome::Passed),
        1
    );
}

/// TST-13: "Check each selected owner independently alongside the
/// surviving module". With the owner left out the module is accepted;
/// put back, the owner is rejected, and that error is its case's. These
/// are the rejections the units-and-names chapter lists for its own
/// illustration, each with the code the catalogue gives it.
#[test]
fn a_name_or_module_rejection_is_a_case() {
    let ran = run_all(&[(
        "defaults.hgl",
        concat!(
            "module examples.defaults\n",
            "\n",
            // An import owns a case too.
            "# expect-error(module, \"import.unknown_module\")\n",
            "use examples.missing::{thing}\n",
            "\n",
            "const fn seed() -> i64 => 7\n",
            "const fn choose(base: i64, value: i64 = seed()) -> i64 => base + value\n",
            "\n",
            // UNIT-7: "a default of `base` in place of `seed()` reads a
            // formal argument".
            "# expect-error(name, \"default.formal_argument\")\n",
            "const fn defaulted(base: i64, value: i64 = base) -> i64 => value\n",
            "\n",
            // UNIT-12: "a first parameter named `seed`".
            "# expect-error(name, \"name.duplicate\")\n",
            "const fn first(seed: i64) -> i64 => seed\n",
            "\n",
            // UNIT-12: "a local named `base` in a block body".
            "const fn local(base: i64) -> i64 {\n",
            "    # expect-error(name, \"name.duplicate\")\n",
            "    let base = 1\n",
            "    return base\n",
            "}\n",
            "\n",
            // UNIT-12: "a local declared in two cases of one `switch`".
            "const fn cases(base: i64) -> i64 {\n",
            "    switch base {\n",
            "        case 1:\n",
            "            let chosen = 1\n",
            "        case 2:\n",
            "            # expect-error(name, \"name.duplicate\")\n",
            "            let chosen = 2\n",
            "    }\n",
            "    return base\n",
            "}\n",
            "\n",
            // UNIT-12, TST-2: "a test helper named `choose`".
            "test {\n",
            "    # expect-error(name, \"name.duplicate\")\n",
            "    const fn choose() -> i64 => 1\n",
            "}\n",
            "\n",
            // TIME-8: "`inject` names no other", and SDOC-1: a
            // documentation comment that documents nothing.
            "fn node(value: i64) -> i64 {\n",
            "    # expect-error(injectable, \"inject.unknown_capability\")\n",
            "    inject banana\n",
            "    # expect-error(parse, \"doc.unattached\")\n",
            "    /** In a body. */\n",
            "    when { return value }\n",
            "}\n",
            "\n",
            "test defining_closure {\n",
            "    assert choose(1) == 8\n",
            "}\n",
        ),
    )]);
    assert_eq!(ran.stopped, NONE);
    assert_eq!(
        ran.cases,
        [
            "passed rejection defaults.hgl:4 use examples.missing",
            "passed rejection defaults.hgl:10 const fn defaulted",
            "passed rejection defaults.hgl:13 const fn first",
            "passed rejection defaults.hgl:15 const fn local",
            "passed rejection defaults.hgl:21 const fn cases",
            "passed rejection defaults.hgl:34 const fn choose",
            "passed rejection defaults.hgl:37 fn node",
            "not-run executable examples.defaults::defining_closure",
        ]
    );
}

/// TST-13: an error the owner causes is its case's "wherever it is
/// located", and must meet one of the owner's annotations. One with a
/// different code, or one the owner causes at another declaration, does
/// not, and the case fails.
#[test]
fn an_error_of_the_analyser_must_meet_an_annotation_of_its_owner() {
    let ran = run_all(&[
        (
            "cases.hgl",
            concat!(
                "module m\n",
                "fn scale() {}\n",
                // Put back, this is a duplicate, which is not what its
                // annotation expects.
                "# expect-error(parse, \"syntax.expected_token\")\n",
                "fn scale() {}\n",
                // Put back, this import makes a helper's name one its module
                // imports. The error is at the helper, which has no
                // annotation of this owner.
                "test {\n",
                "    fn shared() {}\n",
                "}\n",
            ),
        ),
        ("lib.hgl", "module lib\nexport fn shared() {}\n"),
        (
            "part.hgl",
            concat!(
                "module m part second\n",
                "# expect-error(name, \"name.duplicate\")\n",
                "use lib::{shared}\n",
            ),
        ),
    ]);
    assert_eq!(ran.stopped, NONE);
    assert_eq!(
        ran.cases,
        [
            "failed rejection cases.hgl:4 fn scale",
            "  missing parse/syntax.expected_token cases.hgl:4",
            "  unexpected name/name.duplicate cases.hgl:4:4",
            "failed rejection part.hgl:3 use lib",
            "  missing name/name.duplicate part.hgl:3",
            "  unexpected name/name.duplicate cases.hgl:6:8",
        ]
    );
}

/// TST-14: "surviving-source errors prevent execution." An error with no
/// annotation, in an item that owns no case, is one.
#[test]
fn an_error_in_the_surviving_source_stops_the_run() {
    let ran = run_all(&[(
        "cases.hgl",
        concat!(
            "module m\n",
            "# expect-error(parse, \"string.unknown_escape\")\n",
            "const fn expected() -> str => \"a\\q\"\n",
            "const fn stray() -> str => \"b\\q\"\n",
            "fn twice() {}\n",
            "fn twice() {}\n",
        ),
    )]);
    assert_eq!(
        ran.stopped,
        [
            "cases.hgl:4:30 parse/string.unknown_escape",
            "cases.hgl:6:4 name/name.duplicate",
        ]
    );
    assert_eq!(ran.cases, NONE);
    assert!(ran.report.prevented());
}

/// TST-13: "Use the primary range's first character and the EOF
/// position's line for EOF errors."
#[test]
fn an_error_at_the_end_of_the_file_is_on_its_last_line() {
    // The block is never closed: the parser reports the missing `}` at
    // the end of the file, which is the empty line after the annotation.
    let ran = run_all(&[(
        "cases.hgl",
        concat!(
            "module m\n",
            "test survives { assert true }\n",
            "fn open() {\n",
            "    # expect-error(parse, \"syntax.expected_token\")\n",
        ),
    )]);
    assert_eq!(ran.stopped, NONE);
    assert_eq!(
        ran.cases,
        [
            "passed rejection cases.hgl:3 fn open",
            "not-run executable m::survives",
        ]
    );

    // The primary range's first character: a comment that never closes
    // is reported where it opens, though it runs to the end of the file.
    let ran = run_all(&[(
        "cases.hgl",
        concat!(
            "module m\n",
            "# expect-error(parse, \"comment.unclosed\")\n",
            "# expect-error(parse, \"syntax.expected_token\")\n",
            "fn open() { /* never closed",
        ),
    )]);
    assert_eq!(ran.stopped, NONE);
    assert_eq!(ran.cases, ["passed rejection cases.hgl:4 fn open"]);
}

/// TST-12: "Unnamed context/module headers cannot own a case", and
/// TST-14: "ambiguous boundaries, or surviving-source errors prevent
/// execution." An unnamed context that the end of the file cuts short
/// is what reaches the last line. The item before it does not own that
/// line, nor does a helper inside it.
#[test]
fn the_end_of_the_file_belongs_to_the_item_that_ends_the_file() {
    // The context is left open after an annotated function. Its missing
    // `}` is an error of the surviving source, not one of the function's.
    let ran = run_all(&[(
        "cases.hgl",
        concat!(
            "module m\n",
            "# expect-error(parse, \"string.unknown_escape\")\n",
            "const fn f() -> str => \"a\\q\"\n",
            "test {\n",
            "    fn helper() {}\n",
        ),
    )]);
    assert_eq!(ran.stopped, ["cases.hgl:6:1 parse/syntax.expected_token"]);
    assert_eq!(ran.cases, NONE);

    // An annotation cannot give that line to the function either.
    let ran = run_all(&[(
        "cases.hgl",
        concat!(
            "module m\n",
            "# expect-error(parse, \"string.unknown_escape\")\n",
            "const fn f() -> str => \"a\\q\"\n",
            "test {\n",
            "    fn helper() {}\n",
            "# expect-error(parse, \"syntax.expected_token\")\n",
        ),
    )]);
    assert_eq!(
        ran.stopped,
        [
            "cases.hgl:6:1 this `expect-error` annotation applies to a line of no import, \
             declaration or named test (TST-12)",
            "cases.hgl:7:1 parse/syntax.expected_token",
        ]
    );
    assert_eq!(ran.cases, NONE);

    // Nor does a helper left open inside the context own it: the
    // context is open as well, and which of the two the end of the
    // file cut short is not for the runner to guess.
    let ran = run_all(&[(
        "cases.hgl",
        concat!(
            "module m\n",
            "test {\n",
            "    # expect-error(parse, \"string.unknown_escape\")\n",
            "    const fn helper() -> str => \"a\\q\"\n",
            "    fn open() {\n",
        ),
    )]);
    assert_eq!(ran.stopped, ["cases.hgl:6:1 parse/syntax.expected_token"]);
    assert_eq!(ran.cases, NONE);

    // A complete item that ends the file owns the last line, so an
    // annotation there is read, and fails: nothing is wrong with it.
    let ran = run_all(&[(
        "cases.hgl",
        concat!(
            "module m\n",
            "fn whole() {}\n",
            "# expect-error(parse, \"syntax.expected_token\")\n",
        ),
    )]);
    assert_eq!(ran.stopped, NONE);
    assert_eq!(
        ran.cases,
        [
            "failed rejection cases.hgl:2 fn whole",
            "  missing parse/syntax.expected_token cases.hgl:4",
        ]
    );
}

// ---- TST-14 and TST-15: selection and what is reported ----

/// A program with two tests to run, a test that is a rejection case, and
/// a declaration that is one.
const SELECTABLE: [(&str, &str); 2] = [
    (
        "a.hgl",
        concat!(
            "module one\n",
            "test first { assert true }\n",
            "test rejected {\n",
            "    # expect-error(parse, \"string.unknown_escape\")\n",
            "    assert \"a\\q\" == \"a\"\n",
            "}\n",
            "# expect-error(parse, \"string.unknown_escape\")\n",
            "const fn declared() -> str => \"a\\q\"\n",
        ),
    ),
    (
        "b.hgl",
        "module two\ntest first { assert true }\ntest second { assert true }\n",
    ),
];

/// TST-15: "Report each selected case's identity, kind, and outcome;
/// declaration-owned identities include original file and owner-start
/// line." TST-14: "Report executed/rejected counts separately".
#[test]
fn every_case_is_reported_with_its_identity_kind_and_outcome() {
    let ran = run_all(&SELECTABLE);
    assert_eq!(ran.stopped, NONE);
    assert_eq!(
        ran.cases,
        [
            "passed rejection one::rejected",
            "passed rejection a.hgl:8 const fn declared",
            "not-run executable one::first",
            "not-run executable two::first",
            "not-run executable two::second",
        ]
    );
    let passed = |outcome: &Outcome| *outcome == Outcome::Passed;
    assert_eq!(ran.report.count(CaseKind::Rejection, passed), 2);
    assert_eq!(ran.report.count(CaseKind::Executable, passed), 0);
    assert_eq!(
        ran.report.count(CaseKind::Executable, |outcome| matches!(
            outcome,
            Outcome::NotRun(_)
        )),
        3
    );
    // TST-15: "Success requires every selected executable and required
    // rejection case to pass." A test that was not run did not pass.
    assert!(!ran.report.succeeded());
}

/// TST-15: "A rejection-only run is valid."
#[test]
fn a_rejection_only_run_selects_no_test_to_run() {
    let selection = Selection {
        tests: Vec::new(),
        rejections_only: true,
    };
    let ran = run_selected(&SELECTABLE, &selection);
    assert_eq!(
        ran.cases,
        [
            "passed rejection one::rejected",
            "passed rejection a.hgl:8 const fn declared",
        ]
    );
    assert!(ran.report.succeeded());
}

/// TST-14: "Named selectors include rejection tests; declaration-owned
/// cases always run."
#[test]
fn a_selector_names_tests_and_declaration_owned_cases_always_run() {
    let select = |tests: &[&str]| {
        let selection = Selection {
            tests: tests.iter().map(|name| name.to_string()).collect(),
            rejections_only: false,
        };
        run_selected(&SELECTABLE, &selection).cases
    };
    // A bare name selects the test of that name in every module.
    assert_eq!(
        select(&["first"]),
        [
            "passed rejection a.hgl:8 const fn declared",
            "not-run executable one::first",
            "not-run executable two::first",
        ]
    );
    // A name with its module selects one.
    assert_eq!(
        select(&["two::first", "rejected"]),
        [
            "passed rejection one::rejected",
            "passed rejection a.hgl:8 const fn declared",
            "not-run executable two::first",
        ]
    );
}

/// TST-14: "Unknown selectors reject."
#[test]
fn an_unknown_selector_stops_the_run() {
    let selection = Selection {
        tests: vec![
            "first".to_string(),
            "third".to_string(),
            "one::second".to_string(),
        ],
        rejections_only: false,
    };
    let ran = run_selected(&SELECTABLE, &selection);
    assert_eq!(
        ran.stopped,
        [
            "no test is named `third` (TST-14)",
            "no test is named `one::second` (TST-14)",
        ]
    );
    assert_eq!(ran.cases, NONE);
}

/// TST-15: "A run with no cases supplies no conformance evidence." It is
/// not a failure either.
#[test]
fn a_program_with_no_cases_has_none_to_report() {
    let ran = run_all(&[("m.hgl", "module m\nfn f() {}\n")]);
    assert_eq!(ran.stopped, NONE);
    assert_eq!(ran.cases, NONE);
    assert!(ran.report.succeeded());
    assert!(ran.report.cases.is_empty());
}

/// LEX-15: a line break is LF, CRLF or CR, and they mean the same. The
/// lines an annotation and an error are on are counted alike under each.
#[test]
fn lines_are_counted_alike_under_every_line_break() {
    let text = concat!(
        "module m\n",
        "# expect-error(parse, \"string.unknown_escape\")\n",
        "# expect-error(parse, \"string.unclosed\")\n",
        "const fn two() -> str => \"a\\q\" + \"b\n",
        "test rejected {\n",
        "    # expect-error(parse, \"string.unknown_escape\")\n",
        "    assert \"a\\q\" == \"a\"\n",
        "}\n",
        "fn open() {\n",
        "    # expect-error(parse, \"syntax.expected_token\")\n",
    );
    let expected = [
        "passed rejection cases.hgl:4 const fn two",
        "passed rejection m::rejected",
        "passed rejection cases.hgl:9 fn open",
    ];
    for line_break in ["\n", "\r\n", "\r"] {
        let text = text.replace('\n', line_break);
        let ran = run_all(&[("cases.hgl", text.as_str())]);
        assert_eq!(ran.stopped, NONE, "{line_break:?}");
        assert_eq!(ran.cases, expected, "{line_break:?}");
    }
}
