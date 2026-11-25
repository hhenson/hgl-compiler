//! Native contract phases retained for later analyser and target passes.

mod common;

use common::check;
use hgl_analyser::{DeclarationKind, Phase, Role};

/// NATIVE-3: an empty temporal contract is a graph, and `when;` makes
/// its implementation a node. A declaration without a body has no phase.
#[test]
fn native_implementations_keep_their_declared_temporal_phase() {
    let checked = check(&[
        (
            "interface.hgl",
            "module probe.native_phase\nnative fn graph(const input: i64) -> i64\nnative fn node(input: i64) -> i64\n",
        ),
        (
            "provider.hgl",
            "module probe.native_phase part provider\nnative fn graph(const input: i64) -> i64 {}\nnative fn node(input: i64) -> i64 { when; }\n",
        ),
    ]);
    assert!(checked.summary().is_empty(), "{}", checked.report);
    let module = checked.program.module("probe.native_phase").unwrap();
    let phases: Vec<_> = module
        .declarations
        .iter()
        .zip(&module.phases)
        .filter(|(declaration, _)| {
            declaration.kind == DeclarationKind::NativeImplementation(Role::Temporal)
        })
        .map(|(declaration, phase)| (declaration.name.as_str(), *phase))
        .collect();
    assert_eq!(
        phases,
        [("graph", Some(Phase::Graph)), ("node", Some(Phase::Node))]
    );
    for (declaration, phase) in module.declarations.iter().zip(&module.phases) {
        if matches!(declaration.kind, DeclarationKind::NativeFunction(_)) {
            assert_eq!(*phase, None);
        }
    }
}

/// NATIVE-1/3: candidate implementations retain their contract phase;
/// native const implementations remain value-level. Native inject and
/// lifecycle markers keep the native contract's validation rules.
#[test]
fn native_candidates_and_full_contracts_keep_their_phase_and_role() {
    let checked = check(&[
        (
            "interface.hgl",
            concat!(
                "module m\n",
                "native fn full(input: i64) -> i64\n",
                "native const fn plain(input: i64) -> i64\n",
                "operator choose<T>(input: T) -> T\n",
                "native impl fn choose(input: i64) -> i64\n",
                "native impl fn choose(input: f64) -> f64\n",
                "native impl const fn choose(input: bool) -> bool\n",
            ),
        ),
        (
            "provider.hgl",
            concat!(
                "module m part provider\n",
                "native fn full(input: i64) -> i64 { inject out, clock; start; when; when; stop; }\n",
                "native const fn plain(input: i64) -> i64 {}\n",
                "native impl fn choose(input: i64) -> i64 { when; }\n",
                "native impl fn choose(input: f64) -> f64 {}\n",
                "native impl const fn choose(input: bool) -> bool {}\n",
            ),
        ),
    ]);
    assert!(checked.summary().is_empty(), "{}", checked.report);
    let module = checked.program.module("m").unwrap();
    let phases: Vec<_> = module
        .declarations
        .iter()
        .zip(&module.phases)
        .filter(|(declaration, _)| {
            matches!(
                declaration.kind,
                DeclarationKind::NativeImplementation(_)
                    | DeclarationKind::NativeCandidateImplementation(_)
            )
        })
        .map(|(declaration, phase)| (declaration.name.as_str(), *phase))
        .collect();
    assert_eq!(
        phases,
        [
            ("full", Some(Phase::Node)),
            ("plain", None),
            ("choose", Some(Phase::Node)),
            ("choose", Some(Phase::Graph)),
            ("choose", None),
        ]
    );
}
