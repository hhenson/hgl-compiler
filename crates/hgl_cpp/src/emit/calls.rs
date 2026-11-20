//! Writes a call of value code as C++: of a value function of the
//! program or an instance of one, of an HGL candidate of an operator,
//! and of a native candidate or a native function of core, which the
//! runtime library defines (NATIVE-7). Part of the transpiler, the last
//! stage.
//!
//! Each parameter is given its argument, or what `hgl_lower` says one
//! left out is given (UNIT-8): a default, read with nothing in scope
//! (UNIT-7), or `null`. A `var` parameter is given the place its argument
//! names (FN-15), and a call that assigns places stands as a statement of
//! its own. An argument read from a place that the same call gives a
//! `var` parameter is a copy: "The call changes the place and no value"
//! (FN-15, the copy rule).
//!
//! A native's generic arguments are its template's (`runtime`'s naming
//! scheme): each type, and each constant, an enum's member among them,
//! `div__f64_f64<::hglrt::ZeroPolicy::nan>`. A candidate of an operator
//! whose candidates are told apart by what they give (OPS-4: `convert`)
//! that takes the value alone is named by the type it is given, and is
//! a template whose first argument is the type it gives,
//! `convert_i64<::hgraph::Float>`, as the library names them.
//!
//! EXPR-2: the arguments are evaluated in the order written, and then
//! the defaults of those left out. C++ evaluates a call's arguments in
//! no order it promises, so where more than one of them may fail each
//! is a statement of its own, in that order, in a lambda called where
//! the call is written.

use hgl_analyser::hir::{Argument, Callee, Expr, ExprKind};
use hgl_analyser::{
    Argument as GenericArgument, ConstantValue, DeclarationKind, Detail, OperatorRef, Program,
    Role, Signature, Type,
};
use hgl_diagnostics::Location;
use hgl_lower::calls::{LeftOut, left_out};
use hgl_lower::copies::place_of;

use super::Emitter;
use super::names::{self, identifier};
use super::statements::Scope;
use super::types::{cpp_type, enum_name};
use hgl_analyser::hir::Body;

impl Emitter<'_> {
    /// A call of `callee` with `arguments` as C++ (FN-5, NATIVE-7);
    /// `whole` is the call.
    pub(super) fn call(
        &mut self,
        scope: &mut Scope,
        callee: &Callee,
        arguments: &[Argument],
        whole: &Expr,
    ) -> String {
        let at = whole.at;
        // True for the call that is the whole of a statement.
        let statement = std::mem::take(&mut scope.statement_call);
        let program = self.program;
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
                return self.missing(at, "an operation a generic declaration requires");
            }
            Callee::Unread => return self.missing(at, "this call"),
        };
        let declared = &program.modules[module].declarations[declaration];
        let Some(Detail::Callable(declared_signature)) =
            program.modules[module].details.get(declaration)
        else {
            return self.missing(at, "this call");
        };
        // What is called, and the signature its parameters are of: an
        // instance's has the call's generic arguments in place.
        let (called, signature): (String, &Signature) = match declared.kind {
            DeclarationKind::Function(Role::Value)
            | DeclarationKind::Implementation(Role::Value, _) => {
                if declared_signature.generics.is_empty() {
                    self.piece.reach.uses.insert((module, declaration, None));
                    (
                        names::function(program, module, declaration, None),
                        declared_signature,
                    )
                } else {
                    let instances = self.instances;
                    let Some(instance) = instances.find(program, (module, declaration), generics)
                    else {
                        return self.missing(at, "a call of an instance that is not written");
                    };
                    self.piece.reach.uses.insert((
                        module,
                        declaration,
                        Some(instance.suffix.clone()),
                    ));
                    (
                        names::function(program, module, declaration, Some(&instance.suffix)),
                        &instance.signature,
                    )
                }
            }
            DeclarationKind::NativeCandidate(Role::Value, operator) => {
                let Some(name) = self.native_candidate(
                    module,
                    declaration,
                    declared_signature,
                    operator,
                    generics,
                    at,
                ) else {
                    return "0".to_string();
                };
                (name, declared_signature)
            }
            DeclarationKind::NativeFunction(Role::Value) => {
                let name = names::native_function(&declared.name);
                if Some(module) != program.core() || !(self.provides)(&name) {
                    return self.missing(at, &format!("the native function `{}`", declared.name));
                }
                let Some(templated) = self.templated(&names::native(&name), &[], generics, at)
                else {
                    return "0".to_string();
                };
                (templated, declared_signature)
            }
            _ => return self.missing(at, "a call of a temporal function"),
        };
        // FN-15: a call that assigns places is written with what it
        // assigns where it stands, which only a statement has room for.
        let assigns = signature.parameters.iter().any(|parameter| parameter.var);
        if assigns && !statement {
            return self.missing(at, "a call that assigns a place, inside an expression");
        }
        // The places the call gives its `var` parameters: an argument
        // read from one of them is a copy.
        let given_places: Vec<usize> = arguments
            .iter()
            .filter(|argument| {
                argument
                    .parameter
                    .and_then(|index| signature.parameters.get(index))
                    .is_some_and(|parameter| parameter.var)
            })
            .filter_map(|argument| place_of(&scope.body.locals, &argument.value))
            .collect();
        // FN-15: "a call with several gives each place its value in the
        // order of the parameters"; one place given to two of them would
        // be one C++ object both change.
        let mut distinct = given_places.clone();
        distinct.sort_unstable();
        distinct.dedup();
        if distinct.len() != given_places.len() {
            return self.missing(at, "a call that gives one place to two `var` parameters");
        }
        // Each parameter's argument: the one written for it, or what
        // `hgl_lower` says one left out is given (UNIT-8), a default read
        // with nothing in scope (UNIT-7). Each is the parameter's
        // position, what is written for it, whether it may fail, and
        // whether it is a place.
        let mut written: Vec<(usize, String, bool, bool)> = Vec::new();
        for argument in arguments {
            let Some(index) = argument
                .parameter
                .filter(|&index| index < signature.parameters.len())
            else {
                return self.missing(argument.value.at, "an argument given to a pack");
            };
            let parameter = &signature.parameters[index];
            if parameter.var {
                let Some(place) = self.place(scope, &argument.value) else {
                    return self
                        .missing(argument.value.at, "this place given to a `var` parameter");
                };
                written.push((index, place, may_fail(&argument.value), true));
                continue;
            }
            let value = self.expression(scope, &argument.value);
            let read = scope
                .copies
                .places(program, &scope.body.locals, &argument.value);
            let value = if read.iter().any(|place| given_places.contains(place)) {
                // A copy, which the call's change of the place leaves as
                // it was (FN-15).
                let Some(ty) = cpp_type(program, &parameter.ty) else {
                    return self.missing(at, &format!("the type `{}`", parameter.ty));
                };
                format!("{ty}({value})")
            } else {
                value
            };
            written.push((index, value, may_fail(&argument.value), false));
        }
        let mut defaulted: Vec<(usize, String, bool, bool)> = Vec::new();
        for (index, parameter) in signature.parameters.iter().enumerate() {
            if written.iter().any(|(given, ..)| *given == index) {
                continue;
            }
            if parameter.pack.is_some() {
                return self.missing(at, "a call of a function with a pack");
            }
            match left_out(program, (module, declaration), index, parameter) {
                Some(LeftOut::Default(default)) => {
                    // A default that calls its own function with that
                    // same argument left out would be written inside
                    // itself without end.
                    let key = (module, declaration, index);
                    if self.expanding.contains(&key) {
                        return self.missing(default.at, "a default that leaves itself out");
                    }
                    let cold = Body {
                        locals: Vec::new(),
                        statements: Vec::new(),
                    };
                    let mut defaults = Scope::cold(program, &cold);
                    self.expanding.push(key);
                    let value = self.expression(&mut defaults, default);
                    self.expanding.pop();
                    if !defaults.before.is_empty() {
                        return self.missing(default.at, "a default that propagates a failure");
                    }
                    defaulted.push((index, value, may_fail(default), false));
                }
                // UNIT-8: "A parameter whose type is written `T?` needs no
                // default to be omitted: it is taken to have a default of
                // `null`".
                Some(LeftOut::Null) => {
                    defaulted.push((index, "std::nullopt".to_string(), false, false))
                }
                None => {
                    return self
                        .missing(at, &format!("a call that leaves out `{}`", parameter.name));
                }
            }
        }
        scope.acted = true;
        // EXPR-2: the arguments written, in the order written, and then
        // the defaults of those left out. Where more than one of them
        // may fail, C++, which evaluates a call's arguments in no order
        // it promises, is given each as a statement of its own, in that
        // order, before the call; a lambda called where the call is
        // gives them a block to be in.
        let mut given: Vec<String> = vec![String::new(); signature.parameters.len()];
        let ordered: Vec<(usize, String, bool, bool)> =
            written.into_iter().chain(defaulted).collect();
        if ordered.iter().filter(|(_, _, fails, _)| *fails).count() <= 1 {
            for (index, value, _, _) in ordered {
                given[index] = value;
            }
            return format!("{called}({})", given.join(", "));
        }
        let mut steps = String::new();
        for (index, value, _, place) in ordered {
            let held = names::temporary("argument", self.temporary());
            if place {
                steps.push_str(&format!("auto& {held} = {value}; "));
            } else {
                steps.push_str(&format!("const auto& {held} = {value}; "));
            }
            given[index] = held;
        }
        format!("[&] {{ {steps}return {called}({}); }}()", given.join(", "))
    }

    /// The runtime library's function for the native candidate
    /// `declaration` of `module`, a candidate of `operator`, called with
    /// `generics` (NATIVE-7): its name, with the arguments of its
    /// template. `None` where the library has none, or one of the
    /// arguments is not written yet, which is reported at `at`.
    fn native_candidate(
        &mut self,
        module: usize,
        declaration: usize,
        signature: &Signature,
        operator: Option<OperatorRef>,
        generics: &[GenericArgument],
        at: Location,
    ) -> Option<String> {
        let program = self.program;
        let declared = &program.modules[module].declarations[declaration];
        let name = names::native_candidate(&declared.name, signature);
        let types: Vec<String> = signature
            .parameters
            .iter()
            .map(|parameter| parameter.ty.to_string())
            .collect();
        let shown = format!("the candidate `{}({})`", declared.name, types.join(", "));
        if Some(module) != program.core() || !(self.provides)(&name) {
            self.unsupported(at, &shown);
            return None;
        }
        // OPS-4: candidates told apart by what they give take it first,
        // where they take the value alone.
        let mut first = Vec::new();
        let told_apart = operator.is_some_and(|operator| told_apart_by_result(program, operator));
        if told_apart && signature.parameters.len() == 1 {
            let Some(gives) = given(signature) else {
                self.unsupported(at, &shown);
                return None;
            };
            let Some(gives) = cpp_type(program, gives) else {
                self.unsupported(at, &format!("the type `{gives}`"));
                return None;
            };
            first.push(gives);
        }
        self.templated(&names::native(&name), &first, generics, at)
    }

    /// `name` with the arguments of its template: `first`, and then each
    /// of `generics`, a type or a constant. `None` where one is not
    /// written yet, which is reported at `at`.
    fn templated(
        &mut self,
        name: &str,
        first: &[String],
        generics: &[GenericArgument],
        at: Location,
    ) -> Option<String> {
        let mut arguments: Vec<String> = first.to_vec();
        for argument in generics {
            let written = match argument {
                GenericArgument::Type(ty) => cpp_type(self.program, ty),
                GenericArgument::Constant(constant) => {
                    constant_value(self.program, &constant.value)
                }
            };
            match written {
                Some(written) => arguments.push(written),
                None => {
                    let shown = match argument {
                        GenericArgument::Type(ty) => format!("the type `{ty}`"),
                        GenericArgument::Constant(constant) => {
                            format!("the generic argument `{}`", constant.text)
                        }
                    };
                    self.unsupported(at, &shown);
                    return None;
                }
            }
        }
        if arguments.is_empty() {
            Some(name.to_string())
        } else {
            Some(format!("{name}<{}>", arguments.join(", ")))
        }
    }
}

impl Emitter<'_> {
    /// True where `value` is a call of a function that gives nothing:
    /// it stands as a statement as it is.
    pub(super) fn gives_nothing(&self, value: &Expr) -> bool {
        let ExprKind::Call { callee, .. } = &value.kind else {
            return false;
        };
        callee.declaration().is_some_and(|(module, declaration)| {
            matches!(
                self.program.modules[module].details.get(declaration),
                Some(Detail::Callable(signature)) if signature.result.is_none()
            )
        })
    }
}

/// A constant, a generic value argument (GEN-8), as a C++ constant
/// expression: an integer, a `bool`, or an enum's member. `None` for one
/// not written yet.
fn constant_value(program: &Program, value: &ConstantValue) -> Option<String> {
    match value {
        ConstantValue::Int(i64::MIN) => Some("(-9223372036854775807 - 1)".to_string()),
        ConstantValue::Int(number) => Some(number.to_string()),
        ConstantValue::Bool(flag) => Some(flag.to_string()),
        ConstantValue::Member(member) => {
            let (module, declaration) = member.of.as_ref()?.declared;
            Some(format!(
                "{}::{}",
                enum_name(program, module, declaration),
                identifier(&member.name)
            ))
        }
        _ => None,
    }
}

/// What a candidate of `signature` gives, a `result`'s success type for
/// one that gives a `result`.
fn given(signature: &Signature) -> Option<&Type> {
    match signature.result.as_ref()? {
        Type::Result { value, .. } => Some(value),
        other => Some(other),
    }
}

/// True where core's native candidates of `operator` are told apart by
/// what they give: two of them take the same types and give different
/// ones, as `convert(value: i64) -> f64` and `convert(value: i64) ->
/// bool` do (OPS-4).
fn told_apart_by_result(program: &Program, operator: OperatorRef) -> bool {
    let Some(core) = program.core() else {
        return false;
    };
    let module = &program.modules[core];
    let mut read: Vec<(Vec<&Type>, Option<&Type>)> = Vec::new();
    for (position, declared) in module.declarations.iter().enumerate() {
        let DeclarationKind::NativeCandidate(_, Some(found)) = declared.kind else {
            continue;
        };
        if found != operator {
            continue;
        }
        let Some(Detail::Callable(signature)) = module.details.get(position) else {
            continue;
        };
        let takes: Vec<&Type> = signature
            .parameters
            .iter()
            .map(|parameter| &parameter.ty)
            .collect();
        let gives = given(signature);
        if read
            .iter()
            .any(|(other, result)| *other == takes && *result != gives)
        {
            return true;
        }
        read.push((takes, gives));
    }
    false
}

/// True where evaluating `value` may fail or do anything at all: any
/// expression but a literal, a local, an enum's member, and a tuple or a
/// list made of those.
fn may_fail(value: &Expr) -> bool {
    match &value.kind {
        ExprKind::Literal(_) | ExprKind::Local(_) | ExprKind::EnumMember { .. } => false,
        ExprKind::Tuple(entries) | ExprKind::Sequence(entries) => entries.iter().any(may_fail),
        ExprKind::Widen(inner) => may_fail(inner),
        _ => true,
    }
}
