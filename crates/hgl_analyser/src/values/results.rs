//! The result forms, `ok`, `err`, `is_ok`, `is_err` and `capture`, and the
//! parts of a result and of an error (ERR-2 to ERR-7). The form a result
//! takes from its place is `forms`'s.
//! Part of the typing pass ([`super`]).

use hgl_diagnostics::{Category, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};

use super::calls::arguments_of;
use super::fit::{not_null, nullable, open, whole};
use super::{BOOL, CapturedOutput, Fact, Typer};
use crate::hir::{Callee, ErrorPart, Expr, ExprKind, ValueCategory};
use crate::syntax::{child, name_token, tokens};
use crate::types::{Detail, Scalar, Type};
use crate::{codes, error};

/// ERR-4: the required message and the optional code of a new error.
pub(super) fn error_fields() -> [(Type, &'static str); 2] {
    [
        (Type::Scalar(Scalar::Str), "the message of an error"),
        (nullable(Type::Scalar(Scalar::Str)), "the code of an error"),
    ]
}

/// ERR-2: `ok` fits its value to the success type and `err` to the failure type.
pub(super) fn tagged_side<'a>(
    success: bool,
    value: &'a Type,
    failure: &'a Type,
) -> (&'a Type, &'static str) {
    if success {
        (value, "the type of a success")
    } else {
        (failure, "the type of a failure")
    }
}

impl<'a> Typer<'a> {
    // ---- Result forms (ERR-2 to ERR-5) ----

    /// The call of `name` as the result form it names, when it names
    /// one (ERR-13): `ok`, `err`, `is_ok`, `is_err` or `capture`.
    pub(super) fn result_form(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        expected: Option<&Type>,
    ) -> Option<(ExprKind, Type)> {
        Some(match name.text() {
            "ok" => self.tagged(true, name, list, expected),
            "err" => self.tagged(false, name, list, expected),
            "is_ok" => self.tag_test(true, name, list),
            "is_err" => self.tag_test(false, name, list),
            "capture" => {
                self.reject_configuration(name, list);
                self.capture(name, list)
            }
            _ => return None,
        })
    }

    /// The one argument a result form takes, "by position" (ERR-2,
    /// ERR-3). One with a name, a second, and none at all are each
    /// reported; any other than the first is read for what it reports
    /// itself.
    pub(super) fn one_argument(
        &mut self,
        form: &SyntaxToken,
        list: &SyntaxNode,
        rule: &'static str,
    ) -> Option<SyntaxNode> {
        self.reject_configuration(form, list);
        let mut arguments = arguments_of(list).into_iter();
        let Some((label, first)) = arguments.next() else {
            self.diagnostics.emit(error(
                Category::Type,
                codes::ARGUMENT_MISSING,
                Location::new(self.file, form.span()),
                format!("`{}` is given nothing: it takes one value", form.text()),
                rule,
            ));
            return None;
        };
        if let Some(label) = label {
            self.diagnostics.emit(error(
                Category::Name,
                codes::ARGUMENT_UNKNOWN,
                Location::new(self.file, label.span()),
                format!(
                    "`{}` takes its value by position, and `{}` names no parameter of it",
                    form.text(),
                    label.text()
                ),
                rule,
            ));
        }
        for (_, surplus) in arguments {
            self.diagnostics.emit(error(
                Category::Type,
                codes::ARGUMENT_UNEXPECTED,
                self.at(&surplus),
                format!("`{}` takes one value, and this is one more", form.text()),
                rule,
            ));
            self.expression(&surplus, Some(&Type::Unknown));
        }
        Some(first)
    }

    /// `ok(value)` or `err(value)` (ERR-2): "`ok(value)` constructs a
    /// success and `err(value)` a failure. Each takes one argument by
    /// position and no generic argument ... Both type arguments come
    /// from the complete `result<T,E>` its place expects: `value` is
    /// read as that T for `ok` and as that E for `err`. Where the place
    /// expects no complete result type the constructor rejects
    /// (`result.context_type`), and a payload that is not of its side's
    /// type rejects as any value does (EXPR-5)."
    fn tagged(
        &mut self,
        success: bool,
        name: &SyntaxToken,
        list: &SyntaxNode,
        expected: Option<&Type>,
    ) -> (ExprKind, Type) {
        let argument = self.one_argument(name, list, "ERR-2");
        // The result the place expects: of `result<T, E>?`, the result.
        let sides = match not_null(expected) {
            Some(result @ Type::Result { value, error }) if whole(result) => {
                Some((result.clone(), (**value).clone(), (**error).clone()))
            }
            _ => None,
        };
        let Some((result, value, failure)) = sides else {
            // A place whose type is not known has been reported, or is
            // for a later part: nothing is said of what it expects
            // (ERR-9).
            if !expected.is_some_and(|expected| !whole(expected) || open(expected)) {
                self.diagnostics.emit(
                    error(
                        Category::Type,
                        codes::RESULT_CONTEXT_TYPE,
                        Location::new(self.file, name.span()),
                        format!(
                            "`{}` takes both its types from the `result<T, E>` its place \
                             expects, and this place expects none",
                            name.text()
                        ),
                        "ERR-2",
                    )
                    .with_note("write the type: `let outcome: result<i64, error> = ok(1)`"),
                );
            }
            if let Some(argument) = argument {
                self.expression(&argument, Some(&Type::Unknown));
            }
            return (ExprKind::Unread, Type::Unknown);
        };
        let Some(argument) = argument else {
            return (ExprKind::Unread, result);
        };
        let (side, what) = tagged_side(success, &value, &failure);
        let payload = self.expression(&argument, Some(side));
        let payload = self.fit(payload, side, what);
        let kind = ExprKind::Tagged {
            success,
            payload: Box::new(payload),
        };
        (kind, result)
    }

    /// `is_ok(value)` or `is_err(value)` (ERR-3): they "take one
    /// `result<T,E>` by position and give a `bool`; any other type
    /// rejects."
    fn tag_test(
        &mut self,
        success: bool,
        name: &SyntaxToken,
        list: &SyntaxNode,
    ) -> (ExprKind, Type) {
        let Some(argument) = self.one_argument(name, list, "ERR-3") else {
            return (ExprKind::Unread, BOOL);
        };
        let value = self.expression(&argument, None);
        self.check_tag_test(success, &value);
        let kind = ExprKind::TagTest {
            success,
            value: Box::new(value),
        };
        (kind, BOOL)
    }

    /// ERR-3: a tag test takes a result itself, before any implicit read.
    pub(super) fn check_tag_test(&mut self, success: bool, value: &Expr) {
        if !matches!(value.ty, Type::Result { .. }) && whole(&value.ty) && !open(&value.ty) {
            let name = if success { "is_ok" } else { "is_err" };
            self.diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                value.at,
                format!(
                    "`{name}` tests a `result<T, E>`, and this is `{}`",
                    value.ty
                ),
                "ERR-3",
            ));
        }
    }

    /// `binding.value` or `binding.error` (ERR-3): "`binding.value` is
    /// the T and `binding.error` the E; `.value`/`.error` requires the
    /// matching fact, and without it, or on what is no such binding,
    /// rejects (`result.required_variant`)."
    pub(super) fn payload(
        &mut self,
        base: Expr,
        name: &str,
        at: Location,
        (value, failure): (Type, Type),
    ) -> (ExprKind, Type) {
        let success = match name {
            "value" => true,
            "error" => false,
            other => {
                self.diagnostics.emit(error(
                    Category::Name,
                    codes::MEMBER_UNKNOWN,
                    at,
                    format!(
                        "`{}` has no `{other}`: a result has `.value` and `.error`",
                        base.ty
                    ),
                    "ERR-3",
                ));
                return (ExprKind::Unread, Type::Unknown);
            }
        };
        let fact = match base.kind {
            ExprKind::Local(local) if success => Some(Fact::Success(local)),
            ExprKind::Local(local) => Some(Fact::Failure(local)),
            _ => None,
        };
        if !fact.is_some_and(|fact| self.present.contains(&fact)) {
            let (test, tag) = if success {
                ("is_ok", "a success")
            } else {
                ("is_err", "a failure")
            };
            let message = match &base.kind {
                ExprKind::Local(local) => format!(
                    "`.{}` is read where `{}` is {tag}, and nothing here proves it one",
                    name, self.locals[*local].name
                ),
                _ => format!(
                    "`.{}` is read of a binding a guard has proved {tag}, and this is no \
                     binding",
                    name
                ),
            };
            self.diagnostics.emit(
                error(
                    Category::Type,
                    codes::RESULT_REQUIRED_VARIANT,
                    at,
                    message,
                    "ERR-3",
                )
                .with_note(format!(
                    "bind the result with `let` and guard the read: `if {test}(outcome) {{ ... }}`"
                )),
            );
        }
        let ty = if success { value } else { failure };
        let kind = ExprKind::Payload {
            success,
            result: Box::new(base),
        };
        (kind, ty)
    }

    /// `.message`, `.code` or `.context` of an error (ERR-4):
    /// "`.message` is the `str` and `.code` the `str?`. `.context` is a
    /// read-only nullable record", an `error context?`.
    pub(super) fn error_part(&mut self, base: Expr, name: &str, at: Location) -> (ExprKind, Type) {
        let (part, ty) = match name {
            "message" => (ErrorPart::Message, Type::Scalar(Scalar::Str)),
            "code" => (ErrorPart::Code, nullable(Type::Scalar(Scalar::Str))),
            "context" => (ErrorPart::Context, nullable(Type::ErrorContext)),
            other => {
                self.diagnostics.emit(error(
                    Category::Name,
                    codes::MEMBER_UNKNOWN,
                    at,
                    format!(
                        "an `error` has no `{other}`: it has `.message`, `.code` and `.context`"
                    ),
                    "ERR-4",
                ));
                return (ExprKind::Unread, Type::Unknown);
            }
        };
        let kind = ExprKind::ErrorPart {
            base: Box::new(base),
            part,
        };
        (kind, ty)
    }

    /// `.node`, `.phase` or `.evaluation_time` of an error's context
    /// (ERR-4): "a read-only nullable record of `.node: str`, `.phase:
    /// str`, one of `"evaluate"`, `"start"`, and `"stop"`, and
    /// `.evaluation_time: datetime`". `base` has been proved present.
    pub(super) fn context_part(
        &mut self,
        base: Expr,
        name: &str,
        at: Location,
    ) -> (ExprKind, Type) {
        let (part, ty) = match name {
            "node" => (ErrorPart::Node, Type::Scalar(Scalar::Str)),
            "phase" => (ErrorPart::Phase, Type::Scalar(Scalar::Str)),
            "evaluation_time" => (ErrorPart::EvaluationTime, Type::Scalar(Scalar::Datetime)),
            other => {
                self.diagnostics.emit(error(
                    Category::Name,
                    codes::MEMBER_UNKNOWN,
                    at,
                    format!(
                        "an error's context has no `{other}`: it has `.node`, `.phase` and \
                         `.evaluation_time`"
                    ),
                    "ERR-4",
                ));
                return (ExprKind::Unread, Type::Unknown);
            }
        };
        let kind = ExprKind::ErrorPart {
            base: Box::new(base),
            part,
        };
        (kind, ty)
    }

    /// `error(message: text, code: optional_code)` (ERR-4): it "owns a
    /// required string message and optional string code, each given by
    /// its name: `message` is a `str`, and `code` a `str?` that is null
    /// when omitted."
    ///
    /// The other calls of a type's name, `str(value)`, `bytes(value)`
    /// and `any(value)`, are conversions, and are for a later part.
    pub(super) fn type_call(&mut self, node: &SyntaxNode) -> (ExprKind, Type) {
        let word = tokens(node).next();
        let list = child(node, SyntaxKind::ArgList);
        let (Some(word), Some(list)) = (word, list) else {
            return (ExprKind::Unread, Type::Unknown);
        };
        if word.kind() != SyntaxKind::KwError {
            return self.unread_call(&list);
        }
        let [(text, message_place), (optional, code_place)] = error_fields();
        let mut message: Option<Expr> = None;
        let mut code: Option<Expr> = None;
        for (label, value) in arguments_of(&list) {
            let Some(label) = label else {
                // "Each given by its name".
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::ARGUMENT_UNEXPECTED,
                    self.at(&value),
                    "`error` takes `message` and `code`, each by its name".to_string(),
                    "ERR-4",
                ));
                self.expression(&value, Some(&Type::Unknown));
                continue;
            };
            let (slot, ty, what) = match label.text() {
                "message" => (&mut message, &text, message_place),
                "code" => (&mut code, &optional, code_place),
                other => {
                    self.diagnostics.emit(error(
                        Category::Name,
                        codes::ARGUMENT_UNKNOWN,
                        Location::new(self.file, label.span()),
                        format!("`error` takes `message` and `code`, and no `{other}`"),
                        "ERR-4",
                    ));
                    self.expression(&value, Some(&Type::Unknown));
                    continue;
                }
            };
            if slot.is_some() {
                self.diagnostics.emit(error(
                    Category::Name,
                    codes::ARGUMENT_DUPLICATE,
                    Location::new(self.file, label.span()),
                    format!("`{}` is given twice", label.text()),
                    "ERR-4",
                ));
                self.expression(&value, Some(&Type::Unknown));
                continue;
            }
            // A second `&mut self` use: the slot is filled after the
            // value is read.
            let filled = label.text() == "message";
            let value = self.expression(&value, Some(ty));
            let value = self.fit(value, ty, what);
            if filled {
                message = Some(value);
            } else {
                code = Some(value);
            }
        }
        let Some(message) = message else {
            self.diagnostics.emit(error(
                Category::Type,
                codes::ARGUMENT_MISSING,
                Location::new(self.file, word.span()),
                "`error` is given no `message`: an error owns one".to_string(),
                "ERR-4",
            ));
            return (ExprKind::Unread, Type::Error);
        };
        let kind = ExprKind::Error {
            message: Box::new(message),
            code: code.map(Box::new),
        };
        (kind, Type::Error)
    }

    /// `capture(callable, args...)` (ERR-7): "In composition,
    /// `capture(callable, args...)` creates a temporal error boundary.
    /// It names a callable and gives it the arguments its own call
    /// takes, positional or named, read, checked, and selected as that
    /// call's are (EXPR-2, FN-4, GEN-5). In a value function or a hook
    /// it rejects (`capture.phase`) ... Capture of wiring-time scalar
    /// work rejects (`capture.phase`)."
    fn capture(&mut self, name: &SyntaxToken, list: &SyntaxNode) -> (ExprKind, Type) {
        // The primary location is the call: `capture` to its `)`.
        let at = list
            .parent()
            .map_or(Location::new(self.file, name.span()), |call| self.at(&call));
        if !self.graph {
            // TST-3: a test's statements outside an `eval` or a `run`
            // are value code.
            let place = if self.testing {
                "a test, outside an `eval` or a `run`"
            } else if self.value_code {
                "an application's statement"
            } else if self.hook.is_some() {
                "a hook"
            } else if self.generating {
                "a source"
            } else {
                "a value function"
            };
            self.diagnostics.emit(
                error(
                    Category::Phase,
                    codes::CAPTURE_PHASE,
                    at,
                    format!("`capture` is a boundary in composition, and this is {place}"),
                    "ERR-7",
                )
                .with_note("a graph captures a temporal call: `let seen = capture(node, input)`"),
            );
            return self.unread_call(list);
        }
        // "It names a callable": the first argument, by position.
        let first = arguments_of(list).into_iter().next();
        let Some((label, callable)) = first else {
            self.diagnostics.emit(error(
                Category::Type,
                codes::ARGUMENT_MISSING,
                at,
                "`capture` is given nothing: it names the callable it covers".to_string(),
                "ERR-7",
            ));
            return (ExprKind::Unread, Type::Unknown);
        };
        let named = (label.is_none() && callable.kind() == SyntaxKind::NameExpr)
            .then(|| name_token(&callable))
            .flatten();
        let Some(callee) = named else {
            self.diagnostics.emit(error(
                Category::Phase,
                codes::CAPTURE_PHASE,
                self.at(&callable),
                "`capture` names the callable it covers, and this is no name".to_string(),
                "ERR-7",
            ));
            return self.unread_call(list);
        };
        let Some(found) = self.declarations(&callee) else {
            // A local or a parameter is no callable. A name nothing
            // binds is left for the prelude, and is not judged.
            if self.local_of(&callable).is_some() {
                self.diagnostics.emit(error(
                    Category::Phase,
                    codes::CAPTURE_PHASE,
                    self.at(&callable),
                    format!(
                        "`capture` names the callable it covers, and `{}` is none",
                        callee.text()
                    ),
                    "ERR-7",
                ));
            }
            self.callable = Some(callable);
            let unread = self.unread_call(list);
            self.callable = None;
            return unread;
        };
        // The call it names, read as that call is: from the same list,
        // less the name.
        let outer = self.callable.replace(callable.clone());
        let (kind, ty) = self.call_named(&callee, list, found, None, None);
        self.callable = outer;
        // The call is held to what any call in a graph is: a `const`
        // parameter is given no connection (FN-3).
        self.constants_of(&kind);
        let call = Expr {
            kind,
            ty,
            at,
            category: ValueCategory::Unsettled,
        };
        // "It names a callable": a struct's name constructs a value,
        // and is no call that a boundary covers.
        if matches!(call.kind, ExprKind::Construct { .. }) {
            self.diagnostics.emit(error(
                Category::Phase,
                codes::CAPTURE_PHASE,
                self.at(&callable),
                format!(
                    "`capture` names the callable it covers, and `{}` is a struct: a \
                     construction is no temporal call",
                    callee.text()
                ),
                "ERR-7",
            ));
            return (ExprKind::Unread, Type::Unknown);
        }
        // "Capture of wiring-time scalar work rejects": a call that
        // composes nothing, all its arguments ordinary values.
        if matches!(call.kind, ExprKind::Call { .. })
            && self.category(&call) == ValueCategory::Value
        {
            self.diagnostics.emit(
                error(
                    Category::Phase,
                    codes::CAPTURE_PHASE,
                    at,
                    format!(
                        "`{}` is ordinary work here, done while the graph is built: there is \
                         no temporal call for `capture` to cover",
                        callee.text()
                    ),
                    "ERR-7",
                )
                .with_note("a value function is lifted, and may be captured, where it is given a connection"),
            );
        }
        (ExprKind::Capture(Box::new(call)), Type::Unknown)
    }

    /// What the `.value` of a capture of `call` is (ERR-7): "`.value`
    /// retains the normal output type; outputless calls expose only
    /// `.error`".
    pub(super) fn captured_output(&self, call: &Expr) -> CapturedOutput {
        let callee = match &call.kind {
            ExprKind::Call {
                callee:
                    Callee::Function {
                        module,
                        declaration,
                        ..
                    }
                    | Callee::Candidate {
                        module,
                        declaration,
                        ..
                    },
                ..
            } => self.program.modules[*module].details.get(*declaration),
            _ => None,
        };
        match callee {
            Some(Detail::Callable(signature)) if signature.result.is_none() => CapturedOutput::None,
            _ => CapturedOutput::Of(call.ty.clone()),
        }
    }

    /// `seen.value` or `seen.error` (ERR-7): "`.value` retains the
    /// normal output type; outputless calls expose only `.error`, a
    /// connection of `error`, and `.value` rejects there
    /// (`capture.output_field`)."
    pub(super) fn captured(
        &mut self,
        base: Expr,
        name: &SyntaxToken,
        output: CapturedOutput,
    ) -> (ExprKind, Type) {
        let at = Location::new(self.file, name.span());
        let (failure, ty) = match (name.text(), output) {
            ("error", _) => (true, Type::Error),
            ("value", CapturedOutput::Of(ty)) => (false, ty),
            ("value", CapturedOutput::None) => {
                self.diagnostics.emit(
                    error(
                        Category::Type,
                        codes::CAPTURE_OUTPUT_FIELD,
                        at,
                        "the call this capture covers has no output, and so no `.value`"
                            .to_string(),
                        "ERR-7",
                    )
                    .with_note("an outputless capture is read through `.error`"),
                );
                (false, Type::Unknown)
            }
            (other, _) => {
                self.diagnostics.emit(error(
                    Category::Name,
                    codes::MEMBER_UNKNOWN,
                    at,
                    format!("a capture has no `{other}`: it is read through `.value` and `.error`"),
                    "ERR-7",
                ));
                return (ExprKind::Unread, Type::Unknown);
            }
        };
        let kind = ExprKind::Captured {
            capture: Box::new(base),
            error: failure,
        };
        (kind, ty)
    }
}
