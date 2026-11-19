//! Compares the defaults a struct has for one field from two of its
//! parents (UNIT-9).
//!
//! `declared` works out the fields each struct has, and finds where two
//! parents give one field a default and the struct gives it none of its
//! own. Whether those are one default is a question of what each is
//! written as, which `values` reads. So this pass runs after it: each
//! default is compared as `values` has read it, with the arguments its
//! path gives the struct that writes it in place.
//!
//! UNIT-9: "Compare inherited defaults by bound expression and
//! canonical type after substitution, before folding. Declaration
//! identity matters; source location and alias spelling do not.
//! Conflicting inherited defaults require an explicit child override."

use hgl_diagnostics::{Category, Code, Diagnostic, Diagnostics, Location};
use hgl_lexer::literal::{decode_float, decode_string};
use hgl_source::Span;

use crate::codes;
use crate::declared::{bare_literal, substitute_within};
use crate::hir::{Callee, Expr, ExprKind, Form, Stmt, UnaryOperator, Value};
use crate::program::Program;
use crate::types::{
    Argument, Constant, ConstantValue, Detail, FieldRef, GenericKind, GenericParameter, Scalar,
    Type,
};

/// The defaults two parents give one field of a struct, which gives
/// that field none of its own.
#[derive(Debug, Clone)]
pub(crate) struct InheritedDefaults {
    /// The struct: the position of its module in `Program::modules`,
    /// and its own in that module's declarations.
    pub(crate) owner: (usize, usize),
    /// The field's name.
    pub(crate) field: String,
    /// The default the earlier parent gives it, then the later's.
    pub(crate) defaults: [InheritedDefault; 2],
}

/// One default a struct has for a field from a parent.
#[derive(Debug, Clone)]
pub(crate) struct InheritedDefault {
    /// The field that writes it, whose value is in `Module::defaults`.
    pub(crate) written: FieldRef,
    /// What the generic parameters of the struct that writes it stand
    /// for in the struct that has the field, each by where it is
    /// declared.
    pub(crate) arguments: Vec<(Location, Argument)>,
    /// Where it is written.
    pub(crate) at: Location,
    /// Where the parent it comes through is written, after the `:`.
    pub(crate) parent: Location,
}

/// Reports each struct that has two different defaults for one field
/// from its parents and gives the field none of its own: once for each
/// field, at the later parent.
///
/// A pair either of whose defaults is not known, or names what has no
/// value here, is left out (ERR-9).
pub(crate) fn check(
    program: &Program,
    pending: &[InheritedDefaults],
    diagnostics: &mut Diagnostics,
) {
    let mut reported: Vec<((usize, usize), &str)> = Vec::new();
    for pair in pending {
        if reported.contains(&(pair.owner, pair.field.as_str())) {
            continue;
        }
        let [earlier, later] = &pair.defaults;
        // Every expression of both is put at one place, so that where
        // each is written does not tell them apart.
        let place = Location::new(earlier.at.file, Span::new(0, 0));
        let (Some(mut first), Some(mut second)) =
            (bound(program, earlier, place), bound(program, later, place))
        else {
            continue;
        };
        // A default whose type is not known, as one of a field whose
        // type names a generic parameter is not (it is read with no
        // type expected), is compared by what it is written as alone:
        // the field, and so the type each default is for, is one.
        if !knows_types(&first) || !knows_types(&second) {
            forget_types(&mut first);
            forget_types(&mut second);
        }
        if first == second {
            continue;
        }
        reported.push((pair.owner, pair.field.as_str()));
        let owner = &program.modules[pair.owner.0].declarations[pair.owner.1].name;
        let message = format!(
            "`{owner}` has `{}` from two parents with different defaults, and gives it none \
             of its own: write `{} = ...` in `{owner}`",
            pair.field, pair.field
        );
        diagnostics.emit(
            Diagnostic::error(
                Category::Type,
                Code::Local(codes::local::DEFAULT_CONFLICT),
                later.parent,
                message,
            )
            .with_rule("UNIT-9")
            .with_related(earlier.at, "one default")
            .with_related(later.at, "the other"),
        );
    }
}

/// The default `default` names, as `values` has read it, with the
/// arguments of its path in place of the generic parameters of the
/// struct that writes it, and every expression put at `place`. `None`
/// where it is not read, or names what this pass cannot put in place.
///
/// What is compared is what the default is bound to, and each constant
/// in it by its value: a constant given to a generic value parameter,
/// to a callee or inside a type is that value however it is written
/// (GEN-8: "a compile-time constant"). So `(true)` is `true`, `-(0.5)`
/// is `-0.5`, `1_000` is `1000`, the name of an `i64` constant is its
/// value, and an integer where an `f64` is wanted is that `f64`
/// (EXPR-5). Only folding is not done (UNIT-9: "before folding"): `1 +
/// 1` is no `2`.
fn bound(program: &Program, default: &InheritedDefault, place: Location) -> Option<Expr> {
    let written = default.written;
    let mut value = program.modules[written.module]
        .defaults
        .get(written.declaration)?
        .get(written.field)?
        .clone()?;
    let map: Vec<(Location, &Argument)> = default
        .arguments
        .iter()
        .map(|(at, argument)| (*at, argument))
        .collect();
    let mut known = true;
    // `walk_mut` visits each expression before those inside it, and
    // then those inside what the visit made of it.
    value.walk_mut(&mut |expression| {
        expression.at = place;
        expression.ty = substitute_within(&expression.ty, &map);
        known &= canonical_type(program, &mut expression.ty);
        match &mut expression.kind {
            ExprKind::Unread => known = false,
            // A generic value parameter of the struct that writes the
            // default: the value its path gives it.
            ExprKind::GenericValue { at, .. } => {
                let given = map
                    .iter()
                    .find(|(declared, _)| declared == at)
                    .map(|(_, argument)| *argument);
                match given {
                    // One of the struct that has the field: itself.
                    None => {}
                    Some(Argument::Constant(constant)) => {
                        match given_value(program, constant, &expression.ty) {
                            Some(kind) => expression.kind = kind,
                            None => known = false,
                        }
                    }
                    Some(Argument::Type(_)) => known = false,
                }
            }
            ExprKind::Call { callee, .. } => {
                let (generics, declared) = match callee {
                    Callee::Function {
                        generics,
                        module,
                        declaration,
                        ..
                    }
                    | Callee::Candidate {
                        generics,
                        module,
                        declaration,
                        ..
                    } => (generics, (*module, *declaration)),
                    _ => return,
                };
                // What each of the callee's generic parameters is
                // declared as: an `f64` value takes an integer as that
                // `f64`.
                let kinds: Vec<Option<Type>> =
                    match program.modules[declared.0].details.get(declared.1) {
                        Some(Detail::Callable(signature)) => {
                            signature.generics.iter().map(value_type).collect()
                        }
                        _ => Vec::new(),
                    };
                for (index, generic) in generics.iter_mut().enumerate() {
                    if let Argument::Type(ty) = generic {
                        *ty = substitute_within(ty, &map);
                    }
                    // A generic value parameter given as a callee's
                    // argument: `limit<N>()` is `limit<3>()` where the
                    // path gives `N` 3.
                    if let Argument::Constant(constant) = generic
                        && let ConstantValue::Parameter { at, .. } = &constant.value
                    {
                        let given = map
                            .iter()
                            .find(|(declared, _)| declared == at)
                            .map(|(_, argument)| *argument);
                        match given {
                            None => {}
                            Some(Argument::Constant(given)) => *constant = given.clone(),
                            Some(Argument::Type(_)) => known = false,
                        }
                    }
                    let kind = kinds.get(index).cloned().flatten();
                    known &= canonical_argument(program, generic, kind.as_ref());
                }
            }
            ExprKind::Block { statements, .. } => {
                for statement in statements {
                    put(statement, place);
                }
            }
            _ => {}
        }
    });
    // A minus before a number is the negative number it writes, as a
    // generic argument `-3` is (LEX-7 reads the least `i64` so), and an
    // integer widened to an `f64` is that `f64` (EXPR-5). Each change
    // takes a node away, so the passes end; one exposes the next, as in
    // `-1` where an `f64` is wanted.
    loop {
        let mut changed = false;
        value.walk_mut(&mut |expression| {
            let literal = match &expression.kind {
                ExprKind::Widen(inner) => match inner.kind {
                    // `as` gives the nearest `f64`, as EXPR-5's widening
                    // does.
                    ExprKind::Literal(Value::Int(whole)) => Some(Value::Float(whole as f64)),
                    _ => None,
                },
                kind => negated(program, kind),
            };
            if let Some(literal) = literal {
                expression.kind = ExprKind::Literal(literal);
                changed = true;
            }
        });
        if !changed {
            break;
        }
    }
    // A symbol binds the operator a call by its name does (EXPR-10):
    // how the call is written is no part of what it is bound to.
    value.walk_mut(&mut |expression| {
        if let ExprKind::Call { form, .. } = &mut expression.kind {
            *form = Form::Named;
        }
    });
    known.then_some(value)
}

/// The type of the generic value parameter `generic`; `None` for a
/// type parameter, and for one declared with no type.
fn value_type(generic: &GenericParameter) -> Option<Type> {
    match &generic.kind {
        GenericKind::Value(ty) => Some(ty.clone()),
        _ => None,
    }
}

/// A constant as the value it is: what this pass holds of it, or for a
/// literal it holds as written (a float, a string, a bool, `null`), the
/// literal read through the parentheses and the minus around it
/// (EXPR-1). `None` for one it does not hold: an expression not
/// evaluated, a date or time, a tenor.
#[derive(Debug, Clone, PartialEq)]
enum Held {
    /// An integer.
    Int(i64),
    /// A float.
    Float(f64),
    /// A duration, in microseconds.
    Duration(i64),
    /// `true` or `false`.
    Bool(bool),
    /// A string's contents.
    Str(String),
    /// `null`.
    Null,
    /// A member of an enum, by its name.
    Member(String),
    /// A generic value parameter, by where it is declared.
    Parameter(String, Location),
    /// `unbounded`.
    Unbounded,
}

/// What `constant` is as a value; see [`Held`].
fn held(constant: &Constant) -> Option<Held> {
    Some(match &constant.value {
        ConstantValue::Int(value) => Held::Int(*value),
        ConstantValue::Duration(value) => Held::Duration(*value),
        ConstantValue::Member(name) => Held::Member(name.clone()),
        ConstantValue::Parameter { name, at } => Held::Parameter(name.clone(), *at),
        ConstantValue::Unbounded => Held::Unbounded,
        ConstantValue::Other(kind) => {
            let (negative, text) = bare_literal(&constant.text);
            match *kind {
                "a float" => {
                    let value = decode_float(text).ok()?;
                    Held::Float(if negative { -value } else { value })
                }
                "a string" if !negative => Held::Str(decode_string(text).ok()?),
                "a bool" if !negative => match text {
                    "true" => Held::Bool(true),
                    "false" => Held::Bool(false),
                    _ => return None,
                },
                "null" if !negative => Held::Null,
                _ => return None,
            }
        }
        ConstantValue::Unevaluated => return None,
    })
}

/// What a generic value parameter of type `ty` is, given `constant`:
/// the literal, enum member or generic value parameter of its value.
/// `None` where the value is not held.
fn given_value(program: &Program, constant: &Constant, ty: &Type) -> Option<ExprKind> {
    let literal = |value| Some(ExprKind::Literal(value));
    match held(constant)? {
        // EXPR-5: "An integer literal is an `f64` where one is expected".
        Held::Int(value) if *ty == Type::Scalar(Scalar::F64) => literal(Value::Float(value as f64)),
        Held::Int(value) => literal(Value::Int(value)),
        Held::Float(value) => literal(Value::Float(value)),
        Held::Duration(value) => literal(Value::Duration(value)),
        Held::Bool(value) => literal(Value::Bool(value)),
        Held::Str(value) => literal(Value::Str(value)),
        Held::Null => literal(Value::Null),
        Held::Member(name) => member(program, ty, &name),
        Held::Parameter(name, at) => Some(ExprKind::GenericValue { name, at }),
        Held::Unbounded => None,
    }
}

/// The member `name` of the enum `ty`, as an expression.
fn member(program: &Program, ty: &Type, name: &str) -> Option<ExprKind> {
    let Type::Named {
        module,
        declaration,
        ..
    } = ty
    else {
        return None;
    };
    let Some(Detail::Enum(read)) = program.modules[*module].details.get(*declaration) else {
        return None;
    };
    let member = read.members.iter().position(|member| member.name == name)?;
    Some(ExprKind::EnumMember {
        module: *module,
        declaration: *declaration,
        member,
    })
}

/// The negative number `kind` writes, where it is core's negation of a
/// number literal: `-0.5`, read as a call of `neg_` or not read as one,
/// and `neg_(0.5)`, which binds the same operator (EXPR-10). `None` for
/// any other, and for the negation of the least `i64`, which is no
/// `i64`.
fn negated(program: &Program, kind: &ExprKind) -> Option<Value> {
    // Core's `neg_`, the operator `-` before an operand stands for.
    let negation = |(module, declaration): (usize, usize)| {
        Some(module) == program.core()
            && program.modules[module].declarations[declaration].name
                == UnaryOperator::Negate.identity()
    };
    let operand = match kind {
        ExprKind::Call {
            form: Form::Unary(UnaryOperator::Negate),
            arguments,
            ..
        } if arguments.len() == 1 => &arguments[0].value,
        ExprKind::Call {
            callee: Callee::Candidate { operator, .. },
            arguments,
            ..
        } if arguments.len() == 1 && negation(*operator) => &arguments[0].value,
        ExprKind::Unary {
            operator: UnaryOperator::Negate,
            operand,
        } => operand,
        _ => return None,
    };
    match &operand.kind {
        ExprKind::Literal(Value::Int(value)) => value.checked_neg().map(Value::Int),
        ExprKind::Literal(Value::Float(value)) => Some(Value::Float(-value)),
        ExprKind::Literal(Value::Duration(value)) => value.checked_neg().map(Value::Duration),
        _ => None,
    }
}

/// Writes `constant` in one form for its value, so that two constants
/// that are one value are equal however each is written. `ty` is the
/// type of the generic value parameter it is given to, where that is
/// known: an integer given to an `f64` is that `f64` (EXPR-5). False
/// where its value is not held: it is then compared as written, and the
/// pair is not judged.
fn canonical(constant: &mut Constant, ty: Option<&Type>) -> bool {
    let Some(value) = held(constant) else {
        return false;
    };
    let (text, value) = match value {
        Held::Int(whole) if ty == Some(&Type::Scalar(Scalar::F64)) => (
            format!("{:?}", whole as f64),
            ConstantValue::Other("a float"),
        ),
        // `{:?}` writes a float so that it reads back, and a string
        // with its escapes in one form.
        Held::Float(real) => (format!("{real:?}"), ConstantValue::Other("a float")),
        Held::Str(text) => (format!("{text:?}"), ConstantValue::Other("a string")),
        Held::Bool(flag) => (flag.to_string(), ConstantValue::Other("a bool")),
        Held::Null => ("null".to_string(), ConstantValue::Other("null")),
        // The others are compared by the value this pass holds.
        _ => return true,
    };
    *constant = Constant { text, value };
    true
}

/// [`canonical`], for a generic argument: a constant, or each constant
/// inside a type. `ty` is as for `canonical`.
fn canonical_argument(program: &Program, argument: &mut Argument, ty: Option<&Type>) -> bool {
    match argument {
        Argument::Type(inner) => canonical_type(program, inner),
        Argument::Constant(constant) => canonical(constant, ty),
    }
}

/// [`canonical`], for each constant inside `ty`: a size, a window's
/// bounds, a generic argument of a struct, given as that struct's
/// parameter declares it. False where one's value is not held.
fn canonical_type(program: &Program, ty: &mut Type) -> bool {
    match ty {
        Type::Named {
            module,
            declaration,
            arguments,
            ..
        } => {
            let kinds: Vec<Option<Type>> = match program.modules[*module].details.get(*declaration)
            {
                Some(Detail::Struct(read)) => read.generics.iter().map(value_type).collect(),
                _ => Vec::new(),
            };
            let mut known = true;
            for (index, argument) in arguments.iter_mut().enumerate() {
                let kind = kinds.get(index).cloned().flatten();
                known &= canonical_argument(program, argument, kind.as_ref());
            }
            known
        }
        Type::Tuple(elements) => {
            let mut known = true;
            for element in elements {
                known &= canonical_type(program, element);
            }
            known
        }
        Type::List { element, size } => {
            let sized = size.as_mut().is_none_or(|size| canonical(size, None));
            canonical_type(program, element) && sized
        }
        Type::RollingWindow {
            value,
            maximum,
            minimum,
        } => {
            let least = minimum
                .as_mut()
                .is_none_or(|minimum| canonical(minimum, None));
            let most = canonical(maximum, None);
            canonical_type(program, value) && most && least
        }
        Type::Set(inner)
        | Type::Ref(inner)
        | Type::Endpoint(inner)
        | Type::Atomic(inner)
        | Type::Delta(inner)
        | Type::Trace { cell: inner, .. }
        | Type::Nullable(inner) => canonical_type(program, inner),
        Type::Dict { key, value } => {
            let keyed = canonical_type(program, key);
            canonical_type(program, value) && keyed
        }
        Type::Result { value, error } => {
            let failed = canonical_type(program, error);
            canonical_type(program, value) && failed
        }
        Type::Scalar(_)
        | Type::Parameter { .. }
        | Type::Signal
        | Type::Error
        | Type::ErrorContext
        | Type::Unknown => true,
    }
}

/// Puts `statement`, and every statement inside it, at `place`. The
/// expressions inside are put there by the walk that calls this.
fn put(statement: &mut Stmt, place: Location) {
    let mut bodies: Vec<&mut Vec<Stmt>> = Vec::new();
    match statement {
        Stmt::Declare { .. } | Stmt::Expr(_) => {}
        Stmt::Assign { at, .. }
        | Stmt::AssignPlace { at, .. }
        | Stmt::Inject { at, .. }
        | Stmt::Return { at, .. }
        | Stmt::Assert { at, .. }
        | Stmt::Yield { at, .. }
        | Stmt::Unread { at } => *at = place,
        Stmt::Hook { at, body, .. }
        | Stmt::AssertRaises { at, body, .. }
        | Stmt::For { at, body, .. }
        | Stmt::While { at, body, .. } => {
            *at = place;
            bodies.push(body);
        }
        Stmt::Switch {
            at, cases, default, ..
        } => {
            *at = place;
            bodies.extend(cases.iter_mut().map(|case| &mut case.body));
            bodies.extend(default.as_mut());
        }
    }
    for body in bodies {
        for statement in body {
            put(statement, place);
        }
    }
}

/// True when the type of every expression of `value` is known.
fn knows_types(value: &Expr) -> bool {
    let mut known = true;
    value.walk(&mut |expression| known &= knows(&expression.ty));
    known
}

/// Makes the type of every expression of `value` not known.
fn forget_types(value: &mut Expr) {
    value.walk_mut(&mut |expression| expression.ty = Type::Unknown);
}

/// True when nothing in `ty` is [`Type::Unknown`].
fn knows(ty: &Type) -> bool {
    let argument = |argument: &Argument| match argument {
        Argument::Type(ty) => knows(ty),
        Argument::Constant(_) => true,
    };
    match ty {
        Type::Unknown => false,
        Type::Named { arguments, .. } => arguments.iter().all(argument),
        Type::Tuple(elements) => elements.iter().all(knows),
        Type::List { element, .. } => knows(element),
        Type::Set(inner)
        | Type::Ref(inner)
        | Type::Endpoint(inner)
        | Type::Atomic(inner)
        | Type::Delta(inner)
        | Type::Trace { cell: inner, .. }
        | Type::Nullable(inner) => knows(inner),
        Type::Dict { key, value } => knows(key) && knows(value),
        Type::RollingWindow { value, .. } => knows(value),
        Type::Result { value, error } => knows(value) && knows(error),
        Type::Scalar(_)
        | Type::Parameter { .. }
        | Type::Signal
        | Type::Error
        | Type::ErrorContext => true,
    }
}
