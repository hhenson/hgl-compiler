//! Equality of the analyser's constant and type representations.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use hgl_analyser::hir::{BinaryOperator, CalendarDate, ClockTime, Temporal, Tenor, UnaryOperator};
use hgl_analyser::{Argument, Constant, ConstantValue as V, EnumOf, MemberOf, Scalar, Type};
use hgl_diagnostics::Location;
use hgl_source::{SourceMap, Span};

fn constant(value: V) -> Constant {
    Constant {
        text: "source expression".into(),
        value,
    }
}

fn location() -> Location {
    let file = SourceMap::new().add("m.hgl", "module m").unwrap();
    Location::new(file, Span::new(0, 1))
}

fn hash(value: &impl Hash) -> u64 {
    let mut state = DefaultHasher::new();
    value.hash(&mut state);
    state.finish()
}

#[test]
fn every_constant_form_is_reflexive() {
    let at = location();
    let of = EnumOf {
        declared: (0, 0),
        written: "m::Mode".into(),
    };
    let values = [
        V::Type(Box::new(Type::Scalar(Scalar::I64))),
        V::InstanceTypeQuery {
            at,
            bindings: vec![(at, Argument::Constant(constant(V::Unknown)))],
        },
        V::Int(1),
        V::Float(-0.0),
        V::Float(f64::NAN),
        V::Bool(true),
        V::Str("text".into()),
        V::Null,
        V::Duration(1),
        V::Tenor(Tenor::BusinessDays(1)),
        V::Temporal(Temporal {
            date: None,
            time: None,
            offset: None,
            zone: Some("UTC".into()),
        }),
        V::Member(MemberOf {
            of: Some(of.clone()),
            name: "slow".into(),
        }),
        V::List(vec![V::Unevaluated]),
        V::StringSet(["name".into()].into()),
        V::Tuple(vec![V::Unknown]),
        V::Unbounded,
        V::Parameter {
            name: "N".into(),
            at,
        },
        V::Named {
            module: 0,
            declaration: 1,
        },
        V::Constructed {
            of,
            members: vec![("slow".into(), Some(1))],
            from: Box::new(V::Unknown),
        },
        V::Unary(UnaryOperator::Negate, Box::new(V::Unknown)),
        V::Binary(
            BinaryOperator::Add,
            Box::new(V::Unknown),
            Box::new(V::Int(1)),
        ),
        V::RangeLength(Box::new(V::Int(0)), Box::new(V::Unknown)),
        V::DeferredTypeQuery(at),
        V::DeferredDefaultQuery(at),
        V::Unevaluated,
        V::Unknown,
    ];
    for value in values {
        assert!(value.eq(&value));
        let constant = constant(value);
        let copy = Constant {
            text: "another spelling".into(),
            ..constant.clone()
        };
        assert!(constant.eq(&constant));
        assert_eq!(constant, copy);
        assert_eq!(hash(&constant), hash(&copy));
        assert!(HashSet::from([constant]).contains(&copy));
    }
}

#[test]
fn a_list_type_with_an_unevaluated_extent_can_be_found() {
    let list = |text: &str| Type::List {
        element: Box::new(Type::Scalar(Scalar::I64)),
        size: Some(Constant {
            text: text.into(),
            value: V::Unevaluated,
        }),
    };
    let ty = list("first spelling");
    let copy = list("second spelling");
    assert!(std::slice::from_ref(&ty).contains(&copy));
    assert_eq!(hash(&ty), hash(&copy));
    assert_eq!(HashMap::from([(ty.clone(), 1)]).get(&copy), Some(&1));
    assert!(!ty.same_type(&copy));

    // TYPE-18: an unresolved extent cannot prove the identity of a Type value.
    let value = constant(V::Type(Box::new(ty)));
    assert_eq!(value.same_value(&value), None);
}

#[test]
fn an_untyped_member_cannot_make_structural_equality_nontransitive() {
    let member = |of| {
        constant(V::Member(MemberOf {
            of,
            name: "slow".into(),
        }))
    };
    let typed = member(Some(EnumOf {
        declared: (0, 0),
        written: "m::Mode".into(),
    }));
    let untyped = member(None);
    let other = member(Some(EnumOf {
        declared: (0, 1),
        written: "m::Speed".into(),
    }));
    assert_ne!(typed, untyped);
    assert_ne!(untyped, other);
    assert_ne!(typed, other);
    assert_eq!(typed.same_value(&untyped), Some(true));
    assert_eq!(untyped.same_value(&other), Some(true));
    assert_eq!(typed.same_value(&other), Some(false));
}

/// UNIT-13: signed zeros name distinct constants, all NaNs name one,
/// and datetimes that give the same instant name one.
#[test]
fn semantic_comparison_uses_values_when_representations_differ() {
    let zero = constant(V::Float(0.0));
    let negative_zero = constant(V::Float(-0.0));
    assert_ne!(zero, negative_zero);
    assert_eq!(zero.same_value(&negative_zero), Some(false));

    let nan = constant(V::Float(f64::from_bits(0x7ff8_0000_0000_0001)));
    let other_nan = constant(V::Float(f64::from_bits(0x7ff8_0000_0000_0002)));
    assert_ne!(nan, other_nan);
    assert_eq!(nan.same_value(&other_nan), Some(true));

    let datetime = |hour, offset| {
        constant(V::Temporal(Temporal {
            date: Some(CalendarDate {
                year: 2026,
                month: 9,
                day: 3,
            }),
            time: Some(ClockTime {
                hour,
                minute: 30,
                second: 0,
                microsecond: 0,
            }),
            offset: Some(offset),
            zone: None,
        }))
    };
    let utc = datetime(9, 0);
    let offset = datetime(10, 60);
    assert_ne!(utc, offset);
    assert_eq!(utc.same_value(&offset), Some(true));
    let ty = |value| Type::Named {
        name: "Timed".into(),
        module: 0,
        declaration: 0,
        arguments: vec![Argument::Constant(value)],
        kind: hgl_analyser::NamedKind::Struct,
    };
    assert_ne!(ty(utc.clone()), ty(offset.clone()));
    assert!(ty(utc).same_type(&ty(offset)));
}

#[test]
fn structurally_identical_query_bindings_can_be_semantically_unknown() {
    let at = location();
    let query = constant(V::InstanceTypeQuery {
        at,
        bindings: vec![(at, Argument::Constant(constant(V::Unevaluated)))],
    });
    assert_eq!(query, query.clone());
    assert_eq!(query.same_value(&query), None);
}
