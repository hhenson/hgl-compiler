//! Writes value functions, their instances and module constants as C++.
//! Part of the transpiler, the last stage.
//!
//! A `const fn` (FN-1) is a C++ function of its module's namespace, with
//! its HGL name and where it is declared in a comment above it; an HGL
//! candidate of an operator is one too, an overload of the operator's
//! name in `c_` (GEN-4). A generic one is written once for each set of
//! generic arguments it is called with (GEN-2), as `hgl_lower`'s
//! instances say, each named by its suffix (`names::placed`).
//!
//! The copy rule is `hgl_lower::copies`: a `var` is a place, changed
//! where it is; a `let` and a parameter are references to a value
//! something else may hold, and a copy is made where one is kept or
//! where what it reads could change while it is in scope. So a parameter
//! is a `const` reference, a `var` parameter a reference to the place the
//! call gives (FN-15), and a value that is small and holds nothing on
//! the heap is passed and bound as it is (`types::by_value`): a copy of
//! it is the value itself.
//!
//! A module constant (UNIT-13: "The constant has that one value wherever
//! it is read") is a function that gives a reference to one value, made
//! the first time it is read.

use hgl_analyser::hir::{Body, ExprKind, Raise, walk_statements};
use hgl_analyser::{Detail, Signature, Type};
use hgl_diagnostics::Location;

use super::Emitter;
use super::names::{self, identifier};
use super::statements::Scope;
use super::types::by_value;
use crate::runtime;

/// One level of indentation in the generated code.
pub(super) const INDENT: &str = "    ";

impl Emitter<'_> {
    /// Writes value function or value candidate `index` of the module
    /// (FN-1, GEN-4): its prototype for the header and its definition,
    /// or, for one with generic parameters, a piece for each of its
    /// instances (GEN-2).
    pub(super) fn value_function(&mut self, index: usize) {
        let program = self.program;
        let declared = &program.modules[self.module];
        let declaration = &declared.declarations[index];
        let at = declaration.at;
        let (Some(Detail::Callable(signature)), Some(Some(body))) =
            (declared.details.get(index), declared.bodies.get(index))
        else {
            self.unsupported(at, "a value function with no body");
            return;
        };
        let instances = self.instances;
        for instance in instances.declared(self.module, index) {
            let described = format!(
                "{}{} ({}), the instance `{}`",
                declaration.name,
                signature,
                kind_of(declaration.kind),
                instance.suffix
            );
            if let Some(at) = instance.pack_use {
                self.unsupported(at, "a read of a pack as a whole");
            } else {
                self.function(
                    index,
                    Some(&instance.suffix),
                    &instance.signature,
                    &instance.body,
                    &described,
                );
            }
            let piece = self.close();
            self.instanced.push((index, instance.suffix.clone(), piece));
        }
        if instances.instanced(program, self.module, index) {
            return;
        }
        let described = format!(
            "{}{} ({})",
            declaration.name,
            signature,
            kind_of(declaration.kind)
        );
        self.function(index, None, signature, body, &described);
    }

    /// Writes into the piece the function `index` of the module, or its
    /// instance `instance`, of `signature` and `body`. `described` is
    /// what its comment calls it.
    fn function(
        &mut self,
        index: usize,
        instance: Option<&str>,
        signature: &Signature,
        body: &Body,
        described: &str,
    ) {
        let program = self.program;
        let declaration = &program.modules[self.module].declarations[index];
        let (namespace, name) = names::placed(program, self.module, index, instance);
        let Some((prototype, head)) = self.head(&name, signature, declaration.at) else {
            return;
        };
        // Depending on how the native parameters are passed, a debugger
        // may enter on the signature or on the opening brace.
        let mut text = self.debug_stamp(declaration.at, format!("{head}\n"));
        text.push_str(&self.debug_stamp(declaration.at, "{\n".to_string()));
        if self
            .value_body(body, signature.result.as_ref(), declaration.at, &mut text)
            .is_none()
        {
            return;
        }
        let end = declaration.syntax.text_range().end().into();
        text.push_str(&self.debug_end(declaration.at, end, "}\n".to_string()));
        let place = self.place_of(index);
        let comment = format!("// {described}, {place}\n");
        self.piece.declaration = within(namespace.as_deref(), &format!("{comment}{prototype};\n"));
        self.piece.text = within(namespace.as_deref(), &format!("{comment}{text}"));
    }

    /// Writes the statements of a value body, including its own ERR-5
    /// failure propagation. The caller has already written the head.
    pub(super) fn value_body(
        &mut self,
        body: &Body,
        result: Option<&Type>,
        at: Location,
        text: &mut String,
    ) -> Option<()> {
        let mut scope = Scope::cold(self.program, body);
        let mut propagates = false;
        walk_statements(&body.statements, &mut |expression| {
            propagates |= matches!(
                expression.kind,
                ExprKind::Raised {
                    raise: Raise::Return,
                    ..
                } | ExprKind::NullRead {
                    raise: Raise::Return,
                    ..
                }
            );
        });
        if propagates && let Some(Type::Result { error, .. }) = result {
            let error = self.written_type(error, at)?;
            let library = runtime::NAMESPACE;
            // ERR-5: only a contextual return becomes the function's
            // error. Execution failures still leave it by their own route.
            text.push_str(&format!("{INDENT}try\n{INDENT}{{\n"));
            self.statements(&mut scope, &body.statements, 2, text);
            text.push_str(&format!(
                "{INDENT}}}\n{INDENT}catch (const ::{library}::return_error<{error}>& failure_)\n{INDENT}{{\n{INDENT}{INDENT}return ::std::unexpected(failure_.value);\n{INDENT}}}\n"
            ));
        } else {
            self.statements(&mut scope, &body.statements, 1, text);
        }
        Some(())
    }

    /// Where declaration `index` of the module is written, for a
    /// comment above what it is written as: the module, the file's part
    /// name where it has one, and the line, counted from 1.
    pub(super) fn place_of(&self, index: usize) -> String {
        let declared = &self.program.modules[self.module];
        let declaration = &declared.declarations[index];
        let part = declared
            .files
            .get(declaration.file)
            .and_then(|file| file.part.as_deref());
        // The text of the whole file is the root's of its tree, and the
        // line is one more than the line breaks before the name.
        let root = declaration
            .syntax
            .ancestors()
            .last()
            .unwrap_or_else(|| declaration.syntax.clone());
        let text = root.text().to_string();
        let before = (declaration.at.span.start as usize).min(text.len());
        let line = text.as_bytes()[..before]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count()
            + 1;
        match part {
            Some(part) => format!("module {} part {part}, line {line}", declared.path),
            None => format!("module {}, line {line}", declared.path),
        }
    }

    /// A function's head: its result type, `name` and its parameters,
    /// as its prototype has it, and as its definition does, with each
    /// parameter `[[maybe_unused]]`, since a body need not read every
    /// one. A parameter is taken as `types::by_value` says, or as a
    /// `const` reference; a `var` one as a reference to its place
    /// (FN-15). `None` where one of the types is not written yet, which
    /// is reported at `at`.
    pub(super) fn head(
        &mut self,
        name: &str,
        signature: &Signature,
        at: Location,
    ) -> Option<(String, String)> {
        let result = match &signature.result {
            Some(ty) => self.written_type(ty, at)?,
            None => "void".to_string(),
        };
        let mut parameters = Vec::new();
        let mut defined = Vec::new();
        for parameter in &signature.parameters {
            let taken = match &parameter.ty {
                // FN-14, ERR-14: hidden captures hold the endpoint;
                // the callable decides when to read it.
                Type::Endpoint(payload) => self.bound_child_type(payload, parameter.at)?,
                ty => {
                    let written = self.written_type(ty, parameter.at)?;
                    parameter_type(self.program, ty, parameter.var, &written)
                }
            };
            let parameter = format!("{taken} {}", identifier(&parameter.name));
            defined.push(format!("[[maybe_unused]] {parameter}"));
            parameters.push(parameter);
        }
        Some((
            format!("{result} {name}({})", parameters.join(", ")),
            format!("{result} {name}({})", defined.join(", ")),
        ))
    }

    /// Writes constant `index` of the module (UNIT-13): a function that
    /// makes its value the first time it is read, and gives a reference
    /// to that one value each time.
    pub(super) fn constant(&mut self, index: usize) {
        let program = self.program;
        let declared = &program.modules[self.module];
        let declaration = &declared.declarations[index];
        let at = declaration.at;
        let (Some(Detail::Constant(ty)), Some(Some(value))) =
            (declared.details.get(index), declared.constants.get(index))
        else {
            self.unsupported(at, "a constant with no value");
            return;
        };
        let Some(written) = self.written_type(ty, at) else {
            return;
        };
        let (namespace, name) = names::placed(program, self.module, index, None);
        let body = value.clone().into_body();
        // UNIT-13, EXPR-5: the checked initializer has its own cold
        // locals. The ordinary block writer encloses their statements
        // in an expression, inside this constant's one static value.
        let mut scope = Scope::cold(program, &body);
        let value = self.expression(&mut scope, &value.value);
        if !scope.before.is_empty() {
            self.unsupported(at, "a constant whose value propagates a failure");
            return;
        }
        let place = self.place_of(index);
        let comment = format!("// const {}: {ty} (UNIT-13), {place}\n", declaration.name);
        // The HGL file and line of the value, which a failure of it names.
        let init = format!("{INDENT}static const {written} value = {value};\n");
        let init = match self.line_directive(at) {
            Some(directive) => super::stamped(&directive, &init),
            None => init,
        };
        self.piece.declaration = within(
            namespace.as_deref(),
            &format!("{comment}const {written}& {name}();\n"),
        );
        self.piece.text = within(
            namespace.as_deref(),
            &format!("{comment}const {written}& {name}()\n{{\n{init}{INDENT}return value;\n}}\n"),
        );
    }
}

/// How a parameter of type `ty`, written `written`, is taken: as it is
/// where it is small, by a `const` reference otherwise, and a `var` one
/// by a reference to its place (FN-15).
pub(super) fn parameter_type(
    program: &hgl_analyser::Program,
    ty: &Type,
    var: bool,
    written: &str,
) -> String {
    if var {
        format!("{written}&")
    } else if by_value(program, ty) {
        written.to_string()
    } else {
        format!("const {written}&")
    }
}

/// `text`, inside the namespace `namespace` where there is one, under
/// the module's own.
fn within(namespace: Option<&str>, text: &str) -> String {
    match namespace {
        Some(namespace) => {
            format!("namespace {namespace}\n{{\n{text}}}  // namespace {namespace}\n")
        }
        None => text.to_string(),
    }
}

/// What a declaration of `kind` is called in a comment.
fn kind_of(kind: hgl_analyser::DeclarationKind) -> &'static str {
    match kind {
        hgl_analyser::DeclarationKind::Implementation(..) => "candidate `impl const fn`",
        _ => "`const fn`",
    }
}
