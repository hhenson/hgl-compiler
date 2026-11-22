//! How a `requires`, an operator's candidate and a generic argument are
//! judged for each concrete instance of a generic function (GEN-3,
//! GEN-5, GEN-6, GEN-8, ERR-9, UNIT-13). A condition that is not known
//! where the generic is declared is neither accepted nor rejected there:
//! it is judged where an instance makes it known. Each test cites its
//! rule.

mod common;

use common::{Checked, body, module};
use hgl_analyser::hir::{Body, Callee, Expr, ExprKind, Stmt, ValueCategory, walk_statements};
use hgl_analyser::{Argument, ConstantValue, Detail};

/// No findings.
const NONE: [&str; 0] = [];

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
            Callee::Parameter { .. }
            | Callee::Anonymous(_)
            | Callee::DeferredOperator(_)
            | Callee::DeferredValue
            | Callee::Unread => {
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
                    ..
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

/// GEN-7: "A generic body is checked under its stated requirements;
/// choosing a specialization does not authorize unrelated operations."
/// GEN-3: "under negation/disjunction it only tests already-resolved
/// data": `!(T == i64)` of the candidate `kind_of<T>` is not told of a
/// `U` not given, so no candidate is known to admit `kind_of(value)` in
/// `wrap<U>`, which states nothing of it, and the call is rejected where
/// `wrap` is declared (GEN-5: "No match rejects"), in a value function and
/// in a graph, as `a < b` over a `T` is. GEN-16: under `requires
/// kind_of(U) -> i64`, "its body calls it under that requirement", and a
/// call of `wrap_req` gives the requirement its candidate: `wrap_req(5)`
/// the one for `i64` and `wrap_req(true)` the other (GEN-5: "Concrete
/// types outrank variables"), through a caller that states it again
/// too; and a call for whose types a requirement does not hold is
/// `requires.unmet` (GEN-3). Under a membership, `T in {i64, f64}`, `a <
/// b` is authorised, each of the types having a candidate for it, and
/// each instance selects its own (GEN-3; see
/// `a_membership_authorises_what_each_member_has_a_candidate_for`).
#[test]
fn a_call_in_a_generic_body_is_judged_by_its_stated_requirements() {
    let checked = module(concat!(
        "operator kind_of<T>(value: T) -> i64\n",
        "impl const fn kind_of(value: i64) -> i64 => 1\n",
        "impl const fn kind_of<T>(value: T) -> i64 requires !(T == i64) => 2\n",
        "const fn less<T>(a: T, b: T) -> bool => a < b\n",
        "const fn wrap<U>(value: U) -> i64 => kind_of(value)\n",
        "fn wired<U>(value: U) -> i64 => kind_of(value)\n",
        "const fn wrap_req<U>(value: U) -> i64 requires kind_of(U) -> i64 => kind_of(value)\n",
        "const fn less_num<T>(a: T, b: T) -> bool requires T in {i64, f64} => a < b\n",
        "const fn relay<V>(value: V) -> i64 requires kind_of(V) -> i64 => wrap_req(value)\n",
        "operator only_ints<T>(value: T) -> i64\n",
        "impl const fn only_ints(value: i64) -> i64 => 1\n",
        "const fn needs<U>(value: U) -> i64 requires only_ints(U) -> i64 => only_ints(value)\n",
        "const fn whole() -> i64 => wrap_req(5)\n",
        "const fn other() -> i64 => wrap_req(true)\n",
        "const fn relayed() -> i64 => relay(5)\n",
        "const fn ints() -> i64 => needs(1)\n",
        "const fn bools() -> i64 => needs(true)\n",
        "const fn ordered() -> bool => less_num(1, 2)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:5:43 operator/operator.no_candidate OPS-2",
            "m.hgl:6:38 operator/operator.no_candidate GEN-5",
            "m.hgl:7:33 operator/operator.no_candidate GEN-5",
            "m.hgl:18:28 constraint/requires.unmet GEN-3",
        ],
        "{}",
        checked.report
    );
    // The body selects nothing: the call is of the operation it
    // requires, which each call of it answers.
    assert_eq!(called(&checked, body(&checked, "wrap_req")), ["required"]);
    let instance = |caller: &str| instance_called_by(&checked, body(&checked, caller));
    assert_eq!(called(&checked, &instance("whole")), ["kind_of(i64)"]);
    assert_eq!(called(&checked, &instance("other")), ["kind_of(T) <bool>"]);
    // Under the membership the body selects nothing; the instance does.
    assert_eq!(called(&checked, body(&checked, "less_num")), ["none"]);
    assert_eq!(called(&checked, &instance("ordered")), ["lt_(T, T) <i64>"]);
    // `relay(5)` makes `relay<i64>`, whose call of `wrap_req` makes
    // `wrap_req<i64>`.
    let relayed = instance("relayed");
    assert_eq!(called(&checked, &relayed), ["wrap_req(U) <i64>"]);
    assert_eq!(
        called(&checked, &instance_called_by(&checked, &relayed)),
        ["kind_of(i64)"]
    );
}

/// GEN-6: "A call admits the candidate only where that holds for the
/// parameter's value at the call, a default included." In a generic
/// body the value of an option given a generic value parameter of the
/// body is that parameter's, known there only through what the body's
/// `requires` says of it. GEN-3: "A positive conjunctive equality may
/// bind one unresolved generic from known data": under `requires M ==
/// .slow`, `step(v, mode: M)` is the `.slow` candidate's call, and so is
/// core's division under `requires Z == .nan`; a call of `bound` with
/// another value is `requires.unmet`. Where the body states nothing of
/// the parameter, no candidate is known to admit the call, and it is
/// rejected where the body is declared (GEN-7; GEN-5: "No match
/// rejects"), whatever values a caller gives and whatever default the
/// parameter has: `go`, `later`, and core's division in `ratio`. A
/// membership, `M in {.slow, .fast}`, authorises the call where each of
/// its values selects a candidate, and each instance selects its own;
/// one with a value no candidate admits, `.medium`, rejects at the
/// declaration (GEN-3, proposed: "a membership of a generic value
/// parameter, `requires M in {.fast, .slow}`, authorizes in the body a
/// call that gives M as an option (GEN-6) where each listed value has a
/// candidate that admits it, and each instance selects its own"). Written
/// in place,
/// `step(1, mode: .medium)` is rejected as it was.
#[test]
fn an_option_a_generic_parameter_gives_is_judged_by_its_requirements() {
    let checked = module(concat!(
        "enum Mode { slow, fast, medium }\n",
        "operator step(value: i64, const mode: Mode = .slow) -> i64\n",
        "impl const fn step(value: i64, const mode: Mode) -> i64 requires mode == .slow => value + 1\n",
        "impl const fn step(value: i64, const mode: Mode) -> i64 requires mode == .fast => value + 10\n",
        "const fn go<const M: Mode>(v: i64) -> i64 => step(v, mode: M)\n",
        "const fn later<const M: Mode = .slow>(v: i64) -> i64 => step(v, mode: M)\n",
        "const fn among<const M: Mode>(v: i64) -> i64 requires M in {.slow, .fast} => step(v, mode: M)\n",
        "const fn bound<const M: Mode>(v: i64) -> i64 requires M == .slow => step(v, mode: M)\n",
        "const fn ratio<const Z: ZeroPolicy>(a: f64, b: f64) -> f64 => div_(a, b, zero: Z)\n",
        "const fn quiet<const Z: ZeroPolicy>(a: f64, b: f64) -> f64 requires Z == .nan => div_(a, b, zero: Z)\n",
        "const fn slow() -> i64 => bound<.slow>(1)\n",
        "const fn fast() -> i64 => bound<.fast>(1)\n",
        "const fn nan() -> f64 => quiet<.nan>(1.0, 0.0)\n",
        "const fn direct() -> i64 => step(1, mode: .medium)\n",
        "const fn lacking<const M: Mode>(v: i64) -> i64 requires M in {.slow, .medium} => step(v, mode: M)\n",
        "const fn quick() -> i64 => among<.fast>(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:46 operator/operator.no_candidate GEN-5",
            "m.hgl:7:57 operator/operator.no_candidate GEN-5",
            "m.hgl:10:63 operator/operator.no_candidate GEN-5",
            "m.hgl:13:27 constraint/requires.unmet GEN-3",
            "m.hgl:15:29 operator/operator.no_candidate GEN-5",
            "m.hgl:16:82 operator/operator.no_candidate GEN-5",
        ],
        "{}",
        checked.report
    );
    // Selected where the body is read, with the value bound.
    assert_eq!(
        called(&checked, body(&checked, "bound")),
        ["step(i64, Mode)"]
    );
    assert_eq!(
        called(&checked, body(&checked, "quiet")),
        ["div_(f64, f64) <Z>"]
    );
    // Under the membership each instance selects.
    assert_eq!(called(&checked, body(&checked, "among")), ["none"]);
    assert_eq!(
        called(
            &checked,
            &instance_called_by(&checked, body(&checked, "quick"))
        ),
        ["step(i64, Mode)"]
    );
    let lacking = checked
        .findings
        .iter()
        .position(|finding| finding.at == "m.hgl:16:82")
        .expect("the membership's lacking value is reported");
    assert!(
        checked.report.contains("for `M` = `.medium`"),
        "{lacking}: {}",
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
/// no `requires` anywhere, through another instance, and in a struct's
/// construction. A call whose callee states a `requires` is judged where
/// the body is declared, by what the body states (GEN-7;
/// hhenson/hgl_spec#144), and there the argument fails for a member the
/// body admits before the callee's `requires` is told: `grows_big` under
/// `M in {4}`. Where it works out, nothing is reported.
#[test]
fn a_generic_argument_that_fails_in_an_instance_rejects() {
    let checked = module(concat!(
        "const fn times<const K: i64>(v: i64) -> i64 => v * K\n",
        "const fn grows<const M: i64>(v: i64) -> i64 => times<(M * 4611686018427387904)>(v)\n",
        "const fn divs<const M: i64>(v: i64) -> i64 => times<(10 // M)>(v)\n",
        "const fn only_eight<const K: i64>(v: i64) -> i64 requires K == 8 => v * K\n",
        "const fn grows_big<const M: i64>(v: i64) -> i64 requires M in {4} => only_eight<(M * 4611686018427387904)>(v)\n",
        "const fn grows_req<const M: i64>(v: i64) -> i64 requires M == 4 => only_eight<(M * 2)>(v)\n",
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
            "m.hgl:6:70 type/generic.argument_constant GEN-8",
            "m.hgl:14:24 type/generic.argument_constant GEN-8",
            "m.hgl:16:24 type/generic.argument_constant GEN-8",
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
        "const fn pass<const M: i64>(v: i64) -> i64 requires M == C => named_eq<M>(v)\n",
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

/// GEN-13: "Where the instances of one declaration never end when the
/// program is compiled, the program rejects, whatever a run of it would
/// do: an implementation has a bound on the instances of one
/// declaration, and rejects the program where they pass it. Proposed: the
/// code `generic.instance_limit`, at the call that makes the first of
/// them." The bound is `hgl_analyser::INSTANCES`, past which
/// `hgl_lower` makes no instance either. A call that makes exactly that
/// many of one declaration is followed whole, each once, and so is one that makes
/// that many of each of two; one that makes one more of one rejects, and
/// says that its instances do not end within the bound. `climb<N + 1>` in
/// `climb<N>` ends when the program runs, but its instances do not; nor
/// do those of `nest<list<T>>` in `nest<T>`, a type that grows, with no
/// `requires` anywhere.
#[test]
fn the_instances_one_call_makes_are_followed_up_to_one_bound() {
    assert_eq!(hgl_analyser::INSTANCES, 4096);
    // `turn<0>` makes `turn<1>` and on to `turn<size - 1>`, which makes
    // `turn<0>` again: `size` instances in all.
    let turning = |size: usize| {
        format!(
            "const fn need<const N: i64>(v: i64) -> i64 => v\n\
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
        "const fn need<const N: i64>(v: i64) -> i64 => v\n\
         const fn turn<const N: i64>(v: i64) -> i64 =>\n    \
             if v <= 0 {{ need<N>(v) }} else {{ back<N>(v - 1) }}\n\
         const fn back<const N: i64>(v: i64) -> i64 => turn<((N + 1) % {})>(v)\n\
         const fn a() -> i64 => turn<0>(3)\n",
        hgl_analyser::INSTANCES
    );
    assert_eq!(module(&both).summary(), NONE);
    // An instance is its arguments' values, a constant inside a type
    // too: `list<i64, ((N + 1) % 4096)>` that comes back to `list<i64,
    // 0>` is the root's instance again.
    let typed = format!(
        "const fn need<const N: i64>(v: i64) -> i64 => v\n\
         const fn turn<T, const N: i64>(v: i64, held: T) -> i64 =>\n    \
             if v <= 0 {{ need<N>(v) }} else {{ turn<list<i64, ((N + 1) % {size})>, ((N + 1) % {size})>(v - 1, []) }}\n\
         const fn a() -> i64 => turn<list<i64, 0>, 0>(3, [])\n",
        size = hgl_analyser::INSTANCES
    );
    assert_eq!(module(&typed).summary(), NONE, "{}", module(&typed).report);
    let past = module(&turning(hgl_analyser::INSTANCES + 1));
    assert_eq!(
        past.summary(),
        ["m.hgl:5:24 type/generic.instance_limit GEN-13"]
    );
    let climb = module(concat!(
        "const fn small<const K: i64>(v: i64) -> i64 => v + K\n",
        "const fn climb<const N: i64>(v: i64) -> i64 =>\n",
        "    if v <= 0 { small<N>(v) } else { climb<N + 1>(v - 1) }\n",
        "const fn a() -> i64 => climb<0>(3)\n",
    ));
    assert_eq!(
        climb.summary(),
        ["m.hgl:5:24 type/generic.instance_limit GEN-13"]
    );
    let message = &climb.findings[0].message;
    assert!(
        message.contains("instances of `climb` this call makes do not end within the 4096"),
        "{message}"
    );
    let nest = module(concat!(
        "const fn nest<T>(x: i64, v: T) -> i64 {\n",
        "    if x <= 0 { return 0 }\n",
        "    return nest<list<T>>(x - 1, [v])\n",
        "}\n",
        "const fn a() -> i64 => nest(3, 1)\n",
    ));
    assert_eq!(
        nest.summary(),
        ["m.hgl:6:24 type/generic.instance_limit GEN-13"]
    );
}

/// GEN-3: "A membership of a type parameter in a set of types, `requires
/// T in {i64, f64}`, authorizes in the body (GEN-7) an operation that each
/// listed type has a candidate for, `a < b` there, and each instance
/// selects its own candidate (GEN-5). An operation that a listed type
/// lacks rejects at the declaration." So
/// `kind_of(value)` under `T in {i64, f64}` is accepted and each call
/// selects its own; under `T in {i64, str}` it rejects where it is
/// declared, naming `str`. The same where the membership is joined to
/// another requirement by `&&`, for a membership of one type, and for
/// two type parameters each with a membership, where every combination
/// needs a candidate: `(f64, bool)` has none. A symbol is held alike:
/// core's `<` takes no `list<i64>`. A call of the function with a type
/// outside the membership is `requires.unmet` (GEN-3).
#[test]
fn a_membership_authorises_what_each_member_has_a_candidate_for() {
    let checked = module(concat!(
        "operator kind_of<T>(value: T) -> i64\n",
        "impl const fn kind_of(value: i64) -> i64 => 1\n",
        "impl const fn kind_of(value: f64) -> i64 => 2\n",
        "const fn both<T>(value: T) -> i64 requires T in {i64, f64} => kind_of(value)\n",
        "const fn lacking<T>(value: T) -> i64 requires T in {i64, str} => kind_of(value)\n",
        "const fn nested<T>(value: T) -> i64 requires T in {i64, f64} && eq_(T, T) -> bool => kind_of(value)\n",
        "const fn one<T>(value: T) -> i64 requires T in {f64} => kind_of(value)\n",
        "operator pair_of<A, B>(a: A, b: B) -> i64\n",
        "impl const fn pair_of(a: i64, b: str) -> i64 => 1\n",
        "impl const fn pair_of(a: f64, b: str) -> i64 => 2\n",
        "impl const fn pair_of(a: i64, b: bool) -> i64 => 3\n",
        "const fn pairs<A, B>(a: A, b: B) -> i64 requires A in {i64, f64} && B in {str} => pair_of(a, b)\n",
        "const fn gaps<A, B>(a: A, b: B) -> i64 requires A in {i64, f64} && B in {str, bool} => pair_of(a, b)\n",
        "const fn ordered<T>(a: T, b: T) -> bool requires T in {i64, list<i64>} => a < b\n",
        "const fn c1() -> i64 => both(5)\n",
        "const fn c2() -> i64 => both(1.5)\n",
        "const fn c3() -> i64 => one(2.5)\n",
        "const fn c4() -> i64 => pairs(1.5, \"s\")\n",
        "const fn c5() -> i64 => nested(5)\n",
        "const fn c6() -> i64 => both(\"s\")\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:6:66 operator/operator.no_candidate GEN-5",
            "m.hgl:14:88 operator/operator.no_candidate GEN-5",
            "m.hgl:15:77 operator/operator.no_candidate OPS-2",
            "m.hgl:21:25 constraint/requires.unmet GEN-3",
        ],
        "{}",
        checked.report
    );
    for named in [
        "for `T` = `str`",
        "for `A` = `f64` and `B` = `bool`",
        "for `T` = `list<i64>`",
    ] {
        assert!(
            checked.report.contains(named),
            "{named}: {}",
            checked.report
        );
    }
    assert_eq!(called(&checked, body(&checked, "both")), ["none"]);
    let instance = |caller: &str| instance_called_by(&checked, body(&checked, caller));
    assert_eq!(called(&checked, &instance("c1")), ["kind_of(i64)"]);
    assert_eq!(called(&checked, &instance("c2")), ["kind_of(f64)"]);
    assert_eq!(called(&checked, &instance("c3")), ["kind_of(f64)"]);
    assert_eq!(called(&checked, &instance("c4")), ["pair_of(f64, str)"]);
    assert_eq!(called(&checked, &instance("c5")), ["kind_of(i64)"]);
}

/// The pinned fragments of GEN-3 and GEN-7, as written. GEN-7: "It is
/// judged once, where it is declared, by what its `requires` states, and
/// never again for the generic arguments a call of it gives. A call of
/// an operator in it is admitted where those requirements make a
/// candidate known to take it, and the declaration rejects where they do
/// not (`operator.no_candidate`). They make one known where a candidate
/// takes every generic argument they admit, where a named operator
/// requirement states the call (GEN-16), and, under a membership, where
/// each listed type has one (GEN-3). Each instance then selects its own
/// candidate (GEN-5)." Of the fragment: "the instances that `less(1, 2)`
/// and `less(1.5, 2.0)` make each select their own"; "`wrap(7)` is 1,
/// its instance selecting the first candidate of `kind_of`, and
/// `wrap(true)` is 2, selecting the second"; and "`==`", whose candidate
/// takes every `T`, needs nothing stated.
#[test]
fn the_fragments_judged_where_they_are_declared_are_accepted() {
    let checked = module(concat!(
        "const fn less<T>(a: T, b: T) -> bool requires T in {i64, f64} => a < b\n",
        "operator kind_of<V>(value: V) -> i64\n",
        "impl const fn kind_of(value: i64) -> i64 => 1\n",
        "impl const fn kind_of<T>(value: T) -> i64 requires !(T == i64) => 2\n",
        "const fn wrap<U>(value: U) -> i64 requires kind_of(U) -> i64 => kind_of(value)\n",
        "const fn same<T>(a: T, b: T) -> bool => a == b\n",
        "const fn ints() -> bool => less(1, 2)\n",
        "const fn floats() -> bool => less(1.5, 2.0)\n",
        "const fn seven() -> i64 => wrap(7)\n",
        "const fn yes() -> i64 => wrap(true)\n",
        "const fn same_ints() -> bool => same(1, 2)\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    let instance = |caller: &str| instance_called_by(&checked, body(&checked, caller));
    assert_eq!(called(&checked, &instance("ints")), ["lt_(T, T) <i64>"]);
    assert_eq!(called(&checked, &instance("floats")), ["lt_(T, T) <f64>"]);
    assert_eq!(called(&checked, &instance("seven")), ["kind_of(i64)"]);
    assert_eq!(called(&checked, &instance("yes")), ["kind_of(T) <bool>"]);
    // The candidate for every T admits the body. GEN-7 selects in the instance.
    assert_eq!(called(&checked, body(&checked, "same")), ["none"]);
    assert_eq!(
        called(&checked, &instance("same_ints")),
        ["eq_(i64, i64, f64, f64)"]
    );
}

/// GEN-7's deliberate rejections, "each at its declaration, whatever its
/// calls give (`operator.no_candidate`): `less` with no `requires`, as no
/// candidate of `<` takes every `T`; `less` with `requires T in {i64,
/// Point}`, for a `struct Point { x: f64 }` that `<` has no candidate for;
/// and `wrap` with no `requires`, as the two candidates of `kind_of`
/// between them take every `U` and neither takes every one." Each is
/// rejected though no call of it is written, and though a call of it
/// gives a type a candidate takes.
#[test]
fn the_fragments_rejected_at_their_declaration() {
    let checked = module(concat!(
        "struct Point { x: f64 }\n",
        "const fn less<T>(a: T, b: T) -> bool => a < b\n",
        "const fn points<T>(a: T, b: T) -> bool requires T in {i64, Point} => a < b\n",
        "operator kind_of<V>(value: V) -> i64\n",
        "impl const fn kind_of(value: i64) -> i64 => 1\n",
        "impl const fn kind_of<T>(value: T) -> i64 requires !(T == i64) => 2\n",
        "const fn wrap<U>(value: U) -> i64 => kind_of(value)\n",
        "const fn ints() -> bool => less(1, 2)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:43 operator/operator.no_candidate OPS-2",
            "m.hgl:4:72 operator/operator.no_candidate OPS-2",
            "m.hgl:8:38 operator/operator.no_candidate GEN-5",
        ],
        "{}",
        checked.report
    );
    assert!(
        checked.report.contains("for `T` = `Point`"),
        "{}",
        checked.report
    );
}

/// GEN-3, proposed: "a membership of a generic value parameter, `requires
/// M in {.fast, .slow}`, authorizes in the body a call that gives M as an
/// option (GEN-6) where each listed value has a candidate that admits it,
/// and each instance selects its own." Of the proposed fragment, with the
/// candidates of `ratio`: "The first candidate admits each listed value,
/// and the instance that `ratio_in(a, b, z: .nan)` makes selects it.
/// Deliberate rejections, at the declaration (`operator.no_candidate`):
/// the same with no `requires`, as no candidate admits every `z`. The
/// `.error` candidate supplies an `f64` or raises where an `f64` is
/// expected (ERR-15)."
#[test]
fn the_proposed_option_fragment_is_held_to_its_words() {
    let checked = module(concat!(
        "export enum Zero { ieee, nan, error }\n",
        "operator ratio<L, R, O>(lhs: L, rhs: R) -> O\n",
        "native impl fn ratio<const zero: Zero = .ieee>(lhs: f64, rhs: f64) -> f64\n",
        "    requires zero in {.ieee, .nan}\n",
        "native impl fn ratio<const zero: Zero>(lhs: f64, rhs: f64) -> result<f64, error>\n",
        "    requires zero == .error\n",
        "fn ratio_in<const z: Zero>(lhs: f64, rhs: f64) -> f64\n",
        "    requires z in {.ieee, .nan} => ratio(lhs, rhs, zero: z)\n",
        "fn bare<const z: Zero>(lhs: f64, rhs: f64) -> f64 => ratio(lhs, rhs, zero: z)\n",
        "fn erring<const z: Zero>(lhs: f64, rhs: f64) -> f64\n",
        "    requires z in {.ieee, .error} => ratio(lhs, rhs, zero: z)\n",
        "fn wired(a: f64, b: f64) -> f64 => ratio_in(a, b, z: .nan)\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:10:54 operator/operator.no_candidate GEN-5"],
        "{}",
        checked.report
    );
    assert_eq!(called(&checked, body(&checked, "ratio_in")), ["none"]);
    let (module, declaration, generics, requirements) = first_call(body(&checked, "wired"));
    let (_, instance) = checked
        .program
        .instance(module, declaration, &generics, &requirements)
        .expect("`ratio_in` has a body");
    assert_eq!(called(&checked, &instance), ["ratio(f64, f64) <z>"]);
}

/// GEN-7: the requirements "make one known where a candidate takes every
/// generic argument they admit". GEN-5: "Concrete types outrank
/// variables; structurally constrained variables outrank unconstrained
/// ones." A candidate for `list<T>` with no condition takes every
/// `list<U>`, and outranks one for any `T` whatever that one's condition
/// comes to: the list candidate admits `kind(value)` in
/// `wrap<U>(value: list<U>)` at the declaration, with nothing stated.
/// Where the candidate whose condition is not known is as specific as the
/// one that takes it, which of them is selected is not known, and the
/// body rejects.
#[test]
fn a_candidate_that_outranks_every_one_not_told_is_known() {
    let checked = module(concat!(
        "operator kind<V>(value: V) -> i64\n",
        "impl const fn kind<T>(value: list<T>) -> i64 => 1\n",
        "impl const fn kind<T>(value: T) -> i64 requires !(T == list<i64>) => 2\n",
        "const fn wrap<U>(value: list<U>) -> i64 => kind(value)\n",
        "operator tied<V>(value: V) -> i64\n",
        "impl const fn tied<T>(value: T) -> i64 => 1\n",
        "impl const fn tied<T>(value: T) -> i64 requires !(T == i64) => 2\n",
        "const fn level<U>(value: U) -> i64 => tied(value)\n",
        "const fn lists(values: list<i64>) -> i64 => wrap(values)\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:9:39 operator/operator.no_candidate GEN-5"],
        "{}",
        checked.report
    );
    assert_eq!(called(&checked, body(&checked, "wrap")), ["none"]);
    let instance = instance_called_by(&checked, body(&checked, "lists"));
    assert_eq!(called(&checked, &instance), ["kind(list<T>) <i64>"]);
}

/// FN-3 and GEN-7: a graph checks const arguments when the candidate
/// admits the body and again when an instance selects a candidate.
#[test]
fn a_deferred_operator_call_checks_const_arguments() {
    let checked = module(concat!(
        "operator always_const<T>(value: T) -> i64\n",
        "impl const fn always_const<T>(const value: T) -> i64 => 5\n",
        "fn generic_const<T>(value: T) -> i64 => always_const(value)\n",
        "operator sometimes_const<T>(value: T) -> i64\n",
        "impl const fn sometimes_const<T>(value: T) -> i64 => 5\n",
        "impl const fn sometimes_const(const value: i64) -> i64 => value\n",
        "fn generic_runtime<T>(value: T) -> i64 => sometimes_const(value)\n",
        "fn instance_const(value: i64) -> i64 => generic_runtime(value)\n",
        "fn instance_runtime(value: str) -> i64 => generic_runtime(value)\n",
        "fn instance_already_rejected(value: i64) -> i64 => generic_const(value)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:4:54 phase/connection.as_constant FN-3",
            "m.hgl:9:41 phase/connection.as_constant FN-3",
        ],
        "{}",
        checked.report
    );
}

/// FN-3 and GEN-7: a call selected in an instance can change a value
/// to a connection, including when a later call reads it through a local.
#[test]
fn graph_instances_check_consumers_of_deferred_calls() {
    let checked = module(concat!(
        "operator observe<T>(value: T) -> i64\n",
        "impl const fn observe<T>(value: T) -> i64 => 5\n",
        "impl fn observe(value: i64) -> i64 { when { return value } }\n",
        "fn needs_const(const value: i64) -> i64 => value\n",
        "fn nested<T>(const value: T) -> i64 => needs_const(observe(value))\n",
        "fn via_binding<T>(const value: T) -> i64 {\n",
        "    let observed = observe(value)\n",
        "    return needs_const(observed)\n",
        "}\n",
        "fn value_instance() -> i64 => nested(\"a\")\n",
        "fn rejected_nested() -> i64 => nested(3)\n",
        "fn rejected_binding() -> i64 => via_binding(3)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:12:32 phase/connection.as_constant FN-3",
            "m.hgl:13:33 phase/connection.as_constant FN-3",
        ],
        "{}",
        checked.report
    );
}

/// GEN-7, FN-3 and CTRL-15: a graph's `while` condition must remain
/// an ordinary value after the instance selects its candidates.
#[test]
fn graph_instances_check_while_conditions_after_selection() {
    let checked = module(concat!(
        "operator live_helper<T>(value: T) -> i64\n",
        "impl const fn live_helper<T>(value: T) -> i64 => 5\n",
        "impl fn live_helper(value: i64) -> i64 { when { return value } }\n",
        "fn through_loop<T>(const value: T) -> i64 {\n",
        "    var passed = 0\n",
        "    while live_helper(value) > passed { passed += 1 }\n",
        "    return passed\n",
        "}\n",
        "fn ordinary_loop() -> i64 => through_loop(\"s\")\n",
        "fn temporal_loop() -> i64 => through_loop(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:11:30 phase/connection.as_constant FN-3"],
        "{}",
        checked.report
    );
}

/// GEN-7 and CTRL-13: an inferred local follows the no-else expression's
/// type once its selected condition is an ordinary value or connection.
#[test]
fn graph_instances_settle_no_else_types_after_selection() {
    let checked = module(concat!(
        "operator predicate<T>(value: T) -> bool\n",
        "impl const fn predicate<T>(value: T) -> bool => true\n",
        "impl fn predicate(value: i64) -> bool { when { return value > 0 } }\n",
        "fn wrap<T>(const value: T) -> i64? {\n",
        "    let chosen = if predicate(value) { 1 }\n",
        "    return chosen\n",
        "}\n",
        "fn ordinary_condition() -> i64? => wrap(\"s\")\n",
        "fn temporal_condition() -> i64? => wrap(1)\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    for (name, ty, category) in [
        ("ordinary_condition", "i64?", ValueCategory::Value),
        ("temporal_condition", "i64", ValueCategory::Connection),
    ] {
        let instance = instance_called_by(&checked, body(&checked, name));
        let [
            Stmt::Declare {
                local,
                value: Some(conditional),
            },
            Stmt::Return {
                value: Some(returned),
                ..
            },
        ] = instance.statements.as_slice()
        else {
            panic!("the instance binds and returns the conditional");
        };
        assert_eq!(conditional.ty.to_string(), ty, "{name}");
        assert_eq!(conditional.category, category, "{name}");
        assert_eq!(instance.locals[*local].ty.to_string(), ty, "{name}");
        assert_eq!(returned.ty.to_string(), ty, "{name}");
    }
}

/// GEN-7 and CTRL-14: the chosen candidate can change an assignment's
/// category or select a branch at runtime. The first value of a `var`
/// decides its category, even when that value was deferred.
#[test]
fn graph_instances_recheck_assignments() {
    let checked = module(concat!(
        "operator live<T>(value: T) -> i64\n",
        "impl const fn live<T>(value: T) -> i64 => 5\n",
        "impl fn live(value: i64) -> i64 { when { return value } }\n",
        "fn assign<T>(const input: T) -> i64 {\n",
        "    var output = 0\n",
        "    output = live(input)\n",
        "    return output\n",
        "}\n",
        "fn initialise<T>(const input: T) -> i64 {\n",
        "    var output = live(input)\n",
        "    output = 1\n",
        "    return output\n",
        "}\n",
        "fn branch<T>(const input: T) -> i64 {\n",
        "    var output = 0\n",
        "    if live(input) > 0 { output = 1 }\n",
        "    return output\n",
        "}\n",
        "fn inner<T>(const input: T) -> i64 {\n",
        "    if live(input) > 0 {\n",
        "        var local = 1\n",
        "        local = 2\n",
        "    }\n",
        "    return 0\n",
        "}\n",
        "fn static_cases() -> i64 => assign(\"s\") + initialise(\"s\") + branch(\"s\")\n",
        "fn connected_value() -> i64 => assign(3)\n",
        "fn connected_initialiser() -> i64 => initialise(3)\n",
        "fn connected_branch() -> i64 => branch(3)\n",
        "fn connected_inner() -> i64 => inner(3)\n",
        "fn by_index<T>(const input: T) -> i64 {\n",
        "    var row = [0]\n",
        "    row[live(input)] = 1\n",
        "    return 0\n",
        "}\n",
        "fn by_switch<T>(const input: T) -> i64 {\n",
        "    var output = 0\n",
        "    switch live(input) {\n",
        "        case 1: output = 1\n",
        "    }\n",
        "    return output\n",
        "}\n",
        "fn connected_index() -> i64 => by_index(1)\n",
        "fn connected_switch() -> i64 => by_switch(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:28:32 type/binding.category CTRL-14",
            "m.hgl:29:38 type/binding.category CTRL-14",
            "m.hgl:30:33 type/binding.category CTRL-14",
            "m.hgl:44:32 type/binding.category CTRL-14",
            "m.hgl:45:33 type/binding.category CTRL-14",
        ],
        "{}",
        checked.report
    );
}

/// GEN-7, CTRL-13 and EXPR-5: a deferred conditional's settled type
/// must fit each destination, and an implicit nullable read is retained
/// in instances where that is the fit (ERR-12).
#[test]
fn graph_instances_fit_settled_types_at_destinations() {
    let checked = module(concat!(
        "operator predicate<T>(value: T) -> bool\n",
        "impl const fn predicate<T>(value: T) -> bool => true\n",
        "impl fn predicate(value: i64) -> bool { when { return value > 0 } }\n",
        "const fn takes_str(value: str) -> str => value\n",
        "const fn takes_int(value: i64) -> i64 => value\n",
        "fn wrong_return<T>(const input: T) -> str {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    return chosen\n",
        "}\n",
        "fn wrong_local<T>(const input: T) -> str {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    let annotated: str = chosen\n",
        "    return annotated\n",
        "}\n",
        "fn wrong_assignment<T>(const input: T) -> str {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    var assigned: str = \"s\"\n",
        "    assigned = chosen\n",
        "    return assigned\n",
        "}\n",
        "fn wrong_argument<T>(const input: T) -> str {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    return takes_str(chosen)\n",
        "}\n",
        "fn right_argument<T>(const input: T) -> i64 {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    return takes_int(chosen)\n",
        "}\n",
        "fn bad_return() -> str => wrong_return(\"s\")\n",
        "fn bad_local() -> str => wrong_local(\"s\")\n",
        "fn bad_assignment() -> str => wrong_assignment(\"s\")\n",
        "fn bad_argument() -> str => wrong_argument(\"s\")\n",
        "fn ordinary_argument() -> i64 => right_argument(\"s\")\n",
        "fn temporal_argument() -> i64 => right_argument(1)\n",
        "fn wrong_condition<T>(const input: T) -> i64 {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    return if chosen { 2 } else { 3 }\n",
        "}\n",
        "fn bad_condition() -> i64 => wrong_condition(\"s\")\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:30:27 type/type.mismatch EXPR-5",
            "m.hgl:31:26 type/type.mismatch EXPR-5",
            "m.hgl:32:31 type/type.mismatch EXPR-5",
            "m.hgl:33:29 type/type.mismatch EXPR-5",
            "m.hgl:40:30 type/type.mismatch CTRL-13",
        ],
        "{}",
        checked.report
    );
    for (name, nullable_read) in [("ordinary_argument", true), ("temporal_argument", false)] {
        let instance = instance_called_by(&checked, body(&checked, name));
        let mut reads = false;
        walk_statements(&instance.statements, &mut |expr| {
            reads |= matches!(expr.kind, ExprKind::NullRead { .. });
        });
        assert_eq!(reads, nullable_read, "{name}");
    }
}

/// GEN-7, TYPE-16 and EXPR-5: a delta's named property keeps its
/// destination type when the given value becomes known.
#[test]
fn graph_instances_fit_settled_delta_properties() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "struct DeltaText {\n",
        "    count: i64\n",
        "    text: str\n",
        "}\n",
        "fn wrong_delta<T>(const input: T) -> delta<DeltaText> {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    return delta<DeltaText>(text: chosen, count: 2)\n",
        "}\n",
        "fn right_delta<T>(const input: T) -> delta<DeltaText> {\n",
        "    let chosen = if predicate(input) { \"ready\" }\n",
        "    return delta<DeltaText>(text: chosen, count: 2)\n",
        "}\n",
        "fn bad_delta() -> delta<DeltaText> => wrong_delta(1)\n",
        "fn plain_delta() -> delta<DeltaText> => right_delta(1)\n",
        "fn nullable_delta() -> delta<DeltaText> => right_delta(\"s\")\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:17:39 type/type.mismatch EXPR-5"],
        "{}",
        checked.report
    );
    for (caller, expected) in [("plain_delta", "str"), ("nullable_delta", "str?")] {
        let instance = instance_called_by(&checked, body(&checked, caller));
        let Some(Stmt::Return {
            value: Some(value), ..
        }) = instance.statements.last()
        else {
            panic!("the instance returns its delta");
        };
        let ExprKind::Delta { arguments } = &value.kind else {
            panic!("the return expression constructs a delta");
        };
        assert_eq!(arguments[0].parameter, Some(1));
        assert_eq!(arguments[0].value.ty.to_string(), expected);
        assert_eq!(arguments[1].parameter, Some(0));
    }
}

/// GEN-7 and EXPR-5: a field or entry of a previously typed aggregate
/// still has to fit when its conditional value becomes known.
#[test]
fn graph_instances_fit_aggregate_members() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: f64) -> bool { when { return input > 0.0 } }\n",
        "struct Label<T> { value: T }\n",
        "fn label<T>(const input: T) -> Label<str> {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    return Label<str>(value: chosen)\n",
        "}\n",
        "fn tuple_of<T>(const input: T) -> tuple<str, i64> {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    return (chosen, 0)\n",
        "}\n",
        "fn list_of<T>(const input: T) -> list<str> {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    return [chosen]\n",
        "}\n",
        "fn bad_field() -> Label<str> => label(1)\n",
        "fn bad_tuple() -> tuple<str, i64> => tuple_of(1)\n",
        "fn bad_list() -> list<str> => list_of(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:18:33 type/type.mismatch EXPR-5",
            "m.hgl:19:38 type/type.mismatch EXPR-5",
            "m.hgl:20:31 type/type.mismatch EXPR-5",
        ],
        "{}",
        checked.report
    );
}

/// GEN-7 and CTRL-15: a specialized sequence decides each loop binding's
/// category before FN-3 consumers are checked. A dictionary key is an
/// ordinary value even when its corresponding value is a connection.
#[test]
fn graph_instances_settle_iteration_bindings() {
    let checked = module(concat!(
        "operator items<T>(input: T) -> list<i64>\n",
        "impl const fn items<T>(input: T) -> list<i64> => [1]\n",
        "impl fn items(input: i64) -> list<i64> { when { return [input] } }\n",
        "operator entries<T>(input: T) -> dict<str, i64>\n",
        "impl const fn entries<T>(input: T) -> dict<str, i64> => [\"key\": 1]\n",
        "impl fn entries(input: i64) -> dict<str, i64> { when { return [\"key\": input] } }\n",
        "const fn needs_const(const input: i64) -> i64 => input\n",
        "const fn needs_key(const input: str) -> str => input\n",
        "fn through_list<T>(const input: T) -> i64 {\n",
        "    for item in items(input) { needs_const(item) }\n",
        "    return 1\n",
        "}\n",
        "fn through_dict<T>(const input: T) -> i64 {\n",
        "    for key, value in entries(input) {\n",
        "        needs_key(key)\n",
        "        needs_const(value)\n",
        "    }\n",
        "    return 1\n",
        "}\n",
        "fn ordinary() -> i64 => through_list(\"s\") + through_dict(\"s\")\n",
        "fn temporal_list() -> i64 => through_list(1)\n",
        "fn temporal_dict() -> i64 => through_dict(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:22:30 phase/connection.as_constant FN-3",
            "m.hgl:23:30 phase/connection.as_constant FN-3",
        ],
        "{}",
        checked.report
    );
}

/// GEN-7, EXPR-3 and CTRL-14: a value which takes several passes to
/// settle still sets the category when it is the `var`'s first value.
#[test]
fn graph_instances_wait_for_the_first_value() {
    let checked = module(concat!(
        "operator helper<T>(input: T) -> i64\n",
        "impl const fn helper<T>(input: T) -> i64 => 5\n",
        "impl fn helper(input: i64) -> i64 { when { return input } }\n",
        "fn initialised<T>(const input: T) -> i64 {\n",
        "    var output = {\n",
        "        let nested = helper(input)\n",
        "        nested\n",
        "    }\n",
        "    output = 1\n",
        "    return output\n",
        "}\n",
        "fn assigned<T>(const input: T) -> i64 {\n",
        "    var output: i64\n",
        "    output = {\n",
        "        let nested = helper(input)\n",
        "        nested\n",
        "    }\n",
        "    output = 1\n",
        "    return output\n",
        "}\n",
        "fn ordinary() -> i64 => initialised(\"s\") + assigned(\"s\")\n",
        "fn temporal_initialised() -> i64 => initialised(1)\n",
        "fn temporal_assigned() -> i64 => assigned(1)\n",
        "fn nested_first<T>(const input: T) -> i64 {\n",
        "    var output: i64\n",
        "    output = {\n",
        "        output = 1\n",
        "        helper(input)\n",
        "    }\n",
        "    return output\n",
        "}\n",
        "fn temporal_nested() -> i64 => nested_first(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:23:37 type/binding.category CTRL-14",
            "m.hgl:24:34 type/binding.category CTRL-14",
            "m.hgl:33:32 type/binding.category CTRL-14",
        ],
        "{}",
        checked.report
    );
    assert!(
        checked
            .report
            .contains("`output` holds an ordinary value, and this is a connection"),
        "{}",
        checked.report
    );
}

/// GEN-7, CTRL-13 and EXPR-5: settled tuple elements propagate through
/// blocks and inferred local reads to the type their destination checks.
#[test]
fn graph_instances_propagate_inferred_tuple_types() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: f64) -> bool { when { return input > 0.0 } }\n",
        "fn good<T>(const input: T) -> tuple<tuple<i64?, i64>, i64> {\n",
        "    let pair = ((if predicate(input) { 1 }, 0), 2)\n",
        "    let alias = { pair }\n",
        "    return alias\n",
        "}\n",
        "fn bad<T>(const input: T) -> tuple<str, i64> {\n",
        "    let alias = {\n",
        "        let pair = (if predicate(input) { 1 }, 0)\n",
        "        pair\n",
        "    }\n",
        "    return alias\n",
        "}\n",
        "fn accepted() -> tuple<tuple<i64?, i64>, i64> => good(\"s\")\n",
        "fn rejected() -> tuple<str, i64> => bad(\"s\")\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:18:37 type/type.mismatch EXPR-5"],
        "{}",
        checked.report
    );
    let instance = instance_called_by(&checked, body(&checked, "accepted"));
    for local in instance.locals.iter().filter(|local| local.name != "input") {
        assert_eq!(local.ty.to_string(), "tuple<tuple<i64?, i64>, i64>");
    }
    let Some(Stmt::Return {
        value: Some(returned),
        ..
    }) = instance.statements.last()
    else {
        panic!("the instance returns the tuple");
    };
    assert_eq!(returned.ty.to_string(), "tuple<tuple<i64?, i64>, i64>");
}

/// GEN-7, CTRL-14 and FN-15: passing an outer ordinary variable to a
/// mutable parameter is an assignment in the selected branch.
#[test]
fn graph_instances_check_mutable_calls_under_temporal_selectors() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "const fn bump(var output: i64) { output = 1 }\n",
        "fn conditional<T>(const input: T) -> i64 {\n",
        "    var output = 0\n",
        "    if predicate(input) { bump(output) }\n",
        "    return output\n",
        "}\n",
        "fn selected<T>(const input: T) -> i64 {\n",
        "    var output = 0\n",
        "    switch predicate(input) { case true: bump(output) }\n",
        "    return output\n",
        "}\n",
        "fn local_to_branch<T>(const input: T) -> i64 {\n",
        "    if predicate(input) {\n",
        "        var output = 0\n",
        "        bump(output)\n",
        "    }\n",
        "    return 0\n",
        "}\n",
        "fn ordinary() -> i64 => conditional(\"s\") + selected(\"s\")\n",
        "fn bad_if() -> i64 => conditional(1)\n",
        "fn bad_switch() -> i64 => selected(1)\n",
        "fn valid_inner() -> i64 => local_to_branch(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:24:23 type/binding.category CTRL-14",
            "m.hgl:25:27 type/binding.category CTRL-14",
        ],
        "{}",
        checked.report
    );
}

/// GEN-7 and CTRL-15: once the sequence is temporal, the loop body is
/// a child which cannot carry outer variables or return from the graph.
#[test]
fn graph_instances_recheck_child_loop_writes_and_returns() {
    let checked = module(concat!(
        "operator items<T>(input: T) -> list<i64>\n",
        "impl const fn items<T>(input: T) -> list<i64> => [1]\n",
        "impl fn items(input: i64) -> list<i64> { when { return [input] } }\n",
        "const fn bump(var output: i64) { output = 1 }\n",
        "fn writes<T>(const input: T) -> i64 {\n",
        "    var output = 0\n",
        "    var row = [0]\n",
        "    for item in items(input) {\n",
        "        output = 1\n",
        "        row[0] = 2\n",
        "        bump(output)\n",
        "        var inside = 0\n",
        "        bump(inside)\n",
        "    }\n",
        "    return output\n",
        "}\n",
        "fn early<T>(const input: T) -> i64 {\n",
        "    for item in items(input) { return 1 }\n",
        "    return 0\n",
        "}\n",
        "fn ordinary() -> i64 => writes(\"s\") + early(\"s\")\n",
        "fn bad_write() -> i64 => writes(1)\n",
        "fn bad_return() -> i64 => early(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:23:26 local:phase/loop.carried CTRL-15",
            "m.hgl:23:26 local:phase/loop.carried CTRL-15",
            "m.hgl:23:26 local:phase/loop.carried CTRL-15",
            "m.hgl:24:27 local:phase/loop.carried CTRL-15",
        ],
        "{}",
        checked.report
    );
}

/// GEN-7 and CTRL-13: the two branches of a consumed conditional
/// agree after settlement. A dropped conditional still needs no type.
#[test]
fn graph_instances_join_deferred_branches_only_when_consumed() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: f64) -> bool { when { return input > 0.0 } }\n",
        "fn good<T>(const input: T) -> i64? {\n",
        "    let chosen = if true { if predicate(input) { 1 } } else { 0 }\n",
        "    return chosen\n",
        "}\n",
        "fn mismatch<T>(const input: T) -> i64? {\n",
        "    let chosen = if true { if predicate(input) { 1 } } else { \"s\" }\n",
        "    return chosen\n",
        "}\n",
        "fn dropped<T>(const input: T) -> i64 {\n",
        "    if true { if predicate(input) { 1 } } else { \"s\" }\n",
        "    return 0\n",
        "}\n",
        "fn accepted() -> i64? => good(1)\n",
        "fn rejected() -> i64? => mismatch(1)\n",
        "fn unused() -> i64 => dropped(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:18:26 type/type.mismatch CTRL-13"],
        "{}",
        checked.report
    );
    let instance = instance_called_by(&checked, body(&checked, "accepted"));
    assert_eq!(instance.locals[1].ty.to_string(), "i64?");
    let Some(Stmt::Return {
        value: Some(returned),
        ..
    }) = instance.statements.last()
    else {
        panic!("the instance returns the conditional");
    };
    assert_eq!(returned.ty.to_string(), "i64?");
}

/// GEN-7, CTRL-13 and CTRL-10: a return supplies no branch value.
/// The other branch supplies the type after selection, in either order.
#[test]
fn graph_instances_type_the_surviving_conditional_branch() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "fn from_then<T>(const input: T, const flag: bool) -> i64 {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    let copied = if flag { chosen } else { return 0 }\n",
        "    return copied\n",
        "}\n",
        "fn from_else<T>(const input: T, const flag: bool) -> i64 {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    let copied = if flag {\n",
        "        return 0\n",
        "        if predicate(input) { \"discarded\" }\n",
        "    } else { chosen }\n",
        "    return copied\n",
        "}\n",
        "fn dropped<T>(const input: T, const flag: bool) -> i64 {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    if flag { chosen } else { return 0 }\n",
        "    return 5\n",
        "}\n",
        "fn no_value<T>(const input: T, const flag: bool) -> i64 {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    let copied = if flag { let held = chosen } else { return 0 }\n",
        "    return 5\n",
        "}\n",
        "fn wrong<T>(const input: T, const flag: bool) -> str {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    let copied: str = if flag { chosen } else { return \"\" }\n",
        "    return copied\n",
        "}\n",
        "fn temporal_then() -> i64 => from_then(1, true)\n",
        "fn temporal_else() -> i64 => from_else(1, false)\n",
        "fn ordinary_then() -> i64 => from_then(\"s\", true)\n",
        "fn dropped_value() -> i64 => dropped(1, true)\n",
        "fn wrong_annotation() -> str => wrong(1, true)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:25:18 type/type.mismatch CTRL-10",
            "m.hgl:37:33 type/type.mismatch EXPR-5",
        ],
        "{}",
        checked.report
    );
    for (caller, expected) in [
        ("temporal_then", "i64"),
        ("temporal_else", "i64"),
        ("ordinary_then", "i64?"),
    ] {
        let instance = instance_called_by(&checked, body(&checked, caller));
        let copied = instance
            .locals
            .iter()
            .find(|local| local.name == "copied")
            .expect("the copied local");
        assert_eq!(copied.ty.to_string(), expected, "{caller}");
    }
    let dropped = instance_called_by(&checked, body(&checked, "dropped_value"));
    assert!(matches!(
        &dropped.statements[1],
        Stmt::Expr(Expr {
            kind: ExprKind::If { .. },
            ty: hgl_analyser::Type::Unknown,
            ..
        })
    ));
}

/// GEN-7, EXPR-5 and EXPR-9: inferred list element types survive nested
/// lists and local reads, and constrain both entries and destinations.
#[test]
fn graph_instances_propagate_inferred_list_types() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: f64) -> bool { when { return input > 0.0 } }\n",
        "fn good<T>(const input: T) -> list<list<i64?>> {\n",
        "    let values = [[if predicate(input) { 1 }, null]]\n",
        "    let alias = { values }\n",
        "    return alias\n",
        "}\n",
        "fn bad_entry<T>(const input: T) -> list<i64?> {\n",
        "    let values = [if predicate(input) { 1 }, \"s\"]\n",
        "    return values\n",
        "}\n",
        "fn bad_result<T>(const input: T) -> list<str> {\n",
        "    let values = [if predicate(input) { 1 }]\n",
        "    return values\n",
        "}\n",
        "fn accepted() -> list<list<i64?>> => good(1)\n",
        "fn wrong_entry() -> list<i64?> => bad_entry(1)\n",
        "fn wrong_result() -> list<str> => bad_result(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:19:35 type/type.mismatch EXPR-5",
            "m.hgl:20:35 type/type.mismatch EXPR-5",
        ],
        "{}",
        checked.report
    );
    let instance = instance_called_by(&checked, body(&checked, "accepted"));
    for local in &instance.locals[1..] {
        assert_eq!(local.ty.to_string(), "list<list<i64?>>");
    }
    walk_statements(&instance.statements, &mut |expr| {
        if matches!(expr.kind, ExprKind::Literal(hgl_analyser::hir::Value::Null)) {
            assert_eq!(expr.ty.to_string(), "i64?");
        }
    });
}

/// GEN-7, CTRL-12 and CTRL-15: the selected input determines what a
/// deferred switch accepts as its selector and label types.
#[test]
fn graph_instances_check_settled_switch_selectors_and_labels() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "fn wrong_label<T>(const input: T) -> i64 {\n",
        "    let key = if predicate(input) { 1 }\n",
        "    switch key { case \"s\": return 1 }\n",
        "    return 0\n",
        "}\n",
        "fn right_label<T>(const input: T) -> i64 {\n",
        "    let key = if predicate(input) { 1 }\n",
        "    switch key { case 1: return 1 }\n",
        "    return 0\n",
        "}\n",
        "fn wrong() -> i64 => wrong_label(1)\n",
        "fn nullable() -> i64 => right_label(\"s\")\n",
        "fn temporal() -> i64 => right_label(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:15:22 type/type.mismatch EXPR-5",
            "m.hgl:16:25 type/type.mismatch CTRL-15",
        ],
        "{}",
        checked.report
    );
}

/// GEN-7, EXPR-5 and EXPR-9: contextual dictionary and set entries
/// retain their destination types even when the value settled later.
#[test]
fn graph_instances_fit_dictionary_and_set_entries() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: f64) -> bool { when { return input > 0.0 } }\n",
        "fn wrong_value<T>(const input: T) -> dict<str, str> {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    return [\"key\": chosen]\n",
        "}\n",
        "fn wrong_key<T>(const input: T) -> dict<str, i64> {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    return [chosen: 1]\n",
        "}\n",
        "fn wrong_set<T>(const input: T) -> set<str> {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    return [chosen]\n",
        "}\n",
        "fn right<T>(const input: T) -> dict<str, i64> {\n",
        "    let key = if predicate(input) { \"key\" }\n",
        "    let value = if predicate(input) { 1 }\n",
        "    return [key: value]\n",
        "}\n",
        "fn bad_value() -> dict<str, str> => wrong_value(1)\n",
        "fn bad_key() -> dict<str, i64> => wrong_key(1)\n",
        "fn bad_set() -> set<str> => wrong_set(1)\n",
        "fn accepted() -> dict<str, i64> => right(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:22:37 type/type.mismatch EXPR-5",
            "m.hgl:23:35 type/type.mismatch EXPR-5",
            "m.hgl:24:29 type/type.mismatch EXPR-5",
        ],
        "{}",
        checked.report
    );
    let instance = instance_called_by(&checked, body(&checked, "accepted"));
    let Some(Stmt::Return {
        value: Some(returned),
        ..
    }) = instance.statements.last()
    else {
        panic!("the instance returns the dictionary");
    };
    let ExprKind::Pairs(entries) = &returned.kind else {
        panic!("dictionary entries")
    };
    let [(key, value)] = entries.as_slice() else {
        panic!("one key and value")
    };
    for (entry, expected) in [(key, "str"), (value, "i64")] {
        assert_eq!(entry.ty.to_string(), expected);
        assert!(matches!(entry.kind, ExprKind::NullRead { .. }));
    }
}

/// GEN-7, EXPR-7 and ERR-12: a settled collection determines its
/// projection, and brackets read nullable bases and indices as usual.
#[test]
fn graph_instances_propagate_indexed_types_and_reads() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: f64) -> bool { when { return input > 0.0 } }\n",
        "fn list_item<T>(const input: T) -> i64? {\n",
        "    let values = [if predicate(input) { 1 }]\n",
        "    let item = values[0]\n",
        "    return item\n",
        "}\n",
        "fn tuple_item<T>(const input: T) -> i64? {\n",
        "    let values = (if predicate(input) { 1 }, 0)\n",
        "    return values[0]\n",
        "}\n",
        "fn nullable_base<T>(const input: T) -> i64 {\n",
        "    let values = if predicate(input) { [1] }\n",
        "    return values[0]\n",
        "}\n",
        "fn nullable_index<T>(const input: T) -> i64 {\n",
        "    let key = if predicate(input) { \"key\" }\n",
        "    let values: dict<str, i64> = [\"key\": 1]\n",
        "    return values[key]\n",
        "}\n",
        "fn wrong<T>(const input: T) -> str {\n",
        "    let values = [if predicate(input) { 1 }]\n",
        "    return values[0]\n",
        "}\n",
        "fn from_list() -> i64? => list_item(1)\n",
        "fn from_tuple() -> i64? => tuple_item(1)\n",
        "fn base() -> i64 => nullable_base(1)\n",
        "fn keyed() -> i64 => nullable_index(1)\n",
        "fn rejected() -> str => wrong(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:31:25 type/type.mismatch EXPR-5"],
        "{}",
        checked.report
    );
    for (name, ty, reads) in [
        ("from_list", "i64?", 0),
        ("from_tuple", "i64?", 0),
        ("base", "i64", 1),
        ("keyed", "i64", 1),
    ] {
        let instance = instance_called_by(&checked, body(&checked, name));
        let mut projections = 0;
        let mut null_reads = 0;
        walk_statements(&instance.statements, &mut |expr| match expr.kind {
            ExprKind::Index { .. } => {
                projections += 1;
                assert_eq!(expr.ty.to_string(), ty, "{name}");
            }
            ExprKind::NullRead { .. } => null_reads += 1,
            _ => {}
        });
        assert_eq!(projections, 1, "{name}");
        assert_eq!(null_reads, reads, "{name}");
    }
}

/// GEN-7 and EXPR-7: a deferred base must satisfy the same bracket
/// rules once the instance knows the collection it reads.
#[test]
fn graph_instances_reject_indexed_sets() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "const fn items() -> set<i64> => [1]\n",
        "const fn result_items() -> result<set<i64>, error> => ok(items())\n",
        "fn bad<T>(const input: T) -> i64 {\n",
        "    let values = if predicate(input) { items() }\n",
        "    let item = values[0]\n",
        "    return 0\n",
        "}\n",
        "fn bad_result<T>(const input: T) -> i64 {\n",
        "    let values = if predicate(input) { result_items() }\n",
        "    let item = values[0]\n",
        "    return 0\n",
        "}\n",
        "fn already_checked<T>(const input: T) -> i64 {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    let item = result_items()[0]\n",
        "    return 0\n",
        "}\n",
        "fn good<T>(const input: T) -> i64 {\n",
        "    let values = if predicate(input) { [1] }\n",
        "    return values[0]\n",
        "}\n",
        "fn live_set() -> i64 => bad(1)\n",
        "fn live_result_set() -> i64 => bad_result(1)\n",
        "fn direct_once() -> i64 => already_checked(1)\n",
        "fn live_list() -> i64 => good(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:19:16 type/type.mismatch EXPR-7",
            "m.hgl:26:25 type/type.mismatch EXPR-7",
            "m.hgl:27:32 type/type.mismatch EXPR-7",
        ],
        "{}",
        checked.report
    );
    let instance = instance_called_by(&checked, body(&checked, "live_list"));
    let Some(Stmt::Return {
        value: Some(value), ..
    }) = instance.statements.last()
    else {
        panic!("the instance returns the list element");
    };
    assert_eq!(value.ty.to_string(), "i64");
    assert!(matches!(value.kind, ExprKind::Index { .. }));
}

/// GEN-7 and EXPR-7: a tuple that settles in the instance takes a
/// constant index within its bounds, and that position selects a type.
#[test]
fn graph_instances_check_settled_tuple_indices() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "fn folded<T>(const input: T) -> str {\n",
        "    let values = if predicate(input) { (1, \"selected\") }\n",
        "    let item = values[1 + 0]\n",
        "    return item\n",
        "}\n",
        "fn outside<T>(const input: T) -> i64 {\n",
        "    let values = if predicate(input) { (1, 2) }\n",
        "    let item = values[5]\n",
        "    return item\n",
        "}\n",
        "fn moving<T>(const input: T, const position: i64) -> i64 {\n",
        "    let values = if predicate(input) { (1, 2) }\n",
        "    return values[position]\n",
        "}\n",
        "fn known_shape<T>(const input: T) -> i64 {\n",
        "    let values = (if predicate(input) { 1 }, 2)\n",
        "    return values[5]\n",
        "}\n",
        "fn ordinary() -> str => folded(\"s\")\n",
        "fn temporal() -> str => folded(1)\n",
        "fn out_of_bounds() -> i64 => outside(1)\n",
        "fn nonconstant() -> i64 => moving(\"s\", 1)\n",
        "fn source_check_once() -> i64 => known_shape(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:21:19 type/tuple.index EXPR-7",
            "m.hgl:25:30 type/tuple.index EXPR-7",
            "m.hgl:26:28 type/tuple.index EXPR-7",
        ],
        "{}",
        checked.report
    );
    for (caller, category) in [
        ("ordinary", ValueCategory::Value),
        ("temporal", ValueCategory::Connection),
    ] {
        let instance = instance_called_by(&checked, body(&checked, caller));
        let mut projections = 0;
        walk_statements(&instance.statements, &mut |value| {
            let ExprKind::Index { base, index } = &value.kind else {
                return;
            };
            assert_eq!(value.ty.to_string(), "str");
            assert_eq!(value.category, category);
            assert_eq!(
                matches!(base.kind, ExprKind::NullRead { .. }),
                caller == "ordinary"
            );
            assert!(matches!(
                index.kind,
                ExprKind::Literal(hgl_analyser::hir::Value::Int(1))
            ));
            projections += 1;
        });
        assert_eq!(projections, 1);
    }
}

/// GEN-2 and GEN-7: a deferred constructor field can determine an
/// unbound type parameter. EXPR-7 projections and GEN-3 requirements
/// use that concrete struct application.
#[test]
fn graph_instances_finish_inferred_constructor_arguments() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "struct Holder<X> { value: X }\n",
        "struct OnlyNumber<X> requires X == i64 { value: X }\n",
        "fn reads<T>(const input: T) -> i64? {\n",
        "    let held = Holder(value: [if predicate(input) { 1 }])\n",
        "    let values = held.value\n",
        "    return values[0]\n",
        "}\n",
        "fn expected<T>(const input: T) -> Holder<list<i64?>> {\n",
        "    return Holder(value: [if predicate(input) { 1 }])\n",
        "}\n",
        "fn wrong_result<T>(const input: T) -> str {\n",
        "    let held = Holder(value: [if predicate(input) { 1 }])\n",
        "    return held.value[0]\n",
        "}\n",
        "fn wrong_requirement<T>(const input: T) -> i64 {\n",
        "    let held = OnlyNumber(value: [if predicate(input) { 1 }])\n",
        "    return 0\n",
        "}\n",
        "fn accepted() -> i64? => reads(\"s\")\n",
        "fn accepted_expected() -> Holder<list<i64?>> => expected(\"s\")\n",
        "fn mismatch() -> str => wrong_result(\"s\")\n",
        "fn unmet() -> i64 => wrong_requirement(\"s\")\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:25:25 type/type.mismatch EXPR-5",
            "m.hgl:26:22 constraint/requires.unmet GEN-3",
        ],
        "{}",
        checked.report
    );
    let instance = instance_called_by(&checked, body(&checked, "accepted"));
    assert_eq!(instance.locals[1].ty.to_string(), "Holder<list<i64?>>");
    assert_eq!(instance.locals[2].ty.to_string(), "list<i64?>");
    let expected = instance_called_by(&checked, body(&checked, "accepted_expected"));
    let Some(Stmt::Return {
        value: Some(returned),
        ..
    }) = expected.statements.last()
    else {
        panic!("the expected constructor is returned");
    };
    assert_eq!(returned.ty.to_string(), "Holder<list<i64?>>");
    assert!(matches!(
        returned.kind,
        ExprKind::Construct {
            infer_types: false,
            ..
        }
    ));
}

/// GEN-7 and CTRL-15: settled sequence types reach the iteration
/// binding and its local reads, using the existing rules for shape.
#[test]
fn graph_instances_settle_iteration_binding_types() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "fn list_loop<T>(const input: T) -> i64 {\n",
        "    for item in [if predicate(input) { 1 }] { let copied = item }\n",
        "    return 0\n",
        "}\n",
        "fn dict_loop<T>(const input: T) -> i64 {\n",
        "    let entries: dict<str, i64> = [\"key\": 1]\n",
        "    for key, value in (if predicate(input) { entries }) {\n",
        "        let copied_key = key\n",
        "        let copied_value = value\n",
        "    }\n",
        "    return 0\n",
        "}\n",
        "fn wrong<T>(const input: T) -> i64 {\n",
        "    for item in (if predicate(input) { (1, 2) }) { let copied = item }\n",
        "    return 0\n",
        "}\n",
        "fn ordinary() -> i64 => list_loop(\"s\")\n",
        "fn connected() -> i64 => list_loop(1)\n",
        "fn keyed() -> i64 => dict_loop(\"s\")\n",
        "fn rejected() -> i64 => wrong(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:24:25 type/type.mismatch CTRL-15"],
        "{}",
        checked.report
    );
    for (name, ty, category) in [
        ("ordinary", "i64?", ValueCategory::Value),
        ("connected", "i64", ValueCategory::Connection),
    ] {
        let instance = instance_called_by(&checked, body(&checked, name));
        for local in &instance.locals[1..] {
            assert_eq!(
                (local.ty.to_string().as_str(), local.category),
                (ty, category),
                "{name}"
            );
        }
    }
    let instance = instance_called_by(&checked, body(&checked, "keyed"));
    for (name, ty) in [
        ("key", "str"),
        ("value", "i64"),
        ("copied_key", "str"),
        ("copied_value", "i64"),
    ] {
        let local = instance
            .locals
            .iter()
            .find(|local| local.name == name)
            .expect(name);
        assert_eq!(local.ty.to_string(), ty);
    }
    let Some(Stmt::For { sequence, .. }) = instance.statements.get(1) else {
        panic!("dictionary loop")
    };
    assert!(matches!(sequence.kind, ExprKind::NullRead { .. }));
}

/// GEN-5, GEN-7 and OPS-1: a symbol in a graph uses the operand type
/// recovered for its loop binding and rejects an incompatible operand.
#[test]
fn graph_instances_select_symbols_with_deferred_iteration_operands() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "fn sum<T>(const input: T) -> i64 {\n",
        "    var total = 0\n",
        "    for item in (if predicate(input) { [2, 3] }) { total = total + item }\n",
        "    return total\n",
        "}\n",
        "fn incompatible<T>(const input: T) -> i64 {\n",
        "    var total = 0\n",
        "    for item in (if predicate(input) { [true] }) { total = total + item }\n",
        "    return total\n",
        "}\n",
        "fn accepted() -> i64 => sum(\"s\")\n",
        "fn rejected() -> i64 => incompatible(\"s\")\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:16:25 operator/operator.no_candidate OPS-1"],
        "{}",
        checked.report
    );
    let instance = instance_called_by(&checked, body(&checked, "accepted"));
    assert!(called(&checked, &instance).contains(&"add_(i64, i64)".to_string()));
}

/// GEN-5 and NUM-1: the selected operand type determines unary minus.
/// An incompatible operand still gets the ordinary OPS-1 diagnostic.
#[test]
fn graph_instances_select_unary_minus_after_types_settle() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "fn negative<T>(const input: T) -> i64 {\n",
        "    let x = if predicate(input) { 1 }\n",
        "    let y = -x\n",
        "    return y\n",
        "}\n",
        "fn incompatible<T>(const input: T) -> i64 {\n",
        "    let x = if predicate(input) { true }\n",
        "    return -x\n",
        "}\n",
        "fn accepted() -> i64 => negative(1)\n",
        "fn rejected() -> i64 => incompatible(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:15:25 operator/operator.no_candidate OPS-1"],
        "{}",
        checked.report
    );
    let instance = instance_called_by(&checked, body(&checked, "accepted"));
    assert!(called(&checked, &instance).contains(&"neg_(i64)".to_string()));
}

/// EXPR-10 and EXPR-13: options survive instance selection, retain their
/// written order, and get the same checks as on a source-known symbol.
#[test]
fn graph_instances_select_configured_symbols_after_types_settle() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "fn close<T, const Tolerance: f64 = 0.1>(const input: T) -> bool {\n",
        "    let chosen = if predicate(input) { 2.5 }\n",
        "    return chosen =={rel: 0.0, abs: {\n",
        "        let observer = Tolerance + 0.1\n",
        "        1\n",
        "    }} 2.55\n",
        "}\n",
        "fn incompatible<T>(const input: T) -> bool {\n",
        "    let chosen = if predicate(input) { 2.5 }\n",
        "    return chosen =={abs: \"x\"} 2.55\n",
        "}\n",
        "fn accepted() -> bool => close(1)\n",
        "fn rejected() -> bool => incompatible(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:17:26 operator/operator.no_candidate OPS-2"],
        "{}",
        checked.report
    );
    let instance = instance_called_by(&checked, body(&checked, "accepted"));
    let Some(Stmt::Return {
        value: Some(value), ..
    }) = instance.statements.last()
    else {
        panic!("return with the specialized comparison");
    };
    let ExprKind::Call {
        callee: Callee::Candidate { .. },
        arguments,
        ..
    } = &value.kind
    else {
        panic!("comparison candidate: {:?}", value.kind);
    };
    let slots: Vec<_> = arguments
        .iter()
        .map(|argument| argument.parameter)
        .collect();
    assert_eq!(slots, [Some(0), Some(3), Some(2), Some(1)]);
    assert!(matches!(arguments[2].value.kind, ExprKind::Block { .. }));
    assert_eq!(arguments[2].value.ty.to_string(), "f64");
}

/// GEN-5 and GEN-8: a named operator uses the same written options,
/// explicit result type and admission checks after its arguments settle.
#[test]
fn graph_instances_select_named_operators_after_types_settle() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "operator shifted<T, R>(value: T) -> R\n",
        "impl const fn shifted(value: i64, amount: i64) -> i64 => value + amount\n",
        "impl const fn shifted(value: i64, amount: i64) -> f64 => value + amount + 0.5\n",
        "fn wrapper<T>(const input: T) -> f64 {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    let copied = shifted<i64, f64>{amount: 5}(value: chosen)\n",
        "    return copied\n",
        "}\n",
        "fn incompatible<T>(const input: T) -> i64 {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    return add_(chosen, true)\n",
        "}\n",
        "fn admitted() -> f64 => wrapper(1)\n",
        "fn rejected() -> i64 => incompatible(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:18:25 operator/operator.no_candidate GEN-5"],
        "{}",
        checked.report
    );
    let instance = instance_called_by(&checked, body(&checked, "admitted"));
    let Stmt::Declare {
        value: Some(value), ..
    } = &instance.statements[1]
    else {
        panic!("the saved operator result");
    };
    let ExprKind::Call {
        callee: Callee::Candidate { .. },
        arguments,
        ..
    } = &value.kind
    else {
        panic!("the selected operator: {:?}", value.kind);
    };
    assert_eq!(value.ty.to_string(), "f64");
    assert_eq!(
        arguments
            .iter()
            .map(|argument| argument.parameter)
            .collect::<Vec<_>>(),
        [Some(1), Some(0)]
    );
}

/// GEN-16: the generic result place chooses among the stated calls,
/// and the replay uses the candidate evidence already supplied to it.
#[test]
fn graph_instances_use_requirements_for_deferred_named_operators() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "operator shape<A, R>(input: A) -> R\n",
        "impl const fn shape(input: i64) -> i64 => input + 1\n",
        "impl const fn shape(input: i64) -> f64 => input + 0.5\n",
        "fn wrapped<I, A, B>(const input: I) -> B requires shape(i64) -> A && shape(i64) -> B {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    return shape(chosen)\n",
        "}\n",
        "fn admitted() -> f64 => wrapped<i64, i64, f64>(1)\n",
        "fn rejected() -> str => wrapped<i64, i64, str>(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:13:25 constraint/requires.unmet GEN-3"],
        "{}",
        checked.report
    );
    let instance = instance_called_by(&checked, body(&checked, "admitted"));
    let Some(Stmt::Return {
        value: Some(value), ..
    }) = instance.statements.last()
    else {
        panic!("the graph returns the required operator's call");
    };
    assert!(matches!(
        value.kind,
        ExprKind::Call {
            callee: Callee::Candidate { .. },
            ..
        }
    ));
    assert_eq!(value.ty.to_string(), "f64");
}

/// GEN-7 and EXPR-7: a member name and base survive the source pass
/// until selection can check the field and choose its read form.
#[test]
fn graph_instances_resolve_members_of_deferred_bases() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "struct Item { value: i64 }\n",
        "fn good<T>(const input: T) -> i64 {\n",
        "    let held = if predicate(input) { Item(value: 1) }\n",
        "    return held.value\n",
        "}\n",
        "fn bad<T>(const input: T) -> i64 {\n",
        "    let held = if predicate(input) { Item(value: 1) }\n",
        "    return held.missing\n",
        "}\n",
        "fn ordinary() -> i64 => good(\"s\")\n",
        "fn connected() -> i64 => good(1)\n",
        "fn rejected() -> i64 => bad(1)\n",
        "const fn bump(var output: i64) { output = 2 }\n",
        "fn deferred_write<T>(const input: T) -> i64 {\n",
        "    var held = if predicate(input) { Item(value: 1) }\n",
        "    bump(held.value)\n",
        "    return 0\n",
        "}\n",
        "fn mutation() -> i64 => deferred_write(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:16:25 name/member.unknown EXPR-7",
            "m.hgl:23:25 phase/connection.as_constant FN-15",
        ],
        "{}",
        checked.report
    );
    let sources = body(&checked, "good");
    let Some(Stmt::Return {
        value: Some(source),
        ..
    }) = sources.statements.last()
    else {
        panic!("the function returns the member");
    };
    assert!(matches!(source.kind, ExprKind::DeferredMember { .. }));
    for (name, nullable, category) in [
        ("ordinary", true, ValueCategory::Value),
        ("connected", false, ValueCategory::Connection),
    ] {
        let instance = instance_called_by(&checked, body(&checked, name));
        let Some(Stmt::Return {
            value: Some(returned),
            ..
        }) = instance.statements.last()
        else {
            panic!("the instance returns the member");
        };
        let ExprKind::Field { base, member } = &returned.kind else {
            panic!("a resolved field")
        };
        assert_eq!(*member, 0);
        assert_eq!(returned.ty.to_string(), "i64");
        assert_eq!(returned.category, category);
        assert_eq!(matches!(base.kind, ExprKind::NullRead { .. }), nullable);
    }
}

/// GEN-7, EXPR-6 and ERR-12: specialization refits intrinsic boolean
/// operands without changing the expression's short-circuit structure.
#[test]
fn graph_instances_fit_settled_boolean_operands() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "fn logic<T>(const input: T) -> bool {\n",
        "    let flag = if predicate(input) { false }\n",
        "    return !flag && (flag || true)\n",
        "}\n",
        "fn wrong<T>(const input: T) -> bool {\n",
        "    let number = if predicate(input) { 1 }\n",
        "    return !number || number && number\n",
        "}\n",
        "fn ordinary() -> bool => logic(\"s\")\n",
        "fn connected() -> bool => logic(1)\n",
        "fn rejected() -> bool => wrong(\"s\")\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:15:26 type/type.mismatch EXPR-5"; 3],
        "{}",
        checked.report
    );
    for (name, expected) in [("ordinary", 2), ("connected", 0)] {
        let instance = instance_called_by(&checked, body(&checked, name));
        let mut reads = 0;
        walk_statements(&instance.statements, &mut |expr| {
            if matches!(expr.kind, ExprKind::NullRead { .. }) {
                reads += 1;
                assert_eq!(expr.ty.to_string(), "bool");
            }
        });
        assert_eq!(reads, expected, "{name}");
    }
}

/// GEN-7, ERR-2 and EXPR-5: each result tag constrains its payload even
/// when the payload's type first becomes known in the instance.
#[test]
fn graph_instances_fit_settled_result_payloads() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "fn success<T>(const input: T) -> result<i64, str> {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    return ok(chosen)\n",
        "}\n",
        "fn failure<T>(const input: T) -> result<i64, str> {\n",
        "    let chosen = if predicate(input) { \"problem\" }\n",
        "    return err(chosen)\n",
        "}\n",
        "fn wrong_success<T>(const input: T) -> result<str, error> {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    return ok(chosen)\n",
        "}\n",
        "fn wrong_failure<T>(const input: T) -> result<i64, str> {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    return err(chosen)\n",
        "}\n",
        "fn reads_success() -> result<i64, str> => success(\"s\")\n",
        "fn reads_failure() -> result<i64, str> => failure(\"s\")\n",
        "fn bad_success() -> result<str, error> => wrong_success(1)\n",
        "fn bad_failure() -> result<i64, str> => wrong_failure(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:23:43 type/type.mismatch EXPR-5",
            "m.hgl:24:41 type/type.mismatch EXPR-5",
        ],
        "{}",
        checked.report
    );
    for (name, expected_success, ty) in [
        ("reads_success", true, "i64"),
        ("reads_failure", false, "str"),
    ] {
        let instance = instance_called_by(&checked, body(&checked, name));
        let Some(Stmt::Return {
            value: Some(value), ..
        }) = instance.statements.last()
        else {
            panic!("the graph returns a result");
        };
        let ExprKind::Tagged { success, payload } = &value.kind else {
            panic!("the result is tagged");
        };
        assert_eq!(*success, expected_success);
        assert_eq!(payload.ty.to_string(), ty);
        assert!(matches!(payload.kind, ExprKind::NullRead { .. }));
    }
}

/// GEN-7 and ERR-4: the constructor reads a required message and keeps
/// an optional code. Both inputs must fit after instance selection.
#[test]
fn graph_instances_fit_settled_error_fields() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "fn text<T>(const input: T) -> error {\n",
        "    let message = if predicate(input) { \"message\" }\n",
        "    let code = if predicate(input) { \"code\" }\n",
        "    return error(message: message, code: code)\n",
        "}\n",
        "fn number<T>(const input: T) -> error {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    return error(message: chosen, code: chosen)\n",
        "}\n",
        "fn accepted() -> error => text(\"s\")\n",
        "fn rejected() -> error => number(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:15:27 type/type.mismatch EXPR-5"; 2],
        "{}",
        checked.report
    );
    let instance = instance_called_by(&checked, body(&checked, "accepted"));
    let Some(Stmt::Return {
        value: Some(value), ..
    }) = instance.statements.last()
    else {
        panic!("the instance returns an error");
    };
    let ExprKind::Error {
        message,
        code: Some(code),
    } = &value.kind
    else {
        panic!("the error has both fields");
    };
    assert_eq!(message.ty.to_string(), "str");
    assert!(matches!(message.kind, ExprKind::NullRead { .. }));
    assert_eq!(code.ty.to_string(), "str?");
    assert!(matches!(code.kind, ExprKind::Local(_)));
}

/// GEN-7 and ERR-3: a tag test takes a result in the concrete graph,
/// including when its operand's type was unknown at declaration time.
#[test]
fn graph_instances_check_settled_tag_tests() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "const fn sample() -> result<i64, error> => ok(1)\n",
        "fn invalid<T>(const input: T) -> bool {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    return is_ok(chosen)\n",
        "}\n",
        "fn result_tag<T>(const input: T) -> bool {\n",
        "    let chosen = if predicate(input) { sample() }\n",
        "    return is_err(chosen)\n",
        "}\n",
        "fn ordinary() -> bool => invalid(\"s\")\n",
        "fn temporal() -> bool => invalid(1)\n",
        "fn nullable_result() -> bool => result_tag(\"s\")\n",
        "fn connected_result() -> bool => result_tag(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:14:26 type/type.mismatch ERR-3",
            "m.hgl:15:26 type/type.mismatch ERR-3",
            "m.hgl:16:33 type/type.mismatch ERR-3",
        ],
        "{}",
        checked.report
    );
    let instance = instance_called_by(&checked, body(&checked, "connected_result"));
    let Some(Stmt::Return {
        value: Some(value), ..
    }) = instance.statements.last()
    else {
        panic!("the instance returns a boolean");
    };
    let ExprKind::TagTest {
        success: false,
        value,
    } = &value.kind
    else {
        panic!("the expression tests the result's failure tag");
    };
    assert_eq!(value.ty.to_string(), "result<i64, error>");
    assert_eq!(value.category, ValueCategory::Connection);
}

/// GEN-7 and ERR-3: an instance resolves a deferred result projection
/// with the fact at that read, and a temporal tag test gives no fact.
#[test]
fn graph_instances_recheck_deferred_result_projections() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "const fn sample() -> result<i64, error> => ok(1)\n",
        "fn unguarded<T>(const input: T) -> i64 {\n",
        "    let chosen = if predicate(input) { sample() }\n",
        "    return chosen.value\n",
        "}\n",
        "fn temporal_guard<T>(const input: T) -> i64 {\n",
        "    let chosen = if predicate(input) { sample() }\n",
        "    let answer = if is_ok(chosen) { chosen.value } else { 0 }\n",
        "    return answer\n",
        "}\n",
        "fn guarded<T>(const input: T) -> i64 {\n",
        "    let samples = if predicate(input) { [sample()] }\n",
        "    let chosen = samples[0]\n",
        "    if is_err(chosen) { return 0 }\n",
        "    return chosen.value\n",
        "}\n",
        "fn wrong_side<T>(const input: T) -> str {\n",
        "    let samples = if predicate(input) { [sample()] }\n",
        "    let chosen = samples[0]\n",
        "    if is_ok(chosen) { return chosen.error.message }\n",
        "    return \"\"\n",
        "}\n",
        "fn temporal_unguarded() -> i64 => unguarded(1)\n",
        "fn temporal_guarded() -> i64 => temporal_guard(1)\n",
        "fn ordinary_guarded() -> i64 => guarded(\"s\")\n",
        "fn ordinary_wrong_side() -> str => wrong_side(\"s\")\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:27:35 type/result.required_variant ERR-3",
            "m.hgl:28:33 type/result.required_variant ERR-3",
            "m.hgl:30:36 type/result.required_variant ERR-3",
        ],
        "{}",
        checked.report
    );
    let instance = instance_called_by(&checked, body(&checked, "ordinary_guarded"));
    let Some(Stmt::Return {
        value: Some(value), ..
    }) = instance.statements.last()
    else {
        panic!("the instance returns the guarded payload");
    };
    assert_eq!(value.ty.to_string(), "i64");
    assert!(matches!(
        value.kind,
        ExprKind::Payload { success: true, .. }
    ));
}

/// ERR-12 and ERR-3: a terminating null branch refines the same binding
/// before its tag is tested in the ordinary specialization (GEN-7).
#[test]
fn graph_instances_keep_presence_before_testing_a_result_tag() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "const fn sample() -> result<i64, error> => ok(1)\n",
        "fn guarded<T>(const input: T) -> i64 {\n",
        "    let chosen = if predicate(input) { sample() }\n",
        "    if chosen == null { return 0 }\n",
        "    if is_err(chosen) { return 0 }\n",
        "    return chosen.value\n",
        "}\n",
        "fn wrong_side<T>(const input: T) -> str {\n",
        "    let chosen = if predicate(input) { sample() }\n",
        "    if chosen == null { return \"\" }\n",
        "    if is_err(chosen) { return \"\" }\n",
        "    return chosen.error.message\n",
        "}\n",
        "fn ordinary_guarded() -> i64 => guarded(\"s\")\n",
        "fn ordinary_wrong_side() -> str => wrong_side(\"s\")\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:19:36 type/result.required_variant ERR-3"],
        "{}",
        checked.report
    );
    let instance = instance_called_by(&checked, body(&checked, "ordinary_guarded"));
    let Some(Stmt::Return {
        value: Some(value), ..
    }) = instance.statements.last()
    else {
        panic!("the instance returns the success payload");
    };
    let ExprKind::Payload {
        success: true,
        result,
    } = &value.kind
    else {
        panic!("the tag fact allows a success projection");
    };
    assert_eq!(value.ty.to_string(), "i64");
    assert_eq!(result.ty.to_string(), "result<i64, error>");
    let ExprKind::Local(binding) = result.kind else {
        panic!("the result is the same binding refined by its null test");
    };
    assert_eq!(
        instance.locals[binding].ty.to_string(),
        "result<i64, error>?"
    );
}

/// GEN-7: a generic body "is judged once, where it is declared, by what
/// its `requires` states, and never again for the generic arguments a
/// call of it gives". hhenson/hgl_spec#144: a call in it of another
/// generic function whose `requires` the body's own stated requirements
/// do not make known to hold rejects where the body is declared; it is
/// admitted where the body states enough. So `outer<Y> => inner(v)`, with
/// `inner` under `X in {i64, f64}`, rejects unless `outer` states that
/// membership or a subset of it; a superset rejects, naming the member
/// `inner` does not admit. A value parameter passed on needs the value:
/// `one_only<Y>` under `X == 1.0` holds under `requires Y == 1.0`, and not
/// under `Y == 2.0`. A named operator requirement is answered by the
/// body's own (GEN-16). With two parameters, every combination of their
/// members. In a graph as in a value function. A call of the body itself
/// is then held to the body's `requires` (GEN-3).
#[test]
fn a_call_of_another_generic_function_is_judged_by_what_the_body_states() {
    let checked = module(concat!(
        "const fn inner<X>(v: X) -> X requires X in {i64, f64} => v\n",
        "const fn outer<Y>(v: Y) -> Y => inner(v)\n",
        "const fn stated<Y>(v: Y) -> Y requires Y in {i64, f64} => inner(v)\n",
        "const fn subset<Y>(v: Y) -> Y requires Y in {i64} => inner(v)\n",
        "const fn superset<Y>(v: Y) -> Y requires Y in {i64, f64, str} => inner(v)\n",
        "const fn one_only<const X: f64>(v: f64) -> f64 requires X == 1.0 => v * X\n",
        "const fn valued<const Y: f64>(v: f64) -> f64 requires Y == 1.0 => one_only<Y>(v)\n",
        "const fn other<const Y: f64>(v: f64) -> f64 requires Y == 2.0 => one_only<Y>(v)\n",
        "const fn added<X>(v: X) -> X requires add_(X, X) -> X => v + v\n",
        "const fn adds<Y>(v: Y) -> Y requires add_(Y, Y) -> Y => added(v)\n",
        "const fn unadded<Y>(v: Y) -> Y => added(v)\n",
        "const fn pair<A, B>(x: A, y: B) -> A requires A in {i64} && B in {str} => x\n",
        "const fn pairs<P, Q>(p: P, q: Q) -> P requires P in {i64} && Q in {str} => pair(p, q)\n",
        "const fn gaps<P, Q>(p: P, q: Q) -> P requires P in {i64} && Q in {str, bool} => pair(p, q)\n",
        "fn wired<Y>(v: Y) -> Y => inner(v)\n",
        "const fn a() -> i64 => stated(1) + subset(2) + adds(3)\n",
        "const fn b() -> f64 => valued<1.0>(2.0)\n",
        "const fn c() -> str => stated(\"s\")\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:3:33 constraint/requires.unmet GEN-7",
            "m.hgl:6:66 constraint/requires.unmet GEN-7",
            "m.hgl:9:66 constraint/requires.unmet GEN-7",
            // No candidate of `add_` takes a `Y` the body says nothing of:
            // the requirement does not hold.
            "m.hgl:12:35 constraint/requires.unmet GEN-3",
            "m.hgl:15:81 constraint/requires.unmet GEN-7",
            "m.hgl:16:27 constraint/requires.unmet GEN-7",
            "m.hgl:19:24 constraint/requires.unmet GEN-3",
        ],
        "{}",
        checked.report
    );
    for named in [
        "for `Y` = `str`",
        "`P` = `i64` and `Q` = `bool`",
        "under what the `requires` of `outer` states",
        "does not hold for this call with what the `requires` of `other` binds",
    ] {
        assert!(
            checked.report.contains(named),
            "{named}: {}",
            checked.report
        );
    }
}

/// GEN-2 and GEN-7: a graph instance fills the missing type argument
/// without changing explicit and defaulted arguments. GEN-3 checks the
/// newly concrete call's requirement.
#[test]
fn graph_instances_complete_named_function_inference() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "const fn retain<Tag, U, const Amount: i64 = 9>(value: U, tag: Tag) -> U => value\n",
        "const fn only_text<U>(value: U) -> U requires U in {str} => value\n",
        "fn wrap_copied<T>(const input: T) -> i64 {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    let copied = retain<str>(chosen, \"tag\")\n",
        "    return copied\n",
        "}\n",
        "fn constrained<T>(const input: T) -> i64 {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    let copied = only_text(chosen)\n",
        "    return copied\n",
        "}\n",
        "fn accepted() -> i64 => wrap_copied(3)\n",
        "fn rejected() -> i64 => constrained(3)\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:18:25 constraint/requires.unmet GEN-3"],
        "{}",
        checked.report
    );
    let instance = instance_called_by(&checked, body(&checked, "accepted"));
    let Some(Stmt::Return {
        value: Some(result),
        ..
    }) = instance.statements.last()
    else {
        panic!("the instance returns the value of the call");
    };
    assert_eq!(result.ty.to_string(), "i64");
    let mut generics = None;
    walk_statements(&instance.statements, &mut |value| {
        if let ExprKind::Call {
            callee:
                Callee::Function {
                    module,
                    declaration,
                    generics: arguments,
                    ..
                },
            ..
        } = &value.kind
            && checked.program.modules[*module].declarations[*declaration].name == "retain"
        {
            generics = Some(
                arguments
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            );
        }
    });
    assert_eq!(generics.unwrap(), ["str", "i64", "9"]);
}

/// FN-8, GEN-8 and GEN-10: late positional and named elements keep
/// capture order, while the types written for a pack still decide it.
#[test]
fn graph_instances_complete_inferred_function_type_packs() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "const fn positional<...Ts>(items: ...Ts) -> i64 => 1\n",
        "const fn named<...Fields>(items: ...{Fields}) -> i64 => 2\n",
        "const fn fixed<U, ...Ts>(head: U, items: ...Ts) -> U => head\n",
        "fn wrapped<T>(const input: T) -> i64 {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    let first = positional(chosen, true)\n",
        "    let second = named(flag: true, number: chosen)\n",
        "    let explicit = positional<i64, bool>(chosen, true)\n",
        "    let fixed_result = fixed(chosen, 5.0)\n",
        "    let nested = positional((chosen, true))\n",
        "    return fixed_result\n",
        "}\n",
        "fn incompatible<T>(const input: T) -> i64 {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    return positional<f64>(chosen)\n",
        "}\n",
        "fn admitted() -> i64 => wrapped(1)\n",
        "fn rejected() -> i64 => incompatible(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:22:25 type/type.mismatch EXPR-5"],
        "{}",
        checked.report
    );
    let instance = instance_called_by(&checked, body(&checked, "admitted"));
    let mut packed = Vec::new();
    walk_statements(&instance.statements, &mut |value| {
        if let ExprKind::Call {
            callee: Callee::Function { type_packs, .. },
            arguments,
            ..
        } = &value.kind
            && let [pack] = type_packs.as_slice()
        {
            assert!(!pack.infer_from_arguments);
            let types = pack
                .types
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            let keys = arguments
                .iter()
                .filter_map(|argument| argument.pack.clone())
                .collect::<Vec<_>>();
            packed.push((types, keys, value.ty.to_string()));
        }
    });
    assert_eq!(packed.len(), 5);
    assert_eq!(packed[0].0, ["i64", "bool"]);
    assert_eq!(packed[1].0, ["bool", "i64"]);
    assert_eq!(
        packed[1].1,
        [
            hgl_analyser::hir::PackKey::Name("flag".into()),
            hgl_analyser::hir::PackKey::Name("number".into())
        ]
    );
    assert_eq!(packed[2], packed[0]);
    assert_eq!(packed[3].0, ["f64"]);
    assert_eq!(packed[3].2, "i64");
    assert_eq!(packed[4].0, ["tuple<i64, bool>"]);
}

/// ERR-2 and GEN-2: inferred result bindings carry the types of both
/// the success and the error after the generic argument types settle.
#[test]
fn graph_instances_propagate_settled_result_sides() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "const fn boxed<U, E>(value: U, failure: E) -> result<U, E> => ok(value)\n",
        "fn wrapper<T>(const input: T) -> result<i64, str> {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    let failure = if predicate(input) { \"bad\" }\n",
        "    let outcome = boxed(chosen, failure)\n",
        "    let copied = outcome\n",
        "    return copied\n",
        "}\n",
        "fn admitted() -> result<i64, str> => wrapper(1)\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    let instance = instance_called_by(&checked, body(&checked, "admitted"));
    assert_eq!(instance.locals[3].ty.to_string(), "result<i64, str>");
    assert_eq!(instance.locals[4].ty, instance.locals[3].ty);
    let Some(Stmt::Return {
        value: Some(value), ..
    }) = instance.statements.last()
    else {
        panic!("the graph returns the copied result");
    };
    assert_eq!(value.ty, instance.locals[3].ty);
}

/// GEN-13: a default that depends on the inferred type is read once a
/// graph instance supplies that type.
#[test]
fn graph_instances_check_dependent_function_defaults() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "const fn choose<U, const Fallback: U = 1>(value: U) -> U => Fallback\n",
        "fn real<T>(const input: T) -> f64 {\n",
        "    let chosen = if predicate(input) { 2.5 }\n",
        "    let copied = choose(chosen)\n",
        "    return copied\n",
        "}\n",
        "fn wrong<T>(const input: T) -> bool {\n",
        "    let chosen = if predicate(input) { true }\n",
        "    let copied = choose(chosen)\n",
        "    return copied\n",
        "}\n",
        "fn accepted() -> f64 => real(3)\n",
        "fn rejected() -> bool => wrong(3)\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:17:26 type/type.mismatch GEN-13"],
        "{}",
        checked.report
    );
    assert!(checked.report.contains("leaves `Fallback` to its default"));
    let instance = instance_called_by(&checked, body(&checked, "accepted"));
    let mut found = false;
    walk_statements(&instance.statements, &mut |value| {
        if let ExprKind::Call {
            callee: Callee::Function { generics, .. },
            ..
        } = &value.kind
            && let [Argument::Type(ty), Argument::Constant(fallback)] = generics.as_slice()
        {
            assert_eq!(ty.to_string(), "f64");
            assert!(matches!(fallback.value, ConstantValue::Float(1.0)));
            found = true;
        }
    });
    assert!(found);
}

/// GEN-3 and GEN-13: a call completed in a graph instance must follow
/// the same downstream requirements as a call already concrete in source.
#[test]
fn graph_instances_follow_completed_function_calls_for_admission() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "struct OnlyText<U> requires U in {str} { value: U }\n",
        "const fn through<U>(value: U) -> U {\n",
        "    let held = OnlyText<U>(value: value)\n",
        "    return held.value\n",
        "}\n",
        "fn wrapped<T>(const input: T) -> i64 {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    let copied = through(chosen)\n",
        "    return copied\n",
        "}\n",
        "fn late() -> i64 => wrapped(1)\n",
        "fn direct() -> i64 => through(1)\n",
        "operator delegated<V>(value: V) -> V\n",
        "impl const fn delegated<V>(value: V) -> V => through(value)\n",
        "fn wrapped_operator<T>(const input: T) -> i64 {\n",
        "    let chosen = if predicate(input) { 1 }\n",
        "    return delegated(chosen)\n",
        "}\n",
        "fn late_operator() -> i64 => wrapped_operator(1)\n",
        "fn direct_operator() -> i64 => delegated(1)\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:15:21 constraint/requires.unmet GEN-3",
            "m.hgl:16:23 constraint/requires.unmet GEN-3",
            "m.hgl:23:30 constraint/requires.unmet GEN-3",
            "m.hgl:24:32 constraint/requires.unmet GEN-3",
        ],
        "{}",
        checked.report
    );
    assert_eq!(
        checked
            .report
            .matches("the `requires` of `OnlyText`")
            .count(),
        4
    );
}

/// GEN-7 and TYPE-7: a graph instance checks the enum operand once it
/// knows the type, and keeps the declared enum on the resulting expression.
#[test]
fn graph_instances_check_enum_constructors_after_their_operands_settle() {
    let checked = module(concat!(
        "enum Side { buy, sell }\n",
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "fn side<T>(const input: T) -> Side {\n",
        "    let chosen = if predicate(input) { 0 }\n",
        "    return Side(chosen)\n",
        "}\n",
        "fn incompatible<T>(const input: T) -> Side {\n",
        "    let chosen = if predicate(input) { true }\n",
        "    return Side(chosen)\n",
        "}\n",
        "fn accepted() -> Side => side(3)\n",
        "fn rejected() -> Side => incompatible(3)\n",
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:15:26 type/type.mismatch TYPE-7"],
        "{}",
        checked.report
    );

    let instance = instance_called_by(&checked, body(&checked, "accepted"));
    let Some(Stmt::Return {
        value: Some(returned),
        ..
    }) = instance.statements.last()
    else {
        panic!("the instance returns its constructed enum");
    };
    let ExprKind::EnumFrom {
        module,
        declaration,
        value,
    } = &returned.kind
    else {
        panic!("expected a checked enum constructor");
    };
    assert_eq!(value.ty.to_string(), "i64");
    assert!(matches!(
        &returned.ty,
        hgl_analyser::Type::Named { name, module: owner, declaration: declared, .. }
            if name == "Side" && owner == module && declared == declaration
    ));
}

/// GEN-7 and ERR-15: a result that settles in a graph instance takes
/// its nullable form beside a literal null, in either operand order.
#[test]
fn graph_instances_read_results_for_null_comparisons() {
    let checked = module(concat!(
        "operator predicate<T>(input: T) -> bool\n",
        "impl const fn predicate<T>(input: T) -> bool => true\n",
        "impl fn predicate(input: i64) -> bool { when { return input > 0 } }\n",
        "const fn sample() -> result<i64, error> => err(error(message: \"failed\"))\n",
        "fn missing<T>(const input: T) -> bool {\n",
        "    let samples = if predicate(input) { [sample()] }\n",
        "    let chosen = samples[0]\n",
        "    return chosen == null\n",
        "}\n",
        "fn present<T>(const input: T) -> bool {\n",
        "    let samples = if predicate(input) { [sample()] }\n",
        "    let chosen = samples[0]\n",
        "    return (null) != chosen\n",
        "}\n",
        "fn equal() -> bool => missing(\"s\")\n",
        "fn unequal() -> bool => present(\"s\")\n",
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    for (name, null_first) in [("equal", false), ("unequal", true)] {
        let instance = instance_called_by(&checked, body(&checked, name));
        let Some(Stmt::Return {
            value: Some(returned),
            ..
        }) = instance.statements.last()
        else {
            panic!("the instance returns its comparison");
        };
        let ExprKind::Binary { left, right, .. } = &returned.kind else {
            panic!("the null comparison stays intrinsic");
        };
        let (null, result_read) = if null_first {
            (left, right)
        } else {
            (right, left)
        };
        assert!(matches!(
            null.kind,
            ExprKind::Literal(hgl_analyser::hir::Value::Null)
        ));
        assert!(matches!(result_read.kind, ExprKind::OrNull { .. }));
        assert_eq!(result_read.ty.to_string(), "i64?");
    }
}
