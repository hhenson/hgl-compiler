//! The static target and declared slots of a callback under GEN-17.

mod common;

use common::{Checked, module};
use hgl_analyser::hir::{Callee, ExprKind, Stmt};
use hgl_analyser::{FunctionType, Pack, Scalar, Type};

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
