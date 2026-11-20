//! Writes value functions and the statements and expressions of value
//! code, a named test's among them, as C++. Part of the transpiler, the
//! last stage.
//!
//! A `const fn` is a C++ function of the module's namespace. A call of
//! one is written qualified, and a call of a native candidate or a
//! native function of core is the runtime library's function
//! (NATIVE-7). Arguments are evaluated in the order they are written
//! (EXPR-2, EXPR-13), which C++ does not promise of a call's arguments:
//! where more than one of them may fail, they are evaluated one
//! statement at a time first.
//!
//! Written so far: parameters and `let`s of the scalars `types` writes,
//! `return`, `let`, `var` and its assignment, an expression statement,
//! literals, locals, `&&`, `||`, `!`, an `i64` widened to `f64`, and
//! calls. The rest is reported.

use hgl_analyser::hir::{Argument, Body, Callee, Expr, ExprKind, Stmt, UnaryOperator, Value};
use hgl_analyser::{DeclarationKind, Detail, LocalKind, Role, Signature};
use hgl_diagnostics::Location;
use hgl_lower::calls::{LeftOut, left_out};

use super::Emitter;
use super::names::{self, identifier};
use super::types::cpp_type;

/// One level of indentation in the generated code.
pub(super) const INDENT: &str = "    ";

impl Emitter<'_> {
    /// Writes value function `index` of the module (FN-1): its
    /// prototype for the header, and its definition.
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
        if !signature.generics.is_empty() {
            self.unsupported(at, "a value function with generic parameters");
            return;
        }
        let Some((prototype, head)) = self.head(&identifier(&declaration.name), signature, at)
        else {
            return;
        };
        let mut text = format!("{head}\n{{\n");
        for statement in &body.statements {
            self.statement(body, statement, 1, &mut text);
        }
        text.push_str("}\n");
        self.piece.declaration = format!(
            "// const fn {}{signature}\n{prototype};\n",
            declaration.name
        );
        self.piece.text = text;
    }

    /// A function's head: its result type, `name` and its parameters,
    /// as its prototype has it, and as its definition does, with each
    /// parameter `[[maybe_unused]]`, since a body need not read every
    /// one. `None` where one of the types, or a parameter's mode, is not
    /// written yet, which is reported at `at`.
    fn head(
        &mut self,
        name: &str,
        signature: &Signature,
        at: Location,
    ) -> Option<(String, String)> {
        let result = match &signature.result {
            Some(ty) => self.written_type(ty, at)?,
            None => "void",
        };
        let mut parameters = Vec::new();
        let mut defined = Vec::new();
        for parameter in &signature.parameters {
            if parameter.var {
                self.unsupported(parameter.at, "a `var` parameter");
                return None;
            }
            if parameter.pack.is_some() {
                self.unsupported(parameter.at, "a pack");
                return None;
            }
            let ty = self.written_type(&parameter.ty, parameter.at)?;
            let parameter = format!("{ty} {}", identifier(&parameter.name));
            defined.push(format!("[[maybe_unused]] {parameter}"));
            parameters.push(parameter);
        }
        Some((
            format!("{result} {name}({})", parameters.join(", ")),
            format!("{result} {name}({})", defined.join(", ")),
        ))
    }

    /// Writes a constant of the module (UNIT-13): none is written yet.
    pub(super) fn constant(&mut self, at: Location) {
        self.unsupported(at, "a module constant");
    }

    /// Writes `statement` of `body` into `text`, `depth` levels in.
    pub(super) fn statement(
        &mut self,
        body: &Body,
        statement: &Stmt,
        depth: usize,
        text: &mut String,
    ) {
        let indent = INDENT.repeat(depth);
        match statement {
            Stmt::Return { value: None, .. } => text.push_str(&format!("{indent}return;\n")),
            Stmt::Return {
                value: Some(value), ..
            } => {
                let value = self.expression(body, value);
                text.push_str(&format!("{indent}return {value};\n"));
            }
            // EXPR-3: `let` is immutable and `var` may be assigned.
            Stmt::Declare {
                local,
                value: Some(value),
            } => {
                let declared = &body.locals[*local];
                let Some(ty) = self.written_type(&declared.ty, declared.at) else {
                    return;
                };
                let constant = if declared.kind == LocalKind::Var {
                    ""
                } else {
                    "const "
                };
                let value = self.expression(body, value);
                text.push_str(&format!(
                    "{indent}[[maybe_unused]] {constant}{ty} {} = {value};\n",
                    identifier(&declared.name)
                ));
            }
            // A compound assignment's value is the symbol with the
            // local as its left operand.
            Stmt::Assign { local, value, .. } => {
                let value = self.expression(body, value);
                text.push_str(&format!(
                    "{indent}{} = {value};\n",
                    identifier(&body.locals[*local].name)
                ));
            }
            Stmt::Expr(value) => {
                let value = self.expression(body, value);
                text.push_str(&format!("{indent}static_cast<void>({value});\n"));
            }
            Stmt::Declare { local, value: None } => {
                self.unsupported(body.locals[*local].at, "a `var` declared with no value");
            }
            Stmt::AssignPlace { at, .. } => self.unsupported(*at, "an assignment to a place"),
            Stmt::Assert { at, .. } => self.unsupported(*at, "`assert` here"),
            Stmt::AssertRaises { at, .. } => self.unsupported(*at, "`assert raises`"),
            Stmt::For { at, .. } => self.unsupported(*at, "a `for` loop"),
            Stmt::While { at, .. } => self.unsupported(*at, "a `while` loop"),
            Stmt::Switch { at, .. } => self.unsupported(*at, "a `switch`"),
            Stmt::Yield { at, .. } => self.unsupported(*at, "a `yield`"),
            Stmt::Hook { at, .. } => self.unsupported(*at, "a hook"),
            Stmt::Inject { at, .. } => self.unsupported(*at, "an injected capability"),
            Stmt::Unread { at } => self.unsupported(*at, "this statement"),
        }
    }

    /// `value` as a C++ expression. A form that is not written yet is
    /// reported, and `0` stands in its place: the piece is left out.
    pub(super) fn expression(&mut self, body: &Body, value: &Expr) -> String {
        match &value.kind {
            ExprKind::Literal(literal) => self.literal(literal, value.at),
            ExprKind::Local(local) => identifier(&body.locals[*local].name),
            // EXPR-5, NUM-1: an `i64` taken as the `f64` of the same
            // number.
            ExprKind::Widen(inner) => {
                let inner = self.expression(body, inner);
                format!("static_cast<::hgraph::Float>({inner})")
            }
            // EXPR-6: `&&` and `||` short-circuit left to right, as
            // C++'s do.
            ExprKind::Binary {
                operator,
                left,
                right,
            } if matches!(
                operator,
                hgl_analyser::hir::BinaryOperator::And | hgl_analyser::hir::BinaryOperator::Or
            ) =>
            {
                let symbol = if *operator == hgl_analyser::hir::BinaryOperator::And {
                    "&&"
                } else {
                    "||"
                };
                let left = self.expression(body, left);
                let right = self.expression(body, right);
                format!("({left} {symbol} {right})")
            }
            ExprKind::Unary {
                operator: UnaryOperator::Not,
                operand,
            } => {
                let operand = self.expression(body, operand);
                format!("(!{operand})")
            }
            ExprKind::Call {
                callee, arguments, ..
            } => self.call(body, callee, arguments, value.at),
            ExprKind::Eval { .. } => self.missing(value.at, "an eval"),
            // TST-4: the harness's own comparison of two traces; what
            // either side lacks is reported first.
            ExprKind::TraceCompare { left, right, .. } => {
                self.expression(body, left);
                self.expression(body, right);
                self.missing(value.at, "a comparison of traces")
            }
            ExprKind::Trace(_) => self.missing(value.at, "a trace"),
            _ => self.missing(value.at, "this expression"),
        }
    }

    /// Reports `what` at `at` as not written yet, and gives what stands
    /// in its place.
    pub(super) fn missing(&mut self, at: Location, what: &str) -> String {
        self.unsupported(at, what);
        "0".to_string()
    }

    /// A literal as C++.
    fn literal(&mut self, literal: &Value, at: Location) -> String {
        match literal {
            // The least `i64` has no C++ literal: its negation is out
            // of range.
            Value::Int(i64::MIN) => "(-9223372036854775807 - 1)".to_string(),
            Value::Int(number) if *number < 0 => format!("({number})"),
            Value::Int(number) => number.to_string(),
            // `{:?}` writes the shortest text that reads back as the
            // same `f64`, always with a `.` or an exponent, as C++ reads
            // a double.
            Value::Float(number) if number.is_finite() => {
                let text = format!("{number:?}");
                if text.starts_with('-') {
                    format!("({text})")
                } else {
                    text
                }
            }
            Value::Bool(true) => "true".to_string(),
            Value::Bool(false) => "false".to_string(),
            _ => self.missing(at, "this literal"),
        }
    }

    /// A call of `callee` with `arguments` as C++ (FN-5, NATIVE-7).
    fn call(
        &mut self,
        body: &Body,
        callee: &Callee,
        arguments: &[Argument],
        at: Location,
    ) -> String {
        let program = self.program;
        let (module, declaration) = match callee {
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
            } => {
                if !generics.is_empty() {
                    return self.missing(at, "a call with generic arguments");
                }
                (*module, *declaration)
            }
            Callee::Required { .. } => {
                return self.missing(at, "an operation a generic declaration requires");
            }
            Callee::Unread => return self.missing(at, "this call"),
        };
        let declared = &program.modules[module].declarations[declaration];
        let Some(Detail::Callable(signature)) = program.modules[module].details.get(declaration)
        else {
            return self.missing(at, "this call");
        };
        let called = match declared.kind {
            DeclarationKind::Function(Role::Value) => {
                self.piece.reach.uses.insert((module, declaration, None));
                names::function(program, module, declaration)
            }
            DeclarationKind::NativeCandidate(Role::Value, operator) => {
                let name = names::native_candidate(&declared.name, signature);
                let types: Vec<String> = signature
                    .parameters
                    .iter()
                    .map(|parameter| parameter.ty.to_string())
                    .collect();
                let shown = format!("the candidate `{}({})`", declared.name, types.join(", "));
                // OPS-4: candidates of one operator that take the same
                // types and give different ones are told apart by what
                // they give, which the library's names do not say yet.
                let told_apart = operator.is_some_and(|operator| {
                    self.same_parameters_elsewhere(operator, module, declaration, signature)
                });
                if Some(module) != program.core() || told_apart || !(self.provides)(&name) {
                    return self.missing(at, &shown);
                }
                names::native(&name)
            }
            DeclarationKind::NativeFunction(Role::Value) => {
                let name = names::native_function(&declared.name);
                if Some(module) != program.core() || !(self.provides)(&name) {
                    return self.missing(at, &format!("the native function `{}`", declared.name));
                }
                names::native(&name)
            }
            DeclarationKind::Implementation(Role::Value, _) => {
                return self.missing(at, "a call of a candidate with a body");
            }
            _ => return self.missing(at, "a call of a temporal function"),
        };
        if !signature.generics.is_empty() {
            return self.missing(at, "a call of a function with generic parameters");
        }
        // Each parameter's argument: the one written for it, or what
        // `hgl_lower` says one left out is given (UNIT-8), a default
        // read with nothing in scope (UNIT-7). Each is the parameter's
        // position, what is written for it, and whether it may fail.
        let mut written: Vec<(usize, String, bool)> = Vec::new();
        for argument in arguments {
            let Some(index) = argument
                .parameter
                .filter(|&index| index < signature.parameters.len())
            else {
                return self.missing(argument.value.at, "an argument given to a pack");
            };
            let value = self.expression(body, &argument.value);
            written.push((index, value, may_fail(&argument.value)));
        }
        let mut defaulted: Vec<(usize, String, bool)> = Vec::new();
        for (index, parameter) in signature.parameters.iter().enumerate() {
            if written.iter().any(|(given, _, _)| *given == index) {
                continue;
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
                    self.expanding.push(key);
                    let value = self.expression(&cold, default);
                    self.expanding.pop();
                    defaulted.push((index, value, may_fail(default)));
                }
                // UNIT-8: a `T?` left out is null; no nullable type is
                // written yet.
                Some(LeftOut::Null) => {
                    return self.missing(at, &format!("the type `{}`", parameter.ty));
                }
                None => {
                    return self
                        .missing(at, &format!("a call that leaves out `{}`", parameter.name));
                }
            }
        }
        // EXPR-2: the arguments written, in the order written, and then
        // the defaults of those left out. Where more than one of them
        // may fail, C++, which evaluates a call's arguments in no order
        // it promises, is given each as a statement of its own, in that
        // order, before the call; an immediately called lambda gives
        // them a block to be in. Its names, `argument_0_`, are of a form
        // `identifier` gives no HGL name.
        let mut given: Vec<String> = vec![String::new(); signature.parameters.len()];
        let ordered: Vec<(usize, String, bool)> = written.into_iter().chain(defaulted).collect();
        if ordered.iter().filter(|(_, _, fails)| *fails).count() <= 1 {
            for (index, value, _) in ordered {
                given[index] = value;
            }
            return format!("{called}({})", given.join(", "));
        }
        let mut steps = String::new();
        for (index, value, _) in ordered {
            let Some(ty) = cpp_type(&signature.parameters[index].ty) else {
                return self.missing(
                    at,
                    &format!("the type `{}`", signature.parameters[index].ty),
                );
            };
            steps.push_str(&format!("const {ty} argument_{index}_ = {value}; "));
            given[index] = format!("argument_{index}_");
        }
        format!("[&] {{ {steps}return {called}({}); }}()", given.join(", "))
    }

    /// True where operator `operator` has a native candidate other than
    /// `declaration` of `module` whose parameters are of the types
    /// `signature`'s are: two such are told apart by their results
    /// (OPS-4), as `convert`'s are.
    fn same_parameters_elsewhere(
        &self,
        operator: hgl_analyser::OperatorRef,
        module: usize,
        declaration: usize,
        signature: &Signature,
    ) -> bool {
        let types = |signature: &Signature| -> Vec<String> {
            signature
                .parameters
                .iter()
                .map(|parameter| parameter.ty.to_string())
                .collect()
        };
        let wanted = types(signature);
        self.program
            .modules
            .iter()
            .enumerate()
            .any(|(index, candidate_module)| {
                candidate_module
                    .declarations
                    .iter()
                    .enumerate()
                    .any(|(other, candidate)| {
                        (index, other) != (module, declaration)
                            && candidate
                                .kind
                                .candidate()
                                .is_some_and(|(_, found)| found == Some(operator))
                            && matches!(
                                candidate_module.details.get(other),
                                Some(Detail::Callable(other)) if types(other) == wanted
                            )
                    })
            })
    }
}

/// True where evaluating `value` may fail or do anything at all: any
/// expression but a literal and a local.
fn may_fail(value: &Expr) -> bool {
    !matches!(value.kind, ExprKind::Literal(_) | ExprKind::Local(_))
}
