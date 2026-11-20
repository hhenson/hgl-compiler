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

/// ERR-9: "Required constant failures reject where specified ... Failure
/// during argument evaluation precedes later admission checks." NUM-2:
/// "An unrepresentable i64 result produces an HGL error". GEN-8: what a
/// call gives a generic value parameter is a compile-time constant, and
/// "Any other rejects". A generic argument that an instance works out,
/// and whose evaluation fails there, an `i64` out of range or a division
/// of two `i64` by zero, is rejected where the instance is made, as the
/// same argument written in place is (`generic.argument_constant`): with
/// no `requires` anywhere, through another instance, in a struct's
/// construction, and before the callee's `requires`, which is then not
/// told. Where it works out, nothing is reported.
#[test]
fn a_generic_argument_that_fails_in_an_instance_rejects() {
    let checked = module(concat!(
        "const fn times<const K: i64>(v: i64) -> i64 => v * K\n",
        "const fn grows<const M: i64>(v: i64) -> i64 => times<(M * 4611686018427387904)>(v)\n",
        "const fn divs<const M: i64>(v: i64) -> i64 => times<(10 // M)>(v)\n",
        "const fn only_eight<const K: i64>(v: i64) -> i64 requires K == 8 => v * K\n",
        "const fn grows_big<const M: i64>(v: i64) -> i64 => only_eight<(M * 4611686018427387904)>(v)\n",
        "const fn grows_req<const M: i64>(v: i64) -> i64 => only_eight<(M * 2)>(v)\n",
        "const fn relay<const N: i64>(v: i64) -> i64 => grows<N>(v)\n",
        "struct Sized<const N: i64> { v: i64 }\n",
        "const fn boxed<const M: i64>(v: i64) -> i64 {\n",
        "    let held = Sized<(M * 4611686018427387904)>(v: v)\n",
        "    return held.v\n",
        "}\n",
        "const fn a() -> i64 => grows<4>(1)\n",
        "const fn b() -> i64 => grows<1>(1)\n",
        "const fn c() -> i64 => divs<0>(1)\n",
        "const fn d() -> i64 => divs<5>(1)\n",
        "const fn e() -> i64 => grows_big<4>(1)\n",
        "const fn f() -> i64 => grows_req<5>(1)\n",
        "const fn g() -> i64 => grows_req<4>(1)\n",
        "const fn h() -> i64 => relay<4>(1)\n",
        "const fn k() -> i64 => boxed<4>(1)\n",
        "const fn m() -> i64 => boxed<1>(1)\n",
        "const fn n() -> i64 => times<(4 * 4611686018427387904)>(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:14:24 type/generic.argument_constant GEN-8",
            "m.hgl:16:24 type/generic.argument_constant GEN-8",
            "m.hgl:18:24 type/generic.argument_constant GEN-8",
            "m.hgl:19:24 constraint/requires.unmet GEN-3",
            "m.hgl:21:24 type/generic.argument_constant GEN-8",
            "m.hgl:22:24 type/generic.argument_constant GEN-8",
            "m.hgl:24:30 type/generic.argument_constant GEN-8",
        ],
        "{}",
        checked.report
    );
    // The instance's report says what fails, and points at the argument
    // in the generic body.
    let grown = checked
        .findings
        .iter()
        .find(|finding| finding.at == "m.hgl:14:24")
        .expect("the instance's argument is reported");
    assert!(
        grown.message.contains("`K` of `times`"),
        "{}",
        grown.message
    );
    assert!(grown.message.contains("fails"), "{}", grown.message);
}

/// UNIT-13: "A constant is read by its name wherever an ordinary value
/// is." GEN-3: a `requires` may use "equalities, the membership of a
/// constant in a set of constants". A module's constant compared with a
/// generic value parameter, `N == C` and `C == N`, among a set, `N in {C,
/// 5}`, and compared with a candidate's option, `mode == FAST`, is that
/// constant: the condition holds where the parameter's value is the
/// constant's, at a call that writes it and for each instance a call
/// makes. One compared with a type parameter, `T == C`, compares a type
/// with a value, and is still no type (`type.not_a_type`).
#[test]
fn a_constant_in_a_requires_is_read_by_its_name() {
    let checked = module(concat!(
        "enum Mode { slow, fast }\n",
        "const C: i64 = 3\n",
        "const FAST: Mode = .fast\n",
        "const fn named_eq<const N: i64>(v: i64) -> i64 requires N == C => v + N\n",
        "const fn named_left<const N: i64>(v: i64) -> i64 requires C == N => v + N\n",
        "const fn named_in<const N: i64>(v: i64) -> i64 requires N in {C, 5} => v + N\n",
        "operator step(value: i64, const mode: Mode = .slow) -> i64\n",
        "impl const fn step(value: i64, const mode: Mode) -> i64 requires mode == FAST => value + 10\n",
        "impl const fn step(value: i64, const mode: Mode) -> i64 requires mode == .slow => value + 1\n",
        "const fn typed<T>(v: T) -> i64 requires T == C => 1\n",
        "const fn pass<const M: i64>(v: i64) -> i64 => named_eq<M>(v)\n",
        "const fn a() -> i64 => named_eq<3>(1)\n",
        "const fn b() -> i64 => named_eq<4>(1)\n",
        "const fn c() -> i64 => named_left<3>(1) + named_left<2>(1)\n",
        "const fn d() -> i64 => named_in<3>(1) + named_in<5>(1) + named_in<4>(1)\n",
        "const fn e() -> i64 => step(1, mode: .fast) + step(1)\n",
        "const fn f() -> i64 => pass<3>(1) + pass<4>(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:11:46 type/type.not_a_type UNIT-10",
            "m.hgl:14:24 constraint/requires.unmet GEN-3",
            "m.hgl:15:43 constraint/requires.unmet GEN-3",
            "m.hgl:16:58 constraint/requires.unmet GEN-3",
            "m.hgl:18:37 constraint/requires.unmet GEN-3",
        ],
        "{}",
        checked.report
    );
    // The candidate for `FAST` is selected by `.fast`, and the other by
    // the operator's default.
    assert_eq!(
        called(&checked, body(&checked, "e")),
        ["add_(i64, i64)", "step(i64, Mode)", "step(i64, Mode)"]
    );
}

/// No rule pins a program whose instances never end, so the instances
/// of one declaration that one call makes are followed up to one bound,
/// `hgl_analyser::INSTANCES`, which the hgraph target writes no more than
/// either. A call that makes exactly that many of one is followed whole,
/// each once, and so is one that makes that many of each of two; one
/// that makes one more of one is the local `generic.instance_limit`,
/// which says that its instances do not end within the bound.
/// `climb<N + 1>` in `climb<N>` ends when the program runs, and every
/// instance's `requires` holds, but its instances do not end.
#[test]
fn the_instances_one_call_makes_are_followed_up_to_one_bound() {
    assert_eq!(hgl_analyser::INSTANCES, 4096);
    // `turn<0>` makes `turn<1>` and on to `turn<size - 1>`, which makes
    // `turn<0>` again: `size` instances in all.
    let turning = |size: usize| {
        format!(
            "const fn need<const N: i64>(v: i64) -> i64 requires !(N == 100000) => v\n\
             const fn turn<const N: i64>(v: i64) -> i64 =>\n    \
                 if v <= 0 {{ need<N>(v) }} else {{ turn<((N + 1) % {size})>(v - 1) }}\n\
             const fn a() -> i64 => turn<0>(3)\n"
        )
    };
    assert_eq!(
        module(&turning(hgl_analyser::INSTANCES - 1)).summary(),
        NONE
    );
    assert_eq!(module(&turning(hgl_analyser::INSTANCES)).summary(), NONE);
    // `turn` and `back` call each other, each making as many instances.
    let both = format!(
        "const fn need<const N: i64>(v: i64) -> i64 requires !(N == 100000) => v\n\
         const fn turn<const N: i64>(v: i64) -> i64 =>\n    \
             if v <= 0 {{ need<N>(v) }} else {{ back<N>(v - 1) }}\n\
         const fn back<const N: i64>(v: i64) -> i64 => turn<((N + 1) % {})>(v)\n\
         const fn a() -> i64 => turn<0>(3)\n",
        hgl_analyser::INSTANCES
    );
    assert_eq!(module(&both).summary(), NONE);
    let past = module(&turning(hgl_analyser::INSTANCES + 1));
    assert_eq!(
        past.summary(),
        ["m.hgl:5:24 local:constraint/generic.instance_limit GEN-3"]
    );
    let climb = module(concat!(
        "const fn small<const K: i64>(v: i64) -> i64 requires !(K == 1000000) => v + K\n",
        "const fn climb<const N: i64>(v: i64) -> i64 =>\n",
        "    if v <= 0 { small<N>(v) } else { climb<N + 1>(v - 1) }\n",
        "const fn a() -> i64 => climb<0>(3)\n",
    ));
    assert_eq!(
        climb.summary(),
        ["m.hgl:5:24 local:constraint/generic.instance_limit GEN-3"]
    );
    let message = &climb.findings[0].message;
    assert!(
        message.contains("instances of `climb` this call makes do not end within the 4096"),
        "{message}"
    );
}
