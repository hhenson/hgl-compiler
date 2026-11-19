//! How a `requires`, an operator's candidate and a generic argument are
//! judged for each concrete instance of a generic function (GEN-3,
//! GEN-5, GEN-6, GEN-8, ERR-9, UNIT-13). A condition that is not known
//! where the generic is declared is neither accepted nor rejected there:
//! it is judged where an instance makes it known. Each test cites its
//! rule.

mod common;

use common::{Checked, body, module};
use hgl_analyser::hir::{Body, Callee, Expr, ExprKind, walk_statements};
use hgl_analyser::{Argument, Detail};

/// No findings.
const NONE: [&str; 0] = [];

/// The position of module `m` in the program, and of its declaration
/// `name` among its declarations.
fn declaration_of(checked: &Checked, name: &str) -> (usize, usize) {
    let module = checked
        .program
        .modules
        .iter()
        .position(|module| module.path == "m")
        .expect("module m exists");
    let declaration = checked.program.modules[module]
        .declarations
        .iter()
        .position(|declared| declared.name == name)
        .expect("the declaration exists");
    (module, declaration)
}

/// What each call in `body` calls, in the order the calls are met: the
/// function or the candidate by its name and its parameters' types,
/// `kind_of(i64)`, then its generic arguments in angle brackets where it
/// has any. A call with no candidate selected is `none`.
fn called(checked: &Checked, body: &Body) -> Vec<String> {
    let mut found = Vec::new();
    walk_statements(&body.statements, &mut |value: &Expr| {
        let ExprKind::Call { callee, .. } = &value.kind else {
            return;
        };
        let (module, declaration, generics) = match callee {
            Callee::Function {
                module,
                declaration,
                generics,
                ..
            }
            | Callee::Candidate {
                module,
                declaration,
                generics,
                ..
            } => (*module, *declaration, generics),
            Callee::Required { .. } => {
                found.push("required".to_string());
                return;
            }
            Callee::Unread => {
                found.push("none".to_string());
                return;
            }
        };
        let within = &checked.program.modules[module];
        // Core's own candidates, `add_(i64, i64)`, are named as any.
        let Some(Detail::Callable(read)) = within.details.get(declaration) else {
            panic!("a callable");
        };
        let types: Vec<String> = read
            .parameters
            .iter()
            .map(|parameter| parameter.ty.to_string())
            .collect();
        let mut name = format!(
            "{}({})",
            within.declarations[declaration].name,
            types.join(", ")
        );
        if !generics.is_empty() {
            let given: Vec<String> = generics
                .iter()
                .map(|argument| match argument {
                    Argument::Type(ty) => ty.to_string(),
                    Argument::Constant(constant) => constant.text.clone(),
                })
                .collect();
            name = format!("{name} <{}>", given.join(", "));
        }
        found.push(name);
    });
    found
}

/// The first call of a function in the body of `caller`: what it calls,
/// with what it gives that function's generic parameters and what
/// answers its requirements there.
fn first_call(body: &Body) -> (usize, usize, Vec<Argument>, Vec<hgl_analyser::hir::Answer>) {
    let mut found = None;
    walk_statements(&body.statements, &mut |value: &Expr| {
        if found.is_some() {
            return;
        }
        if let ExprKind::Call {
            callee:
                Callee::Function {
                    module,
                    declaration,
                    generics,
                    requirements,
                },
            ..
        } = &value.kind
        {
            found = Some((
                *module,
                *declaration,
                generics.clone(),
                requirements.clone(),
            ));
        }
    });
    found.expect("a call of a function")
}

/// The body of the instance the first call of a function in the body of
/// `caller` asks for.
fn instance_called_by(checked: &Checked, caller: &Body) -> Body {
    let (module, declaration, generics, requirements) = first_call(caller);
    checked
        .program
        .instance(module, declaration, &generics, &requirements)
        .expect("the callee has a body")
        .1
}

/// GEN-3: "A positive conjunctive equality may bind one unresolved
/// generic from known data; under negation/disjunction it only tests
/// already-resolved data." In `wrap<U>`, `!(T == i64)` of the candidate
/// `kind_of<T>` is not told while `U` is not given: it is neither held
/// nor refused there, and the generic body selects no candidate. Each
/// instance does, with its own types (GEN-5: "Select exactly one
/// most-specific admitted candidate. Concrete types outrank
/// variables"): `wrap(5)` calls `kind_of(value: i64)`, and `wrap(true)`
/// the candidate for any other type. So does an instance reached through
/// another, `relay`'s; each of two such calls in one body; one in a
/// graph, whose candidate is lifted (FN-6); and a symbol, `<`, whose
/// candidate in core is for a `T` among the ordered types. What the call
/// gives is the operator's `i64` in the generic body too, so `+` over
/// two of them is core's candidate there.
#[test]
fn a_candidate_a_type_parameter_selects_is_selected_for_each_instance() {
    let checked = module(concat!(
        "operator kind_of<T>(value: T) -> i64\n",
        "impl const fn kind_of(value: i64) -> i64 => 1\n",
        "impl const fn kind_of<T>(value: T) -> i64 requires !(T == i64) => 2\n",
        "const fn wrap<U>(value: U) -> i64 => kind_of(value)\n",
        "const fn twice<U>(value: U) -> i64 => kind_of(value) + kind_of(value)\n",
        "const fn relay<V>(value: V) -> i64 => wrap(value)\n",
        "fn wired<U>(value: U) -> i64 => kind_of(value)\n",
        "const fn less<T>(a: T, b: T) -> bool => a < b\n",
        "const fn ordered() -> bool => less(1, 2)\n",
        "const fn whole() -> i64 => wrap(5)\n",
        "const fn other() -> i64 => wrap(true)\n",
        "const fn relayed() -> i64 => relay(5)\n",
        "const fn both() -> i64 => twice(\"s\")\n",
        "fn graph(x: i64) -> i64 => wired(x)\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    // As it is declared, no candidate is selected: the operator's `i64`
    // is what the call gives, and core's `+` takes two of them.
    assert_eq!(called(&checked, body(&checked, "wrap")), ["none"]);
    assert_eq!(
        called(&checked, body(&checked, "twice")),
        ["add_(i64, i64)", "none", "none"]
    );
    let instance = |caller: &str| instance_called_by(&checked, body(&checked, caller));
    assert_eq!(called(&checked, &instance("whole")), ["kind_of(i64)"]);
    assert_eq!(called(&checked, &instance("other")), ["kind_of(T) <bool>"]);
    assert_eq!(
        called(&checked, &instance("both")),
        ["add_(i64, i64)", "kind_of(T) <str>", "kind_of(T) <str>"]
    );
    assert_eq!(called(&checked, body(&checked, "less")), ["none"]);
    assert_eq!(called(&checked, &instance("ordered")), ["lt_(T, T) <i64>"]);
    // `relay(5)` makes `relay<i64>`, whose call of `wrap` makes
    // `wrap<i64>`.
    let relayed = instance("relayed");
    assert_eq!(called(&checked, &relayed), ["wrap(U) <i64>"]);
    assert_eq!(
        called(&checked, &instance_called_by(&checked, &relayed)),
        ["kind_of(i64)"]
    );
    // In a graph the value candidate is lifted, and is the one selected.
    let (module, declaration) = declaration_of(&checked, "wired");
    let (_, wired) = checked
        .program
        .instance(
            module,
            declaration,
            &[Argument::Type(hgl_analyser::Type::Scalar(
                hgl_analyser::Scalar::I64,
            ))],
            &[],
        )
        .expect("`wired` has a body");
    assert_eq!(called(&checked, &wired), ["kind_of(i64)"]);
}

/// GEN-6: "A candidate states in `requires` the values of a generic value
/// parameter it is for, by comparing it with constants ... A call admits
/// the candidate only where that holds for the parameter's value at the
/// call, a default included." GEN-5: "No match rejects; tied
/// most-specific candidates are ambiguous." In `go<const M: Mode>`,
/// `step(v, mode: M)` is told for each instance and rejected where the
/// instance is made: `go<.medium>(1)` makes one no candidate admits, as
/// `step(1, mode: .medium)` written in place is; `go<.both>(1)` one two
/// candidates admit equally; `relay<.medium>(1)` one through another
/// instance; `later(1)` one through `M`'s default; and two calls of one
/// instance are each reported. A candidate whose result is not what the
/// place asks for admits none either (GEN-5: "the requested output
/// before inputs"): core's `div_` with `zero: .error` gives a `result`
/// where an `f64` is returned. Core's `<` takes no `list<i64>`. Where the
/// instance selects, nothing is reported.
#[test]
fn a_call_no_one_candidate_admits_in_an_instance_rejects() {
    let checked = module(concat!(
        "enum Mode { slow, fast, medium, both }\n",
        "operator step(value: i64, const mode: Mode = .slow) -> i64\n",
        "impl const fn step(value: i64, const mode: Mode) -> i64 requires mode in {.slow, .both} => value + 1\n",
        "impl const fn step(value: i64, const mode: Mode) -> i64 requires mode in {.fast, .both} => value + 10\n",
        "const fn go<const M: Mode>(v: i64) -> i64 => step(v, mode: M)\n",
        "const fn relay<const N: Mode>(v: i64) -> i64 => go<N>(v)\n",
        "const fn later<const M: Mode = .medium>(v: i64) -> i64 => step(v, mode: M)\n",
        "const fn ratio<const Z: ZeroPolicy>(a: f64, b: f64) -> f64 => div_(a, b, zero: Z)\n",
        "const fn less<T>(a: T, b: T) -> bool => a < b\n",
        "const fn slow() -> i64 => go<.slow>(1)\n",
        "const fn fast() -> i64 => relay<.fast>(1)\n",
        "const fn medium() -> i64 => go<.medium>(1)\n",
        "const fn direct() -> i64 => step(1, mode: .medium)\n",
        "const fn both() -> i64 => go<.both>(1)\n",
        "const fn deep() -> i64 => relay<.medium>(1)\n",
        "const fn defaulted() -> i64 => later(1)\n",
        "const fn again() -> i64 => go<.medium>(1) + go<.medium>(1)\n",
        "const fn nan() -> f64 => ratio<.nan>(1.0, 0.0)\n",
        "const fn checked() -> f64 => ratio<.error>(1.0, 0.0)\n",
        "const fn ordered() -> bool => less(1, 2)\n",
        "const fn lists() -> bool => less([1], [2])\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:13:29 operator/operator.no_candidate GEN-5",
            "m.hgl:14:29 operator/operator.no_candidate GEN-5",
            "m.hgl:15:27 operator/operator.ambiguous GEN-5",
            "m.hgl:16:27 operator/operator.no_candidate GEN-5",
            "m.hgl:17:32 operator/operator.no_candidate GEN-5",
            "m.hgl:18:28 operator/operator.no_candidate GEN-5",
            "m.hgl:18:45 operator/operator.no_candidate GEN-5",
            "m.hgl:20:30 operator/operator.no_candidate GEN-5",
            "m.hgl:22:29 operator/operator.no_candidate GEN-5",
        ],
        "{}",
        checked.report
    );
    // Each points at the call in the generic body it is about.
    let medium = checked
        .findings
        .iter()
        .find(|finding| finding.at == "m.hgl:13:29")
        .expect("the instance's call is reported");
    assert!(
        medium.message.contains("operator `step`"),
        "{}",
        medium.message
    );
    assert!(
        checked
            .report
            .contains("`step` is called here, with the generic arguments this call gives"),
        "{}",
        checked.report
    );
}
