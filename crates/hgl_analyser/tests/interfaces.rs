//! The analyser against the rules on what checking an interface reads:
//! a native declaration and no provider (NATIVE-7), and a
//! documentation comment, whose keys it checks (SDOC-3) and which
//! changes nothing else (SDOC-6). Each test cites its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{check, module};
use hgl_analyser::Phase;

/// No lines.
const NONE: [&str; 0] = [];

// ---- NATIVE-7: an interface is checked without a provider ----

/// The native-boundaries chapter's shared interface, with no part that
/// implements it, and functions that use it.
const INTERFACE: &str = concat!(
    "module examples.native\n",
    "export native type Handle {\n",
    "    size: i64\n",
    "}\n",
    "native fn accumulate(value: i64) -> i64\n",
    "export native const fn describe(value: i64) -> str\n",
    "operator total<T>(lhs: T, rhs: T) -> T\n",
    "native impl const fn total(lhs: i64, rhs: i64) -> i64\n",
    "native impl const fn total(lhs: f64, rhs: f64) -> f64\n",
    "const fn larger(handle: Handle, limit: i64) -> bool => handle.size > limit\n",
    "const fn described(value: i64) -> str => describe(total(value, 1))\n",
    "fn running(value: i64) -> i64 => accumulate(value)\n",
);

/// NATIVE-7: "Checking an interface needs no executable provider
/// loading." A program of native declarations and no implementation part
/// is checked whole: a property read, a native function's call, an
/// operator's native candidate selected, each from what is declared.
/// `running` is a graph, `accumulate` a temporal function by its
/// declaration (NATIVE-1).
#[test]
fn an_interface_is_checked_without_a_provider() {
    let checked = check(&[("native.hgl", INTERFACE)]);
    assert_eq!(checked.summary(), NONE);
    let module = checked.program.module("examples.native").unwrap();
    let running = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == "running")
        .unwrap();
    assert_eq!(module.phases[running], Some(Phase::Graph));
}

/// NATIVE-7: "An absent binding is an implementation/admission failure,
/// never permission to change the role". With no provider, the
/// temporal `accumulate` is still a temporal function, which a value
/// function does not call (`call.role`, FN-3), and `total` still has
/// only the candidates declared (`operator.no_candidate`, GEN-5).
#[test]
fn an_absent_provider_changes_no_role() {
    let text = format!(
        "{INTERFACE}{}",
        concat!(
            "const fn wrong(value: i64) -> i64 => accumulate(value)\n",
            "const fn mixed(value: str) -> str => total(value, value)\n",
        )
    );
    let checked = check(&[("native.hgl", &text)]);
    assert_eq!(
        checked.summary(),
        [
            "native.hgl:13:38 function-kind/call.role FN-3",
            "native.hgl:14:38 operator/operator.no_candidate GEN-5",
        ]
    );
}

// ---- SDOC-3: structured keys name what the declaration has ----

/// SDOC-3: "Structured keys must name real parameters, generics,
/// requirements, or property domains. Missing descriptions are
/// permitted." The source-documentation chapter's accepted
/// illustration, and keys of each kind: a parameter, a pack, a generic
/// parameter, a generic value parameter, a struct's generic parameter.
/// A key with no description is a key; a line indented deeper than four
/// spaces continues a description; a same-line summary sets no
/// indentation (SDOC-2); `Returns:` and an unknown heading are prose.
#[test]
fn structured_keys_name_the_declarations_parameters() {
    let checked = check(&[(
        "documented.hgl",
        concat!(
            "module examples.documented\n",
            "\n",
            "/**\n",
            "Double a value.\n",
            "\n",
            "Args:\n",
            "    value: Input value.\n",
            "\n",
            "Returns:\n",
            "    Twice the input.\n",
            "*/\n",
            "const fn doubled(value: i64) -> i64 => value * 2\n",
            "\n",
            "/** A side of the order book. */\n",
            "export enum Side {\n",
            "    /** The bid side. */\n",
            "    buy,\n",
            "    /** The offer side. */\n",
            "    sell,\n",
            "}\n",
            "\n",
            "/** Sums values.\n",
            "    Args:\n",
            "        first:\n",
            "        rest: Every other value,\n",
            "            each: of `T`.\n",
            "    Type Args:\n",
            "        T: Their type.\n",
            "        N: How many to keep.\n",
            "    Returns:\n",
            "        total: not a key.\n",
            "    Arguments:\n",
            "        other: prose.\n",
            "*/\n",
            "const fn summed<T, const N: i64>(first: T, rest: ...T) -> T => first\n",
            "\n",
            "/**\n",
            "A pair.\n",
            "\n",
            "Type Args:\n",
            "    T: What it holds.\n",
            "*/\n",
            "struct Pair<T> {\n",
            "    left: T\n",
            "    right: T\n",
            "}\n",
        ),
    )]);
    assert_eq!(checked.summary(), NONE);
}

/// SDOC-3: the chapter's deliberate rejection, "`Args:` names a
/// parameter absent from that declaration", and the same of `Type
/// Args:`. A declaration with no parameters, an enum's member here,
/// has none for `Args:` to name. The catalogue has no row for it, and
/// the code is local.
#[test]
fn a_key_that_names_nothing_is_rejected() {
    let checked = module(concat!(
        "/**\n",
        "Double a value.\n",
        "\n",
        "Args:\n",
        "    value: Input value.\n",
        "    factor: Absent.\n",
        "\n",
        "Type Args:\n",
        "    T: Absent.\n",
        "*/\n",
        "const fn doubled(value: i64) -> i64 => value * 2\n",
        "enum Side {\n",
        "    /** The bid side.\n",
        "\n",
        "    Args:\n",
        "        price: Absent.\n",
        "    */\n",
        "    buy,\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:7:5 local:name/doc.unknown_key SDOC-3",
            "m.hgl:10:5 local:name/doc.unknown_key SDOC-3",
            "m.hgl:17:9 local:name/doc.unknown_key SDOC-3",
        ]
    );
}

// ---- SDOC-6: documentation is no input to checking ----

/// A program with an operator of two candidates, each documented, a
/// call that selects one, and a node; `doc` is the text of each comment,
/// of one length whatever it says, so that every position is the same.
fn documented(doc: &str) -> String {
    format!(
        concat!(
            "module m\n",
            "/** {doc} */\n",
            "operator scaled<T>(value: T) -> T\n",
            "/** {doc} */\n",
            "impl const fn scaled(value: i64) -> i64 => value * 2\n",
            "/** {doc} */\n",
            "impl const fn scaled(value: f64) -> f64 => value * 2.0\n",
            "/** {doc} */\n",
            "const fn used(value: f64) -> f64 => scaled(value)\n",
            "/** {doc} */\n",
            "fn kept(value: i64) -> i64 {{\n",
            "    when {{ return scaled(value) }}\n",
            "}}\n",
        ),
        doc = doc
    )
}

/// SDOC-6: "Documentation-only changes do not affect execution, overload
/// resolution, or provider semantics." SDOC-3: "Documentation declares
/// no new type, capability, property, constraint, or runtime behavior."
/// Two programs that differ in their documentation alone, one of whose
/// comments reads as a signature, a constraint and a section would,
/// analyse to the same signatures, phases, typed bodies and candidates
/// selected, with nothing reported.
#[test]
fn documentation_is_no_input_to_checking_or_selection() {
    let plain = documented("Scales a value, as its candidates say: twice it......");
    let suggestive = documented("requires false -> str. Args: value: f64. Ticks: never");
    assert_eq!(plain.len(), suggestive.len());
    let plain = check(&[("m.hgl", &plain)]);
    let suggestive = check(&[("m.hgl", &suggestive)]);
    assert_eq!(plain.summary(), NONE);
    assert_eq!(suggestive.summary(), NONE);
    let (plain, suggestive) = (
        plain.program.module("m").unwrap(),
        suggestive.program.module("m").unwrap(),
    );
    assert_eq!(
        format!("{:?}", plain.details),
        format!("{:?}", suggestive.details)
    );
    assert_eq!(plain.phases, suggestive.phases);
    assert_eq!(plain.bodies, suggestive.bodies);
    assert_eq!(
        format!("{:?}", plain.defaults),
        format!("{:?}", suggestive.defaults)
    );
}
