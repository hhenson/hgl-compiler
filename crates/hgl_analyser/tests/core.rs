//! Calls through core's own module, as the pinned `hgl_core` declares
//! it: `spec/hgl_core/hgl/core/core.hgl` and the native candidates in
//! `spec/hgl_core/hgl/native/scalar_operators.hgl`, which are built into
//! the compiler. The prelude binds its public declarations in every
//! module (UNIT-11).
//!
//! These tests hold the compiler to core's declarations as they are
//! written, so they fail when a pin moves them: that is their purpose.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use common::{Checked, check, locals};
use hgl_analyser::hir::{Callee, ExprKind, Stmt, ValueCategory};
use hgl_analyser::{DeclarationKind, Role};

/// Checks one file, `m.hgl`, of module `m`. The `module` line is added,
/// so the text given starts on line 2. The program has core's module,
/// as every program does: `check` supplies the one the compiler is
/// built with.
fn with_core(text: &str) -> Checked {
    check(&[("m.hgl", &format!("module m\n{text}"))])
}

/// FMT-4 excludes fmt's byte-oriented integer character specifier.
#[test]
fn formatting_rejects_integer_character_specifier() {
    let checked = with_core(concat!(
        "const fn character() -> str => format(\"{:c}\", 233)\n",
        "const fn padded_character() -> str => format(\"{:>3c}\", 256)\n",
        "const fn number() -> str => format(\"{:+d} {:04x}\", 233, 233)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:42 type/format.specifier FMT-4",
            "m.hgl:3:49 type/format.specifier FMT-4",
        ],
        "{}",
        checked.report,
    );
}

/// FMT-4 rejects the zero flag on bool and str, including once GEN-7
/// supplies the argument type at the call outside the generic body.
#[test]
fn formatting_rejects_zero_padding_for_boolean_and_text() {
    let checked = with_core(concat!(
        "const fn direct() -> str => format(\"{:05}\", true)\n",
        "const fn left() -> str => format(\"{:<05}\", true)\n",
        "const fn right() -> str => format(\"{:>05}\", false)\n",
        "const fn string_control() -> str => format(\"{:05}\", \"true\")\n",
        "const fn show<T>(value: T) -> str => format(\"{:05}\", value)\n",
        "const fn bool_instance() -> str => show(true)\n",
        "const fn int_instance() -> str => show(42)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:2:39 type/format.specifier FMT-4",
            "m.hgl:3:37 type/format.specifier FMT-4",
            "m.hgl:4:38 type/format.specifier FMT-4",
            "m.hgl:5:47 type/format.specifier FMT-4",
            "m.hgl:7:36 type/format.specifier FMT-4",
        ],
        "{}",
        checked.report,
    );
}

/// FMT-1, FMT-3, FMT-4 and FMT-9: check a generic format pattern
/// against the argument types of the same instance.
#[test]
fn formatting_checks_a_generic_pattern_and_argument_together() {
    let checked = with_core(concat!(
        "const fn show<const P: str, T>(value: T) -> str => format(P, value)\n",
        "test mark { assert show<\"{*}\">(1) == \"1\" }\n",
        "test hex { assert show<\"{:x}\">(1.5) == \"1.5\" }\n",
        "test character { assert show<\"{:c}\">(65) == \"A\" }\n",
        "test clock { assert show<\"{engine_time}\">(1) == \"1\" }\n",
        "test unused { assert show<\"none\">(1) == \"none\" }\n",
        "test valid { assert show<\"{:x}\">(42) == \"2a\" }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:20 type/format.specifier FMT-9",
            "m.hgl:4:19 type/format.specifier FMT-4",
            "m.hgl:5:25 type/format.specifier FMT-4",
            "m.hgl:6:21 type/format.argument FMT-9",
            "m.hgl:7:22 type/format.unused FMT-3",
        ],
        "{}",
        checked.report,
    );
}

/// FN-1, FMT-3 and GEN-7: report each call whose const input makes the
/// body reject, even when two callers supply the same bad pattern.
#[test]
fn formatting_checks_const_inputs_at_every_call() {
    let checked = with_core(concat!(
        "const fn show(const pattern: str, value: f64) -> str => format(pattern, value)\n",
        "const fn relay<const P: str>(value: f64) -> str => show(P, value)\n",
        "test first { assert show(\"{:x}\", 1.5) == \"\" }\n",
        "test second { assert show(\"{:x}\", 2.5) == \"\" }\n",
        "test relay_call { assert relay<\"{:x}\">(2.5) == \"\" }\n",
        "test good { assert show(\"{:.1f}\", 1.5) == \"1.5\" }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:21 type/format.specifier FMT-4",
            "m.hgl:5:22 type/format.specifier FMT-4",
            "m.hgl:6:26 type/format.specifier FMT-4",
        ],
        "{}",
        checked.report,
    );
}

/// FMT-3, FMT-4 and GEN-7: a concrete selection checks its pattern and
/// reports at the outside call, including through a relay or a capture.
#[test]
fn formatting_checks_deferred_const_inputs_at_the_caller() {
    let checked = with_core(concat!(
        "const fn show<T>(const pattern: str, value: T) -> str => format(pattern, value)\n",
        "const fn relay<T>(const pattern: str, value: T) -> str => show(pattern, value)\n",
        "const fn named<T>(const pattern: str, value: T) -> str => format(pattern, item: value)\n",
        "operator shown<T>{const pattern: str}(value: T) -> str\n",
        "impl const fn shown{const pattern: str}(value: f64) -> str => format(pattern, value)\n",
        "impl const fn shown{const pattern: str}(value: i64) -> str => pattern\n",
        "const fn choose<T>(const pattern: str, value: T) -> str requires T in {f64, i64} => shown{pattern: pattern}(value)\n",
        "const fn apply(f: const fn(f64) -> str) -> str => f(1.5)\n",
        "const fn cap(const pattern: str) -> str => apply(fn(value) => format(pattern, value))\n",
        "test direct { assert show<f64>(\"{:x}\", 1.5) == \"\" }\n",
        "test chain { assert relay(\"{:x}\", 1.5) == \"\" }\n",
        "test option { assert choose(\"{:x}\", 1.5) == \"\" }\n",
        "test named_argument { assert named(\"{item:x}\", 1.5) == \"\" }\n",
        "test captured { assert cap(\"{:x}\") == \"\" }\n",
        "test runtime { assert choose(\"{:x}\", 1) == \"{:x}\" }\n",
        "const fn repeat<T>(const pattern: str, value: T, turns: i64) -> str requires T in {f64, i64} {\n",
        "    if turns == 0 { return choose(pattern, value) }\n",
        "    return repeat(pattern + \"x\", value, turns - 1)\n",
        "}\n",
        "test runtime_recursion { assert repeat(\"\", 1, 3) == \"xxx\" }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:11:22 type/format.specifier FMT-4",
            "m.hgl:12:21 type/format.specifier FMT-4",
            "m.hgl:13:22 type/format.specifier FMT-4",
            "m.hgl:14:30 type/format.specifier FMT-4",
            "m.hgl:15:24 type/format.specifier FMT-4",
        ],
        "{}",
        checked.report,
    );
    for instance in ["show<f64>", "relay<f64>", "shown", "choose<f64>", "cap"] {
        assert!(
            checked
                .report
                .contains(&format!("this call makes the instance `{instance}`")),
            "{}",
            checked.report,
        );
    }
}

/// FMT-3 and GEN-7: a named requirement admits the declaration, then the
/// selected candidate's pattern demand reaches the outside making call.
#[test]
fn formatting_follows_a_named_requirements_selected_const_demand() {
    let checked = with_core(concat!(
        "operator variant<T>{const pattern: str = \"<{}>\"}(value: T) -> str\n",
        "impl const fn variant{const pattern: str}(value: f64) -> str => format(pattern, value)\n",
        "impl const fn variant{const pattern: str}(value: i64) -> str => pattern\n",
        "const fn req<T>(const p: str, v: T) -> str requires variant(T) -> str => variant{pattern: p}(v)\n",
        "test bad { assert req(\"{:x}\", 1.5) == \"\" }\n",
        "test good { assert req(\"{:.1f}\", 1.5) == \"1.5\" }\n",
        "test literal { assert req(\"{:x}\", 7) == \"{:x}\" }\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:6:19 type/format.specifier FMT-4"],
        "{}",
        checked.report,
    );
    assert!(
        checked
            .report
            .contains("this call makes the instance `req<f64>`")
    );
}

/// FMT-3, FN-19 and GEN-7: an indexed pack leaves the operator unread
/// until specialization, including its ordinary const option demand.
#[test]
fn formatting_follows_an_indexed_packs_selected_const_demand() {
    let checked = with_core(concat!(
        "operator lab<T>{const pattern: str = \"<{}>\"}(value: T) -> str\n",
        "impl const fn lab{const pattern: str}(value: f64) -> str => format(pattern, value)\n",
        "impl const fn lab{const pattern: str}(value: i64) -> str => format(pattern, value)\n",
        "const fn top<...As>(const p: str, args: ...As) -> str => lab{pattern: p}(args[0])\n",
        "test bad { assert top(\"{:x}\", 1.5) == \"\" }\n",
        "test good { assert top(\"{:.1f}\", 1.5) == \"1.5\" }\n",
        "test integer { assert top(\"{:x}\", 15) == \"f\" }\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:6:19 type/format.specifier FMT-4"],
        "{}",
        checked.report,
    );
    assert!(
        checked
            .report
            .contains("this call makes the instance `top<[f64]>`")
    );
}

/// FMT-3 and GEN-7: conditional discovery does not read an ordinary
/// const parameter when the selected candidate returns it at run time.
#[test]
fn selected_operator_patterns_can_remain_runtime_values() {
    let checked = with_core(concat!(
        "operator variant<T>{const pattern: str = \"<{}>\"}(value: T) -> str\n",
        "impl const fn variant{const pattern: str}(value: f64) -> str => format(pattern, value)\n",
        "impl const fn variant{const pattern: str}(value: i64) -> str => pattern\n",
        "const fn req<T>(const p: str, v: T) -> str requires variant(T) -> str => variant{pattern: p}(v)\n",
        "const fn packed<...As>(const p: str, args: ...As) -> str => variant{pattern: p}(args[0])\n",
        "const fn identity(value: str) -> str => value\n",
        "const P: str = identity(\"{:x}\")\n",
        "test required { assert req(P, 7) == \"{:x}\" }\n",
        "test indexed { assert packed(P, 7) == \"{:x}\" }\n",
    ));
    assert!(!checked.rejected, "{}", checked.report);
}

/// CAP-6 and TIME-8: a runtime-only selected use must not erase the
/// separate store-key demand of the same ordinary const parameter.
#[test]
fn selected_runtime_patterns_preserve_fixed_store_key_checks() {
    for call in ["keep{pattern: key}(value)", "keep{pattern: key}(values[0])"] {
        let parameter = if call.contains("values[0]") {
            "<...As>(store: GlobalState, const key: str, values: ...As)"
        } else {
            "<T>(store: GlobalState, const key: str, value: T) requires keep(T) -> str"
        };
        let source = format!(
            "operator keep<T>{{const pattern: str = \"<{{}}>\"}}(value: T) -> str\n\
             impl const fn keep{{const pattern: str}}(value: i64) -> str => pattern\n\
             const fn save{parameter} {{\n\
                 let ignored = {call}\n\
                 put(store, key, 7)\n\
             }}\n\
             fn node(const key: str) {{\n\
                 inject global_state\n\
                 when {{}}\n\
                 stop {{ save(global_state, key, 1) }}\n\
             }}\n\
             test bad {{ eval node(key: \"run.output\") }}\n\
             test safe {{ eval node(key: \"user.key\") }}\n\
             const fn grow(store: GlobalState, depth: i64, const key: str) {{\n\
                 if depth == 0 {{ save(store, key, 1) }}\n\
                 else {{ grow(store, depth - 1, key + \".output\") }}\n\
             }}\n\
             fn dynamic(const key: str) {{\n\
                 inject global_state\n\
                 when {{}}\n\
                 stop {{ grow(global_state, 1, key) }}\n\
             }}\n\
             test changing {{ eval dynamic(key: \"run\") }}\n"
        );
        let checked = with_core(&source);
        assert_eq!(checked.findings.len(), 1, "{call}: {}", checked.report);
        assert_eq!(checked.findings[0].code, "injectable/store.reserved_key");
        assert_eq!(checked.findings[0].at, "m.hgl:13:12");
    }
}

/// GEN-13: a recovered store-only read retains its lower budget priority
/// when another ordinary const input is actually read for formatting.
#[test]
fn recovered_store_inputs_do_not_spend_the_formatting_read_budget_first() {
    let body = concat!(
        "operator keep<T>{const pattern: str = \"<{}>\"}(value: T) -> str\n",
        "impl const fn keep{const pattern: str}(value: i64) -> str => pattern\n",
        "operator shown<T>{const pattern: str}(value: T) -> str\n",
        "impl const fn shown{const pattern: str}(value: i64) -> str => format(pattern, value)\n",
        "impl const fn shown{const pattern: str}(value: f64) -> str => format(pattern, value)\n",
        "const fn show<T>(const p: str, value: T) -> str requires T in {i64, f64} => shown{pattern:p}(value)\n",
        "const fn save<T>(store: GlobalState, const key: str, const p: str, value: T) requires T in {i64, f64} && keep(T) -> str {\n",
        "    let ignored = keep{pattern: key}(value)\n",
        "    let message = show(p, value)\n",
        "    put(store, key, 7)\n",
        "}\n",
        "fn node(const key: str) {\n",
        "    inject global_state\n",
        "    when {}\n",
        "    stop { save(global_state, key, \"{:x}\", 1) }\n",
        "}\n",
    );
    for reversed in [false, true] {
        let body = if reversed {
            body.replace(
                "const key: str, const p: str",
                "const p: str, const key: str",
            )
            .replace(
                "save(global_state, key, \"{:x}\", 1)",
                "save(global_state, \"{:x}\", key, 1)",
            )
        } else {
            body.to_owned()
        };
        let source = format!(
            "{body}test safe {{ eval node(key: \"{}\") }}\n",
            "a".repeat(1 << 16),
        );
        let checked = with_core(&source);
        assert!(!checked.rejected, "reversed={reversed}: {}", checked.report);
    }
}

/// GEN-7: an independent const input rejects at its body call once.
#[test]
fn formatting_checks_independent_const_inputs_once() {
    let body = concat!(
        "const fn show(const pattern: str, value: f64) -> str => format(pattern, value)\n",
        "const fn outer<T>(w: f64, v: T) -> str => show(\"{:x}\", w)\n",
        "test first { assert outer(1.5, 4) == \"\" }\n",
    );
    for additional_call in ["", "test second { assert outer(1.5, true) == \"\" }\n"] {
        let checked = with_core(&format!("{body}{additional_call}"));
        assert_eq!(
            checked.summary(),
            ["m.hgl:3:43 type/format.specifier FMT-4"],
            "{}",
            checked.report,
        );
    }
}

/// GEN-7: a separate const-input call does not replay a direct rejection.
#[test]
fn formatting_reread_keeps_one_direct_source_rejection() {
    let checked = with_core(concat!(
        "const fn show(const pattern: str, value: f64) -> str => format(pattern, value)\n",
        "const fn outer<T>(w: f64, v: T) -> str => format(\"{:x}\", w) + show(\"{}\", w)\n",
        "test outer_call { assert outer(1.5, 4) == \"\" }\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:3:53 type/format.specifier FMT-4"],
        "{}",
        checked.report,
    );
}

/// GEN-7: a call that passes its type parameter still checks at the caller.
#[test]
fn formatting_checks_type_dependent_const_inputs_at_the_caller() {
    let checked = with_core(concat!(
        "const fn show<T>(const pattern: str, value: f64, witness: T) -> str => format(pattern, value)\n",
        "const fn outer<T>(w: f64, v: T) -> str => show(\"{:x}\", w, v)\n",
        "test outside { assert outer(1.5, 4) == \"\" }\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:4:23 type/format.specifier FMT-4"],
        "{}",
        checked.report,
    );
    assert!(
        checked
            .report
            .contains("this call makes the instance `show<i64>`")
    );
    assert!(
        checked
            .report
            .contains("this call makes the instance `outer<i64>`")
    );
}

/// FMT-3: an independent unread pattern does not hide a known error.
#[test]
fn formatting_checks_known_const_inputs_when_another_is_unread() {
    let checked = with_core(concat!(
        "const fn pattern(value: str) -> str => value\n",
        "const P: str = pattern(\"{}\")\n",
        "const fn fuse(const a: str, const b: str, value: f64) -> str => format(a, value) + format(b, value)\n",
        "test t { assert fuse(\"{:x}\", P, 1.5) == \"\" }\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:5:17 type/format.specifier FMT-4"],
        "{}",
        checked.report,
    );
}

/// UNIT-13 rejects a cyclic const input without a second instance limit.
#[test]
fn cyclic_const_input_keeps_its_constant_cycle_diagnostic() {
    let checked = with_core(concat!(
        "const Pattern: str = Pattern\n",
        "const fn show(const pattern: str) -> str => format(pattern)\n",
        "test t { assert show(Pattern) == \"\" }\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:2:7 name/constant.cycle UNIT-13"],
        "{}",
        checked.report,
    );
}

/// GEN-13: a growing const input must stop making new instances.
#[test]
fn demanded_const_input_growth_reports_the_instance_limit() {
    let checked = with_core(concat!(
        "const fn grow(const pattern: str) -> str => format(pattern) + grow(pattern + \"x\")\n",
        "test t { assert grow(\"\") == \"\" }\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:3:17 type/generic.instance_limit GEN-13"],
        "{}",
        checked.report,
    );
}

/// GEN-13: each instance reaches the next demanded pattern through a relay.
#[test]
fn demanded_const_input_growth_through_a_generic_relay() {
    let checked = with_core(concat!(
        "const fn grow<T>(const pattern: str, value: T) -> str => format(pattern, value) + relay(pattern + \"x\", value)\n",
        "const fn relay<T>(const pattern: str, value: T) -> str => grow(pattern, value)\n",
        "test t { assert grow(\"{}\", 1) == \"\" }\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:4:17 type/generic.instance_limit GEN-13"],
        "{}",
        checked.report,
    );
}

/// GEN-13: following a possible const demand shares the instance walk's bounds.
#[test]
fn demanded_const_input_growth_bounds_conditional_selection() {
    for next in ["[w]", "(w, w)"] {
        let source = format!(
            "operator variant<T>{{const pattern: str}}(value: T) -> str\n\
             impl const fn variant{{const pattern: str}}(value: f64) -> str => format(pattern, value)\n\
             impl const fn variant{{const pattern: str}}(value: i64) -> str => pattern\n\
             const fn grow<T, V, U>(const p: str, v: T, other: V, w: U, turns: i64) -> str\n\
                 requires T in {{f64, i64}} && V in {{f64, i64}} {{\n\
                 if turns == 0 {{ return variant{{pattern: p}}(v) }}\n\
                 return grow(p, other, v, {next}, turns - 1)\n\
             }}\n\
             test integer {{ assert grow(\"<{{}}>\", 1, 2, 1, 2) == \"<{{}}>\" }}\n\
             test float {{ assert grow(\"<{{}}>\", 1.5, 2.5, 1, 2) == \"<1.5>\" }}\n\
             test alternating {{ assert grow(\"<{{}}>\", 1, 2.5, 1, 2) == \"<{{}}>\" }}\n"
        );
        let checked = with_core(&source);
        assert_eq!(
            checked.summary(),
            [
                "m.hgl:10:23 type/generic.instance_limit GEN-13",
                "m.hgl:11:21 type/generic.instance_limit GEN-13",
                "m.hgl:12:27 type/generic.instance_limit GEN-13",
            ],
            "{next}: {}",
            checked.report,
        );
    }
}

/// FN-1: a read is fixed only if its base and every index are fixed.
#[test]
fn const_arguments_reject_reads_of_one_evaluation() {
    let checked = with_core(concat!(
        "struct Settings { pattern: str }\n",
        "const fn take(const pattern: str) -> str => pattern\n",
        "const fn field(settings: Settings) -> str => take(settings.pattern)\n",
        "const fn indexed(const items: list<str>, index: i64) -> str => take(items[index])\n",
        "const fn nested(const items: list<Settings>, index: i64) -> str => take(items[index].pattern)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:51 type/argument.constant FN-1",
            "m.hgl:5:69 type/argument.constant FN-1",
            "m.hgl:6:73 type/argument.constant FN-1",
        ],
        "{}",
        checked.report,
    );
}

/// The code and rule of each finding, in order.
fn codes(checked: &Checked) -> Vec<String> {
    checked
        .findings
        .iter()
        .map(|finding| format!("{} {}", finding.code, finding.rule))
        .collect()
}

/// FN-1, FN-19, FMT-3, FMT-4 and GEN-7: a const pattern given beside
/// forwarded packs is read in each instance with its entries. Each call
/// whose entries the pattern does not take rejects once, at the call that
/// makes the instance, with the code the same direct `format` call has.
#[test]
fn formatting_checks_const_patterns_forwarded_beside_packs() {
    let calls = [
        ("good", "both(\"{} {name}\", 1, name: \"a\") == \"1 a\""),
        (
            "positional",
            "positional(\"{:x}|{:.1f}\", 255, 2.5) == \"ff|2.5\"",
        ),
        (
            "named",
            "named(\"{b}-{a:>3}\", a: 1, b: \"x\") == \"x-  1\"",
        ),
        ("no_entries", "both(\"plain {{x}}\") == \"plain {x}\""),
        ("specifier", "both(\"{:x}\", 1.5) == \"\""),
        ("few", "both(\"{} {}\", 1) == \"\""),
        ("unknown", "named(\"{nope}\", a: 1) == \"\""),
        ("unused", "positional(\"{}\", 1, 2) == \"\""),
        ("syntax", "positional(\"{\", 1) == \"\""),
        ("empty", "both(\"{}\") == \"\""),
    ];
    let tests = |direct: bool| -> String {
        calls
            .iter()
            .map(|(name, call)| {
                let call = if direct {
                    call.replacen("both(", "format(", 1)
                        .replacen("positional(", "format(", 1)
                        .replacen("named(", "format(", 1)
                } else {
                    (*call).to_string()
                };
                format!("test {name} {{ assert {call} }}\n")
            })
            .collect()
    };
    let checked = with_core(&format!(
        "{}{}{}{}",
        "const fn both<...As, ...Ks>(const p: str, args: ...As, kwargs: ...{Ks}) -> str => format(p, ...args, ...kwargs)\n",
        "const fn positional<...As>(const p: str, args: ...As) -> str => format(p, ...args)\n",
        "const fn named<...Ks>(const p: str, kwargs: ...{Ks}) -> str => format(p, ...kwargs)\n",
        tests(false),
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:25 type/format.specifier FMT-4",
            "m.hgl:10:19 type/format.argument FMT-3",
            "m.hgl:11:23 type/format.argument FMT-3",
            "m.hgl:12:22 type/format.unused FMT-3",
            "m.hgl:13:22 type/format.syntax FMT-3",
            "m.hgl:14:21 type/format.argument FMT-3",
        ],
        "{}",
        checked.report,
    );
    for instance in [
        "both<[f64], []>",
        "both<[i64], []>",
        "named<[i64]>",
        "positional<[i64, i64]>",
        "positional<[i64]>",
        "both<[], []>",
    ] {
        assert!(
            checked
                .report
                .contains(&format!("this call makes the instance `{instance}`")),
            "{}",
            checked.report,
        );
    }
    assert_eq!(
        checked
            .report
            .matches("the instance's body rejects here")
            .count(),
        6,
        "{}",
        checked.report,
    );
    let direct = with_core(&tests(true));
    assert_eq!(codes(&checked), codes(&direct), "{}", direct.report);
}

/// FN-19 and GEN-7: the pattern reaches `format` through a function that
/// forwards to one that forwards, by name, from a literal in a forwarding
/// body, through a generic caller, and to a pack entry's own call.
#[test]
fn formatting_checks_const_patterns_forwarded_twice() {
    let checked = with_core(concat!(
        "const fn inner<...As, ...Ks>(const p: str, args: ...As, kwargs: ...{Ks}) -> str => format(p, ...args, ...kwargs)\n",
        "const fn outer<...As, ...Ks>(const p: str, args: ...As, kwargs: ...{Ks}) -> str => inner(p, ...args, ...kwargs)\n",
        "const fn fixed<...As>(args: ...As) -> str => inner(\"{:x}\", ...args)\n",
        "const fn by_name<...As>(const q: str, args: ...As) -> str => inner(...args, p: q)\n",
        "const fn typed<T>(const q: str, v: T) -> str => outer(q, v)\n",
        "const fn helper<T>(const p: str, v: T) -> str => format(p, v)\n",
        "const fn entry<...As>(const p: str, args: ...As) -> str => helper(p, args[0])\n",
        "test good {\n",
        "    assert outer(\"{} {name}\", 1, name: \"a\") == \"1 a\"\n",
        "    assert fixed(255) == \"ff\"\n",
        "    assert by_name(\"<{}>\", 1) == \"<1>\"\n",
        "    assert typed(\"{:x}\", 255) == \"ff\"\n",
        "    assert entry(\"<{}>\", 1) == \"<1>\"\n",
        "}\n",
        "test twice { assert outer(\"{:x}\", 1.5) == \"\" }\n",
        "test twice_named { assert outer(\"{} {nope}\", 1, name: \"a\") == \"\" }\n",
        "test literal { assert fixed(1.5) == \"\" }\n",
        "test named_argument { assert by_name(\"{:x}\", 1.5) == \"\" }\n",
        "test generic { assert typed(\"{:x}\", 1.5) == \"\" }\n",
        "test pack_entry { assert entry(\"{:x}\", 1.5) == \"\" }\n",
        "test twice_few { assert outer(\"{} {}\", 1) == \"\" }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:16:21 type/format.specifier FMT-4",
            "m.hgl:17:27 type/format.argument FMT-3",
            "m.hgl:18:23 type/format.specifier FMT-4",
            "m.hgl:19:30 type/format.specifier FMT-4",
            "m.hgl:20:23 type/format.specifier FMT-4",
            "m.hgl:21:26 type/format.specifier FMT-4",
            "m.hgl:22:25 type/format.argument FMT-3",
        ],
        "{}",
        checked.report,
    );
    for instance in [
        "outer<[f64], []>",
        "inner<[f64], []>",
        "inner<[i64], [str]>",
        "fixed<[f64]>",
        "by_name<[f64]>",
        "typed<f64>",
        "helper<f64>",
        "entry<[f64]>",
    ] {
        assert!(
            checked
                .report
                .contains(&format!("this call makes the instance `{instance}`")),
            "{}",
            checked.report,
        );
    }
}

/// GEN-7 and GEN-13: two calls with different patterns, or one pattern
/// and different entries, are different instances, each with its own
/// verdict, whichever is checked first. The pattern written in the body
/// beside the packs is checked as before.
#[test]
fn formatting_checks_each_forwarded_pattern_instance_on_its_own() {
    let declarations = concat!(
        "const fn show<...As>(const p: str, args: ...As) -> str => format(p, ...args)\n",
        "const fn written<...As, ...Ks>(args: ...As, kwargs: ...{Ks}) -> str => format(\"{:x} {name}\", ...args, ...kwargs)\n",
    );
    let good = [
        "test hex { assert show(\"{:x}\", 255) == \"ff\" }\n",
        "test float { assert show(\"{:.1f}\", 1.5) == \"1.5\" }\n",
        "test flag { assert show(\"{:x}\", true) == \"1\" }\n",
        "test body { assert written(255, name: \"a\") == \"ff a\" }\n",
    ];
    let bad = [
        (
            "test hex_float { assert show(\"{:x}\", 1.5) == \"\" }\n",
            25,
            "format.specifier FMT-4",
        ),
        (
            "test body_float { assert written(1.5, name: \"a\") == \"\" }\n",
            26,
            "format.specifier FMT-4",
        ),
        (
            "test body_missing { assert written(1) == \"\" }\n",
            28,
            "format.argument FMT-3",
        ),
    ];
    for bad_first in [false, true] {
        let mut lines: Vec<(&str, Option<(usize, &str)>)> =
            good.iter().map(|line| (*line, None)).collect();
        let failing = bad
            .iter()
            .map(|(line, column, code)| (*line, Some((*column, *code))));
        if bad_first {
            lines.splice(0..0, failing);
        } else {
            lines.extend(failing);
        }
        let source: String = lines.iter().map(|(line, _)| *line).collect();
        let checked = with_core(&format!("{declarations}{source}"));
        let expected: Vec<String> = lines
            .iter()
            .enumerate()
            .filter_map(|(index, (_, failing))| {
                let (column, code) = (*failing)?;
                Some(format!("m.hgl:{}:{column} type/{code}", index + 4))
            })
            .collect();
        assert_eq!(checked.summary(), expected, "{}", checked.report);
    }
}

/// GEN-13: a const pattern that grows through a forwarded call makes
/// instances without end, whether it is appended to, doubled, or grown
/// through a generic caller of the forwarding body. Each walk ends at the
/// bound, reported once at the call that makes the first of them.
#[test]
fn forwarded_const_pattern_growth_reports_the_instance_limit() {
    for (body, call) in [
        (
            "const fn grow<...As>(const p: str, args: ...As) -> str => format(p, ...args) + grow(p + \"x\", ...args)\n",
            "grow(\"{}\", 1)",
        ),
        (
            "const fn grow<...As>(const p: str, args: ...As) -> str => format(p, ...args) + grow(p + p, ...args)\n",
            "grow(\"a\")",
        ),
        (
            concat!(
                "const fn show<...As>(const p: str, args: ...As) -> str => format(p, ...args)\n",
                "const fn grow<T>(const p: str, v: T) -> str => show(p, v) + grow(p + \"x\", v)\n",
            ),
            "grow(\"{}\", 1)",
        ),
        // Two declarations that grow from one call: that call rejects once.
        (
            concat!(
                "const fn ga<...As>(const p: str, args: ...As) -> str => format(p, ...args) + ga(p + \"x\", ...args)\n",
                "const fn gb<T>(const p: str, v: T) -> str => format(p, v) + gb(p + \"y\", v)\n",
                "const fn grow<...As>(const p: str, args: ...As) -> str => ga(p, ...args) + gb(p, args[0])\n",
            ),
            "grow(\"{}\", 1)",
        ),
    ] {
        let lines = body.lines().count();
        let checked = with_core(&format!("{body}test t {{ assert {call} == \"\" }}\n"));
        assert_eq!(
            checked.summary(),
            [format!(
                "m.hgl:{}:17 type/generic.instance_limit GEN-13",
                lines + 2
            )],
            "{body}: {}",
            checked.report,
        );
    }
}

/// FN-19 and GEN-7: a pack entry given to a generic relay of a forwarding
/// body is read in each instance, where the entry is known; where the
/// body is declared it makes no instance to judge. The shapes, with no
/// call: an entry and a const pattern, a literal pattern, `debug_print`'s
/// own (a forwarding body, a generic relay of it, and a first entry), a
/// named entry, a node's hook, and a mutual recursion that ends.
#[test]
fn forwarded_entries_given_to_a_generic_relay_check_where_declared() {
    let show = "const fn show<...As>(const p: str, args: ...As) -> str => format(p, ...args)\n";
    let mid = "const fn mid<T>(const p: str, v: T) -> str => show(p, v)\n";
    let shapes = [
        format!("{show}{mid}const fn top<...As>(const p: str, args: ...As) -> str => mid(p, args[0])\n"),
        format!("{show}{mid}const fn top<...As>(args: ...As) -> str => mid(\"<{{}}>\", args[0])\n"),
        concat!(
            "const fn logline<...As, ...Ks>(const fmt_str: str, args: ...As, kwargs: ...{Ks}) -> str\n",
            "    => format(fmt_str, ...args, ...kwargs) + \"\\n\"\n",
            "const fn log_one<T>(const fmt_str: str, v: T) -> str => logline(fmt_str, v)\n",
            "const fn first<...As>(const fmt_str: str, args: ...As) -> str => log_one(fmt_str, args[0])\n",
        )
        .to_string(),
        format!("{show}{mid}const fn top<...Ks>(const p: str, kwargs: ...{{Ks}}) -> str => mid(p, kwargs[\"a\"])\n"),
        format!(
            "{show}{mid}fn nod<...As>(args: ...As) -> str {{\n    when {{ return mid(\"<{{}}>\", args[0]) }}\n}}\n"
        ),
        concat!(
            "const fn ping<...As>(const p: str, n: i64, args: ...As) -> str {\n",
            "    if n == 0 { return format(p, ...args) }\n",
            "    return pong(p, n - 1, args[0])\n",
            "}\n",
            "const fn pong<T>(const p: str, n: i64, v: T) -> str => ping(p, n, v)\n",
            "test t { assert ping(\"<{}>\", 3, 1) == \"<1>\" }\n",
        )
        .to_string(),
    ];
    for shape in &shapes {
        let checked = with_core(shape);
        assert!(checked.summary().is_empty(), "{shape}: {}", checked.report);
    }
    // Called, the debug_print shape is checked in its instances.
    let checked = with_core(&format!(
        "{}test good {{ assert first(\"<{{}}>\", 1) == \"<1>\\n\" }}\ntest bad {{ assert first(\"{{:x}}\", 1.5) == \"\" }}\n",
        shapes[2]
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:7:19 type/format.specifier FMT-4"],
        "{}",
        checked.report,
    );
    for instance in ["first<[f64]>", "log_one<f64>", "logline<[f64], []>"] {
        assert!(
            checked
                .report
                .contains(&format!("this call makes the instance `{instance}`")),
            "{}",
            checked.report,
        );
    }
}

/// GEN-7: an instance two calls make, one through another instance, is
/// reported once at each call, whichever of its paths is found first.
#[test]
fn a_shared_forwarded_instance_reports_once_at_each_call() {
    for (fa, fb) in [
        ("h1(p, args[0]) + h2(p, args[0])", "h2"),
        ("h2(p, args[0]) + h1(p, args[0])", "h1"),
    ] {
        let source = format!(
            "const fn h2<T>(const p: str, v: T) -> str => format(p, v)\n\
             const fn h1<T>(const p: str, v: T) -> str => h2(p, v)\n\
             const fn fa<...As>(const p: str, args: ...As) -> str => {fa}\n\
             const fn fb<...As>(const p: str, args: ...As) -> str => {fb}(p, args[0])\n\
             test t1 {{ assert fa(\"{{:x}}\", 1.5) == \"\" }}\n\
             test t2 {{ assert fb(\"{{:x}}\", 1.5) == \"\" }}\n"
        );
        let checked = with_core(&source);
        assert_eq!(
            checked.summary(),
            [
                "m.hgl:6:18 type/format.specifier FMT-4",
                "m.hgl:7:18 type/format.specifier FMT-4",
            ],
            "{source}: {}",
            checked.report,
        );
    }
}

/// FN-1 and GEN-7: a call that gives a concrete body const inputs it reads
/// makes an instance of its own, so a pattern given there reaches the
/// forwarding body: from a value function, a node's const and option, a
/// graph's const, and an operator candidate's option, chosen directly or
/// by a generic caller.
#[test]
fn concrete_const_inputs_reach_a_forwarding_body() {
    let checked = with_core(concat!(
        "const fn show<...As>(const p: str, args: ...As) -> str => format(p, ...args)\n",
        "const fn v_fwd(const p: str, x: f64) -> str => show(p, x)\n",
        "fn node_fwd(x: f64, const p: str) -> str {\n",
        "    when { return show(p, x) }\n",
        "}\n",
        "fn option_fwd{const p: str = \"{:x}\"}(x: f64) -> str {\n",
        "    when { return show(p, x) }\n",
        "}\n",
        "fn graph_fwd(x: f64, const p: str) -> str => show(p, x)\n",
        "operator pick<T>{const pattern: str = \"{:x}\"}(value: T) -> str\n",
        "impl const fn pick{const pattern: str}(value: f64) -> str => show(pattern, value)\n",
        "impl const fn pick{const pattern: str}(value: i64) -> str => show(pattern, value)\n",
        "const fn choose<T>(const p: str, v: T) -> str requires T in {f64, i64} => pick{pattern: p}(v)\n",
        "test good {\n",
        "    assert v_fwd(\"{:.1f}\", 1.5) == \"1.5\"\n",
        "    assert pick(255) == \"ff\"\n",
        "    assert choose(\"{:x}\", 255) == \"ff\"\n",
        "}\n",
        "test value { assert v_fwd(\"{:x}\", 1.5) == \"\" }\n",
        "test node { assert eval node_fwd(x: [1.5], p: \"{:x}\") == [\"\"] }\n",
        "test option { assert eval option_fwd(x: [1.5]) == [\"\"] }\n",
        "test graph { assert eval graph_fwd(x: [1.5], p: \"{:x}\") == [\"\"] }\n",
        "test candidate { assert pick(1.5) == \"\" }\n",
        "test chosen { assert choose(\"{:x}\", 1.5) == \"\" }\n",
        "test chosen_integer { assert choose(\"{:.1f}\", 255) == \"\" }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:20:21 type/format.specifier FMT-4",
            "m.hgl:21:20 type/format.specifier FMT-4",
            "m.hgl:22:22 type/format.specifier FMT-4",
            "m.hgl:23:21 type/format.specifier FMT-4",
            "m.hgl:24:25 type/format.specifier FMT-4",
            "m.hgl:25:22 type/format.specifier FMT-4",
            "m.hgl:26:30 type/format.specifier FMT-4",
        ],
        "{}",
        checked.report,
    );
}

/// FMT-5: use the nullable payload's text conversion where present.
/// Other fields and collection members still need a conversion to `str`.
#[test]
fn formatting_checks_conversions_of_values_and_nullable_payloads() {
    let checked = with_core(concat!(
        "const fn number(x: i64?) -> str => format(\"{}\", x)\n",
        "const fn optional_list(x: list<i64>?) -> str => format(\"{}\", x)\n",
        "const fn failed() -> str => format(\"{}\", convert<i64>(\"3\"))\n",
        "const fn bytes_text(x: bytes) -> str => format(\"{}\", x)\n",
        "const fn byte_parts(x: list<bytes>) -> str => format(\"{}\", x)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:42 operator/operator.no_candidate FMT-5",
            "m.hgl:5:54 operator/operator.no_candidate FMT-5",
            "m.hgl:6:60 operator/operator.no_candidate FMT-5",
        ],
        "{}",
        checked.report,
    );
}

/// UNIT-13, FMT-4: a pattern returned by a value function is checked
/// when the module constant supplies it to a formatting call.
#[test]
fn formatting_checks_a_pattern_returned_to_a_module_constant() {
    let checked = with_core(concat!(
        "const fn pattern() -> str => \"{:x}\"\n",
        "const P: str = pattern()\n",
        "const fn display() -> str => format(P, 1.5)\n",
        "const fn prefix() -> str => \"{:\"\n",
        "const Q: str = prefix() + \"x}\"\n",
        "const fn joined() -> str => format(Q, 1.5)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:37 type/format.specifier FMT-4",
            "m.hgl:7:36 type/format.specifier FMT-4",
        ],
        "{}",
        checked.report,
    );
}

/// FMT-3/4 and UNIT-13: ordinary module helpers keep lexical frames and
/// actual inputs distinct, including when the pattern is forwarded.
#[test]
fn formatting_checks_module_helper_arguments_and_immutable_locals() {
    for helper in [
        "const fn pattern(s: str) -> str => s",
        "const fn pattern(s: str) -> str { let first = s\nlet second = first\nreturn second }",
        "const fn id(s: str) -> str => s\nconst fn pattern(s: str) -> str => id(id(s))",
        "const GLOBAL: str = \"{:x}\"\nconst fn pattern(s: str) -> str { let GLOBAL = s\nreturn GLOBAL }",
    ] {
        let checked = with_core(&format!(
            "{helper}\nconst P: str = pattern(\"{{:.1f}}\")\nconst Q: str = pattern(\"{{:x}}\")\n\
             const fn good() -> str => format(P, 1.5)\n\
             const fn relay<T>(const p: str, value: T) -> str => format(p, value)\n\
             test bad {{ assert relay(Q, 1.5) == \"\" }}\n"
        ));
        assert_eq!(
            checked
                .findings
                .iter()
                .map(|f| (f.code.as_str(), f.rule))
                .collect::<Vec<_>>(),
            [("type/format.specifier", "FMT-4")],
            "{helper}: {}",
            checked.report
        );
    }
}

/// FMT-3/4/9: late module patterns use the original source phase and
/// diagnostics, rather than treating an entire test as graph code.
#[test]
fn formatting_checks_late_module_patterns_in_their_original_context() {
    for (pattern, operand, code, rule) in [
        ("{:x}", ", 1.5", "type/format.specifier", "FMT-4"),
        ("{", "", "type/format.syntax", "FMT-3"),
        ("{missing}", ", 1.5", "type/format.argument", "FMT-3"),
        (
            "{}",
            ", convert<i64>(\"3\")",
            "operator/operator.no_candidate",
            "FMT-5",
        ),
        ("{engine_time}", "", "type/format.argument", "FMT-9"),
    ] {
        let checked = with_core(&format!(
            "const fn pattern(s: str) -> str => s\nconst P: str = pattern(\"{pattern}\")\n\
             test t {{ assert format(P{operand}) == \"\" }}\n"
        ));
        assert_eq!(
            checked
                .findings
                .iter()
                .map(|f| (f.code.as_str(), f.rule))
                .collect::<Vec<_>>(),
            [(code, rule)],
            "{pattern}: {}",
            checked.report
        );
    }
}

/// UNIT-13 and ERR-9: written dead branches still take part in cycles.
/// An unrelated failed constant must not hide an independent FMT-4 error.
#[test]
fn formatting_module_helpers_preserve_constant_dependency_failures() {
    for (helper, unrelated, expected) in [
        ("P", "", vec!["name/constant.cycle"]),
        ("if false { P } else { s }", "", vec!["name/constant.cycle"]),
        (
            "s",
            "const Z: i64 = if false { Z } else { 1 }\n",
            vec!["name/constant.cycle", "type/format.specifier"],
        ),
    ] {
        for forwarded in [false, true] {
            let call = if forwarded {
                "relay(P, 1.5)"
            } else {
                "format(P, 1.5)"
            };
            let checked = with_core(&format!(
                "{unrelated}const fn pattern(s: str) -> str => {helper}\n\
                 const P: str = pattern(\"{{:x}}\")\n\
                 const fn relay<T>(const p: str, value: T) -> str => format(p, value)\n\
                 test t {{ assert {call} == \"\" }}\n"
            ));
            assert_eq!(
                checked
                    .findings
                    .iter()
                    .map(|f| f.code.as_str())
                    .collect::<Vec<_>>(),
                expected,
                "{helper}, forwarded={forwarded}: {}",
                checked.report
            );
        }
    }
}

/// FMT-3 remains partial: opaque, effectful and recursive helper bodies
/// are not evaluated by the bounded module-pattern reader.
#[test]
fn formatting_leaves_unsupported_module_helpers_unread() {
    for helper in [
        "native const fn pattern(s: str) -> str",
        "const fn pattern(s: str) -> str { write(\"effect\")\nreturn s }",
        "const fn pattern(s: str) -> str => pattern(s)",
    ] {
        let checked = with_core(&format!(
            "{helper}\nconst P: str = pattern(\"{{:x}}\")\n\
             test t {{ assert format(P, 1.5) == \"\" }}\n"
        ));
        assert!(checked.findings.is_empty(), "{}", checked.report);
    }
}

/// UNIT-13 and FN-1: the helper extension must not fold a non-helper
/// expression and add a format error after its existing source failure.
#[test]
fn formatting_helper_fallback_preserves_non_helper_source_failures() {
    for (source, code, rule) in [
        (
            "const P: str = if false { P } else { \"{:x}\" }\nconst fn show(value: f64) -> str => format(P, value)\n",
            "name/constant.cycle",
            "UNIT-13",
        ),
        (
            "const fn show(value: f64) -> str => format(if true { \"{:x}\" } else { \"{}\" }, value)\n",
            "type/argument.constant",
            "FN-1",
        ),
    ] {
        let checked = with_core(source);
        assert_eq!(
            checked
                .findings
                .iter()
                .map(|f| (f.code.as_str(), f.rule))
                .collect::<Vec<_>>(),
            [(code, rule)],
            "{}",
            checked.report
        );
    }
}

/// UNIT-13, FN-1: evidence for one helper constant cannot validate a
/// different written root, including in a non-selected conditional arm.
#[test]
fn formatting_helper_fallback_proves_every_written_module_root() {
    for cyclic in [false, true] {
        for inline in [false, true] {
            for condition in [false, true] {
                let other = if cyclic {
                    "const Q: str = if false { Q } else { \"\" }\n"
                } else {
                    "native const fn unknown() -> str\nconst Q: str = unknown()\n"
                };
                let conditional = format!("if {condition} {{ P }} else {{ Q }}");
                let (definition, pattern) = if inline {
                    (String::new(), conditional)
                } else {
                    (format!("const R: str = {conditional}\n"), "R".to_string())
                };
                let checked = with_core(&format!(
                    "const fn pat(text: str) -> str => text\nconst P: str = pat(\"{{:x}}\")\n{other}{definition}const fn show(value: f64) -> str => format({pattern}, value)\n"
                ));
                let mut expected = Vec::new();
                if cyclic {
                    expected.push(("name/constant.cycle", "UNIT-13"));
                }
                if inline {
                    expected.push(("type/argument.constant", "FN-1"));
                }
                assert_eq!(
                    checked
                        .findings
                        .iter()
                        .map(|f| (f.code.as_str(), f.rule))
                        .collect::<Vec<_>>(),
                    expected,
                    "{}",
                    checked.report
                );
            }
        }
    }
    for forwarded in [false, true] {
        let call = if forwarded {
            "relay(P + Q, value)"
        } else {
            "format(P + Q, value)"
        };
        let checked = with_core(&format!(
            "const fn pat(text: str) -> str => text\nconst P: str = pat(\"{{:x}}\")\nconst Q: str = if false {{ Q }} else {{ \"\" }}\nconst fn relay<T>(const p: str, value: T) -> str => format(p, value)\nconst fn show(value: f64) -> str => {call}\n"
        ));
        assert_eq!(
            checked
                .findings
                .iter()
                .map(|f| (f.code.as_str(), f.rule))
                .collect::<Vec<_>>(),
            [("name/constant.cycle", "UNIT-13")],
            "{}",
            checked.report
        );
    }
}

/// FMT-4, UNIT-13: an earlier input's helper proof must not impose
/// new proof requirements on an independent later input.
#[test]
fn formatting_helper_input_proofs_do_not_leak_to_another_input() {
    let checked = with_core(
        "const fn pat(text: str) -> str => text\nconst P: str = pat(\"{}\")\nnative const fn opaque() -> str\nconst Q: str = if true { \"{:x}\" } else { opaque() }\nconst fn show(const a: str, const b: str, value: f64) -> str => format(a, 1) + format(b, value)\ntest t { assert show(P, Q, 1.5) == \"\" }\n",
    );
    assert_eq!(
        checked
            .findings
            .iter()
            .map(|f| (f.code.as_str(), f.rule))
            .collect::<Vec<_>>(),
        [("type/format.specifier", "FMT-4")],
        "{}",
        checked.report
    );
}

/// FMT-4, UNIT-13: an earlier helper may call a later module's helper.
/// Defer the pattern until every typed body exists instead of indexing it early.
#[test]
fn formatting_helper_patterns_wait_for_later_module_bodies() {
    let checked = check(&[
        (
            "a.hgl",
            "module ordering.a\nuse ordering.z::{inner}\nexport const fn pattern(text: str) -> str => inner(text)\n",
        ),
        (
            "b.hgl",
            "module ordering.b\nuse ordering.a::{pattern}\nconst P: str = pattern(\"{:x}\")\nconst fn show(value: f64) -> str => format(P, value)\n",
        ),
        (
            "z.hgl",
            "module ordering.z\nexport const fn inner(text: str) -> str => text\n",
        ),
    ]);
    assert_eq!(
        checked
            .findings
            .iter()
            .map(|f| (f.code.as_str(), f.rule))
            .collect::<Vec<_>>(),
        [("type/format.specifier", "FMT-4")],
        "{}",
        checked.report
    );
    assert!(checked.findings[0].at.starts_with("b.hgl:4:"));
}

/// FMT-5: an implicit conversion reaches the same instance and
/// dependency checks as a written call (GEN-7, UNIT-13).
#[test]
fn formatting_checks_implicit_conversion_instances_and_dependencies() {
    let checked = check(&[
        (
            "generic.hgl",
            r#"module formatting_instance
struct Wrapper<T> { field: T }
impl const fn convert<T>(value: Wrapper<T>) -> str => format("{:x}", value.field)
test t {
    let wrapped = Wrapper<f64>(field: 1.5)
    assert format("{}", wrapped) == "1.5"
}
"#,
        ),
        (
            "constant.hgl",
            r#"module formatting_dependency
struct Wrapper<T> { field: T }
impl const fn convert<T>(value: Wrapper<T>) -> str => A
const A: str = format("{}", Wrapper<i64>(field: 1))
test t { assert A == "first" }
"#,
        ),
    ]);
    assert_eq!(
        checked.summary(),
        [
            "constant.hgl:4:7 name/constant.cycle UNIT-13",
            "generic.hgl:6:25 type/format.specifier FMT-4",
        ],
        "{}",
        checked.report,
    );
}

/// The module the compiler supplies is the pinned core's, whole: every
/// `.hgl` file under `spec/hgl_core/hgl`, with its text as it stands
/// there. A pin that adds a file fails this until
/// `hgl_analyser::CORE_FILES` lists it.
#[test]
fn the_core_supplied_is_every_file_of_the_pinned_core() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/hgl_core/hgl");
    let mut pinned = BTreeMap::new();
    let mut directories = vec![root.clone()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory).expect("the pinned core is read") {
            let path = entry.expect("the pinned core is read").path();
            if path.is_dir() {
                directories.push(path);
            } else if path.extension().is_some_and(|extension| extension == "hgl") {
                // Named as `CORE_FILES` names it, with `/` on every
                // system.
                let relative = path
                    .strip_prefix(&root)
                    .expect("the file is under the root");
                let parts: Vec<String> = relative
                    .iter()
                    .map(|part| part.to_string_lossy().into_owned())
                    .collect();
                let text = fs::read_to_string(&path).expect("the file is text");
                pinned.insert(format!("<core>/{}", parts.join("/")), text);
            }
        }
    }
    let supplied: BTreeMap<String, String> = hgl_analyser::CORE_FILES
        .iter()
        .map(|(name, text)| (name.to_string(), text.to_string()))
        .collect();
    assert_eq!(
        supplied.keys().collect::<Vec<_>>(),
        pinned.keys().collect::<Vec<_>>()
    );
    assert_eq!(supplied, pinned);
}

/// NUM-3, NUM-9: "Division with `zero: .ieee` or `zero: .nan` returns
/// f64; division with `zero: .error` returns `result<f64, error>`."
/// Core's candidates of `div_` for numbers declare `zero` and say in
/// `requires` which values each is for (GEN-6), so the option a call
/// gives selects the candidate and the result type (EXPR-12).
#[test]
fn divisions_zero_option_selects_the_result_type() {
    let checked = with_core(concat!(
        "const fn uses(a: f64, b: f64, n: i64, text: str) {\n",
        "    let plain = div_(a, b)\n",
        "    let nan = div_(a, b, zero: .nan)\n",
        "    let checked = div_(a, b, zero: .error)\n",
        "    let mixed = div_(n, b, zero: .error)\n",
        // OPS-1: a `str` is not divided, with an option or without.
        "    let wrong = div_(text, n, zero: .nan)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:7:17 operator/operator.no_candidate GEN-5"]
    );
    assert_eq!(
        locals(&checked, "uses")[4..8],
        [
            "plain: f64",
            "nan: f64",
            "checked: result<f64, error>",
            "mixed: result<f64, error>",
        ]
    );
}

/// NUM-9: "`zero` is a generic value parameter of the candidates that
/// declare it, and a call gives it a compile-time constant." OPS-8:
/// `invalid` "is a generic value parameter of those conversions, and a
/// call gives it a compile-time constant. `rounding` and `nan` are
/// parameters." GEN-8: a value known only when the program runs is
/// rejected for the first two, and is what a parameter takes.
#[test]
fn an_option_that_selects_the_result_type_is_given_a_constant() {
    let checked = with_core(concat!(
        "const fn uses(a: f64, b: f64, policy: ZeroPolicy, strict: InvalidPolicy, mode: Rounding) {\n",
        "    let runtime = div_(a, b, zero: policy)\n",
        "    let forced = convert<i64>(a, invalid: strict)\n",
        // `rounding` does not select: a parameter.
        "    let rounded = convert<i64>(a, rounding: mode)\n",
        // TYPE-7: an enum member written in full.
        "    let full = div_(a, b, zero: ZeroPolicy.error)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:36 type/generic.argument_constant GEN-8",
            "m.hgl:4:43 type/generic.argument_constant GEN-8",
        ]
    );
    assert_eq!(
        locals(&checked, "uses")[7..],
        ["rounded: result<i64, error>", "full: result<f64, error>"]
    );
}

/// NUM-12: "Every division takes `zero`: `/`, `//`, and `%`, of numbers
/// and of durations (OPS-1). The policies one admits, and its default,
/// follow from what its result can hold". NUM-3: `.fail` makes a zero
/// divisor an HGL error, and so keeps the division's own type. NUM-9: a
/// division returns a `result` of its type under `zero: .error`.
#[test]
fn every_division_takes_the_policies_its_result_can_hold() {
    let checked = with_core(concat!(
        "const fn uses(a: f64, n: i64, span: duration, gap: duration) {\n",
        // A result that holds no infinity or NaN: `.fail`, the default,
        // and `.error`.
        "    let whole = floordiv_(n, n)\n",
        "    let rest = mod_(n, n, zero: .fail)\n",
        "    let checked = floordiv_(n, n, zero: .error)\n",
        "    let piece = div_(span, n)\n",
        "    let pieces = div_(span, a, zero: .error)\n",
        "    let count = floordiv_(span, gap, zero: .error)\n",
        // A result that is an `f64`: every policy.
        "    let raised = div_(a, a, zero: .fail)\n",
        "    let real = floordiv_(a, n, zero: .nan)\n",
        "    let ratio = div_(span, gap)\n",
        "    let safe = div_(span, gap, zero: .error)\n",
        // An `i64` and a `duration` hold no NaN or infinity.
        "    let bad1 = floordiv_(n, n, zero: .nan)\n",
        "    let bad2 = div_(span, n, zero: .ieee)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:13:16 operator/operator.no_candidate GEN-5",
            "m.hgl:14:16 operator/operator.no_candidate GEN-5",
        ]
    );
    assert_eq!(
        locals(&checked, "uses")[4..14],
        [
            "whole: i64",
            "rest: i64",
            "checked: result<i64, error>",
            "piece: duration",
            "pieces: result<duration, error>",
            "count: result<i64, error>",
            "raised: f64",
            "real: f64",
            "ratio: f64",
            "safe: result<f64, error>",
        ]
    );
}

/// OPS-4: "`convert<T>(value)` converts a value to the type T ... It
/// admits the conversions of the second table below and rejects every
/// other." OPS-8: "`invalid` determines the result type ... A
/// conversion that cannot fail returns T." Core's `convert` requires
/// its result to be T or `result<T, error>`, which with the T a call
/// writes (GEN-8) says which candidate the call asks for.
#[test]
fn a_conversion_is_the_type_asked_for_or_a_result_of_it() {
    let checked = with_core(concat!(
        "const fn uses(a: f64, n: i64, text: str, raw: bytes) {\n",
        // The conversions that cannot fail.
        "    let real = convert<f64>(n)\n",
        "    let words = convert<str>(n)\n",
        // Those that can: a `result` by default, the plain type under
        // `invalid: .fail`.
        "    let whole = convert<i64>(a)\n",
        "    let rounded = convert<i64>(a, rounding: .half_even)\n",
        "    let zeroed = convert<i64>(a, nan: 0)\n",
        "    let forced = convert<i64>(a, invalid: .fail)\n",
        "    let parsed = convert<f64>(text)\n",
        "    let decoded = convert<str>(raw, invalid: .fail)\n",
        // No conversion of an `i64` to a `date`; no `rounding` but from
        // an `f64` to an `i64`.
        "    let none = convert<date>(n)\n",
        "    let odd = convert<f64>(n, rounding: .floor)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:11:16 operator/operator.no_candidate GEN-5",
            "m.hgl:12:15 operator/operator.no_candidate GEN-5",
        ]
    );
    assert_eq!(
        locals(&checked, "uses")[4..12],
        [
            "real: f64",
            "words: str",
            "whole: result<i64, error>",
            "rounded: result<i64, error>",
            "zeroed: result<i64, error>",
            "forced: i64",
            "parsed: result<f64, error>",
            "decoded: str",
        ]
    );
}

/// OPS-2: "`<`, `<=`, `>`, `>=`, and `<=>` compare two values of one
/// type among `str`, `bool`, `date`, `time`, `datetime`,
/// `civil_datetime`, and `duration`. `==` and `!=` compare two values
/// of one type. The zoned types have no order ... Numbers compare as
/// the numeric contract says, an `i64` with an `f64` included." OPS-3:
/// `<=>` returns a `CmpResult`. NUM-4: the tolerances are for numbers.
/// NUM-11: "The built-in `abs` and `rel` tolerance options are scalar
/// f64 configuration."
///
/// Core declares one generic candidate for each comparison, with the
/// types it is for in its `requires`, beside the candidates for
/// numbers.
#[test]
fn a_comparison_is_of_two_values_of_one_type() {
    let checked = with_core(concat!(
        "struct Quote { bid: f64 }\n",
        "const fn uses(a: f64, n: i64, text: str, day: date, q: Quote, zone: zoned_datetime) {\n",
        "    let e1 = eq_(n, n)\n",
        "    let e2 = eq_(text, text)\n",
        "    let e3 = eq_(q, q)\n",
        "    let e4 = eq_(n, a)\n",
        "    let e5 = eq_{abs: 0.01}(a, a)\n",
        "    let l1 = lt_(text, text)\n",
        "    let l2 = lt_(day, day)\n",
        "    let l3 = lt_(n, a)\n",
        "    let c1 = compare_(text, text)\n",
        // No order for a struct or a zoned type; no comparison of a
        // `str` with an `i64`; no tolerance for a `str`.
        "    let bad1 = lt_(q, q)\n",
        "    let bad2 = lt_(zone, zone)\n",
        "    let bad3 = eq_(text, n)\n",
        "    let bad4 = eq_{abs: 0.01}(text, text)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:13:16 operator/operator.no_candidate GEN-5",
            "m.hgl:14:16 operator/operator.no_candidate GEN-5",
            "m.hgl:15:16 operator/operator.no_candidate GEN-5",
            "m.hgl:16:16 operator/operator.no_candidate GEN-5",
        ]
    );
    assert_eq!(
        locals(&checked, "uses")[6..15],
        [
            "e1: bool",
            "e2: bool",
            "e3: bool",
            "e4: bool",
            "e5: bool",
            "l1: bool",
            "l2: bool",
            "l3: bool",
            "c1: CmpResult",
        ]
    );
}

/// OPS-9: "Same-type temporal addition and subtraction may select an
/// `invalid: .identity`, a const generic `InvalidInputPolicy` whose
/// member is `identity`. Addition requires a value `add_(T, T) -> T`;
/// subtraction requires value `sub_(T, T) -> T` and `neg_(T) -> T`. The
/// option admits no mixed operand or result types ... Omission keeps
/// ordinary candidate selection and lifting and validity admission.
/// These temporal candidates require the explicit `.identity` option."
///
/// Core's two candidates are nodes, generic in the operand type, that
/// state those operations in `requires` (GEN-16); a call in composition
/// selects among temporal and value candidates (GEN-5).
#[test]
fn identity_arithmetic_is_selected_by_its_option() {
    let checked = with_core(concat!(
        "fn uses(a: f64, b: f64, n: i64, text: str, flag: bool) -> f64 {\n",
        // Omitted: the value candidate, lifted.
        "    let plain = a + b\n",
        "    let kept = a +{invalid: .identity} b\n",
        "    let named = add_(a, b, invalid: .identity)\n",
        "    let less = a -{invalid: .identity} b\n",
        "    let whole = n -{invalid: .identity} n\n",
        "    let words = text +{invalid: .identity} text\n",
        // No mixed operands; a `bool` is not added; a `str` is not
        // subtracted.
        "    let mixed = a +{invalid: .identity} n\n",
        "    let bad = flag +{invalid: .identity} flag\n",
        "    let gone = text -{invalid: .identity} text\n",
        "    return kept\n",
        "}\n",
        // A generic graph states what its parameter needs (GEN-7).
        "fn stated<T>(lhs: T, rhs: T) -> T\n",
        "    requires add_(T, T) -> T => lhs +{invalid: .identity} rhs\n",
        "fn unstated<T>(lhs: T, rhs: T) -> T => lhs +{invalid: .identity} rhs\n",
        // A value function selects among value candidates, and none
        // declares the option (EXPR-10: a configuration entry names an
        // option of the candidates).
        "const fn value(a: f64, b: f64) -> f64 => a +{invalid: .identity} b\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:19 operator/operator.no_candidate OPS-1",
            "m.hgl:10:20 operator/operator.no_candidate OPS-1",
            "m.hgl:11:21 operator/operator.no_candidate OPS-1",
            "m.hgl:16:44 operator/operator.no_candidate OPS-1",
            "m.hgl:17:46 name/argument.unknown EXPR-10",
        ]
    );
    // The role of the candidate each of the first four locals calls.
    let module = checked.program.module("m").expect("module m exists");
    let index = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == "uses")
        .expect("the declaration exists");
    let body = module.bodies[index].as_ref().expect("it has a body");
    let roles: Vec<Role> = body.statements[..4]
        .iter()
        .map(|statement| {
            let Stmt::Declare {
                value: Some(value), ..
            } = statement
            else {
                panic!("a local with a value: {statement:?}");
            };
            let ExprKind::Call {
                callee:
                    Callee::Candidate {
                        module,
                        declaration,
                        ..
                    },
                ..
            } = &value.kind
            else {
                panic!("a candidate's call: {:?}", value.kind);
            };
            match checked.program.modules[*module].declarations[*declaration].kind {
                DeclarationKind::Implementation(role, _)
                | DeclarationKind::NativeCandidate(role, _) => role,
                other => panic!("a candidate: {other:?}"),
            }
        })
        .collect();
    assert_eq!(
        roles,
        [Role::Value, Role::Temporal, Role::Temporal, Role::Temporal]
    );
    let locals: Vec<String> = body.locals[5..11]
        .iter()
        .map(|local| {
            let connection = match local.category {
                ValueCategory::Connection => " (connection)",
                _ => "",
            };
            format!("{}: {}{connection}", local.name, local.ty)
        })
        .collect();
    assert_eq!(
        locals,
        [
            "plain: f64 (connection)",
            "kept: f64 (connection)",
            "named: f64 (connection)",
            "less: f64 (connection)",
            "whole: i64 (connection)",
            "words: str (connection)",
        ]
    );
}
