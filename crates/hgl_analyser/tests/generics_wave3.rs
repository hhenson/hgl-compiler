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
    ));
    assert_eq!(checked.summary(), NONE, "{}", checked.report);
    let instance = |caller: &str| instance_called_by(&checked, body(&checked, caller));
    assert_eq!(called(&checked, &instance("ints")), ["lt_(T, T) <i64>"]);
    assert_eq!(called(&checked, &instance("floats")), ["lt_(T, T) <f64>"]);
    assert_eq!(called(&checked, &instance("seven")), ["kind_of(i64)"]);
    assert_eq!(called(&checked, &instance("yes")), ["kind_of(T) <bool>"]);
    // A candidate that takes every `T` is selected in the body itself.
    assert_eq!(called(&checked, body(&checked, "same")), ["eq_(T, T) <T>"]);
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
/// comes to: `kind(value)` in `wrap<U>(value: list<U>)` is the list
/// candidate's, known where the body is declared, with nothing stated.
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
    ));
    assert_eq!(
        checked.summary(),
        ["m.hgl:9:39 operator/operator.no_candidate GEN-5"],
        "{}",
        checked.report
    );
    assert_eq!(
        called(&checked, body(&checked, "wrap")),
        ["kind(list<T>) <U>"]
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
