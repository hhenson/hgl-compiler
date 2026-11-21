//! The analyser against the rule on module constants (UNIT-13), and the
//! rules that name a constant: FN-1, GEN-8, TYPE-10. Each test cites its
//! rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::check;
use hgl_analyser::hir::ExprKind;

/// No lines.
const NONE: [&str; 0] = [];

/// UNIT-13: "A constant, `const name: T = expression`, names one
/// ordinary value of the type T. Its expression is a const expression
/// (EXPR-5) checked against T: of literals, enum members, constructors,
/// other constants, and calls of value functions and operators on
/// those ... It is referenced forward as any module declaration is
/// (UNIT-6)".
#[test]
fn a_constant_names_one_value_of_its_type() {
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "enum Side { buy, sell }\n",
            "struct Quote { bid: f64 }\n",
            "const fn doubled(value: i64) -> i64 => value * 2\n",
            "export const scale: i64 = 2\n",
            // Another constant, written after this one; an operator.
            "const first: i64 = 3 * late + 1\n",
            "const late: i64 = scale\n",
            "const side: Side = .buy\n",
            "const quote: Quote = Quote(bid: 1.0)\n",
            "const twice: i64 = doubled(scale)\n",
            "const wide: f64 = 1\n",
            "const pace: duration = 5m\n",
            "const names: list<str> = [\"a\", \"b\"]\n",
            "const none: i64? = null\n",
        ),
    )]);
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    let module = checked.program.module("m").unwrap();
    // Each constant has the type it is written with, and its expression
    // read as a value of that type.
    let typed: Vec<String> = module
        .declarations
        .iter()
        .zip(&module.constants)
        .filter_map(|(declaration, value)| {
            let value = value.as_ref()?;
            Some(format!("{}: {}", declaration.name, value.ty))
        })
        .collect();
    assert_eq!(
        typed,
        [
            "scale: i64",
            "first: i64",
            "late: i64",
            "side: Side",
            "quote: Quote",
            "twice: i64",
            "wide: f64",
            "pace: duration",
            "names: list<str>",
            "none: i64?",
        ]
    );
    // `late` is read by its name: a constant, of this module.
    let late = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == "late")
        .unwrap();
    let scale = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == "scale")
        .unwrap();
    assert!(matches!(
        module.constants[late].as_ref().unwrap().kind,
        ExprKind::Constant { declaration, .. } if declaration == scale
    ));
    // UNIT-3: it is private unless exported.
    assert!(module.declarations[scale].exported);
    assert!(!module.declarations[late].exported);
}

/// UNIT-13: "A constant is read by its name wherever an ordinary value
/// is. It is a constant expression where a `const` parameter requires
/// one (FN-1), and a compile-time constant where a generic value
/// argument (GEN-8) ... requires one." TYPE-10: a list's size is one
/// too. UNIT-2: an exported constant is imported as any declaration is.
#[test]
fn a_constant_is_read_wherever_a_value_or_a_constant_is() {
    let checked = check(&[
        (
            "m.hgl",
            concat!(
                "module m\n",
                "export const scale: i64 = 2\n",
                "export const size: i64 = scale + 1\n",
                "const hidden: i64 = 7\n",
            ),
        ),
        (
            "n.hgl",
            concat!(
                "module n\n",
                "use m::{scale, size}\n",
                "use m as other\n",
                // In a value function, and as a default.
                "const fn scaled(value: i64, by: i64 = scale) -> i64 => value * by * other::scale\n",
                // A list's size, and a generic value argument.
                "const fn sized(values: list<i64, size>) -> i64 => 1\n",
                "const fn count<const N: i64>(value: i64) -> i64 => value + N\n",
                "const fn uses() -> i64 => sized([1, 2, 3]) + count<size>(1)\n",
                // A generic value argument by name, and a tuple's index.
                "const fn kept<const N: i64>(values: list<i64, N>) -> i64 => 1\n",
                "const fn named() -> i64 => kept([1, 2, 3], N: size)\n",
                "const fn second(pair: tuple<str, i64>) -> i64 => pair[size - scale]\n",
                // What a `const` parameter takes, in a graph.
                "fn node(value: i64, const factor: i64) -> i64 {\n",
                "    when { return value * factor }\n",
                "}\n",
                "fn wired(value: i64) -> i64 => node(value, scale)\n",
                "test read {\n",
                "    assert scaled(1) == 4\n",
                "    assert eval wired(value: [1]) == [2]\n",
                "}\n",
            ),
        ),
    ]);
    assert_eq!(checked.summary(), NONE, "{}", checked.report);

    let wrong = check(&[
        (
            "m.hgl",
            concat!(
                "module m\n",
                "export const size: i64 = 3\n",
                "const hidden: i64 = 7\n",
            ),
        ),
        (
            "n.hgl",
            concat!(
                "module n\n",
                "use m::{size}\n",
                // UNIT-2, UNIT-3: a constant that is not exported.
                "use m::{hidden}\n",
                "const fn sized(values: list<i64, size>) -> i64 => 1\n",
                // The size is the constant's value: 3.
                "const fn short() -> i64 => sized([1, 2])\n",
                // A constant is a value, and no type.
                "const fn typed(value: size) -> i64 => 1\n",
                // By name the size is 3 too, and a tuple's index is the
                // constant's value: 3 is past a pair's end.
                "const fn kept<const N: i64>(values: list<i64, N>) -> i64 => 1\n",
                "const fn named() -> i64 => kept([1, 2], N: size)\n",
                "const fn past(pair: tuple<str, i64>) -> i64 => pair[size]\n",
            ),
        ),
    ]);
    assert_eq!(
        wrong.summary(),
        [
            "n.hgl:3:9 module/import.private UNIT-2",
            "n.hgl:5:34 type/type.mismatch TYPE-10",
            "n.hgl:6:23 type/type.not_a_type UNIT-10",
            "n.hgl:8:33 type/type.mismatch TYPE-10",
            "n.hgl:9:53 type/tuple.index EXPR-7",
        ],
        "{}",
        wrong.report
    );
}

/// UNIT-13, GEN-6: a constant given as an operator's option is the
/// value a candidate's condition compares, as the same value written
/// out is.
#[test]
fn a_constant_given_as_an_option_selects_as_its_value_does() {
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "const policy: ZeroPolicy = .error\n",
            "const fail: ZeroPolicy = .fail\n",
            // `.error` selects the candidate that gives a result, and
            // `.fail` the one that gives the number.
            "const fn checked(a: f64, b: f64) -> result<f64, error> => a /{zero: policy} b\n",
            "const fn failing(a: f64, b: f64) -> f64 => a /{zero: fail} b\n",
            // ERR-15: the f64 place takes the selected candidate's value
            // and raises its error, for either spelling of the policy.
            "const fn plain(a: f64, b: f64) -> f64 => a /{zero: policy} b\n",
            "const fn written(a: f64, b: f64) -> f64 => a /{zero: .error} b\n",
        ),
    )]);
    assert_eq!(
        checked.summary(),
        Vec::<String>::new(),
        "{}",
        checked.report
    );
}

/// UNIT-13: the expression is "checked against T ... It reads no
/// parameter, local, temporal value, or capability." The type is an
/// ordinary value type.
#[test]
fn a_constant_written_wrong_is_reported() {
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "fn beats() -> i64 => 1\n",
            // Another type than the one written.
            "const text: i64 = \"a\"\n",
            // A temporal value.
            "const temporal: i64 = beats()\n",
            // No ordinary value type: its expression is held to none.
            "const shaped: signal = 1\n",
            // A name nothing in a constant's scope binds.
            "const fn of(value: i64) -> i64 => value\n",
        ),
    )]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:19 type/type.mismatch EXPR-5",
            "m.hgl:4:23 function-kind/call.role FN-3",
            "m.hgl:5:15 type/type.not_ordinary TYPE-12",
        ],
        "{}",
        checked.report
    );
}

/// UNIT-13: a constant is "a compile-time constant where a generic
/// value argument (GEN-8) ... requires one". Each constant is folded
/// once, however many name it: thirty that each name the last twice are
/// thirty folds, and not two to the thirtieth.
#[test]
fn a_constant_many_others_name_is_folded_once() {
    let mut source = String::from("module m\nconst c0: i64 = 1\n");
    for index in 1..=30 {
        let last = index - 1;
        source.push_str(&format!("const c{index}: i64 = c{last} - c{last} + 1\n"));
    }
    // Each comes to 1, so the size is 1.
    source.push_str("const fn sized(values: list<i64, c30>) -> i64 => 1\n");
    source.push_str("const fn right() -> i64 => sized([1])\n");
    source.push_str("const fn wrong() -> i64 => sized([1, 2])\n");
    let checked = check(&[("m.hgl", &source)]);
    assert_eq!(
        checked.summary(),
        ["m.hgl:35:34 type/type.mismatch TYPE-10"],
        "{}",
        checked.report
    );

    // A chain of constants is followed however long it is: each is
    // worked out once, the ones it names first (UNIT-13). `d40` and
    // `d3` are each 2, so a list of one is given to neither.
    let mut chain = String::from("module m\nconst d0: i64 = 2\n");
    for index in 1..=40 {
        chain.push_str(&format!("const d{index}: i64 = d{}\n", index - 1));
    }
    chain.push_str("const fn far(values: list<i64, d40>) -> i64 => 1\n");
    chain.push_str("const fn near(values: list<i64, d3>) -> i64 => 1\n");
    chain.push_str("const fn uses() -> i64 => far([1]) + near([1])\n");
    let checked = check(&[("m.hgl", &chain)]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:45:31 type/type.mismatch TYPE-10",
            "m.hgl:45:43 type/type.mismatch TYPE-10",
        ],
        "{}",
        checked.report
    );
}

/// UNIT-13: "its value depends on no constant that depends on it."
/// `constant.cycle`: "Constant whose value depends on itself; name".
#[test]
fn a_constant_whose_value_depends_on_itself_is_reported() {
    let checked = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            // Itself.
            "const own: i64 = own + 1\n",
            // Two of one another.
            "const left: i64 = right\n",
            "const right: i64 = left\n",
            // Through a function its expression calls, and through the
            // default of a struct it makes.
            "const looped: i64 = through()\n",
            "const fn through() -> i64 => looped\n",
            "const made: i64 = Held().value\n",
            "struct Held { value: i64 = made }\n",
            // A function that calls itself is no constant's cycle, and
            // one that reads a constant makes none.
            "const depth: i64 = 3\n",
            "const fn down(value: i64) -> i64 => if value > depth { down(value - 1) } else { value }\n",
            "const reached: i64 = down(9)\n",
        ),
    )]);
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:7 name/constant.cycle UNIT-13",
            "m.hgl:3:7 name/constant.cycle UNIT-13",
            "m.hgl:4:7 name/constant.cycle UNIT-13",
            "m.hgl:5:7 name/constant.cycle UNIT-13",
            "m.hgl:7:7 name/constant.cycle UNIT-13",
        ],
        "{}",
        checked.report
    );
    // FN-4: of a temporal and a value definition of one name, a
    // constant's expression calls the value one. The temporal one may
    // read the constant.
    let paired = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "const answer: i64 = choose()\n",
            "const fn choose() -> i64 => 1\n",
            "fn choose() -> i64 => answer\n",
        ),
    )]);
    assert_eq!(paired.summary(), NONE, "{}", paired.report);

    // UNIT-8: a call reads the default of a parameter where it leaves
    // the parameter out, and of no other. What a default names is a
    // dependence of the calls that use it.
    let defaults = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            // Given by position, and by name: the default is not read.
            "const answer: i64 = choose(1)\n",
            "const fn choose(value: i64 = answer) -> i64 => value\n",
            "const named: i64 = pick(value: 1)\n",
            "const fn pick(value: i64 = named) -> i64 => value\n",
            // Left out: it is.
            "const left: i64 = take()\n",
            "const fn take(value: i64 = left) -> i64 => value\n",
            "const second: i64 = two(1)\n",
            "const fn two(first: i64, other: i64 = second) -> i64 => first + other\n",
            // A default that calls on, with its own argument left out.
            "const far: i64 = outer(1)\n",
            "const fn outer(value: i64, more: i64 = inner()) -> i64 => value + more\n",
            "const fn inner(value: i64 = far) -> i64 => value\n",
        ),
    )]);
    assert_eq!(
        defaults.summary(),
        [
            "m.hgl:6:7 name/constant.cycle UNIT-13",
            "m.hgl:8:7 name/constant.cycle UNIT-13",
            "m.hgl:10:7 name/constant.cycle UNIT-13",
        ],
        "{}",
        defaults.report
    );

    // OPS-2: a call of an operator, by its name or as a symbol, is of
    // the candidate its operands select. That candidate is what the
    // value is worked out through, and no other.
    let candidates = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "operator choose<T>(value: T) -> T\n",
            "impl const fn choose(value: i64) -> i64 => answer\n",
            "impl const fn choose(value: str) -> str => value\n",
            // The `i64` candidate reads `answer`: a cycle. The `str`
            // one does not, though it is of the same operator.
            "const answer: i64 = choose(1)\n",
            "const label: str = choose(\"a\")\n",
            // A symbol selects a candidate too.
            "struct Money { cents: i64 }\n",
            "impl const fn add_(lhs: Money, rhs: Money) -> Money => Money(cents: lhs.cents + total.cents)\n",
            "const total: Money = Money(cents: 1) + Money(cents: 2)\n",
            // `1 + 2` selects core's candidate, and not the one above.
            "const three: i64 = 1 + 2\n",
        ),
    )]);
    assert_eq!(
        candidates.summary(),
        [
            "m.hgl:5:7 name/constant.cycle UNIT-13",
            "m.hgl:9:7 name/constant.cycle UNIT-13",
        ],
        "{}",
        candidates.report
    );

    // GEN-8: a constant given as a generic value argument is read to
    // give it, in angle brackets or by name, and one written as a size
    // is read to have the type.
    let generic = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "const fn identity<const N: i64>() -> i64 => N\n",
            "const fn kept<const N: i64>(values: list<i64, N>) -> i64 => N\n",
            "const bracketed: i64 = identity<bracketed>()\n",
            "const by_name: i64 = kept([1], N: by_name)\n",
            // The size of the parameter's type is the constant that the
            // call of the function is to give.
            "const fn sized(values: list<i64, wide>) -> i64 => 1\n",
            "const wide: i64 = sized([1])\n",
            // Given another constant, there is no cycle.
            "const plain: i64 = 2\n",
            "const fine: i64 = identity<plain>()\n",
        ),
    )]);
    assert_eq!(
        generic.summary(),
        [
            "m.hgl:4:7 name/constant.cycle UNIT-13",
            "m.hgl:5:7 name/constant.cycle UNIT-13",
            "m.hgl:7:7 name/constant.cycle UNIT-13",
        ],
        "{}",
        generic.report
    );

    // A size that names such a constant is not followed round it.
    let sized = check(&[(
        "m.hgl",
        concat!(
            "module m\n",
            "const left: i64 = right\n",
            "const right: i64 = left\n",
            "const fn sized(values: list<i64, left>) -> i64 => 1\n",
        ),
    )]);
    assert_eq!(
        sized.summary(),
        [
            "m.hgl:2:7 name/constant.cycle UNIT-13",
            "m.hgl:3:7 name/constant.cycle UNIT-13",
        ],
        "{}",
        sized.report
    );

    // GEN-5: an operator's candidates are found wherever they are
    // declared. So a value is worked out through a module that does not
    // import the constant's: `ops` imports nothing, and its `through`
    // selects the candidate `m` declares, which reads `answer`.
    let across = check(&[
        (
            "ops.hgl",
            concat!(
                "module ops\n",
                "operator choose<T>(value: T) -> T\n",
                "export const fn through() -> i64 => choose(1)\n",
                // Read by `m` and reading nothing of it: no cycle.
                "export const base: i64 = 1\n",
            ),
        ),
        (
            "m.hgl",
            concat!(
                "module m\n",
                // UNIT-3: an operator is public, and GEN-4: a candidate
                // binds it through a selective import.
                "use ops::{base, choose, through}\n",
                "impl const fn choose(value: i64) -> i64 => answer\n",
                "const answer: i64 = through()\n",
                "const above: i64 = base + 1\n",
            ),
        ),
    ]);
    assert_eq!(
        across.summary(),
        ["m.hgl:4:7 name/constant.cycle UNIT-13"],
        "{}",
        across.report
    );
    assert!(
        across.report.contains(
            "the value of `answer` depends on itself, through `choose` and `through` of `ops`"
        ),
        "{}",
        across.report
    );
}
