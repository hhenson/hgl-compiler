//! The identity of an instance requested by a call with captured arguments.

use std::ptr;

use hgl_analyser::hir::{Argument, Callee, ExprKind};
use hgl_analyser::{Detail, Scalar, Type};

use crate::instances::{CallableLimits, Instances};
use crate::packs::Captures;

use super::{body, declaration, program, returned};

const TEST_CALLABLE_LIMITS: CallableLimits = CallableLimits {
    size: 1,
    depth: 1,
    growth: 1,
};

fn call(body: &hgl_analyser::hir::Body) -> (&Callee, &[Argument]) {
    let ExprKind::Call {
        callee, arguments, ..
    } = &returned(body).kind
    else {
        panic!("fixture returns a call");
    };
    (callee, arguments)
}

/// TST-1 and UNIT-13: enum constants from different tests identify
/// different generic types, even when their enum and member names match.
#[test]
fn test_local_enum_constants_identify_distinct_instances() {
    let program = program(&[(
        "m.hgl",
        r#"module m
struct Box<const Tag: any> { value: i64 }
const fn keep<T>(value: T) -> T => value

test first {
    enum Mode { x }
    assert keep(Box<Mode.x>(value: 1)).value == 1
}

test second {
    enum Mode { x }
    assert keep(Box<Mode.x>(value: 2)).value == 2
}
"#,
    )]);
    let module = program.module_index("m").expect("fixture module");
    let keep = declaration(&program.modules[module], "keep");
    let instances = Instances::typed(&program, TEST_CALLABLE_LIMITS);
    let keys: Vec<_> = instances
        .declared(module, keep)
        .iter()
        .map(|instance| {
            crate::canonical::canonical(&program, &instance.signature.parameters[0].ty)
                .expect("the concrete argument has a canonical type")
        })
        .collect();
    assert_eq!(
        keys,
        ["m::Box<m::first::Mode.x>", "m::Box<m::second::Mode.x>"]
    );
}

/// FN-8, GEN-2, GEN-10: a named pack's types and source keys identify the
/// instance. The same declaration and shape share that instance across
/// call sites and modules, even with different argument values.
#[test]
fn named_pack_instances_are_shared_by_shape() {
    let program = program(&[
        (
            "library.hgl",
            r#"module library
export const fn keep<...Fields>(fixed: i64, fields: ...{Fields}) -> i64 => fixed
"#,
        ),
        (
            "first.hgl",
            r#"module first
use library::{keep}
const fn one() -> i64 => keep(1, left: 2, right: true)
const fn two() -> i64 => keep(3, left: 4, right: false)
const fn renamed() -> i64 => keep(5, other: 6, right: true)
const fn retyped() -> i64 => keep(7, left: false, right: true)
"#,
        ),
        (
            "second.hgl",
            r#"module second
use library as lib
const fn three() -> i64 => lib::keep(8, left: 9, right: false)
"#,
        ),
    ]);

    let module = program.module_index("library").expect("library module");
    let index = declaration(&program.modules[module], "keep");
    let Detail::Callable(signature) = &program.modules[module].details[index] else {
        panic!("keep is a function");
    };
    let instances = Instances::typed(&program, TEST_CALLABLE_LIMITS);
    assert_eq!(instances.declared(module, index).len(), 3);

    let (callee, arguments) = call(body(&program, "first", "one"));
    let original = instances
        .find_call(&program, callee, arguments)
        .expect("the first call has an instance");
    for (path, name) in [("first", "two"), ("second", "three")] {
        let (callee, arguments) = call(body(&program, path, name));
        let instance = instances
            .find_call(&program, callee, arguments)
            .expect(name);
        assert!(ptr::eq(instance, original), "{path}::{name}");
    }

    for (name, expected_key) in [
        ("one", ["1[left]:i64", "1[right]:bool"]),
        ("renamed", ["1[other]:i64", "1[right]:bool"]),
        ("retyped", ["1[left]:bool", "1[right]:bool"]),
    ] {
        let (callee, arguments) = call(body(&program, "first", name));
        let captures = Captures::of(&program, callee, signature, arguments);
        assert_eq!(captures.key(&program).expect("concrete pack"), expected_key);
        let instance = instances
            .find_call(&program, callee, arguments)
            .expect(name);
        assert_eq!(ptr::eq(instance, original), name == "one", "{name}");
        assert_eq!(instance.signature.parameters.len(), 3, "{name}");
        assert!(instance.pack_use.is_none(), "{name}");
        for (position, argument) in arguments.iter().enumerate() {
            assert_eq!(instance.parameter(argument), Some(position), "{name}");
            let parameter = &instance.signature.parameters[position];
            assert_eq!(parameter.ty, argument.value.ty, "{name}");
            assert_eq!(instance.body.locals[position].ty, parameter.ty, "{name}");
            assert!(parameter.pack.is_none(), "{name}");
        }
    }
}

/// FN-8, GEN-2, GEN-10: ordered heterogeneous positional captures
/// distinguish types at different positions. Explicit type arguments
/// and inference of those same types request the same instance.
#[test]
fn positional_pack_identity_includes_order_and_length() {
    let program = program(&[(
        "m.hgl",
        r#"module m
const fn gather<...Items>(items: ...Items) -> i64 => 1
const fn inferred() -> i64 => gather(1, false)
const fn explicit() -> i64 => gather<i64, bool>(2, true)
const fn reversed() -> i64 => gather(true, 3)
const fn shorter() -> i64 => gather(4)
"#,
    )]);
    let module = program.module_index("m").expect("fixture module");
    let index = declaration(&program.modules[module], "gather");
    let Detail::Callable(signature) = &program.modules[module].details[index] else {
        panic!("gather is a function");
    };
    let instances = Instances::typed(&program, TEST_CALLABLE_LIMITS);
    assert_eq!(instances.declared(module, index).len(), 3);

    let (callee, arguments) = call(body(&program, "m", "inferred"));
    let inferred = instances
        .find_call(&program, callee, arguments)
        .expect("the inferred instance");
    let (callee, arguments) = call(body(&program, "m", "explicit"));
    let explicit = instances
        .find_call(&program, callee, arguments)
        .expect("the explicitly typed instance");
    assert!(ptr::eq(inferred, explicit));

    for (name, key, types) in [
        (
            "inferred",
            &["0[0]:i64", "0[1]:bool"][..],
            &[Scalar::I64, Scalar::Bool][..],
        ),
        (
            "reversed",
            &["0[0]:bool", "0[1]:i64"][..],
            &[Scalar::Bool, Scalar::I64][..],
        ),
        ("shorter", &["0[0]:i64"][..], &[Scalar::I64][..]),
    ] {
        let (callee, arguments) = call(body(&program, "m", name));
        let captures = Captures::of(&program, callee, signature, arguments);
        assert_eq!(captures.key(&program).expect("concrete pack"), key);
        let instance = instances
            .find_call(&program, callee, arguments)
            .expect(name);
        let expected: Vec<_> = types.iter().copied().map(Type::Scalar).collect();
        let parameters: Vec<_> = instance
            .signature
            .parameters
            .iter()
            .map(|parameter| parameter.ty.clone())
            .collect();
        assert_eq!(parameters, expected, "{name}");
    }
}

/// FN-8, GEN-10: a homogeneous pack with no generic parameters still
/// needs one instance for each arity, including the empty pack.
#[test]
fn homogeneous_pack_instances_include_empty_and_nonempty_shapes() {
    let program = program(&[(
        "m.hgl",
        r#"module m
const fn gather(items: ...i64) -> i64 => 1
const fn empty() -> i64 => gather()
const fn one() -> i64 => gather(1)
const fn two() -> i64 => gather(2, 3)
const fn also_two() -> i64 => gather(4, 5)
"#,
    )]);
    let module = program.module_index("m").expect("fixture module");
    let index = declaration(&program.modules[module], "gather");
    let instances = Instances::typed(&program, TEST_CALLABLE_LIMITS);
    assert_eq!(instances.declared(module, index).len(), 3);

    for (name, arity) in [("empty", 0), ("one", 1), ("two", 2), ("also_two", 2)] {
        let (callee, arguments) = call(body(&program, "m", name));
        let instance = instances
            .find_call(&program, callee, arguments)
            .expect(name);
        assert_eq!(instance.signature.parameters.len(), arity, "{name}");
        for (position, argument) in arguments.iter().enumerate() {
            assert_eq!(instance.parameter(argument), Some(position), "{name}");
            assert_eq!(
                instance.signature.parameters[position].ty,
                Type::Scalar(Scalar::I64)
            );
        }
    }
}

/// A backend can only project the identities supplied by lowering. Even an
/// unused concrete parameter needs the selected field type for its layout.
#[test]
fn checked_layout_projection_reads_only_reached_applications() {
    let program = program(&[(
        "m.hgl",
        r#"module m
const fn values<T>() -> list<T> => []
operator pick<A,R>(value: A) -> R
impl const fn pick(value: list<i64>) -> bool { assert false
    return true }
impl const fn pick(value: list<str>) -> str { assert false
    return "cold" }
struct Marker<const Kind: Type> { id: i64 }
struct Frame<T> requires T in {i64, str} { tag: Marker<type(pick(values<T>()))> }
const fn unused(x: Frame<i64>) -> i64 => 3
"#,
    )]);
    let module = program.module("m").unwrap();
    let Detail::Struct(frame) = &module.details[declaration(module, "Frame")] else {
        panic!("fixture defines Frame");
    };
    let field = &frame.members[0].ty;
    let project = |scalar| {
        let argument = hgl_analyser::Argument::Type(Type::Scalar(scalar));
        program.checked_layout_type(field, &[(frame.generics[0].at, &argument)])
    };
    let identity = |ty: Type| {
        let Type::Named { mut arguments, .. } = ty else {
            panic!("field has type Marker");
        };
        let Some(hgl_analyser::Argument::Constant(c)) = arguments.pop() else {
            panic!("Marker takes a constant");
        };
        c.value
    };
    assert!(matches!(
        identity(project(Scalar::I64)),
        hgl_analyser::ConstantValue::InstanceTypeQuery { .. }
    ));
    Instances::typed(&program, TEST_CALLABLE_LIMITS);
    assert!(matches!(
        identity(project(Scalar::I64)),
        hgl_analyser::ConstantValue::Type(ty) if *ty == Type::Scalar(Scalar::Bool)
    ));
    assert!(matches!(
        identity(project(Scalar::Str)),
        hgl_analyser::ConstantValue::InstanceTypeQuery { .. }
    ));
}
