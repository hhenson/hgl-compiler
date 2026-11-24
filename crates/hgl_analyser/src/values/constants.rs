//! The values a body reads as compile-time constants: the value a module
//! constant is written with, the defaults of parameters and of fields, the
//! identity of an operator, and a constant written where a generic value
//! parameter takes one (UNIT-8, UNIT-13, GEN-8, GEN-9).
//! Part of the typing pass ([`super`]); what a constant
//! comes to is worked out by [`crate::constant`].

use std::cell::RefCell;

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};
use hgl_source::FileId;

use super::expressions::written_null;
use super::fit::{not_null, whole};
use super::inference::placed;
use super::{Tables, Typer, tables};
use crate::constant::{Fault, Scope, Typed};
use crate::declared::{names_parameter, substitute_within};
use crate::hir::{Body, Callee, DefaultExpr, Expr, ExprKind, Value, walk_statements_mut};
use crate::program::{Declaration, Program, Role};
use crate::read::{default_of, parameters};
use crate::syntax::{child, name_token, tokens};
use crate::types::{
    Argument, CallArguments, Constant, ConstantValue, Detail, Field, FieldRef, GenericKind,
    GenericParameter, Parameter, Signature, StructType, Type,
};
use crate::{codes, error};

impl<'a> Typer<'a> {
    /// GEN-9: the identity is read independently against every concrete
    /// domain named by its clause, allowing the usual EXPR-5 widening.
    pub(super) fn property_identities(mut self) {
        for clause in crate::laws::clauses(&self.declaration.syntax) {
            for property in clause
                .children()
                .filter(|part| part.kind() == SyntaxKind::OperatorProperty)
            {
                if !name_token(&property).is_some_and(|name| name.text() == "identity") {
                    continue;
                }
                let Some(identity) = property.children().next() else {
                    continue;
                };
                for domain in crate::laws::domains(&clause) {
                    let Some(&expected) = self.written.get(&(self.file, domain.span().start))
                    else {
                        continue;
                    };
                    if names_parameter(expected) || matches!(expected, Type::Unknown) {
                        continue;
                    }
                    // Keep only the diagnostics from each read.
                    let mark = self.mark();
                    let value = self.expression(&identity, Some(expected));
                    self.fit(value, expected, "the identity's domain");
                    self.restore_read(mark, false);
                }
            }
        }
    }

    /// The value a constant is written with, read as a value of
    /// `expected`, the type it is written with (UNIT-13): "Its
    /// expression is a const expression (EXPR-5) checked against T ...
    /// It reads no parameter, local, temporal value, or capability."
    /// The typer is made with no parameter, so none is in scope.
    pub(super) fn constant(mut self, expected: &Type) -> Option<DefaultExpr> {
        let syntax = self.declaration.syntax.clone();
        // The expression is what follows the type: the one child that
        // is no type.
        let ty = crate::read::type_child(&syntax);
        let written = syntax.children().find(|child| Some(child) != ty.as_ref())?;
        // A type that is no ordinary value type has been reported
        // (TYPE-12), and the expression is then held to none.
        let expected = if shaped(expected) {
            &Type::Unknown
        } else {
            expected
        };
        let value = self.expression(&written, Some(expected));
        let place = format!("the constant `{}`", self.declaration.name);
        let value = self.fit(value, expected, &place);
        Some(self.take_default(value))
    }

    /// The default of each parameter of the declaration (UNIT-8), read
    /// as a value of the parameter's type; `None` for a parameter with
    /// none.
    ///
    /// A default is cold and reads no formal argument (UNIT-7), so it
    /// is read apart from the body. A generic parameter's default is
    /// checked again when an omitted argument gives it a concrete type.
    pub(super) fn defaults(mut self) -> Vec<Option<DefaultExpr>> {
        self.locals.clear();
        self.declared.clear();
        self.assigned.clear();
        let signature = self.signature;
        let syntax = self.declaration.syntax.clone();
        signature
            .parameters
            .iter()
            .map(|parameter| {
                if !parameter.has_default {
                    return None;
                }
                let written = default_at(&syntax, parameter)?;
                let reported = self.diagnostics.len();
                // FN-1 has already rejected a temporal shape in a
                // value declaration or a `const` parameter.
                let invalid_shape = shaped(&parameter.ty)
                    && (parameter.constant || self.declaration.kind.role() == Some(Role::Value));
                let expected = if invalid_shape {
                    &Type::Unknown
                } else {
                    parameter_default_type(&parameter.ty, written_null(&written))
                };
                let expected = if names_parameter(expected) {
                    &Type::Unknown
                } else {
                    expected
                };
                let value = self.expression(&written, Some(expected));
                let place = format!("the parameter `{}`", parameter.name);
                // TYPE-12: "A `signal` input takes no default but
                // `null`": it has no payload a constant could supply.
                let signal = matches!(parameter.ty.omission_removed(), Type::Signal);
                if signal && !matches!(value.kind, ExprKind::Literal(Value::Null)) {
                    self.diagnostics.emit(error(
                        Category::Type,
                        codes::TYPE_MISMATCH,
                        value.at,
                        format!("{place} is a `signal`, which takes no default but `null`"),
                        "TYPE-12",
                    ));
                    return Some(self.take_default(value));
                }
                let value = self.fit_parameter_default(value, parameter, expected, &place);
                // ERR-9: reject a failure this reader can establish.
                // Calls the constant reader cannot evaluate are still
                // checked by the ordinary expression reader.
                if !invalid_shape
                    && self.diagnostics.len() == reported
                    && self
                        .typed_in(self.file, &written, Some(expected))
                        .is_ok_and(|constant| constant.evaluation_failed())
                {
                    self.report_failed_default(value.at, &parameter.name);
                }
                Some(self.take_default(value))
            })
            .collect()
    }

    // UNIT-8, ERR-9: the catalogue's proposed constant.failure row
    // covers parameter and field defaults as well as module constants.
    fn report_failed_default(&mut self, at: Location, name: &str) {
        self.diagnostics.emit(error(
            Category::Type,
            codes::CONSTANT_FAILURE,
            at,
            format!("evaluating the default of `{name}` fails"),
            "UNIT-8",
        ));
    }

    /// A window of nullable values also accepts the delta of an arrival,
    /// including a published null (TIME-7, TYPE-11).
    pub(super) fn fit_parameter_default(
        &mut self,
        value: Expr,
        parameter: &Parameter,
        expected: &Type,
        place: &str,
    ) -> Expr {
        let ty = parameter.ty.omission_removed();
        if let Type::RollingWindow { value: arrival, .. } = ty
            && matches!(arrival.as_ref(), Type::Nullable(_))
            && !self.admits(expected, &value.ty)
            && self.admits(&Type::delta(ty.clone()), &value.ty)
        {
            return value;
        }
        self.fit(value, expected, place)
    }

    /// The default of each field the struct writes, read as a value of
    /// the field's type; `None` for a field with none. `read` is the
    /// struct the typer was made for.
    ///
    /// TYPE-4: "A typed field introduction fixes its type and
    /// optionality ... An untyped inherited override changes only the
    /// construction default of a field a parent has." So `name = value`
    /// is read against the type the struct has that field with, the
    /// parent's arguments in place. UNIT-8: "Defaults remain
    /// required-constant/cold expressions checked against the expected
    /// type."
    ///
    /// A default is cold, as a parameter's is: it reads no field of the
    /// struct (UNIT-7, which the resolver holds it to). A field whose
    /// type names a generic parameter of the struct has a type only a
    /// construction gives, and its default is not held to one; neither
    /// is the default of an `atomic<...>` field, as a parameter's of
    /// that shape is not.
    pub(super) fn field_defaults(mut self, read: &StructType) -> Vec<Option<DefaultExpr>> {
        let syntax = self.declaration.syntax.clone();
        let own: Vec<Location> = read.generics.iter().map(|generic| generic.at).collect();
        read.fields
            .iter()
            .map(|field| {
                if !field.has_default {
                    return None;
                }
                let written = field_initializer(&syntax, field)?;
                // The type the struct has the field with: the one
                // written, or for `name = value` the parent's.
                let declared = match &field.ty {
                    Some(ty) => Some(ty),
                    None => read
                        .members
                        .iter()
                        .find(|member| member.name == field.name)
                        .map(|member| &member.ty),
                };
                let ty = declared.map_or(Type::Unknown, |ty| placed(ty.clone(), &own));
                // "Only an optional field's default may be null": a
                // `null` on any other field is reported as that
                // (`field.null_default`), and not again as no value of
                // the field's type.
                let judged = field.null_default && !matches!(ty, Type::Nullable(_));
                let invalid_shape = shaped(&ty);
                let expected = if invalid_shape || judged {
                    Type::Unknown
                } else {
                    ty
                };
                let reported = self.diagnostics.len();
                let value = self.expression(&written, Some(&expected));
                let place = format!("the field `{}`", field.name);
                let value = self.fit(value, &expected, &place);
                // UNIT-8, ERR-9: a field default is required constant,
                // with the same evaluation order as a parameter default.
                if !invalid_shape
                    && !judged
                    && self.diagnostics.len() == reported
                    && self
                        .typed_in(self.file, &written, Some(&expected))
                        .is_ok_and(|constant| constant.evaluation_failed())
                {
                    self.report_failed_default(value.at, &field.name);
                }
                Some(self.take_default(value))
            })
            .collect()
    }

    /// Keep local IDs relative to this default alone (UNIT-7).
    fn take_default(&mut self, value: Expr) -> DefaultExpr {
        self.declared.clear();
        self.assigned.clear();
        self.returned = false;
        DefaultExpr {
            value,
            locals: std::mem::take(&mut self.locals),
        }
    }

    /// What the compile-time constant `node`, written in `file`, reads
    /// as, with nothing yet held to its place's type
    /// (`crate::constant`). A generic value parameter of the declaration
    /// being read is held as that parameter, of its declared type.
    pub(super) fn typed_in(
        &self,
        file: FileId,
        node: &SyntaxNode,
        expected: Option<&Type>,
    ) -> Result<Typed, Fault> {
        let parameter = |at: Location| {
            self.signature
                .generics
                .iter()
                .find(|generic| generic.at == at)
                .and_then(|generic| match &generic.kind {
                    GenericKind::Value(ty) => Some(ty.clone()),
                    _ => None,
                })
        };
        self.typed_in_scope(file, node, expected, &parameter, &[])
    }

    fn typed_in_scope(
        &self,
        file: FileId,
        node: &SyntaxNode,
        expected: Option<&Type>,
        parameter: &dyn Fn(Location) -> Option<Type>,
        arguments: &[(Location, &Argument)],
    ) -> Result<Typed, Fault> {
        let binding = |file: FileId, offset: u32| self.binding_at(file, offset);
        let scope = Scope {
            program: self.program,
            file,
            binding: &binding,
            parameter,
            arguments,
            type_packs: &[],
            constants: &self.constants,
        };
        crate::constant::typed(node, expected, &scope)
    }

    /// UNIT-8: a generic argument can make an omitted default's evaluation
    /// fail, including an operand that a short-circuiting guard now reaches.
    pub(super) fn check_default_evaluation(
        &mut self,
        at: Location,
        parameter: &Parameter,
        arguments: &[Argument],
    ) {
        let Some(written) = self.node_at(at) else {
            return;
        };
        let generics = &self.signature.generics;
        let map: Vec<_> = generics
            .iter()
            .map(|generic| generic.at)
            .zip(arguments)
            .collect();
        let generic_type = |at: Location| {
            let generic = generics.iter().find(|generic| generic.at == at)?;
            match &generic.kind {
                GenericKind::Value(ty) => Some(substitute_within(ty, &map)),
                _ => None,
            }
        };
        let expected = parameter_default_type(&parameter.ty, written_null(&written));
        if self
            .typed_in_scope(at.file, &written, Some(expected), &generic_type, &map)
            .is_ok_and(|constant| constant.evaluation_failed())
        {
            self.report_failed_default(at, &parameter.name);
        }
    }

    /// What the compile-time constant `node`, written in `file`, comes
    /// to as a value of `expected`, the type its place expects (EXPR-5).
    pub(super) fn read_in(
        &self,
        file: FileId,
        node: &SyntaxNode,
        expected: Option<&Type>,
    ) -> Result<ConstantValue, Fault> {
        let expected = expected.filter(|expected| whole(expected));
        self.typed_in(file, node, expected)
            .and_then(|read| crate::constant::fit(read, expected, self.program))
    }

    /// Read a public operator default in its definition scope, with the
    /// types and values the reference gives its generic parameters (GEN-8).
    /// Each parameter keeps its own type while the candidate provides the
    /// expected type of the default expression (EXPR-5).
    pub(super) fn read_instance_default(
        &self,
        file: FileId,
        node: &SyntaxNode,
        expected: &Type,
        (generics, arguments): (&[GenericParameter], &[Argument]),
    ) -> Result<ConstantValue, Fault> {
        let map: Vec<_> = generics
            .iter()
            .map(|generic| generic.at)
            .zip(arguments)
            .collect();
        let parameter = |at: Location| {
            let generic = generics.iter().find(|generic| generic.at == at)?;
            match &generic.kind {
                GenericKind::Value(ty) => Some(substitute_within(ty, &map)),
                _ => None,
            }
        };
        let expected = whole(expected).then_some(expected);
        let read = self.typed_in_scope(file, node, expected, &parameter, &[])?;
        let value = crate::constant::fit(read, expected, self.program)?;
        Ok(value.substituted(
            &|at| match map.iter().find(|(declared, _)| *declared == at)?.1 {
                Argument::Constant(given) => Some(given.value.clone()),
                Argument::Type(_) | Argument::NoResult => None,
            },
        ))
    }

    /// [`Typer::read_in`], for a constant written in the file being read.
    pub(super) fn read_constant(
        &self,
        node: &SyntaxNode,
        expected: Option<&Type>,
    ) -> Result<ConstantValue, Fault> {
        self.read_in(self.file, node, expected)
    }

    /// The constant the default of `generic`, a generic value parameter
    /// of `declaration`, comes to (GEN-2), read as a value of the
    /// parameter's type where that is known, in the file the default is
    /// written in. A default that is not of that type is reported where
    /// it is declared (`signatures`).
    pub(super) fn default_constant(
        &self,
        declaration: &Declaration,
        generic: &GenericParameter,
    ) -> Option<Constant> {
        let default = generic_default(&declaration.syntax, generic)?;
        let file = declaration.at.file;
        let expected = match &generic.kind {
            GenericKind::Value(ty) if !names_parameter(ty) => Some(ty),
            _ => None,
        };
        let value = self
            .read_in(file, &default, expected)
            .unwrap_or(ConstantValue::Unknown);
        Some(Constant {
            text: constant_text(&default),
            value,
        })
    }

    /// Why the constant read as `read` is no compile-time constant this
    /// pass has the value of, for a report: GEN-8, "The value a call
    /// gives a generic value parameter is a compile-time constant ... Any
    /// other rejects." `None` where it is one, or where what is wrong has
    /// been reported.
    pub(super) fn not_constant(read: &Result<ConstantValue, Fault>) -> Option<&'static str> {
        match read {
            Err(Fault::NotConstant) => Some("the compiler does not know its value here"),
            Ok(value) if !crate::constant::worked_out(value) => {
                Some("this compiler does not work out its value")
            }
            _ => None,
        }
    }
}

impl Program {
    /// TYPE-5 and UNIT-7: read the default in the defining struct's
    /// lexical scope, with its generic arguments and the applied field type.
    /// Returns none when the applied default cannot be read as that type.
    pub fn specialize_field_default(
        &self,
        field: FieldRef,
        arguments: &[Argument],
        expected: &Type,
    ) -> Option<DefaultExpr> {
        let owner = self.modules.get(field.module)?;
        let declared = owner.declarations.get(field.declaration)?;
        let Detail::Struct(read) = owner.details.get(field.declaration)? else {
            return None;
        };
        let member = read.fields.get(field.field)?;
        let initialized = field_initializer(&declared.syntax, member)?;
        let map: Vec<_> = read
            .generics
            .iter()
            .map(|generic| generic.at)
            .zip(arguments)
            .collect();
        let signature = super::candidates::in_instance(&Signature::of_struct(read), &map);
        let (bindings, mut written, mut calls) = tables(owner);
        let span = initialized.span();
        let within =
            |(file, start)| file == member.at.file && (span.start..span.end).contains(&start);
        // A written type or call argument in the expression can refer to
        // the struct's generic parameters, just as its local values can.
        let types: Vec<_> = written
            .iter()
            .filter(|&(&at, _)| within(at))
            .map(|(&at, ty)| (at, substitute_within(ty, &map)))
            .collect();
        written.extend(types.iter().map(|(at, ty)| (*at, ty)));
        let applied_calls: Vec<_> = calls
            .iter()
            .filter(|&(&at, _)| within(at))
            .map(|(&at, call)| {
                (
                    at,
                    CallArguments {
                        at: call.at,
                        arguments: call
                            .arguments
                            .iter()
                            .map(|(at, argument)| (*at, crate::instance::placed(argument, &map)))
                            .collect(),
                    },
                )
            })
            .collect();
        calls.extend(applied_calls.iter().map(|(at, call)| (*at, call)));
        let selecting = RefCell::new(Vec::new());
        let mut diagnostics = Diagnostics::new();
        let defaults = Tables {
            bindings: &bindings,
            written: &written,
            calls: &calls,
            selecting: &selecting,
        };
        let mut typer = Typer::bare(
            self,
            owner,
            declared,
            &signature,
            defaults,
            &mut diagnostics,
        );
        typer.instance = map
            .iter()
            .map(|(at, argument)| (*at, (*argument).clone()))
            .collect();
        let value = typer.expression(&initialized, Some(expected));
        let value = typer.fit(value, expected, &format!("the field `{}`", member.name));
        let parameter = |at| {
            let generic = signature.generics.iter().find(|generic| generic.at == at)?;
            match &generic.kind {
                GenericKind::Value(ty) => Some(ty.clone()),
                _ => None,
            }
        };
        // UNIT-8 and ERR-9: an argument can make evaluation of the cold
        // default fail when the generic declaration alone could not tell.
        let failed = typer
            .typed_in_scope(
                member.at.file,
                &initialized,
                Some(expected),
                &parameter,
                &map,
            )
            .is_ok_and(|constant| constant.evaluation_failed());
        let mut body = typer.take_default(value).into_body();
        if failed || diagnostics.has_errors() {
            return None;
        }
        record_field_default_lambda_arguments(&mut body, arguments);
        super::record(self, &mut body, false);
        Some(DefaultExpr::from_body(body))
    }
}

/// GEN-13 and FN-17: the same lambda in a reread field default can hold
/// different values and types for different applications of its struct.
fn record_field_default_lambda_arguments(body: &mut Body, arguments: &[Argument]) {
    walk_statements_mut(&mut body.statements, &mut |expression| {
        let callee = match &mut expression.kind {
            ExprKind::Call { callee, .. } => callee,
            ExprKind::StaticCallable(callee) => callee.as_mut(),
            _ => return,
        };
        if let Callee::Anonymous(function) = callee {
            function
                .specialization
                .generics
                .extend_from_slice(arguments);
            record_field_default_lambda_arguments(&mut function.body, arguments);
        }
    });
}

/// The member that writes the initializer, identified by its first token.
fn field_initializer(declaration: &SyntaxNode, field: &Field) -> Option<SyntaxNode> {
    declaration
        .children()
        .filter(|member| member.kind() == SyntaxKind::StructMember)
        .find(|member| {
            tokens(member)
                .next()
                .is_some_and(|name| name.span().start == field.at.span.start)
        })
        .and_then(|member| default_of(&member))
}

/// The default a declaration writes for its generic value parameter
/// `generic` (GEN-2).
pub(crate) fn generic_default(
    declaration: &SyntaxNode,
    generic: &GenericParameter,
) -> Option<SyntaxNode> {
    let written = child(declaration, SyntaxKind::GenericParams)?
        .children()
        .filter(|parameter| parameter.kind() == SyntaxKind::GenericParam)
        .find(|parameter| {
            tokens(parameter)
                .take_while(|token| token.kind() != SyntaxKind::Colon)
                .any(|token| token.span().start == generic.at.span.start)
        })?;
    default_of(&written)
}

/// A constant as it is written, for messages: `Constant::text`.
pub(super) fn constant_text(node: &SyntaxNode) -> String {
    node.text().to_string().trim().to_string()
}

/// The default a declaration writes for `parameter`, one of its own.
pub(super) fn default_at(
    declaration: &SyntaxNode,
    parameter: &crate::types::Parameter,
) -> Option<SyntaxNode> {
    let written = parameters(declaration).into_iter().find(|written| {
        tokens(written)
            .take_while(|token| token.kind() != SyntaxKind::Colon)
            .any(|token| token.span().start == parameter.at.span.start)
    })?;
    default_of(&written)
}

/// UNIT-8: a temporal input's default is the ordinary value its
/// constant can publish. A window takes one arrival (TYPE-11); an
/// atomic takes its whole payload (TYPE-9). Written null marks omission.
pub(super) fn parameter_default_type(ty: &Type, null: bool) -> &Type {
    match ty.omission_removed() {
        Type::RollingWindow { value, .. } | Type::Atomic(value) if !null => value,
        Type::RollingWindow { .. } | Type::Atomic(_) | Type::Signal | Type::Endpoint(_) => {
            &Type::Unknown
        }
        _ => ty,
    }
}

/// True for a type that is a temporal shape and no ordinary value
/// type, under a `?` or not: a window, an atomic, a signal, an
/// endpoint. What a default of one is held to is not settled.
fn shaped(ty: &Type) -> bool {
    matches!(
        not_null(Some(ty)),
        Some(Type::RollingWindow { .. } | Type::Atomic(_) | Type::Signal | Type::Endpoint(_))
    )
}
