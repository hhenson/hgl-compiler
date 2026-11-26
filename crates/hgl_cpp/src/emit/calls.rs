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
//! An inline computed spread supplies the arguments selected by the
//! analyser's passes (FN-19, CTRL-16). A skipped entry leaves the
//! ordinary argument-binding and default rules to apply.
//!
//! A native's generic arguments are its template's (`runtime`'s naming
//! scheme): each type, and each constant, an enum's member among them,
//! `div__f64_f64<::hglrt::ZeroPolicy::nan>`. A candidate of an operator
//! whose candidates are told apart by what they give (OPS-4: `convert`)
//! that takes the value alone is named by the type it is given, and is
//! a template whose first argument is the type it gives,
//! `convert_i64<::hgraph::Float>`, as the library names them.
//! The same binding serves a named, symbolic or lifted call (OPS-1,
//! NUM-8). The provider keeps zero policies (NUM-3, NUM-9, NUM-12),
//! tolerance (NUM-4, NUM-5, NUM-11), and full comparison (NUM-6).
//! The declared result type preserves ordinary checked-division data
//! and IEEE values (NUM-10, ERR-11). The conversion's result and
//! template arguments keep its selected policy (OPS-7, OPS-8).
//! Duration arithmetic (OPS-10) and tenor arithmetic, comparison,
//! movement primitives and text (TEN-1, TEN-2, TEN-3, TEN-4, TEN-9)
//! use the selected binding in the same way.
//! Core's `write` uses the ordinary native function binding, with its
//! stream and flush arguments (OUT-1, OUT-2).
//!
//! EXPR-2: the arguments are evaluated in the order written, and then
//! the defaults of those left out. C++ evaluates a call's arguments in
//! no order it promises, so where more than one of them may fail each
//! is a statement of its own, in that order, in a lambda called where
//! the call is written.

use hgl_analyser::hir::{Argument, Callee, Expr, ExprKind, Value};
use hgl_analyser::{
    Argument as GenericArgument, ConstantValue, DeclarationKind, Detail, OperatorRef, Program,
    Role, Signature, Type,
};
use hgl_diagnostics::Location;
use hgl_lower::calls::{DefaultValue, LeftOut, left_out, runtime_arguments};
use hgl_lower::copies::place_of;
use hgl_lower::instances::CallableLimit;

use super::Emitter;
use super::anonymous::{anonymous_arguments, anonymous_signature};
use super::expressions::integer_literal;
use super::names::{self, identifier};
use super::statements::Scope;
use super::types::enum_name;

/// The concrete signature and C++ name at a value call site.
struct ValueTarget<'a> {
    name: String,
    signature: &'a Signature,
    instance: Option<&'a hgl_lower::instances::Instance>,
    /// The declaration that owns any defaults (UNIT-8).
    callee: &'a Callee,
}

impl Emitter<'_> {
    /// Reports the missing instance at the call that asks for it.
    pub(super) fn missing_instance(
        &mut self,
        at: Location,
        callee: &Callee,
        arguments: &[Argument],
        otherwise: &str,
    ) -> String {
        if let Some(limit) = self
            .instances
            .callable_limit(self.program, callee, arguments)
            && let Some((module, declaration)) = callee.declaration()
        {
            let name = &self.program.modules[module].declarations[declaration].name;
            let reason = match limit {
                CallableLimit::Size => format!(
                    "a function argument to `{name}` whose specialization copies more than {} nodes",
                    crate::CALLABLE_LIMITS.size,
                ),
                CallableLimit::Depth => format!(
                    "a function argument to `{name}` with a specialized body deeper than {} nodes",
                    crate::CALLABLE_LIMITS.depth,
                ),
                CallableLimit::Growth => format!(
                    "a function argument to `{name}` with more than {} deeper specializations on one call chain",
                    crate::CALLABLE_LIMITS.growth,
                ),
            };
            return self.missing(at, &reason);
        }
        self.missing(at, otherwise)
    }

    /// A handler's contextual input or output payload is a checked read,
    /// even when the HIR represents it as a local (TIME-7, EXPR-2).
    pub(super) fn may_fail_here(&self, value: &Expr) -> bool {
        if may_fail(value) {
            return true;
        }
        let mut checked_read = false;
        if self.temporal.is_some() {
            value.walk(&mut |expression| {
                if let ExprKind::Local(local) = expression.kind {
                    checked_read |= self.role(local) == super::nodes::Role::Out
                        || self.input_reader(local).is_some();
                }
            });
        }
        checked_read
    }

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
        let text = match &whole.kind {
            ExprKind::Call { text, .. } => text.as_deref(),
            _ => None,
        };
        if let Some(renderer) = hgl_analyser::formatting::renderer(self.program, callee) {
            return self.format_call(scope, callee, renderer, arguments, text, whole);
        }
        if let Callee::Anonymous(function) = callee {
            if !function.template.value {
                return self.missing(at, "a call of a temporal function");
            }
            let signature = anonymous_signature(function);
            let Some(name) = self.anonymous_helper(function, &signature) else {
                return "0".to_string();
            };
            let Some(arguments) = anonymous_arguments(scope, function, arguments) else {
                return self.missing(
                    function.at,
                    "an anonymous function with no enclosing capture",
                );
            };
            return self.value_call(
                scope,
                &arguments,
                whole,
                statement,
                ValueTarget {
                    name,
                    signature: &signature,
                    instance: None,
                    callee,
                },
            );
        }
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
            Callee::Parameter { .. } => return self.missing(at, "an unresolved function argument"),
            Callee::Anonymous(_)
            | Callee::DeferredOperator(_)
            | Callee::DeferredPackCall
            | Callee::DeferredValue
            | Callee::Unread => {
                return self.missing(at, "this call");
            }
        };
        let declared = &program.modules[module].declarations[declaration];
        let Some(Detail::Callable(declared_signature)) =
            program.modules[module].details.get(declaration)
        else {
            return self.missing(at, "this call");
        };
        // What is called, and the signature its parameters are of: an
        // instance's has the call's generic arguments in place.
        let mut concrete = None;
        let (called, signature): (String, &Signature) = match declared.kind {
            DeclarationKind::Function(Role::Value)
            | DeclarationKind::Implementation(Role::Value, _) => {
                if !self.instances.instanced_call(program, callee) {
                    self.piece.reach.uses.insert((module, declaration, None));
                    (
                        names::function(program, module, declaration, None),
                        declared_signature,
                    )
                } else {
                    let instances = self.instances;
                    let Some(instance) = instances.find_call(program, callee, arguments) else {
                        return self.missing_instance(
                            at,
                            callee,
                            arguments,
                            "a call of an instance that is not written",
                        );
                    };
                    self.piece.reach.uses.insert((
                        module,
                        declaration,
                        Some(instance.suffix.clone()),
                    ));
                    concrete = Some(instance);
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
                let name = names::native(&name);
                // TIME-9: the adapter deduces its view types from the borrows.
                let name = if declared_signature
                    .parameters
                    .iter()
                    .any(|p| matches!(p.ty, Type::Endpoint(_)))
                {
                    name
                } else {
                    let Some(name) = self.templated(&name, &[], generics, at) else {
                        return "0".to_string();
                    };
                    name
                };
                (name, declared_signature)
            }
            _ => return self.missing(at, "a call of a temporal function"),
        };
        self.value_call(
            scope,
            arguments,
            whole,
            statement,
            ValueTarget {
                name: called,
                signature,
                instance: concrete,
                callee,
            },
        )
    }

    /// Writes ordinary argument passing once for named and anonymous
    /// targets, in the written evaluation order (EXPR-2).
    fn value_call(
        &mut self,
        scope: &mut Scope,
        arguments: &[Argument],
        whole: &Expr,
        statement: bool,
        ValueTarget {
            name: called,
            signature,
            instance: concrete,
            callee,
        }: ValueTarget<'_>,
    ) -> String {
        let at = whole.at;
        let program = self.program;
        // FN-15: a call that assigns places is written with what it
        // assigns where it stands, which only a statement has room for.
        // This also supplies the list place to core's `append` and
        // `extend` (LIST-1, LIST-2, LIST-3).
        let native_pack = hgl_lower::instances::NativePack::of(program, callee, arguments);
        let signature = native_pack
            .as_ref()
            .map_or(signature, |pack| &pack.signature);
        // TIME-8, CAP-6: every store operation checks its key as it is
        // evaluated, but core's recorder, which writes the run's own key.
        // The check is the call's own: it follows every argument, which is
        // evaluated once in the order written (EXPR-2).
        let store_key = program.store_key_argument(callee, arguments).filter(|_| {
            !self
                .temporal
                .as_ref()
                .is_some_and(|context| context.run_recorder)
        });
        let assigns = signature.parameters.iter().any(|parameter| parameter.var);
        if assigns && !statement {
            return self.missing(at, "a call that assigns a place, inside an expression");
        }
        // The places the call gives its `var` parameters: an argument
        // read from one of them is a copy.
        let parameter_for = |argument: &Argument| {
            if let Some(pack) = &native_pack {
                return pack.parameter(argument);
            }
            concrete.map_or_else(
                || argument.parameter.filter(|_| argument.pack.is_none()),
                |instance| instance.parameter(argument),
            )
        };
        let given_places: Vec<usize> = runtime_arguments(arguments)
            .filter(|(_, argument)| {
                parameter_for(argument)
                    .and_then(|index| signature.parameters.get(index))
                    .is_some_and(|parameter| parameter.var)
            })
            .filter_map(|(_, argument)| place_of(&scope.body.locals, &argument.value))
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
        // An argument that runs statements of its own may change what an
        // argument before it reads: then every argument is evaluated in
        // turn, and each that is no place is held as a value of its own,
        // what it read then.
        let changing = arguments.iter().any(|argument| changes(&argument.value));
        // Each parameter's argument: the one written for it, or what
        // `hgl_lower` says one left out is given (UNIT-8), a default read
        // with nothing in scope (UNIT-7). Each is the parameter's
        // position, what is written for it, whether it may fail, and how
        // it is held where it is evaluated before the call.
        let mut written: Vec<(usize, String, bool, Held)> = Vec::new();
        // The store operation's key, which the call checks as it starts.
        let mut checked_key = None;
        for (_, argument) in runtime_arguments(arguments) {
            let Some(index) =
                parameter_for(argument).filter(|&index| index < signature.parameters.len())
            else {
                return self.missing(argument.value.at, "an argument given to a pack");
            };
            let parameter = &signature.parameters[index];
            if let Type::Endpoint(payload) = &parameter.ty {
                if callee.declaration().is_some_and(|(module, declaration)| {
                    matches!(
                        program.modules[module].declarations[declaration].kind,
                        DeclarationKind::NativeFunction(_)
                    )
                }) {
                    let view = self.expression(scope, &argument.value);
                    written.push((
                        index,
                        view,
                        self.may_fail_here(&argument.value),
                        Held::Borrow,
                    ));
                    continue;
                }
                // A hidden FN-14 capture passes the selector itself,
                // without checking its position or reading its payload (ERR-14).
                let value = match argument.value.kind {
                    ExprKind::Local(local) => Some(identifier(&scope.body.locals[local].name)),
                    ExprKind::EndpointOf(local) => self.view(&scope.body.locals, local),
                    _ => None,
                };
                let Some(value) = value else {
                    return self.missing(argument.value.at, "this captured endpoint");
                };
                let Some(ty) = self.bound_child_type(payload, argument.value.at) else {
                    return "0".to_string();
                };
                written.push((index, value, false, Held::Value(ty)));
                continue;
            }
            if parameter.var {
                // A `list<bool>` is a `std::vector<bool>`, which holds its
                // elements as bits: an element is no `bool` a `bool&`
                // can refer to.
                if let ExprKind::Index { base, .. } = &argument.value.kind
                    && let Type::List { element, .. } = &base.ty
                    && **element == Type::Scalar(hgl_analyser::Scalar::Bool)
                {
                    return self.missing(
                        argument.value.at,
                        "an element of a `list<bool>` given to a `var` parameter",
                    );
                }
                // A place held before an argument that runs statements
                // could be one that argument replaces or grows, a list's
                // element gone: its indices are held in turn, and the
                // place is taken where the call is made
                // (`composites::held_place`).
                if changing {
                    let Some((steps, place)) = self.held_place(scope, &argument.value) else {
                        return self
                            .missing(argument.value.at, "this place given to a `var` parameter");
                    };
                    written.push((
                        index,
                        place,
                        self.may_fail_here(&argument.value),
                        Held::Steps(steps),
                    ));
                    continue;
                }
                let Some(place) = self.place(scope, &argument.value) else {
                    return self
                        .missing(argument.value.at, "this place given to a `var` parameter");
                };
                written.push((
                    index,
                    place,
                    self.may_fail_here(&argument.value),
                    Held::Place,
                ));
                continue;
            }
            let key = store_key.is_some_and(|key| std::ptr::eq(key, &argument.value));
            if key {
                checked_key = Some(index);
            }
            let value = self.expression(scope, &argument.value);
            // TYPE-10: the ordinary unsized parameter takes the fixed
            // list's entries. Read the array once, in argument order.
            if let (
                Type::List { size: None, .. },
                Type::List {
                    element,
                    size: Some(_),
                },
            ) = (&parameter.ty, &argument.value.ty)
            {
                // A generic native declaration can still name its type
                // parameter here. The fitted argument has its concrete element.
                let list_type = Type::List {
                    element: element.clone(),
                    size: None,
                };
                let Some(ty) = self.named_type(&list_type) else {
                    return self.missing(at, &format!("the type `{list_type}`"));
                };
                let list = format!(
                    "[](const auto& entries) {{ return {ty}(entries.begin(), entries.end()); }}({value})"
                );
                written.push((
                    index,
                    list,
                    self.may_fail_here(&argument.value),
                    Held::Value(ty),
                ));
                continue;
            }
            let Some(ty) = self.named_type(&argument.value.ty) else {
                return self.missing(at, &format!("the type `{}`", argument.value.ty));
            };
            let read = scope
                .copies
                .places(program, &scope.body.locals, &argument.value);
            let (value, held) = if read.iter().any(|place| given_places.contains(place)) {
                // A copy, which the call's change of the place leaves as
                // it was (FN-15).
                let Some(ty) = self.named_type(&parameter.ty) else {
                    return self.missing(at, &format!("the type `{}`", parameter.ty));
                };
                (format!("{ty}({value})"), Held::Value(ty))
            } else if self.refers(&argument.value) {
                (value, Held::Reference(ty))
            } else {
                (value, Held::Value(ty))
            };
            // The key counts as one that may fail, so that any other that
            // may is held first, in the order written, and the call's own
            // check comes after them all.
            written.push((
                index,
                value,
                key || self.may_fail_here(&argument.value),
                held,
            ));
        }
        let mut defaulted: Vec<(usize, String, bool, Held)> = Vec::new();
        for (index, parameter) in signature.parameters.iter().enumerate() {
            if matches!(parameter.ty, Type::Function(_))
                || written.iter().any(|(given, ..)| *given == index)
            {
                continue;
            }
            if parameter.pack.is_some() {
                return self.missing(at, "a call of a function with a pack");
            }
            let declared = native_pack.as_ref().map_or_else(
                || concrete.map_or(index, |instance| instance.original_parameter(index)),
                |pack| pack.original_parameter(index),
            );
            let default = left_out(program, callee, declared, parameter);
            match default {
                Some(LeftOut::Default(default)) => {
                    // A default that calls its own function with that
                    // same argument left out would be written inside
                    // itself without end.
                    let (module, declaration) = callee.declaration().expect("a named default");
                    let key = (module, declaration, declared);
                    if self.expanding.contains(&key) {
                        return self.missing(default.at, "a default that leaves itself out");
                    }
                    self.expanding.push(key);
                    let value = self.write_default(&default);
                    self.expanding.pop();
                    let Some(value) = value else {
                        return "0".to_string();
                    };
                    // EXPR-5, TYPE-14, UNIT-8: null takes the selected
                    // nullable parameter's type. Its generic source type
                    // need not have a concrete C++ name here.
                    let ty = if matches!(default.kind, ExprKind::Literal(Value::Null))
                        && matches!(parameter.ty, Type::Nullable(_))
                    {
                        &parameter.ty
                    } else {
                        &default.ty
                    };
                    let Some(ty) = self.named_type(ty) else {
                        return self.missing(default.at, &format!("the type `{ty}`"));
                    };
                    defaulted.push((index, value, may_fail(&default), Held::Value(ty)));
                }
                // UNIT-8: "A parameter whose type is written `T?` needs no
                // default to be omitted: it is taken to have a default of
                // `null`".
                Some(LeftOut::Null) => {
                    let Some(ty) = self.named_type(&parameter.ty) else {
                        return self.missing(at, &format!("the type `{}`", parameter.ty));
                    };
                    defaulted.push((index, "::std::nullopt".to_string(), false, Held::Value(ty)))
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
        let native_candidate = callee.declaration().is_some_and(|(module, declaration)| {
            matches!(
                program.modules[module].declarations[declaration].kind,
                DeclarationKind::NativeCandidate(..)
            )
        });
        let joined = |mut given: Vec<String>| {
            // The key, read or held, is checked where the operation is
            // called: after the arguments, before the operation runs.
            if let Some(index) = checked_key {
                given[index] = super::store::checked(&given[index]);
            }
            let argument = |(slot, parameter): (usize, &hgl_analyser::Parameter)| {
                (!matches!(parameter.ty, Type::Function(_))).then(|| given[slot].as_str())
            };
            let ordered: Vec<_> = if native_candidate {
                names::native_candidate_parameters(signature)
                    .filter_map(argument)
                    .collect()
            } else {
                signature
                    .parameters
                    .iter()
                    .enumerate()
                    .filter_map(argument)
                    .collect()
            };
            ordered.join(", ")
        };
        let ordered: Vec<(usize, String, bool, Held)> =
            written.into_iter().chain(defaulted).collect();
        if !changing && ordered.iter().filter(|(_, _, fails, _)| *fails).count() <= 1 {
            for (index, value, _, _) in ordered {
                given[index] = value;
            }
            return format!("{called}({})", joined(given));
        }
        // A place is held by a reference to it, and what something else
        // holds by a `const` reference; any other value as it is, so that
        // nothing it is part of is gone before the call: `[7][0]` is an
        // element of a list that ends with its statement. Each value is
        // held as its own type, never as `auto` of what was read: an
        // element of a `list<bool>` is read as the vector's reference to
        // its bit, which a later argument could change.
        let mut steps = String::new();
        for (index, value, _, held) in ordered {
            let declared = match held {
                Held::Steps(held) => {
                    // The place's indices, held in turn; the place itself
                    // at the call.
                    steps.push_str(&held);
                    given[index] = value;
                    continue;
                }
                Held::Borrow => "const auto&".to_string(),
                Held::Place => "auto&".to_string(),
                Held::Reference(ty) if !changing => format!("const {ty}&"),
                Held::Reference(ty) | Held::Value(ty) => format!("const {ty}"),
            };
            let name = names::temporary("argument", self.temporary());
            steps.push_str(&format!("{declared} {name} = {value}; "));
            given[index] = name;
        }
        format!("[&] {{ {steps}return {called}({}); }}()", joined(given))
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
        // ERR-6 and CAP-6: the native returns the whole result.
        // A recorded HIR read applies the form independently of the lookup.
        let name = match (name.as_str(), &signature.result) {
            ("get_GlobalState_str", Some(Type::Result { .. })) => "get_store_result",
            ("get_dict_K__T__K", Some(Type::Result { .. })) => "get_dict_result",
            (name, _) => name,
        };
        let types: Vec<String> = signature
            .parameters
            .iter()
            .map(|parameter| parameter.ty.to_string())
            .collect();
        let shown = format!("the candidate `{}({})`", declared.name, types.join(", "));
        if Some(module) != program.core() || !(self.provides)(name) {
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
            // OPS-4: the generic enum destination of `convert(i64)` is
            // already the first template argument. A fixed result is extra.
            if !matches!(gives, Type::Parameter { at, .. }
                if signature.generics.first().is_some_and(|generic| generic.at == *at))
            {
                let Some(gives) = self.named_type(gives) else {
                    self.unsupported(at, &format!("the type `{gives}`"));
                    return None;
                };
                first.push(gives);
            }
        }
        self.templated(&names::native(name), &first, generics, at)
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
                GenericArgument::Type(ty) => self.named_type(ty),
                GenericArgument::NoResult => None,
                GenericArgument::Constant(constant) => {
                    if let ConstantValue::Member(member) = &constant.value
                        && let Some(of) = &member.of
                    {
                        self.note_enum(of.declared.0, of.declared.1);
                    }
                    constant_value(self.program, &constant.value)
                }
            };
            match written {
                Some(written) => arguments.push(written),
                None => {
                    let shown = match argument {
                        GenericArgument::Type(ty) => format!("the type `{ty}`"),
                        GenericArgument::NoResult => {
                            "a generic argument with no result".to_string()
                        }
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
        callee.has_result(self.program) == Some(false)
    }
}

/// How an argument evaluated before its call (EXPR-2) is held until the
/// call is made, with the C++ type of the value it holds.
#[derive(Clone)]
enum Held {
    /// A native endpoint view, borrowed until return even if another argument writes.
    Borrow,
    /// A place given to a `var` parameter: a reference to it.
    Place,
    /// A place given to a `var` parameter beside an argument that runs
    /// statements: the steps that hold its indices, in order, and the
    /// place taken where the call is made.
    Steps(String),
    /// What a local, a constant or a part of one holds, which outlives
    /// the call: a `const` reference to it.
    Reference(String),
    /// Any other value: a value of its own.
    Value(String),
}

/// A constant, a generic value argument (GEN-8), as a C++ constant
/// expression: an integer, a `bool`, or an enum's member. `None` for one
/// not written yet.
fn constant_value(program: &Program, value: &ConstantValue) -> Option<String> {
    match value {
        ConstantValue::Int(number) => Some(integer_literal(*number)),
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

impl Emitter<'_> {
    /// Write the omitted parameter's expression in its own lexical scope
    /// (UNIT-7, UNIT-8), including the locals of a statement block.
    pub(super) fn write_default(&mut self, default: &DefaultValue<'_>) -> Option<String> {
        let body = default.body();
        let mut scope = Scope::cold(self.program, &body);
        // A default's local IDs do not index the caller's temporal readers.
        let temporal = self.temporal.take();
        let value = self.expression(&mut scope, default);
        self.temporal = temporal;
        if !scope.before.is_empty() {
            self.unsupported(default.at, "a default that propagates a failure");
            return None;
        }
        Some(value)
    }
}

/// True where evaluating `value` runs statements of its own, a block's
/// or a branch's, which may assign a local (EXPR-3).
pub(super) fn changes(value: &Expr) -> bool {
    let mut found = false;
    value.walk(&mut |inner| {
        found |=
            matches!(&inner.kind, ExprKind::Block { statements, .. } if !statements.is_empty());
    });
    found
}

/// True where evaluating `value` may fail or do anything at all: any
/// expression but a literal, a local, an enum's member, a field of one of
/// those, and a tuple, a list, a set or a dictionary made of those.
pub(super) fn may_fail(value: &Expr) -> bool {
    match &value.kind {
        // LEX-12: a literal that names a zone is checked by the run's
        // provider where it is evaluated.
        ExprKind::Literal(Value::Temporal(temporal)) => temporal.zone.is_some(),
        ExprKind::Literal(_) | ExprKind::Local(_) | ExprKind::EnumMember { .. } => false,
        ExprKind::Tuple(entries) | ExprKind::Sequence(entries) => entries.iter().any(may_fail),
        ExprKind::Pairs(entries) => entries
            .iter()
            .any(|(key, value)| may_fail(key) || may_fail(value)),
        // A field of a struct or a property of a delta is read as it is.
        ExprKind::Widen(inner) | ExprKind::Field { base: inner, .. } => may_fail(inner),
        _ => true,
    }
}
