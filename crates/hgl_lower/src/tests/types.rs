//! Canonical identity and the fields and whole-read requirements of types.

use hgl_analyser::{Scalar, Type};

use crate::canonical::{canonical, canonical_argument};
use crate::types::{empty_struct, fields_of, inherited, read_whole_all_valid};

use super::{declaration, program, signature};

/// UNIT-4, TYPE-3, UNIT-13: the origin and the complete canonical
/// arguments identify a struct. Its import spelling and how a constant
/// argument is written do not affect the key.
#[test]
fn nominal_keys_preserve_origin_and_canonical_arguments() {
    let program = program(&[
        (
            "left.hgl",
            "module left\nexport struct Box<T, const N: i64> { values: list<T, N> }\n",
        ),
        (
            "right.hgl",
            "module right\nexport struct Box<T, const N: i64> { values: list<T, N> }\n",
        ),
        (
            "m.hgl",
            r#"module m
use left as l
use left::{Box}
use right as r
fn read(aliased: l::Box<i64, 2 + 3>, imported: Box<i64, 5>,
        other_origin: r::Box<i64, 5>, other_type: Box<bool, 5>,
        other_size: Box<i64, 6>) {}
"#,
        ),
    ]);
    let signature = signature(&program, "m", "read");
    let keys: Vec<_> = signature
        .parameters
        .iter()
        .map(|parameter| canonical(&program, &parameter.ty).expect("concrete type key"))
        .collect();
    assert_eq!(
        keys,
        [
            "left::Box<i64, 5>",
            "left::Box<i64, 5>",
            "right::Box<i64, 5>",
            "left::Box<bool, 5>",
            "left::Box<i64, 6>",
        ]
    );
    let Type::Named { arguments, .. } = &signature.parameters[0].ty else {
        panic!("a named application");
    };
    assert_eq!(
        canonical_argument(&program, &arguments[1]).as_deref(),
        Some("5")
    );
}

/// TYPE-10, TYPE-11: fixed lists use their evaluated size, growing
/// lists and tuples have their own identities, and writing a window's
/// minimum equal to its maximum is equivalent to leaving it out.
#[test]
fn canonical_keys_distinguish_composite_shapes() {
    let program = program(&[(
        "m.hgl",
        r#"module m
fn read(folded: list<i64, 1 + 1>, fixed: list<i64, 2>, growing: list<i64>,
        tupled: tuple<i64, i64>, omitted: rolling_window<i64, 2>,
        same: rolling_window<i64, 2, 2>, other: rolling_window<i64, 2, 1>) {}
fn generic<T>(value: list<T>) {}
"#,
    )]);
    let keys: Vec<_> = signature(&program, "m", "read")
        .parameters
        .iter()
        .map(|parameter| canonical(&program, &parameter.ty).expect("concrete type key"))
        .collect();
    assert_eq!(
        keys,
        [
            "list<i64, 2>",
            "list<i64, 2>",
            "list<i64>",
            "tuple<i64, i64>",
            "rolling_window<i64, 2>",
            "rolling_window<i64, 2>",
            "rolling_window<i64, 2, 1>",
        ]
    );
    let unresolved = &signature(&program, "m", "generic").parameters[0].ty;
    assert_eq!(canonical(&program, unresolved), None);
}

/// TYPE-3, TYPE-4: inheritance keeps a parent's ordered fields with
/// concrete types, including optionality, in each application.
#[test]
fn fields_follow_the_concrete_struct_identity() {
    let program = program(&[(
        "m.hgl",
        r#"module m
abstract struct Base<T> { value: T }
struct Row : Base<i64?> { enabled: bool }
struct Box<T> { value: T }
fn read(derived: Row, optional: Box<i64?>, required: Box<i64>) {}
"#,
    )]);
    let read = signature(&program, "m", "read");
    let fields = |index: usize| {
        fields_of(&program, &read.parameters[index].ty)
            .expect("fixture parameter is a struct")
            .into_iter()
            .map(|(member, ty)| (member.name.as_str(), ty))
            .collect::<Vec<_>>()
    };
    let integer = Type::Scalar(Scalar::I64);
    let optional = Type::Nullable(Box::new(integer.clone()));
    assert_eq!(
        fields(0),
        [
            ("value", optional.clone()),
            ("enabled", Type::Scalar(Scalar::Bool))
        ]
    );
    assert_eq!(fields(1), [("value", optional)]);
    assert_eq!(fields(2), [("value", integer)]);
    let module = program.module_index("m").expect("fixture module");
    let base = declaration(&program.modules[module], "Base");
    let row = declaration(&program.modules[module], "Row");
    assert!(inherited(&program, module, base));
    assert!(!inherited(&program, module, row));
}

/// TYPE-9, TIME-1: a whole value with child fields or fixed positions
/// needs all of them valid. An empty concrete struct publishes whole.
#[test]
fn whole_read_requirements_distinguish_temporal_shapes() {
    let program = program(&[(
        "m.hgl",
        r#"module m
struct Empty {}
struct Data { value: i64 }
fn read(empty: Empty, data: Data, fixed: list<i64, 2>, growing: list<i64>,
        pair: tuple<i64, bool>, atom: atomic<Data>) {}
"#,
    )]);
    let read = signature(&program, "m", "read");
    let found: Vec<_> = read
        .parameters
        .iter()
        .map(|parameter| {
            (
                parameter.name.as_str(),
                empty_struct(&program, &parameter.ty),
                read_whole_all_valid(&program, &parameter.ty),
            )
        })
        .collect();
    assert_eq!(
        found,
        [
            ("empty", true, false),
            ("data", false, true),
            ("fixed", false, true),
            ("growing", false, false),
            ("pair", false, true),
            ("atom", false, false),
        ]
    );
}
