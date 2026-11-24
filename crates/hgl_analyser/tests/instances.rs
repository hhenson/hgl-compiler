//! The analyser's `Program::instance`: a generic declaration for the
//! types of one call of it. Each test cites its rule.

mod common;

use common::{Checked, body, module};
use hgl_analyser::hir::{Answer, Body, Callee, Expr, ExprKind, Raise, Value, walk_statements};
use hgl_analyser::{Argument, Detail, Signature};

/// No lines.
const NONE: [&str; 0] = [];

const INSTANCE_CALLS: [&str; 4] = [
    "checked(value)",
    "apply(checked<i64>, value)",
    "apply(fn(inner) => checked(inner), value)",
    "relay(value)",
];

/// FN-19: a computed entry has the pass's type. A written argument
/// still takes its place's type (EXPR-5, TYPE-7, TYPE-14).
#[test]
fn computed_pack_entries_have_no_callee_type_context() {
    let definitions = r#"
enum Mode { slow, fast }
const fn speed(mode: Mode) -> i64 => if mode == Mode.fast { 2 } else { 1 }
const fn half(x: f64) -> f64 => x / 2.0
const fn maybe(x: i64?) -> i64 => if x == null { 5 } else { 6 }
const fn size(xs: list<i64>) -> i64 => 7
"#;
    let written = module(&format!(
        "{definitions}test written {{\n    speed(.fast)\n    half(3)\n    maybe(null)\n    size([])\n}}\n"
    ));
    assert_eq!(written.summary(), NONE, "{}", written.report);
    for (function, entry, result, code) in [
        ("speed", ".fast", "i64", "type/enum.context_type"),
        ("half", "3", "f64", "type/type.mismatch"),
        ("maybe", "null", "i64", "type/null.context_type"),
        ("size", "[]", "i64", "type/sequence.context_type"),
    ] {
        let pack = format!("{{for key in arg_keys(A): if key == 0 {{ {entry} }}}}");
        for body in [
            format!("=> {function}(...{pack})"),
            format!("{{\n    let given = {pack}\n    return {function}(...given)\n}}"),
        ] {
            let found = module(&format!(
                "{definitions}const fn through<...A>(items: ...A) -> {result} {body}\n\
                 test computed {{ through(1) }}\n"
            ));
            assert_eq!(found.findings.len(), 1, "{body}: {}", found.report);
            assert_eq!(found.findings[0].code, code, "{body}: {}", found.report);
        }
    }
}

/// UNIT-12: reusing a pass name reports the duplicate alone.
#[test]
fn duplicated_pack_pass_names_report_only_the_duplicate() {
    let checked = module(
        "const fn one(a: i64 = 0) -> i64 => a\n\
         const fn abc(a: i64, b: i64, c: i64) -> i64 => a * 100 + b * 10 + c\n\
         const fn shadow<...A>(items: ...A) -> i64 =>\n    \
             abc(...{for k in arg_keys(A): one(...{for k in arg_keys(A): if k == 2 { items[k] }})})\n\
         test computed { shadow(1, 2, 3) }\n",
    );
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "name/name.duplicate");
}

/// GEN-7, FN-15: each route to the same instance checks its candidate's
/// `var` argument even when every candidate gives the same result type.
#[test]
fn selected_arguments_are_checked_through_functions_and_forwarded_packs() {
    let definitions = concat!(
        "operator update<T>(value: T) -> i64\n",
        "impl const fn update<T>(value: T) -> i64 => 1\n",
        "impl const fn update(var value: i64) -> i64 {\n",
        "    value = value + 1\n",
        "    return 2\n",
        "}\n",
        "const fn checked<T>(value: T) -> i64 => update(value)\n",
        "const fn apply(f: const fn(i64) -> i64, x: i64) -> i64 => f(x)\n",
        "const fn relay<...Args>(values: ...Args) -> i64 => checked(...values)\n",
    );
    for call in [
        "checked(1)",
        "apply(checked<i64>, 1)",
        "apply(fn(value) => checked(value), 1)",
        "relay(1)",
    ] {
        let checked = module(&format!("{definitions}test invoke {{ {call} }}\n"));
        assert_eq!(checked.findings.len(), 1, "{call}: {}", checked.report);
        let finding = &checked.findings[0];
        assert_eq!(
            (finding.code.as_str(), finding.rule),
            ("name/binding.immutable", "FN-15"),
            "{call}: {}",
            checked.report,
        );
        let column = 15 + call.find("checked").unwrap_or(0);
        assert_eq!(
            finding.at,
            format!("m.hgl:11:{column}"),
            "{}",
            checked.report
        );
        assert!(
            checked.report.contains("`checked<i64>`"),
            "{}",
            checked.report
        );
        assert!(
            checked.report.contains("this call makes the instance"),
            "{}",
            checked.report
        );
    }
    let accepted = module(&format!(
        "{definitions}test invoke {{\n    assert checked(\"value\") == 1\n    assert relay(\"value\") == 1\n}}\n"
    ));
    assert_eq!(accepted.summary(), NONE, "{}", accepted.report);
}

/// FN-19, CTRL-16 and GEN-7: the selected pass has ordinary argument
/// checking. An unselected branch is never checked.
#[test]
fn computed_pack_passes_check_selected_calls_at_the_making_call() {
    for (entry, codes) in [
        ("ident_i(cb)", &["local:type/function.value"][..]),
        ("holder(x: cb).x", &["local:type/function.value"][..]),
        (
            "identity(1, 2, 3)",
            &["type/argument.unexpected", "type/argument.unexpected"][..],
        ),
        (
            "if k == 0 { cb(args[k]) } else { identity(1, 2, 3) }",
            &[][..],
        ),
        (
            "if k == \"named\" { identity(1, 2, 3) } else { cb(args[k]) }",
            &[][..],
        ),
        (
            "if \"named\" != (k) { cb(args[k]) } else { identity(1, 2, 3) }",
            &[][..],
        ),
        (
            "if k == 0 { identity(1, 2, 3) } else { cb(args[k]) }",
            &["type/argument.unexpected", "type/argument.unexpected"][..],
        ),
    ] {
        let checked = module(&format!(
            "const fn identity(x: i64) -> i64 => x\n\
             const fn ident_i(x: i64) -> i64 => x\n\
             struct holder {{ x: i64 }}\n\
             const fn over<...A>(cb: const fn(i64) -> i64, args: ...A) -> i64 =>\n\
                 identity(...{{for k in arg_keys(A): {entry}}})\n\
             test invoke {{ over(identity, 5) }}\n"
        ));
        let reported: Vec<_> = checked
            .findings
            .iter()
            .map(|finding| (finding.at.as_str(), finding.code.as_str()))
            .collect();
        let expected: Vec<_> = codes.iter().map(|code| ("m.hgl:7:15", *code)).collect();
        assert_eq!(reported, expected, "{entry}: {}", checked.report);
        if !codes.is_empty() {
            assert!(
                checked.report.contains("`over<[i64]>`"),
                "{}",
                checked.report
            );
            assert!(
                checked.report.contains("the instance's body rejects here"),
                "{}",
                checked.report
            );
        }
    }
}

/// FN-19: an ordinary const parameter remains a valid condition even
/// when the current pass cannot read its value at compile time.
#[test]
fn computed_pack_passes_leave_unread_const_conditions_deferred() {
    let checked = module(concat!(
        "const fn pick(value: i64 = 3) -> i64 => value\n",
        "const fn collect<...Ts>(const keep: bool, items: ...Ts) -> i64 =>\n",
        "    pick(...{for key in arg_keys(Ts): if keep { items[key] }})\n",
        "test invoke { assert collect(true, 5) == 5 }\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
}

/// GEN-7: a call reached only through a computed entry sees the result
/// type its selected candidate gives. Its error reports both instances.
#[test]
fn a_computed_pack_entry_checks_its_nested_instance() {
    for (extent, rejects) in [(2, false), (3, true)] {
        let checked = module(&format!(
            "operator items_of<T, C>(value: T) -> C\n\
             impl const fn items_of<T>(value: T) -> list<i64, 2> => [1, 2]\n\
             impl const fn items_of(value: i64) -> list<i64, 3> => [value, value + 1, value + 2]\n\
             operator marker<T, C>(value: T) -> C\n\
             impl const fn marker<T>(value: T) -> list<i64, 1> => [9]\n\
             impl const fn marker(value: list<i64, {extent}>) -> set<i64> => [3]\n\
             const fn first<T>(value: T) -> i64 => marker(value)[0]\n\
             const fn take_one(x: i64) -> i64 => x\n\
             const fn over<...A>(xs: ...A) -> i64 =>\n\
                 take_one(...{{for k in arg_keys(A): first(items_of(xs[k]))}})\n\
             test invoke {{ over(5) }}\n"
        ));
        if rejects {
            assert_eq!(
                checked.summary(),
                ["m.hgl:12:15 type/type.mismatch EXPR-7"],
                "{}",
                checked.report
            );
            for instance in ["first<list<i64, 3>>", "over<[i64]>"] {
                assert!(checked.report.contains(instance), "{}", checked.report);
            }
        } else {
            assert_eq!(checked.summary(), NONE, "{}", checked.report);
        }
    }
}

/// GEN-7: each distinct outer caller gets one report with its call chain.
#[test]
fn a_rejected_instance_reports_each_making_call() {
    let checked = module(concat!(
        "operator update<T>(value: T) -> i64\n",
        "impl const fn update<T>(value: T) -> i64 => 1\n",
        "impl const fn update(var value: i64) -> i64 => value\n",
        "const fn checked<T>(value: T) -> i64 => update(value)\n",
        "const fn forward<T>(value: T) -> i64 => checked(value)\n",
        "const fn relay<...Args>(values: ...Args) -> i64 => checked(...values)\n",
        "test invoke {\n",
        "    checked(1)\n",
        "    forward(2)\n",
        "    relay(3)\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:9:5 name/binding.immutable FN-15",
            "m.hgl:10:5 name/binding.immutable FN-15",
            "m.hgl:11:5 name/binding.immutable FN-15",
        ],
        "{}",
        checked.report
    );
    assert!(
        checked.report.contains("`checked<i64>`"),
        "{}",
        checked.report
    );
    assert_eq!(
        checked
            .report
            .matches("this call makes the instance")
            .count(),
        5,
        "{}",
        checked.report
    );
    for instance in ["forward<i64>", "relay<[i64]>"] {
        assert!(
            checked
                .report
                .contains(&format!("this call makes the instance `{instance}`")),
            "{}",
            checked.report
        );
    }
}

/// GEN-7, FN-1 and FN-3: the selected candidate can require a constant
/// in value code or a fixed value in composition.
#[test]
fn selected_constants_are_checked_through_functions_and_forwarded_packs() {
    let candidates = concat!(
        "operator update<T>(value: T) -> i64\n",
        "impl const fn update<T>(value: T) -> i64 => 1\n",
        "impl const fn update(const value: i64) -> i64 => value\n",
    );
    for (phase, diagnosis) in [
        ("const ", "type/argument.constant FN-1"),
        ("", "phase/connection.as_constant FN-3"),
    ] {
        for call in INSTANCE_CALLS {
            let checked = module(&format!(
                "{candidates}\
                 {phase}fn checked<T>(value: T) -> i64 => update(value)\n\
                 {phase}fn apply(f: {phase}fn(i64) -> i64, value: i64) -> i64 => f(value)\n\
                 {phase}fn relay<...Args>(values: ...Args) -> i64 => checked(...values)\n\
                 {phase}fn invoke(value: i64) -> i64 => {call}\n"
            ));
            let column = 33 + phase.len() + call.find("checked").unwrap_or(0);
            assert_eq!(
                checked.summary(),
                [format!("m.hgl:8:{column} {diagnosis}")],
                "{call}: {}",
                checked.report
            );
            assert!(
                checked.report.contains("`checked<i64>`"),
                "{}",
                checked.report
            );
            assert!(
                checked.report.contains("this call makes the instance"),
                "{}",
                checked.report
            );
        }
    }
}

/// GEN-7 and EXPR-14: a configuration can lose its fixed value when its
/// deferred call selects a temporal candidate.
#[test]
fn selected_settings_are_checked_through_functions_and_forwarded_packs() {
    let definitions = concat!(
        "operator helper<T>(input: list<T>) -> f64\n",
        "impl const fn helper<T>(input: list<T>) -> f64 => 0.1\n",
        "impl fn helper(input: list<i64>) -> f64 { when { return 0.1 } }\n",
        "const fn near{abs: f64}(a: f64, b: f64) -> bool => a =={abs: abs} b\n",
        "fn checked<T>(value: T) -> bool {\n",
        "    let fixed: list<T> = []\n",
        "    return near{abs: helper(fixed)}(2.5, 2.55)\n",
        "}\n",
        "fn apply(f: fn(i64) -> bool, value: i64) -> bool => f(value)\n",
        "fn relay<...Args>(values: ...Args) -> bool => checked(...values)\n",
    );
    for call in INSTANCE_CALLS {
        let checked = module(&format!(
            "{definitions}fn invoke(value: i64) -> bool => {call}\n"
        ));
        let column = 34 + call.find("checked").unwrap_or(0);
        assert_eq!(
            checked.summary(),
            [format!(
                "m.hgl:12:{column} phase/connection.as_constant EXPR-14"
            )],
            "{call}: {}",
            checked.report
        );
        assert!(
            checked.report.contains("`checked<i64>`"),
            "{}",
            checked.report
        );
        assert!(
            checked.report.contains("this call makes the instance"),
            "{}",
            checked.report
        );
    }
}

/// What a call calls, as the instance of it: the declaration, what its
/// generic parameters stand for, and what answers its requirements.
type Called = (usize, usize, Vec<Argument>, Vec<Answer>);

/// What each call in `body` calls, in the order the calls are met; a
/// call with no declaration selected is left out.
fn calls(body: &Body) -> Vec<Called> {
    let mut found = Vec::new();
    walk_statements(&body.statements, &mut |value: &Expr| {
        let ExprKind::Call { callee, .. } = &value.kind else {
            return;
        };
        match callee {
            Callee::Function {
                module,
                declaration,
                generics,
                requirements,
                ..
            }
            | Callee::Candidate {
                module,
                declaration,
                generics,
                requirements,
                ..
            } => found.push((
                *module,
                *declaration,
                generics.clone(),
                requirements.clone(),
            )),
            Callee::Required { .. }
            | Callee::Parameter { .. }
            | Callee::Anonymous(_)
            | Callee::DeferredOperator(_)
            | Callee::DeferredPackCall
            | Callee::DeferredValue
            | Callee::Unread => {}
        }
    });
    found
}

/// A signature as a test reads it: `(a: f64, b: f64) -> f64`.
fn signature(signature: &Signature) -> String {
    let parameters: Vec<String> = signature
        .parameters
        .iter()
        .map(|parameter| format!("{}: {}", parameter.name, parameter.ty))
        .collect();
    let result = signature
        .result
        .as_ref()
        .map_or(String::new(), |result| format!(" -> {result}"));
    format!("({}){result}", parameters.join(", "))
}

/// A declaration as a test reads it: its name and its parameters'
/// types, `add_(f64, f64)`.
fn declared(checked: &Checked, module: usize, declaration: usize) -> String {
    let within = &checked.program.modules[module];
    let Some(Detail::Callable(read)) = within.details.get(declaration) else {
        panic!("a callable");
    };
    let types: Vec<String> = read
        .parameters
        .iter()
        .map(|parameter| parameter.ty.to_string())
        .collect();
    format!(
        "{}({})",
        within.declarations[declaration].name,
        types.join(", ")
    )
}

/// What each call in `body` calls, as a test reads it: the declaration,
/// then its generic arguments in angle brackets where it has any.
fn called(checked: &Checked, body: &Body) -> Vec<String> {
    calls(body)
        .into_iter()
        .map(|(module, declaration, generics, _)| {
            let name = declared(checked, module, declaration);
            if generics.is_empty() {
                name
            } else {
                let given: Vec<String> = generics
                    .iter()
                    .map(|argument| match argument {
                        Argument::Type(ty) => ty.to_string(),
                        Argument::Constant(constant) => constant.text.clone(),
                        Argument::NoResult => "none".to_string(),
                    })
                    .collect();
                format!("{name} <{}>", given.join(", "))
            }
        })
        .collect()
}

/// The instance the first call in the body of `name` asks for.
fn first_instance(checked: &Checked, name: &str) -> (Signature, Body) {
    let (module, declaration, generics, requirements) = calls(body(checked, name)).remove(0);
    checked
        .program
        .instance(module, declaration, &generics, &requirements)
        .expect("the callee has a body")
}

/// GEN-7: a selected result reaches inferred bindings and calls that
/// use the binding's whole type.
#[test]
fn an_instance_uses_the_selected_candidates_result() {
    let checked = module(concat!(
        "operator items<T, C>(value: T) -> C\n",
        "impl const fn items<T>(value: T) -> list<i64, 2> => [1, 2]\n",
        "impl const fn items(value: i64) -> list<i64, 3> => [1, 2, 3]\n",
        "operator count<C>(values: C) -> i64\n",
        "impl const fn count(values: list<i64, 2>) -> i64 => 2\n",
        "impl const fn count(values: list<i64, 3>) -> i64 => 3\n",
        "const fn size<T>(value: T) -> i64 {\n",
        "    let values = items(value)\n",
        "    return count(values)\n",
        "}\n",
        "const fn concrete() -> i64 => size(5)\n",
    ));
    assert_eq!(checked.summary(), NONE);
    let (_, instance) = first_instance(&checked, "concrete");
    assert_eq!(instance.locals[1].ty.to_string(), "list<i64, 3>");
    assert_eq!(
        called(&checked, &instance),
        ["items(i64)", "count(list<i64, 3>)"]
    );
}

/// GEN-7 and EXPR-7: when the instance chooses a set, its bracket read
/// rejects at the making call and names the operator call in the body.
#[test]
fn an_instance_rejects_an_index_into_its_selected_set() {
    let checked = module(concat!(
        "operator items<T, C>(value: T) -> C\n",
        "impl const fn items<T>(value: T) -> list<i64, 2> => [1, 2]\n",
        "impl const fn items(value: bool) -> set<i64> => [3]\n",
        "const fn first<T>(value: T) -> i64 => items(value)[0]\n",
        "const fn concrete() -> i64 => first(true)\n",
    ));
    assert!(checked.rejected, "{}", checked.report);
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.summary(), ["m.hgl:6:31 type/type.mismatch EXPR-7"]);
    assert!(checked.report.contains("first<bool>"), "{}", checked.report);
}

/// GEN-7 and EXPR-5: a return checks the selected type of an inferred
/// local. The local has no annotation to guide the earlier selection.
#[test]
fn an_instance_rejects_a_return_of_its_selected_result() {
    let checked = module(concat!(
        "operator get_value<T, R>(value: T) -> R\n",
        "impl const fn get_value<T>(value: T) -> i64 => 5\n",
        "impl const fn get_value(value: i64) -> str => \"different\"\n",
        "const fn inferred<T>(value: T) -> i64 {\n",
        "    let answer = get_value(value)\n",
        "    return answer\n",
        "}\n",
        "const fn concrete() -> i64 => inferred(3)\n",
    ));
    assert!(checked.rejected, "{}", checked.report);
    assert_eq!(checked.summary(), ["m.hgl:9:31 type/type.mismatch EXPR-5"]);
    assert!(
        checked.report.contains("inferred<i64>"),
        "{}",
        checked.report
    );
}

/// GEN-7: a changed result can make a new instance of its consumer.
/// Checking the original consumer's instance does not check that call.
#[test]
fn an_instance_checks_the_consumer_of_its_selected_result() {
    let checked = module(concat!(
        "operator items<T, C>(value: T) -> C\n",
        "impl const fn items<T>(value: T) -> list<i64, 2> => [1, 2]\n",
        "impl const fn items(value: i64) -> list<i64, 3> => [1, 2, 3]\n",
        "operator marker<T, C>(value: T) -> C\n",
        "impl const fn marker<T>(value: T) -> list<i64, 2> => [1, 2]\n",
        "impl const fn marker(value: list<i64, 3>) -> set<i64> => [3]\n",
        "const fn first<T>(value: T) -> i64 => marker(value)[0]\n",
        "const fn outer<T>(value: T) -> i64 {\n",
        "    let values = items(value)\n",
        "    return first(values)\n",
        "}\n",
        "const fn concrete() -> i64 => outer(5)\n",
    ));
    assert!(checked.rejected, "{}", checked.report);
    assert_eq!(checked.summary(), ["m.hgl:13:31 type/type.mismatch EXPR-7"]);
    assert!(
        checked.report.contains("first<list<i64, 3>>"),
        "{}",
        checked.report
    );
    for instance in ["first<list<i64, 3>>", "outer<i64>"] {
        assert!(
            checked
                .report
                .contains(&format!("this call makes the instance `{instance}`")),
            "{}",
            checked.report
        );
    }
}

/// GEN-7: a changed result is checked also through a function reference,
/// an anonymous body, or a forwarded pack.
#[test]
fn instances_reached_through_functions_or_packs_check_the_selected_result() {
    let definitions = concat!(
        "operator fetch<T, R>(value: T) -> R\n",
        "impl const fn fetch<T>(value: T) -> i64 => 5\n",
        "impl const fn fetch(value: i64) -> str => \"other\"\n",
        "const fn checked<T>(value: T) -> i64 {\n",
        "    let answer = fetch(value)\n",
        "    return answer\n",
        "}\n",
    );
    for (call, chain) in [
        ("apply(checked<i64>, 1)", &[][..]),
        ("apply(fn(value) => checked(value), 1)", &[][..]),
        ("relay(1)", &["relay<[i64]>"][..]),
        ("apply(via<i64>, 1)", &["relay<[i64]>", "via<i64>"][..]),
    ] {
        let checked = module(&format!(
            "{definitions}{}{}{}test invoke {{ {call} }}\n",
            "const fn apply(f: const fn(i64) -> i64, x: i64) -> i64 => f(x)\n",
            "const fn relay<...Args>(values: ...Args) -> i64 => checked(...values)\n",
            "const fn via<T>(value: T) -> i64 => relay(value)\n",
        ));
        assert_eq!(checked.findings.len(), 1, "{call}: {}", checked.report);
        let finding = &checked.findings[0];
        assert_eq!(
            (finding.code.as_str(), finding.rule),
            ("type/type.mismatch", "EXPR-5")
        );
        assert!(
            checked.report.contains("checked<i64>"),
            "{}",
            checked.report
        );
        for instance in chain {
            assert!(
                checked
                    .report
                    .contains(&format!("this call makes the instance `{instance}`")),
                "{}",
                checked.report
            );
        }
    }
}

/// GEN-7 and EXPR-7: a concrete bracket result supplies the inner call.
#[test]
fn an_indexed_type_parameter_reaches_its_instances() {
    let definitions = concat!(
        "operator fetch<T, R>(value: T) -> R\n",
        "impl const fn fetch<T>(value: T) -> i64 => 5\n",
        "impl const fn fetch(value: i64) -> str => \"other\"\n",
        "const fn checked<T>(value: T) -> i64 {\n",
        "    let answer = fetch(value)\n",
        "    return answer\n",
        "}\n",
        "const fn first<T>(value: T) -> i64 => checked(value[0])\n",
        "const fn outer<T>(value: T) -> i64 => first([value])\n",
    );
    let accepted = module(&format!(
        "{definitions}test invoke {{ assert outer(\"text\") == 5 }}\n"
    ));
    assert_eq!(accepted.summary(), NONE, "{}", accepted.report);
    let rejected = module(&format!("{definitions}test invoke {{ outer(3) }}\n"));
    assert_eq!(
        rejected.summary(),
        ["m.hgl:11:15 type/type.mismatch EXPR-5"]
    );
    for instance in ["checked<i64>", "first<list<i64>>", "outer<i64>"] {
        assert!(
            rejected
                .report
                .contains(&format!("this call makes the instance `{instance}`")),
            "{}",
            rejected.report
        );
    }
}

const PROVISIONAL_RESULTS: &str = concat!(
    "operator items_of<T, C>(value: T) -> C\n",
    "impl const fn items_of<T>(value: T) -> list<i64, 2> => [1, 2]\n",
    "impl const fn items_of(value: i64) -> list<i64, 3> => [1, 2, 3]\n",
    "operator marker<T, C>(value: T) -> C\n",
    "impl const fn marker<T>(value: T) -> list<i64, 1> => [9]\n",
    "impl const fn marker(value: list<i64, 2>) -> set<i64> => [3]\n",
    "const fn first<T>(value: T) -> i64 => marker(value)[0]\n",
);

/// GEN-7: a provisional result supplies a consumer only in the instance
/// that selects it. Independent concrete calls still make instances.
#[test]
fn a_provisional_result_does_not_make_an_independent_instance() {
    let outer = concat!(
        "const fn outer<T>(value: T) -> i64 {\n",
        "    let items = items_of(value)\n",
        "    let alias = items\n",
        "    return first(alias)\n",
        "}\n",
    );
    let definitions = format!("{PROVISIONAL_RESULTS}{outer}");
    for suffix in [
        "",
        "test invoke { assert outer(5) == 9 }\n",
        "test invoke { let items: list<i64, 3> = [1, 2, 3]\n assert first(items) == 9 }\n",
    ] {
        let accepted = module(&format!("{definitions}{suffix}"));
        assert_eq!(accepted.summary(), NONE, "{}", accepted.report);
    }
    let rejected = module(&format!("{definitions}test invoke {{ outer(\"s\") }}\n"));
    assert_eq!(
        rejected.summary(),
        ["m.hgl:14:15 type/type.mismatch EXPR-7"]
    );
    for instance in ["first<list<i64, 2>>", "outer<str>"] {
        assert!(
            rejected
                .report
                .contains(&format!("this call makes the instance `{instance}`")),
            "{}",
            rejected.report
        );
    }
    let independent = module(&format!(
        "{definitions}const fn concrete<T>(unused: T) -> i64 {{\n    let items: list<i64, 2> = [1, 2]\n    return first(items)\n}}\n"
    ));
    assert_eq!(
        independent.summary(),
        ["m.hgl:16:12 type/type.mismatch EXPR-7"]
    );
    let called = module(&format!(
        "{definitions}\
         const fn concrete<T>(value: T) -> i64 {{\n\
             let changed = items_of(value)\n\
             let items: list<i64, 2> = [1, 2]\n\
             return first(items)\n\
         }}\n\
         test invoke {{ concrete(5) }}\n"
    ));
    assert_eq!(
        called.summary(),
        ["m.hgl:17:8 type/type.mismatch EXPR-7"],
        "{}",
        called.report
    );
    let callbacks = format!(
        "{definitions}\
         const fn apply<T>(f: const fn(T) -> i64, items: T) -> i64 => f(items)\n\
         const fn by_reference<T>(value: T) -> i64 {{\n\
             let items = items_of(value)\n\
             return apply(first, items)\n\
         }}\n"
    );
    for suffix in ["", "test invoke { assert by_reference(5) == 9 }\n"] {
        let accepted = module(&format!("{callbacks}{suffix}"));
        assert_eq!(accepted.summary(), NONE, "{}", accepted.report);
    }
    let rejected = module(&format!(
        "{callbacks}test invoke {{ by_reference(\"s\") }}\n"
    ));
    assert_eq!(
        rejected.summary(),
        ["m.hgl:19:15 type/type.mismatch EXPR-7"],
        "{}",
        rejected.report
    );
    let forwarded = format!(
        "{definitions}\
         const fn relay<...A>(items: ...A) -> i64 => first(...items)\n\
         const fn by_pack<T>(value: T) -> i64 {{\n\
             let items = items_of(value)\n\
             return relay(items)\n\
         }}\n"
    );
    for suffix in ["", "test invoke { assert by_pack(5) == 9 }\n"] {
        let accepted = module(&format!("{forwarded}{suffix}"));
        assert_eq!(accepted.summary(), NONE, "{}", accepted.report);
    }
    let rejected = module(&format!("{forwarded}test invoke {{ by_pack(\"s\") }}\n"));
    assert_eq!(
        rejected.summary(),
        ["m.hgl:19:15 type/type.mismatch EXPR-7"],
        "{}",
        rejected.report
    );
    assert!(rejected.report.contains("`relay<[list<i64, 2>]>`"));

    let indexed = format!(
        "{definitions}\
         const fn with_index<T, U>(value: T, extra: U) -> i64 {{\n\
             let unused = extra[0]\n\
             let items = items_of(value)\n\
             return first(items)\n\
         }}\n"
    );
    let accepted = module(&format!(
        "{indexed}test invoke {{ assert with_index(5, [0]) == 9 }}\n"
    ));
    assert_eq!(accepted.summary(), NONE, "{}", accepted.report);
    let rejected = module(&format!(
        "{indexed}test invoke {{ with_index(\"s\", [0]) }}\n"
    ));
    assert_eq!(
        rejected.summary(),
        ["m.hgl:19:15 type/type.mismatch EXPR-7"],
        "{}",
        rejected.report
    );

    let node = format!(
        "{definitions}\
         fn with_endpoint<T, U>(value: T, extra: U) -> i64 {{\n\
             when all_valid(extra) {{\n\
                 let read = extra\n\
                 let items = items_of(value)\n\
                 return first(items)\n\
             }}\n\
         }}\n"
    );
    let entry = |value_type| {
        module(&format!(
            "{node}fn entry(value: {value_type}, extra: tuple<i64, i64>) -> i64 => with_endpoint(value, extra)\n"
        ))
    };
    let accepted = entry("i64");
    assert_eq!(accepted.summary(), NONE, "{}", accepted.report);
    let rejected = entry("str");
    assert_eq!(rejected.findings.len(), 1, "{}", rejected.report);
    assert_eq!(rejected.findings[0].rule, "EXPR-7", "{}", rejected.report);
    assert!(rejected.report.contains("`first<list<i64, 2>>`"));
    assert!(
        rejected
            .report
            .contains("`with_endpoint<str, tuple<i64, i64>>`")
    );
}

/// GEN-7 and FN-19: the final body supplies types through captures,
/// iteration bindings and a call that consumes a forwarded pack's result.
#[test]
fn a_reread_follows_the_types_it_gives_to_consumers() {
    for route in [
        concat!(
            "const fn apply<T>(f: const fn(T) -> i64, input: T) -> i64 => f(input)\n",
            "const fn visit<T>(value: T) -> i64 {\n",
            "    let items = items_of(value)\n",
            "    return apply(fn(input) => first(items), value)\n",
            "}\n",
        ),
        concat!(
            "operator rows_of<T, C>(value: T) -> C\n",
            "impl const fn rows_of<T>(value: T) -> list<list<i64, 2>, 1> => [[1, 2]]\n",
            "impl const fn rows_of(value: i64) -> list<list<i64, 3>, 1> => [[value, value, value]]\n",
            "const fn visit<T>(value: T) -> i64 {\n",
            "    var total = 0\n",
            "    for row in rows_of(value) { total = total + first(row) }\n",
            "    return total\n",
            "}\n",
        ),
        "const fn visit<...A>(input: ...A) -> i64 => first(items_of(...input))\n",
    ] {
        let accepted = module(&format!(
            "{PROVISIONAL_RESULTS}{route}test invoke {{ assert visit(5) == 9 }}\n"
        ));
        assert_eq!(accepted.summary(), NONE, "{}", accepted.report);
        let rejected = module(&format!(
            "{PROVISIONAL_RESULTS}{route}test invoke {{ visit(\"s\") }}\n"
        ));
        assert_eq!(rejected.findings.len(), 1, "{}", rejected.report);
        assert_eq!(rejected.findings[0].rule, "EXPR-7", "{}", rejected.report);
        assert!(rejected.report.contains("`first<list<i64, 2>>`"));
    }
}

/// GEN-7: a callback's parameter is typed by the final call. This works
/// for an ordinary call and where the input comes from FN-19 forwarding.
#[test]
fn a_callback_parameter_uses_the_type_selected_in_its_instance() {
    let apply = "const fn apply<T>(f: const fn(T) -> i64, input: T) -> i64 => f(input)\n";
    for route in [
        "const fn visit<T>(value: T) -> i64 => apply(fn(input) => first(input), items_of(value))\n",
        "const fn visit<...A>(values: ...A) -> i64 => apply(fn(input) => first(input), items_of(...values))\n",
    ] {
        let call = "test invoke { assert visit(5) == 9 }\n";
        let accepted = module(&format!("{PROVISIONAL_RESULTS}{apply}{route}{call}"));
        assert_eq!(accepted.summary(), NONE, "{}", accepted.report);

        let definitions = PROVISIONAL_RESULTS
            .replace("marker(value: list<i64, 2>)", "marker(value: list<i64, 3>)");
        let rejected = module(&format!("{definitions}{apply}{route}{call}"));
        assert_eq!(
            rejected.summary(),
            ["m.hgl:11:22 type/type.mismatch EXPR-7"],
            "{}",
            rejected.report
        );
        assert!(rejected.report.contains("`first<list<i64, 3>>`"));
    }
}

/// GEN-7: non-generic bodies make instances where they write the calls or
/// references. Calling those bodies does not make those instances again.
#[test]
fn forwarding_through_a_non_generic_body_reports_only_its_making_site() {
    let checked = module(concat!(
        "operator fetch<T, R>(value: T) -> R\n",
        "impl const fn fetch<T>(value: T) -> i64 => 5\n",
        "impl const fn fetch(value: i64) -> str => \"other\"\n",
        "const fn inferred<T>(value: T) -> i64 {\n",
        "    let answer = fetch(value)\n",
        "    return answer\n",
        "}\n",
        "const fn relay<...A>(values: ...A) -> i64 => inferred(...values)\n",
        "const fn via<T>(value: T) -> i64 => relay(value)\n",
        "const fn apply(f: const fn(i64) -> i64, value: i64) -> i64 => f(value)\n",
        "const fn reference(value: i64) -> i64 => apply(via<i64>, value)\n",
        "const fn named(value: i64) -> i64 => relay(value)\n",
        "test invoke { reference(3)\n reference(4)\n named(5) }\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:12:48 type/type.mismatch EXPR-5",
            "m.hgl:13:38 type/type.mismatch EXPR-5",
        ],
        "{}",
        checked.report
    );
}

/// GEN-7, ERR-9: the type error in this instance precedes the recursive
/// call's admission and confines the error to the making site.
#[test]
fn an_ill_typed_indexed_instance_does_not_make_a_recursive_call() {
    let checked = module(concat!(
        "const fn repeat<T>(items: T, n: i64) -> i64 {\n",
        "    if n <= 0 { return 0 }\n",
        "    return repeat([items, items[0]], n - 1)\n",
        "}\n",
        "test invoke { assert repeat([7], 3) == 0 }\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:6:22 type/type.mismatch EXPR-5"],
        "{}",
        checked.report
    );
}

/// GEN-7: an unrelated changed result does not move a call's rejection.
#[test]
fn a_changed_result_does_not_relocate_other_argument_checks() {
    for (mode, expected) in [
        ("var", "name/binding.immutable FN-15"),
        ("const", "type/argument.constant FN-1"),
    ] {
        let checked = module(&format!(
            concat!(
                "operator fetch<T, R>(value: T) -> R\n",
                "impl const fn fetch<T>(value: T) -> i64 => 5\n",
                "impl const fn fetch(value: i64) -> str => \"other\"\n",
                "operator update<T>(value: T) -> i64\n",
                "impl const fn update<T>(value: T) -> i64 => 1\n",
                "impl const fn update({mode} value: i64) -> i64 => value\n",
                "const fn both<T>(value: T) -> i64 {{\n",
                "    let answer = fetch(value)\n",
                "    return update(value)\n",
                "}}\n",
                "test invoke {{ both(3) }}\n",
            ),
            mode = mode,
        ));
        assert_eq!(checked.summary(), [format!("m.hgl:12:15 {expected}")]);
        let lines: Vec<_> = checked.report.lines().collect();
        let at = lines
            .iter()
            .position(|line| line.contains("the instance's body rejects here"))
            .unwrap();
        assert!(
            lines[at - 1].contains("return update(value)"),
            "{}",
            checked.report
        );
    }
}

/// GEN-7: a failing use of an inferred binding points through an alias
/// to its initializer when another call also changed its result type.
#[test]
fn an_instance_reports_the_changed_call_that_its_local_reads() {
    let checked = module(concat!(
        "operator fetch<T, R>(value: T) -> R\n",
        "impl const fn fetch<T>(value: T) -> i64 => 5\n",
        "impl const fn fetch(value: i64) -> str => \"other\"\n",
        "operator items_of<T, R>(value: T) -> R\n",
        "impl const fn items_of<T>(value: T) -> list<i64, 2> => [1, 2]\n",
        "impl const fn items_of(value: i64) -> list<i64, 3> => [1, 2, 3]\n",
        "const fn two<T>(value: T) -> i64 {\n",
        "    let answer = fetch(value)\n",
        "    let alias = answer\n",
        "    let items = items_of(value)\n",
        "    return alias\n",
        "}\n",
        "test invoke { two(1) }\n",
    ));
    assert_eq!(checked.summary(), ["m.hgl:14:15 type/type.mismatch EXPR-5"]);
    assert!(checked.report.contains("two<i64>"), "{}", checked.report);
    assert!(
        !checked
            .report
            .contains("this call selected a different result type"),
        "{}",
        checked.report
    );
}

/// GEN-7: the inferred type of a mutable local originates at its initializer.
#[test]
fn a_changed_result_keeps_the_label_on_a_var_initializer() {
    let checked = module(concat!(
        "operator fetch<T, R>(value: T) -> R\n",
        "impl const fn fetch<T>(value: T) -> i64 => 5\n",
        "impl const fn fetch(value: i64) -> str => \"other\"\n",
        "const fn read<T>(value: T) -> i64 {\n",
        "    var answer = fetch(value)\n",
        "    return answer\n",
        "}\n",
        "test invoke { read(3) }\n",
    ));
    assert_eq!(checked.summary(), ["m.hgl:9:15 type/type.mismatch EXPR-5"]);
    let lines: Vec<_> = checked.report.lines().collect();
    let label = lines
        .iter()
        .position(|line| line.contains("the instance's body rejects here"))
        .unwrap();
    assert!(lines[label - 1].contains("var answer = fetch(value)"));
}

/// GEN-2: a call resolves each generic parameter of its callee, and
/// the instance has what each stands for in place: in the signature, in
/// the locals, and in each expression's type. GEN-16: an operation the
/// declaration requires is the call of the candidate that answers the
/// requirement at that call.
#[test]
fn an_instance_has_the_calls_types_and_candidates() {
    let checked = module(concat!(
        "const fn sum<T>(a: T, b: T) -> T requires add_(T, T) -> T {\n",
        "    let both = add_(a, b)\n",
        "    return both\n",
        "}\n",
        "const fn floats() -> f64 => sum(1.5, 2.5)\n",
        "const fn texts() -> str => sum(\"a\", \"b\")\n",
    ));
    assert_eq!(checked.summary(), NONE);
    // As it is declared, the body requires and selects nothing.
    assert_eq!(called(&checked, body(&checked, "sum")), NONE);

    let (taken, instance) = first_instance(&checked, "floats");
    assert_eq!(signature(&taken), "(a: f64, b: f64) -> f64");
    assert!(taken.generics.is_empty());
    let locals: Vec<String> = instance
        .locals
        .iter()
        .map(|local| format!("{}: {}", local.name, local.ty))
        .collect();
    assert_eq!(locals, ["a: f64", "b: f64", "both: f64"]);
    assert_eq!(called(&checked, &instance), ["add_(f64, f64)"]);

    let (taken, instance) = first_instance(&checked, "texts");
    assert_eq!(signature(&taken), "(a: str, b: str) -> str");
    assert_eq!(called(&checked, &instance), ["add_(str, str)"]);
}

#[test]
fn a_required_call_reads_configuration_as_its_selected_parameter() {
    // GEN-16, EXPR-5, ERR-5: the selected candidate reads the result's
    // value, and its failure is returned by the function doing the read.
    let checked = module(
        "operator choose<T>{fallback: result<i64, error> = ok(0)}(value: T) -> i64
impl const fn choose{fallback: i64}(value: bool) -> i64 => fallback
const fn forward<T>(value: T, fallback: result<i64, error>) -> result<i64, error> requires choose(T) -> i64 {
    return ok(choose{fallback: fallback}(value))
}
const fn use_it(fallback: result<i64, error>) -> result<i64, error> => forward(false, fallback)",
    );
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    let (_, instance) = first_instance(&checked, "use_it");
    let mut found = false;
    walk_statements(&instance.statements, &mut |value| {
        let ExprKind::Call {
            callee: Callee::Candidate { .. },
            arguments,
            ..
        } = &value.kind
        else {
            return;
        };
        let configuration = arguments
            .iter()
            .find(|argument| argument.parameter == Some(0));
        assert!(matches!(
            configuration.map(|argument| &argument.value.kind),
            Some(ExprKind::Raised {
                raise: Raise::Return,
                ..
            })
        ));
        found = true;
    });
    assert!(found);
}

/// GEN-7: a call in a generic body is answered by what that body's
/// declaration states. In the body's instance it has the candidate
/// that answers there, and the types of that instance: so the instance
/// it asks for in turn is whole.
#[test]
fn a_call_in_an_instance_asks_for_a_whole_instance() {
    let checked = module(concat!(
        "operator join<L, R, O>(lhs: L, rhs: R) -> O\n",
        "impl const fn join(lhs: i64, rhs: i64) -> i64 => lhs\n",
        "const fn twice<T>(value: T) -> T requires join(T, T) -> T => join(value, value)\n",
        "const fn outer<U>(value: U) -> list<U> requires join(U, U) -> U => [twice(value)]\n",
        "const fn top() -> list<i64> => outer(2)\n",
    ));
    assert_eq!(checked.summary(), NONE);
    let (taken, outer) = first_instance(&checked, "top");
    assert_eq!(signature(&taken), "(value: i64) -> list<i64>");
    assert_eq!(called(&checked, &outer), ["twice(T) <i64>"]);
    // The call of `twice` in that instance: its own instance.
    let (module, declaration, generics, requirements) = calls(&outer).remove(0);
    let (taken, twice) = checked
        .program
        .instance(module, declaration, &generics, &requirements)
        .expect("`twice` has a body");
    assert_eq!(signature(&taken), "(value: i64) -> i64");
    assert_eq!(called(&checked, &twice), ["join(i64, i64)"]);
}

/// GEN-13: a generic value parameter is a compile-time constant, and
/// read as a value in an instance it is the constant the call gives:
/// an integer, a duration, and a member of an enum (TYPE-7). One given
/// to a callee's generic value parameter is that constant there too.
#[test]
fn a_generic_value_is_the_constant_the_call_gives() {
    let checked = module(concat!(
        "enum Side { buy, sell }\n",
        "const fn width<const N: i64>() -> i64 => N\n",
        "const fn wait<const D: duration>() -> duration => D\n",
        // An option of a candidate: the call says which member it is.
        "operator choose(value: i64) -> Side\n",
        "impl const fn choose<const S: Side = .buy>(value: i64) -> Side => S\n",
        "const fn again<const M: i64>() -> i64 => width<M>()\n",
        "const fn three() -> i64 => width<3>()\n",
        "const fn second() -> duration => wait<1s>()\n",
        "const fn selling() -> Side => choose(1, S: Side.sell)\n",
        "const fn buying() -> Side => choose(1)\n",
        "const fn four() -> i64 => again<4>()\n",
    ));
    assert_eq!(checked.summary(), NONE);
    let returned = |name: &str| -> ExprKind {
        let (_, instance) = first_instance(&checked, name);
        let Some(hgl_analyser::hir::Stmt::Return {
            value: Some(value), ..
        }) = instance.statements.first()
        else {
            panic!("a return with a value");
        };
        value.kind.clone()
    };
    assert_eq!(returned("three"), ExprKind::Literal(Value::Int(3)));
    assert_eq!(
        returned("second"),
        ExprKind::Literal(Value::Duration(1_000_000))
    );
    assert!(
        matches!(returned("selling"), ExprKind::EnumMember { member: 1, .. }),
        "{:?}",
        returned("selling")
    );
    // The option's default, where the call gives none.
    assert!(
        matches!(returned("buying"), ExprKind::EnumMember { member: 0, .. }),
        "{:?}",
        returned("buying")
    );
    // `width<M>()` in the instance of `again<4>` is `width<4>()`.
    let (_, again) = first_instance(&checked, "four");
    assert_eq!(called(&checked, &again), ["width() <4>"]);
}

/// GEN-16, core's OPS-9: a graph that states `add_(T, T) -> T` answers
/// the requirement of the temporal candidate it calls. The instance of
/// the graph for `f64` calls that candidate for `f64`, with core's
/// `add_(f64, f64)` for its requirement; and the candidate's own
/// instance calls it.
#[test]
fn a_graphs_instance_reaches_the_value_candidate() {
    let checked = module(concat!(
        "fn identity_sum<T>(lhs: T, rhs: T) -> T\n",
        "    requires add_(T, T) -> T => lhs +{invalid: .identity} rhs\n",
        "fn wired(a: f64, b: f64) -> f64 => identity_sum(a, b)\n",
    ));
    assert_eq!(checked.summary(), NONE);
    let (taken, graph) = first_instance(&checked, "wired");
    assert_eq!(signature(&taken), "(lhs: f64, rhs: f64) -> f64");
    assert_eq!(called(&checked, &graph), ["add_(T, T) <f64, .identity>"]);
    let (module, declaration, generics, requirements) = calls(&graph).remove(0);
    let (taken, node) = checked
        .program
        .instance(module, declaration, &generics, &requirements)
        .expect("core's candidate has a body");
    assert_eq!(signature(&taken), "(lhs: f64, rhs: f64) -> f64");
    // Its one required call, in the hook for both inputs valid.
    assert_eq!(called(&checked, &node), ["add_(f64, f64)"]);
}

/// GEN-1: a call of an instance whose result is absent is a statement.
#[test]
fn resultless_calls_reject_in_value_positions() {
    let definitions = concat!(
        "fn record(value: i64) {}\n",
        "fn through<R>(f: fn(i64) -> R, value: i64) -> R => f(value)\n",
        "fn accept<T>(value: T) {}\n",
        "fn accept_nullable(value: i64?) {}\n",
    );
    for (definition, rule, detail) in [
        (
            "fn call(value: i64) { let unused = (through(record, value)) }",
            "GEN-1",
            "this call gives no result",
        ),
        (
            "fn call(value: i64) => accept(through(record, value))",
            "GEN-1",
            "this call gives no result",
        ),
        (
            "fn call(value: i64) -> i64 { return through(record, value) }",
            "FN-17",
            "the parameter takes `fn(i64) -> i64`",
        ),
        (
            "fn call(value: i64) { if through(record, value) { record(value) } }",
            "FN-17",
            "the parameter takes `fn(i64) -> bool`",
        ),
        (
            "fn call(value: i64) -> i64? => through(record, value)",
            "GEN-1",
            "this call gives no result",
        ),
        (
            "fn call(value: i64) => accept_nullable(through(record, value))",
            "GEN-1",
            "this call gives no result",
        ),
        (
            "fn call(value: i64) { let unused: i64? = through(record, value) }",
            "GEN-1",
            "this call gives no result",
        ),
        (
            "fn call(value: i64) { let unused = record(value) }",
            "FN-7",
            "this call gives no result",
        ),
    ] {
        let checked = module(&format!("{definitions}{definition}\n"));
        assert_eq!(checked.findings.len(), 1, "{}", checked.report);
        assert_eq!(checked.findings[0].code, "type/type.mismatch");
        assert_eq!(checked.findings[0].rule, rule, "{}", checked.report);
        assert!(checked.report.contains(detail), "{}", checked.report);
    }
    let checked = module(&format!(
        "{definitions}fn call(value: i64) => through(fn(input) => {{ (record(input)) }}, value)\n"
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
}

/// GEN-1 and GEN-7: a resultless binding does not supply a type in a body.
#[test]
fn written_types_in_a_body_cannot_use_a_resultless_binding() {
    let definitions = concat!(
        "fn record(value: i64) {}\n",
        "fn keep<T>(value: T) -> T => value\n",
        "struct Box<T> { value: T? }\n",
    );
    for statement in [
        "let unused = keep<R?>(null)",
        "var unused: Box<R>",
        "let unused: list<R> = []",
        "let unused = Box<R>(value: null)",
    ] {
        let checked = module(&format!(
            "{definitions}fn through<R>(f: fn(i64) -> R, value: i64) -> R {{\n                 {statement}\n    f(value)\n}}\nfn call(value: i64) => through(record, value)\n"
        ));
        assert_eq!(checked.findings.len(), 1, "{}", checked.report);
        assert_eq!(checked.findings[0].code, "type/type.mismatch");
        assert_eq!(checked.findings[0].rule, "GEN-1", "{}", checked.report);
        assert!(
            checked.report.contains("this requires a type")
                && checked.report.contains("through<none>"),
            "{}",
            checked.report,
        );
    }
}

/// GEN-7: an explicit return stays a return in an instance with no result.
#[test]
fn returning_a_resultless_callback_rejects_at_the_call() {
    let checked = module(concat!(
        "fn record(value: i64) {}\n",
        "fn through<R>(f: fn(i64) -> R, value: i64) -> R { return f(value) }\n",
        "fn call(value: i64) => through(record, value)\n",
    ));
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "type/type.mismatch");
    assert_eq!(checked.findings[0].at, "m.hgl:4:24");
    assert!(
        checked
            .report
            .contains("this call makes the instance `through<none>`")
    );
}

/// GEN-1 and FN-17: an absence can only bind a complete result slot.
#[test]
fn resultless_callbacks_respect_all_uses_of_the_result_type() {
    let definitions = "fn record(value: i64) {}\n";
    for signature in [
        "fn use_it<R>(f: fn(i64) -> R, value: i64) -> list<R> => [f(value)]",
        "fn use_it<R>(f: fn(i64) -> R, value: R) -> R => value",
        "fn use_it<R>(f: fn(i64) -> R, value: i64) -> ref R { when {} }",
        "fn use_it<R>(f: fn(i64) -> R, value: i64) -> R requires R in {i64} => f(value)",
    ] {
        let checked = module(&format!(
            "{definitions}{signature}\nfn call(value: i64) => use_it(record, value)\n"
        ));
        assert_eq!(checked.findings.len(), 1, "{}", checked.report);
        assert_eq!(
            checked.findings[0].code, "type/type.mismatch",
            "{}",
            checked.report
        );
    }
    let checked = module(concat!(
        "const fn each_of(f: const fn(i64), value: i64) => f(value)\n",
        "const fn add_one(value: i64) -> i64 => value + 1\n",
        "const fn call() => each_of(add_one, 2)\n",
    ));
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "type/type.mismatch");
}

/// GEN-1 and FN-17: a result bound to a type parameter requires a result.
#[test]
fn a_bound_result_parameter_rejects_a_resultless_callback() {
    let definitions = concat!(
        "fn record(value: i64) {}\n",
        "fn through<R>(f: fn(i64) -> R, value: i64) -> R => f(value)\n",
    );
    for (result_type, body) in [
        ("T", "let got: T = through<T>(record, value)\n    got"),
        ("T", "through<T>(record, value)\n    seed"),
        ("i64", "let got: i64 = through<i64>(record, value)\n    got"),
    ] {
        let checked = module(&format!(
            "{definitions}fn outer<T>(seed: T, value: i64) -> {result_type} {{\n    {body}\n}}\n\
             fn caller(value: i64) -> i64 => outer(1, value)\n"
        ));
        assert_eq!(checked.findings.len(), 1, "{}", checked.report);
        assert_eq!(checked.findings[0].code, "type/type.mismatch");
        assert_eq!(checked.findings[0].rule, "FN-17");
        assert!(
            checked
                .report
                .contains(&format!("the parameter takes `fn(i64) -> {result_type}`"))
        );
    }
}

/// GEN-2: dropping the call's value does not resolve the generic.
#[test]
fn discarding_a_call_does_not_bind_a_result_parameter_to_none() {
    let checked = module("fn make<T>() -> T {}\nfn call() => make()\n");
    assert_eq!(checked.findings.len(), 1, "{}", checked.report);
    assert_eq!(checked.findings[0].code, "type/generic.unresolved");
}
