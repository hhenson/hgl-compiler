//! The analyser's `Program::instance`: a generic declaration for the
//! types of one call of it. Each test cites its rule.

mod common;

use common::{Checked, body, module};
use hgl_analyser::hir::{Answer, Body, Callee, Expr, ExprKind, Value, walk_statements};
use hgl_analyser::{Argument, Detail, Signature};

/// No lines.
const NONE: [&str; 0] = [];

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
            Callee::Required { .. } | Callee::Unread => {}
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
