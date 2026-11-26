//! The static target and declared slots of a callback under GEN-17.

mod common;

use common::{Checked, module};
use hgl_analyser::hir::{Callee, ExprKind, Stmt};
use hgl_analyser::{FunctionType, Pack, Scalar, Type};

/// A fixed callback input in a candidate and a plain function has the
/// same FN-17 mode restriction, including a `const` parameter (FN-1).
#[test]
fn a_candidates_fixed_callback_rejects_a_const_input() {
    let checked = module(
        r#"operator call2<F>(f: F, x: i64) -> i64
impl fn call2(f: fn(i64, i64) -> i64, x: i64) -> i64 => f(x, x)
fn plain2(f: fn(i64, i64) -> i64, x: i64) -> i64 => f(x, x)
fn const_key(member: i64, const key: i64) -> i64 => member + key
fn plain_key(member: i64, key: i64) -> i64 => member + key
fn through_candidate(x: i64) -> i64 => call2(const_key, x)
fn through_function(x: i64) -> i64 => plain2(const_key, x)
fn through_plain_candidate(x: i64) -> i64 => call2(plain_key, x)

operator map_each<F>(f: F, items: dict<i64, i64>) -> dict<i64, i64>
impl fn map_each(f: fn(i64, i64) -> i64, items: dict<i64, i64>) -> dict<i64, i64> => map(f, items)
fn through_map(items: dict<i64, i64>) -> dict<i64, i64> => map_each(const_key, items)
"#,
    );
    assert_eq!(checked.findings.len(), 3, "{}", checked.report);
    for finding in &checked.findings {
        assert_eq!(finding.code, "type/type.mismatch", "{}", checked.report);
        assert_eq!(finding.rule, "FN-17", "{}", checked.report);
        assert!(
            finding.message.contains("takes a plain parameter"),
            "{}",
            checked.report
        );
    }
}

/// The static argument after FN-17 matching, independently of what
/// the enclosing call resolves from the argument's reflected slots.
fn callback<'a>(checked: &'a Checked, caller: &str) -> (&'a Callee, &'a FunctionType) {
    let module = checked.program.module("m").unwrap();
    let index = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == caller)
        .unwrap();
    let body = module.bodies[index].as_ref().unwrap();
    let [
        Stmt::Return {
            value: Some(call), ..
        },
    ] = body.statements.as_slice()
    else {
        panic!("expected the caller to return the call: {}", checked.report);
    };
    let ExprKind::Call { arguments, .. } = &call.kind else {
        panic!("expected a call: {}", checked.report);
    };
    let value = &arguments[0].value;
    let (ExprKind::StaticCallable(target), Type::Function(actual)) = (&value.kind, &value.ty)
    else {
        panic!("expected a typed static callback: {}", checked.report);
    };
    (target, actual)
}

#[test]
fn reflected_slots_keep_names_modes_defaults_and_the_positional_pack() {
    // FN-17 and GEN-17: only the written prefix must be plain. The
    // remaining slots keep their modes, and FN-8 reads their defaults
    // and positional-pack shape when matching a dependent call.
    let checked = module(
        "const fn inspect<...Ps>(f: const fn(scale: f64, ...{Ps}) -> f64) -> i64 => 1
const fn total(scale: f64, var index: i64, weight: f64 = 1.0, values: ...f64{1:3}) -> f64 => scale
const fn other(scale: f64, var index: i64, weight: f64 = 1.0, values: ...f64{1:3}) -> f64 => weight
const fn use_total() -> i64 => inspect(total)
const fn use_other() -> i64 => inspect(other)",
    );
    let (target, actual) = callback(&checked, "use_total");
    let (other, _) = callback(&checked, "use_other");
    let (module, declaration) = target.declaration().unwrap();
    assert_eq!(
        checked.program.modules[module].declarations[declaration].name,
        "total"
    );
    assert_ne!(target.declaration(), other.declaration());
    let inputs: Vec<_> = actual
        .parameters
        .iter()
        .map(|input| input.declared().unwrap())
        .collect();
    assert_eq!(
        inputs
            .iter()
            .map(|parameter| parameter.name.as_str())
            .collect::<Vec<_>>(),
        ["scale", "index", "weight", "values"]
    );
    assert!(inputs[1].var);
    assert!(!inputs[1].has_default);
    assert!(inputs[2].has_default);
    assert_eq!(inputs[3].pack, Some(Pack::Positional));
    assert_eq!(inputs[3].pack_bounds, (1, Some(3)));
    assert_eq!(inputs[3].ty, Type::Scalar(Scalar::F64));
}

#[test]
fn a_callback_keeps_its_explicit_positional_type_pack() {
    // FN-17 and GEN-8: an explicit type sequence belongs to the concrete
    // callback identity. Its signature still names the positional pack.
    let checked = module(
        "const fn inspect<...Ps>(f: const fn(...{Ps}) -> f64) -> i64 => 1
const fn total<...Ts>(scale: f64, values: ...Ts) -> f64 => scale
const fn with_types() -> i64 => inspect(total<i64, f64>)",
    );
    let (target, actual) = callback(&checked, "with_types");
    let [pack] = target.type_packs() else {
        panic!("expected one explicitly supplied callback pack");
    };
    assert_eq!(pack.parameter, 0);
    assert_eq!(
        pack.types,
        [Type::Scalar(Scalar::I64), Type::Scalar(Scalar::F64)]
    );
    let positional = actual.parameters[1].declared().unwrap();
    assert_eq!(positional.pack, Some(Pack::Positional));
    assert!(matches!(&positional.ty, Type::Parameter { name, .. } if name == "Ts"));
}

#[test]
fn written_operator_callbacks_respect_finite_result_memberships() {
    // GEN-3 and FN-17: admit any member of each stated set. The receiver
    // still checks its requirement if none of the callback's results fit.
    let read = |restriction| {
        module(&format!(
            r#"operator fee<const N: i64, A, O>(x: A) -> O
impl const fn fee(x: i64) -> i64 => x + 1
impl const fn fee(x: list<i64, 2>) -> list<i64, 2> => x
const fn receive<R, ...Ps, ...As>(f: const fn(...{{Ps}}) -> R, args: ...{{As for f}}) -> i64
    requires {restriction} && each k in arg_keys(As) {{ As[k] in {{Ps[k], list<Ps[k], 2>}} }}
    => 7
const fn entry(items: list<i64, 2>) -> i64 => receive(fee<4>, items)
"#,
        ))
    };
    for (restriction, expected) in [
        ("R in {list<i64, 2>, i64}", "i64"),
        ("R in {i64, list<i64, 2>}", "i64"),
        (
            "R in {i64, list<i64, 2>} && R in {list<i64, 3>, list<i64, 2>}",
            "list<i64, 2>",
        ),
    ] {
        let checked = read(restriction);
        assert!(!checked.rejected, "{}", checked.report);
        let (_, actual) = callback(&checked, "entry");
        assert_eq!(actual.result.as_ref().unwrap().to_string(), expected);
    }
    let rejected = read("R in {str, bool}");
    assert_eq!(
        rejected
            .findings
            .iter()
            .map(|finding| finding.code.as_str())
            .collect::<Vec<_>>(),
        ["constraint/requires.unmet"],
        "{}",
        rejected.report,
    );
}

#[test]
fn written_callback_result_membership_uses_types_from_the_reading() {
    // GEN-3: splitting flags determines K. The selected result must
    // then fit dict<K, i64>, which leaves prices whole.
    let checked = module(
        r#"operator price<const N: i64, L, A, O>(flag: L, value: A) -> O
impl const fn price(flag: bool, value: i64) -> i64 => value + 1
impl const fn price(flag: bool, value: dict<str, i64>) -> dict<str, i64> => value
const fn receive<K, R, ...Ps, ...As>(f: const fn(...{Ps}) -> R, args: ...{As for f}) -> i64
    requires any k in arg_keys(As) { As[k] == dict<K, Ps[k]> }
        && each k in arg_keys(As) { As[k] in {Ps[k], dict<K, Ps[k]>} }
        && R in {dict<K, i64>}
    => 23
const fn entry(flags: dict<str, bool>, prices: dict<str, i64>) -> i64 =>
    receive(price<4>, flags, prices)
"#,
    );
    assert!(!checked.rejected, "{}", checked.report);
    let (_, actual) = callback(&checked, "entry");
    assert_eq!(
        actual.result.as_ref().unwrap().to_string(),
        "dict<str, i64>"
    );
}

#[test]
fn written_callback_result_requirements_use_the_known_nested_type() {
    // GEN-3: the candidate gives a known result member type even where
    // a disjunction or inequality cannot supply a type on its own.
    let source = r#"operator price<const N: i64, A, O>(value: A) -> O
impl const fn price(value: i64) -> list<f64, 2> => [1.0, 2.0]
impl const fn price(value: list<i64, 2>) -> list<i64, 2> => value
const fn receive<R, ...Ps, ...As>(f: const fn(...{Ps}) -> list<R, 2>, args: ...{As for f}) -> i64
    requires each k in arg_keys(As) { As[k] in {Ps[k], list<Ps[k], 2>} }
        && RESULT_REQUIREMENT
    => 23
const fn entry(values: list<i64, 2>) -> i64 => receive(price<4>, values)
"#;
    for requirement in ["R in {i64}", "(R in {i64} || R == bool)", "R != f64"] {
        let checked = module(&source.replace("RESULT_REQUIREMENT", requirement));
        assert!(!checked.rejected, "{}", checked.report);
        let (_, actual) = callback(&checked, "entry");
        assert_eq!(actual.result.as_ref().unwrap().to_string(), "list<i64, 2>");
    }
}

#[test]
fn an_open_reflected_suffix_does_not_choose_a_callbacks_generic_type() {
    // FN-17: a bare generic callback needs enough evidence to resolve
    // its generic arguments. A pack in the template supplies no type
    // for a parameter it will reflect.
    let checked = module(
        "const fn inspect<...Ps>(f: const fn(...{Ps}) -> f64) -> i64 => 1
const fn generic<T>(extra: T) -> f64 => 1.0
const fn unresolved() -> i64 => inspect(generic)",
    );
    let findings: Vec<_> = checked
        .findings
        .iter()
        .map(|finding| (finding.code.as_str(), finding.rule))
        .collect();
    assert_eq!(
        findings,
        [("type/generic.unresolved", "FN-17")],
        "{}",
        checked.report
    );
}

#[test]
fn fixed_reflection_prefix_checks_names_types_and_modes() {
    // FN-17: reflection still requires the named, plain fixed prefix.
    let checked = module(
        "const fn inspect<...Ps>(f: const fn(scale: f64, ...{Ps}) -> f64) -> i64 => 1
const fn wrong_name(value: f64, more: i64) -> f64 => value
const fn wrong_type(scale: i64, more: f64) -> f64 => more
const fn wrong_mode(var scale: f64, more: f64) -> f64 => scale
const fn wrong_length() -> f64 => 1.0
const fn by_name() -> i64 => inspect(wrong_name)
const fn by_type() -> i64 => inspect(wrong_type)
const fn by_mode() -> i64 => inspect(wrong_mode)
const fn by_count() -> i64 => inspect(wrong_length)",
    );
    assert_eq!(
        checked
            .findings
            .iter()
            .map(|finding| finding.code.as_str())
            .collect::<Vec<_>>(),
        ["type/type.mismatch"; 4],
        "{}",
        checked.report,
    );
}

#[test]
fn an_anonymous_callback_keeps_the_annotated_reflected_input() {
    // FN-7, FN-17: the fixed part supplies scale's type. The extra
    // anonymous parameter writes its own type and is reflected.
    let checked = module(
        "const fn inspect<...Ps>(f: const fn(scale: f64, ...{Ps}) -> f64) -> i64 => 1
const fn anonymous() -> i64 => inspect(fn(scale, label: str) => scale)",
    );
    let (target, actual) = callback(&checked, "anonymous");
    assert!(matches!(target, Callee::Anonymous(_)));
    assert_eq!(actual.parameters.len(), 2);
    assert_eq!(actual.parameters[0].ty(), &Type::Scalar(Scalar::F64));
    assert_eq!(actual.parameters[1].name(), Some("label"));
    assert_eq!(actual.parameters[1].ty(), &Type::Scalar(Scalar::Str));
}

#[test]
fn reflected_templates_keep_the_exact_candidate_when_fixed_types_select_it() {
    // FN-17 and GEN-5: a reflected suffix does not alter selection by
    // the complete fixed prefix or the selected candidate's identity.
    let checked = module(
        "operator choose<T>(item: T) -> T
impl const fn choose(value: i64) -> i64 => value
impl const fn choose(value: f64) -> f64 => value
const fn inspect<...Ps>(f: const fn(item: i64, ...{Ps}) -> i64) -> i64 => 1
const fn chosen() -> i64 => inspect(choose)",
    );
    let (target, actual) = callback(&checked, "chosen");
    let Callee::Candidate {
        operator,
        module,
        declaration,
        ..
    } = target
    else {
        panic!("expected a selected operator candidate");
    };
    assert_eq!(*operator, (*module, 0));
    assert_eq!(*declaration, 1);
    assert_eq!(actual.parameters[0].name(), Some("item"));
    assert_eq!(actual.parameters[0].ty(), &Type::Scalar(Scalar::I64));
}

#[test]
fn dependent_arguments_resolve_a_bare_generic_callback() {
    let checked = module(
        r#"const fn inspect<R, ...Ps, ...As>(
    f: const fn(...{Ps}) -> R,
    args: ...{As for f},
) -> i64
    requires R == f64 && each arg in arg_keys(As) { As[arg] == Ps[arg] }
    => 1
const fn generic<T, ...Ts>(item: T, rest: ...Ts) -> f64
    requires T == str && each item_type in types(Ts) { item_type == i64 }
    => 1.0
const fn chosen() -> i64 => inspect(generic, "item", 4)
const fn empty() -> i64 => inspect(generic, item: "item")"#,
    );
    assert!(checked.findings.is_empty(), "{}", checked.report);
    let (target, actual) = callback(&checked, "chosen");
    assert_eq!(actual.parameters[0].ty(), &Type::Scalar(Scalar::Str));
    let [pack] = target.type_packs() else {
        panic!("expected one inferred callback pack");
    };
    assert_eq!(pack.types, [Type::Scalar(Scalar::I64)]);
    let (empty, _) = callback(&checked, "empty");
    assert!(empty.type_packs()[0].types.is_empty());
}
