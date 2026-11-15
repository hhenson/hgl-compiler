//! Types the bodies of value functions.
//!
//! `read` says what each written type means and `declared` checks the
//! declarations against one another. This pass reads what a value
//! function, `const fn`, does: it gives every expression of the body a
//! type, checks each against the type its place requires, and writes the
//! body down as [`hir::Body`] for the stages after the analyser.
//!
//! It is built up form by form. This part reads:
//!
//! * literals, each with the type its form gives it (TYPE-1, TYPE-2,
//!   LEX-11);
//! * the names of parameters, locals and generic value parameters;
//! * `-`, `!` and the operators with two operands, by core's numeric
//!   contract (NUM-1, NUM-6);
//! * tuples, and sequences of values;
//! * `let`, `var`, assignment and `return` (EXPR-3, EXPR-5, FN-7).
//!
//! A form it does not read yet is left [`ExprKind::Unread`], with no
//! type and no report: calls, members, control flow. A type that is not
//! known is reported once, where it was found, and never again where it
//! is used (ERR-9).

use std::collections::HashMap;

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_lexer::literal::{
    decode_duration, decode_float, decode_integer, decode_string, decode_temporal,
};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};
use hgl_source::FileId;

use crate::declared::agree;
use crate::hir::{
    BinaryOperator, Body, Expr, ExprKind, Local, LocalId, Stmt, UnaryOperator, Value,
};
use crate::program::{Binding, Declaration, DeclarationKind, LocalKind, Module, Program, Role};
use crate::read::{child, is_type, tokens};
use crate::types::{Detail, GenericKind, Scalar, Signature, Type};
use crate::{codes, local_error};

/// Types the body of every value function of the program, filling in
/// [`Module::bodies`].
pub(crate) fn check(program: &mut Program, diagnostics: &mut Diagnostics) {
    for index in 0..program.modules.len() {
        let module = &program.modules[index];
        // What each name resolved to, by where it is used (UNIT-10).
        let bindings: HashMap<(FileId, u32), &Binding> = module
            .uses
            .iter()
            .map(|used| ((used.at.file, used.at.span.start), &used.binding))
            .collect();
        // What each written type means, by where it starts.
        let written: HashMap<(FileId, u32), &Type> = module
            .written
            .iter()
            .map(|written| ((written.at.file, written.at.span.start), &written.ty))
            .collect();
        let mut bodies = Vec::with_capacity(module.declarations.len());
        for (position, declaration) in module.declarations.iter().enumerate() {
            let body = match (&declaration.kind, module.details.get(position)) {
                (DeclarationKind::Function(Role::Value), Some(Detail::Callable(signature))) => {
                    Typer::new(
                        module,
                        declaration,
                        signature,
                        &bindings,
                        &written,
                        diagnostics,
                    )
                    .body()
                }
                _ => None,
            };
            bodies.push(body);
        }
        program.modules[index].bodies = bodies;
    }
}

/// Reads one value function.
struct Typer<'a> {
    /// The file the function is written in.
    file: FileId,
    declaration: &'a Declaration,
    signature: &'a Signature,
    bindings: &'a HashMap<(FileId, u32), &'a Binding>,
    written: &'a HashMap<(FileId, u32), &'a Type>,
    /// The parameters and locals found so far.
    locals: Vec<Local>,
    /// The local each declaration is, by where its name is declared.
    declared: HashMap<Location, LocalId>,
    /// For each local, whether every path to here has given it a value
    /// (EXPR-3).
    assigned: Vec<bool>,
    diagnostics: &'a mut Diagnostics,
}

impl<'a> Typer<'a> {
    fn new(
        module: &'a Module,
        declaration: &'a Declaration,
        signature: &'a Signature,
        bindings: &'a HashMap<(FileId, u32), &'a Binding>,
        written: &'a HashMap<(FileId, u32), &'a Type>,
        diagnostics: &'a mut Diagnostics,
    ) -> Typer<'a> {
        let mut typer = Typer {
            file: module.files[declaration.file].file,
            declaration,
            signature,
            bindings,
            written,
            locals: Vec::new(),
            declared: HashMap::new(),
            assigned: Vec::new(),
            diagnostics,
        };
        for parameter in &signature.parameters {
            // What a pack holds is for the pass that reads calls.
            let ty = match parameter.pack {
                Some(_) => Type::Unknown,
                None => parameter.ty.clone(),
            };
            typer.declare(
                &parameter.name,
                parameter.at,
                LocalKind::Parameter,
                ty,
                true,
            );
        }
        typer
    }

    fn at(&self, node: &SyntaxNode) -> Location {
        Location::new(self.file, node.span())
    }

    /// Adds a local. `assigned` says whether it has a value from the
    /// start.
    fn declare(
        &mut self,
        name: &str,
        at: Location,
        kind: LocalKind,
        ty: Type,
        assigned: bool,
    ) -> LocalId {
        let local = self.locals.len();
        self.locals.push(Local {
            name: name.to_string(),
            at,
            kind,
            ty,
        });
        self.assigned.push(assigned);
        self.declared.insert(at, local);
        local
    }

    /// The body, or `None` for a function with none written.
    fn body(mut self) -> Option<Body> {
        let syntax = &self.declaration.syntax;
        let statements = if let Some(block) = child(syntax, SyntaxKind::Block) {
            block
                .children()
                .map(|statement| self.statement(&statement))
                .collect()
        } else {
            // `=> value`: the value is what the function returns.
            let arrow = child(syntax, SyntaxKind::ArrowBody)?;
            let at = self.at(&arrow);
            let value = arrow.children().next().map(|value| self.returned(&value));
            vec![Stmt::Return { value, at }]
        };
        Some(Body {
            locals: self.locals,
            statements,
        })
    }

    // ---- Statements ----

    fn statement(&mut self, node: &SyntaxNode) -> Stmt {
        match node.kind() {
            SyntaxKind::LocalDecl => self.local(node),
            SyntaxKind::AssignStmt => self.assignment(node),
            SyntaxKind::ReturnStmt => {
                let value = node.children().next().map(|value| self.returned(&value));
                Stmt::Return {
                    value,
                    at: self.at(node),
                }
            }
            SyntaxKind::ExprStmt => match node.children().next() {
                Some(value) => Stmt::Expr(self.expression(&value, None)),
                None => Stmt::Unread { at: self.at(node) },
            },
            _ => self.unread(node),
        }
    }

    /// A statement this pass does not read yet.
    fn unread(&mut self, node: &SyntaxNode) -> Stmt {
        self.forget();
        Stmt::Unread { at: self.at(node) }
    }

    /// FN-7: "A value return yields an ordinary value." EXPR-5: the
    /// result type is the type expected of it.
    fn returned(&mut self, node: &SyntaxNode) -> Expr {
        let result = self.signature.result.clone();
        let value = self.expression(node, result.as_ref());
        match &result {
            Some(result) => self.fit(value, result, "the function's result"),
            None => value,
        }
    }

    /// EXPR-3: "Initialization fixes a binding's value/connection
    /// category and type; an annotation checks that category rather
    /// than changing it ... An uninitialized typed var supplies no value
    /// or connection."
    fn local(&mut self, node: &SyntaxNode) -> Stmt {
        let Some(name) = tokens(node).find(|token| token.kind() == SyntaxKind::Ident) else {
            return self.unread(node);
        };
        let kind = if tokens(node).any(|token| token.kind() == SyntaxKind::KwVar) {
            LocalKind::Var
        } else {
            LocalKind::Let
        };
        // The annotation, read by the pass before this one.
        let annotation = node
            .children()
            .find(|part| is_type(part.kind()))
            .and_then(|part| self.written.get(&(self.file, part.span().start)))
            .map(|ty| (*ty).clone());
        // The first value: the child that is no type.
        let value = node
            .children()
            .find(|part| !is_type(part.kind()))
            .map(|value| {
                let value = self.expression(&value, annotation.as_ref());
                match &annotation {
                    Some(annotation) => self.fit(value, annotation, "the type written for it"),
                    None => value,
                }
            });
        let ty = match (&annotation, &value) {
            (Some(annotation), _) => annotation.clone(),
            (None, Some(value)) => value.ty.clone(),
            (None, None) => Type::Unknown,
        };
        let at = Location::new(self.file, name.span());
        let local = self.declare(name.text(), at, kind, ty, value.is_some());
        Stmt::Declare { local, value }
    }

    /// EXPR-3: "`let` is immutable; `var` may be assigned within the
    /// same category/type ... A compound assignment reads its existing
    /// target."
    fn assignment(&mut self, node: &SyntaxNode) -> Stmt {
        let mut parts = node.children();
        let (Some(place), Some(value)) = (parts.next(), parts.next()) else {
            return self.unread(node);
        };
        // A place inside a value, `point.x = 1` or `row[0] = 1`, is for
        // the pass that reads members.
        let local = match place.kind() {
            SyntaxKind::NameExpr => self.local_of(&place),
            _ => None,
        };
        let Some(local) = local else {
            return self.unread(node);
        };
        let operator = tokens(node).find_map(|token| compound(token.kind()));
        let at = self.at(node);
        let target = self.locals[local].ty.clone();
        if !matches!(
            self.locals[local].kind,
            LocalKind::Var | LocalKind::State | LocalKind::Cache
        ) {
            self.diagnostics.emit(
                local_error(
                    Category::Name,
                    codes::local::BINDING_IMMUTABLE,
                    self.at(&place),
                    format!(
                        "`{}` is {} and cannot be assigned: only a `var` can",
                        self.locals[local].name,
                        self.locals[local].kind.describe()
                    ),
                    "EXPR-3",
                )
                .with_related(self.locals[local].at, "declared here"),
            );
            // The value is still read, for what it reports itself.
            let value = self.expression(&value, Some(&target));
            return Stmt::Assign {
                local,
                operator,
                value,
                at,
            };
        }
        let value = match operator {
            None => {
                let value = self.expression(&value, Some(&target));
                self.fit(value, &target, "the type of the `var`")
            }
            Some(operator) => {
                // The target is the left operand, and is read.
                let left = self.read_local(local, &place);
                let right = self.expression(&value, None);
                let result = self.binary_type(operator, &left, &right);
                self.conforms(&result, &target, self.at(&value), "the type of the `var`");
                right
            }
        };
        self.assigned[local] = true;
        Stmt::Assign {
            local,
            operator,
            value,
            at,
        }
    }

    // ---- Expressions ----

    /// The expression `node`, typed. `expected` is the type its place
    /// expects, which gives a literal or a sequence its type (EXPR-5);
    /// whether the result fits is for [`Typer::fit`].
    fn expression(&mut self, node: &SyntaxNode, expected: Option<&Type>) -> Expr {
        let at = self.at(node);
        let (kind, ty) = match node.kind() {
            SyntaxKind::Literal => self.literal(node, expected),
            SyntaxKind::NameExpr => return self.name(node),
            // "Parentheses select grouping" (EXPR-1): the expression
            // inside, where it stands.
            SyntaxKind::ParenExpr => match node.children().next() {
                Some(inner) => return self.expression(&inner, expected),
                None => (ExprKind::Unread, Type::Unknown),
            },
            SyntaxKind::TupleExpr => self.tuple(node, expected),
            SyntaxKind::SequenceExpr => self.sequence(node, expected),
            SyntaxKind::UnaryExpr => self.unary(node),
            SyntaxKind::BinaryExpr => self.binary(node),
            _ => (ExprKind::Unread, Type::Unknown),
        };
        if matches!(kind, ExprKind::Unread) {
            self.forget();
        }
        Expr { kind, ty, at }
    }

    /// What a form this pass does not read assigns is not followed, so
    /// every `var` may have a value after it.
    fn forget(&mut self) {
        self.assigned.fill(true);
    }

    /// A literal, with the type its form gives it. `null` has the type
    /// its place expects, which is a nullable one (TYPE-14).
    fn literal(&mut self, node: &SyntaxNode, expected: Option<&Type>) -> (ExprKind, Type) {
        let Some(token) = tokens(node).next() else {
            return (ExprKind::Unread, Type::Unknown);
        };
        let text = token.text();
        // A literal the lexer or `forms` has reported has no value
        // here, and no type: it is reported once (ERR-9).
        let read = match token.kind() {
            SyntaxKind::Int => decode_integer(text)
                .ok()
                .and_then(|value| i64::try_from(value).ok())
                .map(|value| (Value::Int(value), Scalar::I64)),
            SyntaxKind::Float => decode_float(text)
                .ok()
                .map(|value| (Value::Float(value), Scalar::F64)),
            SyntaxKind::Str => decode_string(text)
                .ok()
                .map(|value| (Value::Str(value), Scalar::Str)),
            SyntaxKind::KwTrue => Some((Value::Bool(true), Scalar::Bool)),
            SyntaxKind::KwFalse => Some((Value::Bool(false), Scalar::Bool)),
            SyntaxKind::Duration => decode_duration(text)
                .ok()
                .map(|value| (Value::Duration(value), Scalar::Duration)),
            SyntaxKind::Temporal => {
                temporal_type(text).map(|scalar| (Value::Temporal(text.to_string()), scalar))
            }
            SyntaxKind::KwNull => {
                return match expected {
                    // TYPE-14: "`T?` denotes an ordinary value that is
                    // either a T or null."
                    Some(expected @ Type::Nullable(_)) => {
                        (ExprKind::Literal(Value::Null), expected.clone())
                    }
                    // "It does not make null an admitted value of
                    // unqualified T."
                    Some(expected) if !open(expected) => {
                        self.diagnostics.emit(local_error(
                            Category::Type,
                            codes::local::TYPE_MISMATCH,
                            self.at(node),
                            format!("`null` is no value of `{expected}`: write `{expected}?`"),
                            "TYPE-14",
                        ));
                        (ExprKind::Literal(Value::Null), Type::Unknown)
                    }
                    // Nothing says which nullable type it is.
                    _ => (ExprKind::Literal(Value::Null), Type::Unknown),
                };
            }
            _ => None,
        };
        match read {
            Some((value, scalar)) => (ExprKind::Literal(value), Type::Scalar(scalar)),
            None => (ExprKind::Unread, Type::Unknown),
        }
    }

    /// The local a name is, when it names a parameter or a local this
    /// pass has read.
    fn local_of(&self, node: &SyntaxNode) -> Option<LocalId> {
        let name = name_token(node)?;
        match self.bindings.get(&(self.file, name.span().start))? {
            Binding::Local { at, .. } => self.declared.get(at).copied(),
            Binding::Declarations { .. } => None,
        }
    }

    /// A name: a parameter, a local, or a generic value parameter. Any
    /// other, a function or a type or a name left for the prelude, is
    /// for the pass that reads calls and members.
    fn name(&mut self, node: &SyntaxNode) -> Expr {
        let at = self.at(node);
        let unread = Expr {
            kind: ExprKind::Unread,
            ty: Type::Unknown,
            at,
        };
        if let Some(local) = self.local_of(node) {
            return self.read_local(local, node);
        }
        let Some(name) = name_token(node) else {
            return unread;
        };
        let Some(Binding::Local {
            at: declared,
            kind: LocalKind::GenericParameter,
        }) = self.bindings.get(&(self.file, name.span().start))
        else {
            return unread;
        };
        // GEN-13: a generic parameter that is a value has the type it
        // is declared with. One that is a type is no expression.
        let generic = self
            .signature
            .generics
            .iter()
            .find(|generic| generic.at == *declared);
        match generic.map(|generic| &generic.kind) {
            Some(GenericKind::Value(ty)) => Expr {
                kind: ExprKind::GenericValue {
                    name: name.text().to_string(),
                    at: *declared,
                },
                ty: ty.clone(),
                at,
            },
            _ => unread,
        }
    }

    /// A read of the local `local`, written at `node`. EXPR-3: "every
    /// path reaching a read must assign it."
    fn read_local(&mut self, local: LocalId, node: &SyntaxNode) -> Expr {
        let at = self.at(node);
        if !self.assigned[local] {
            self.diagnostics.emit(
                local_error(
                    Category::Name,
                    codes::local::BINDING_UNASSIGNED,
                    at,
                    format!(
                        "`{}` is read before it is given a value",
                        self.locals[local].name
                    ),
                    "EXPR-3",
                )
                .with_related(self.locals[local].at, "declared here"),
            );
            // One report for one `var`.
            self.assigned[local] = true;
        }
        Expr {
            kind: ExprKind::Local(local),
            ty: self.locals[local].ty.clone(),
            at,
        }
    }

    /// `(a, b)`. An expected tuple of as many elements gives each its
    /// expected type.
    fn tuple(&mut self, node: &SyntaxNode, expected: Option<&Type>) -> (ExprKind, Type) {
        let items: Vec<SyntaxNode> = node.children().collect();
        let expected = match not_null(expected) {
            Some(Type::Tuple(expected)) if expected.len() == items.len() => Some(expected),
            _ => None,
        };
        let elements: Vec<Expr> = items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let expected = expected.map(|expected| &expected[index]);
                let element = self.expression(item, expected);
                match expected {
                    Some(expected) => self.fit(element, expected, "this element of the tuple"),
                    None => element,
                }
            })
            .collect();
        // An element that does not fit has been reported: the tuple is
        // of the type expected, and is not reported again.
        let ty = match expected {
            Some(expected) => Type::Tuple(expected.clone()),
            None => Type::Tuple(elements.iter().map(|element| element.ty.clone()).collect()),
        };
        (ExprKind::Tuple(elements), ty)
    }

    /// `[a, b]`: a sequence of values (EXPR-9). "The use site and
    /// expected type constrain element types": an expected list or set
    /// gives the elements their type. With none, the values are a list
    /// of the one type they have, `f64` when some are `i64` and some
    /// `f64` (NUM-1).
    ///
    /// Pairs and `_` are for the passes that read dictionaries and test
    /// traces.
    fn sequence(&mut self, node: &SyntaxNode, expected: Option<&Type>) -> (ExprKind, Type) {
        let items: Vec<SyntaxNode> = node.children().collect();
        if items
            .iter()
            .any(|item| matches!(item.kind(), SyntaxKind::SequencePair | SyntaxKind::Hole))
        {
            return (ExprKind::Unread, Type::Unknown);
        }
        // A sequence is no `null`: of a `set<i64>?` expected, the set.
        let expected = not_null(expected);
        let element = match expected {
            Some(Type::List { element, .. }) | Some(Type::Set(element)) => Some(&**element),
            _ => None,
        };
        if let (Some(element), Some(expected)) = (element, expected) {
            let elements = items
                .iter()
                .map(|item| {
                    let value = self.expression(item, Some(element));
                    self.fit(value, element, "an element")
                })
                .collect();
            return (ExprKind::Sequence(elements), expected.clone());
        }
        let mut elements: Vec<Expr> = items
            .iter()
            .map(|item| self.expression(item, None))
            .collect();
        // Nothing says what an empty sequence holds.
        let Some(first) = elements.first().map(|first| first.ty.clone()) else {
            return (ExprKind::Sequence(elements), Type::Unknown);
        };
        let numbers = |ty: &Type| matches!(ty, Type::Scalar(Scalar::I64 | Scalar::F64));
        let of = if elements.iter().all(|element| numbers(&element.ty))
            && elements
                .iter()
                .any(|element| element.ty == Type::Scalar(Scalar::F64))
        {
            Type::Scalar(Scalar::F64)
        } else {
            first
        };
        elements = elements
            .into_iter()
            .map(|element| self.fit(element, &of, "an element, as the first is"))
            .collect();
        let ty = Type::List {
            element: Box::new(of),
            size: None,
        };
        (ExprKind::Sequence(elements), ty)
    }

    /// `-value`, `!value`. NUM-1: "Unary negation preserves the numeric
    /// type." `!` is of a bool (EXPR-6).
    fn unary(&mut self, node: &SyntaxNode) -> (ExprKind, Type) {
        // An operator given configuration, `-{...} x`, is for the pass
        // that reads configuration.
        if child(node, SyntaxKind::OperatorConfig).is_some() {
            return (ExprKind::Unread, Type::Unknown);
        }
        let (Some(token), Some(operand)) = (tokens(node).next(), node.children().next()) else {
            return (ExprKind::Unread, Type::Unknown);
        };
        match token.kind() {
            SyntaxKind::Bang => {
                let operand = self.expression(&operand, Some(&BOOL));
                let operand = self.fit(operand, &BOOL, "the operand of `!`");
                let kind = ExprKind::Unary {
                    operator: UnaryOperator::Not,
                    operand: Box::new(operand),
                };
                (kind, BOOL)
            }
            SyntaxKind::Minus => {
                // LEX-7: 2^63 is an integer only as the operand of a
                // minus, where the two are the least `i64`.
                if is_least_magnitude(&operand) {
                    return (
                        ExprKind::Literal(Value::Int(i64::MIN)),
                        Type::Scalar(Scalar::I64),
                    );
                }
                let operand = self.expression(&operand, None);
                // What `-` gives for a value that is no number is for
                // core to say.
                let ty = match &operand.ty {
                    Type::Scalar(Scalar::I64 | Scalar::F64) => operand.ty.clone(),
                    _ => Type::Unknown,
                };
                let kind = ExprKind::Unary {
                    operator: UnaryOperator::Negate,
                    operand: Box::new(operand),
                };
                (kind, ty)
            }
            _ => (ExprKind::Unread, Type::Unknown),
        }
    }

    /// `left + right`, and each other operator with two operands.
    fn binary(&mut self, node: &SyntaxNode) -> (ExprKind, Type) {
        // `a /{zero: .error} b` is for the pass that reads
        // configuration: it may select the result type (EXPR-12).
        if child(node, SyntaxKind::OperatorConfig).is_some() {
            return (ExprKind::Unread, Type::Unknown);
        }
        let mut operands = node.children();
        let operator = tokens(node).find_map(|token| binary_operator(token.kind()));
        let (Some(left), Some(right), Some(operator)) =
            (operands.next(), operands.next(), operator)
        else {
            return (ExprKind::Unread, Type::Unknown);
        };
        let (left, right) = match operator {
            // EXPR-6: "Boolean connectives short-circuit left to right."
            // Their operands are bools.
            BinaryOperator::And | BinaryOperator::Or => {
                let what = format!("an operand of `{}`", operator.symbol());
                let left = self.expression(&left, Some(&BOOL));
                let left = self.fit(left, &BOOL, &what);
                let right = self.expression(&right, Some(&BOOL));
                (left, self.fit(right, &BOOL, &what))
            }
            _ => (self.expression(&left, None), self.expression(&right, None)),
        };
        let ty = self.binary_type(operator, &left, &right);
        let kind = ExprKind::Binary {
            operator,
            left: Box::new(left),
            right: Box::new(right),
        };
        (kind, ty)
    }

    /// The type of `left operator right`, by core's numeric contract.
    ///
    /// NUM-1: "For i64 operands, `+`, `-`, `*`, `//`, and `%` return
    /// i64. Unconfigured mixed i64/f64 and f64 arithmetic returns f64;
    /// `/` defaults to f64." A comparison is a bool (NUM-6's table).
    /// What an operator gives for operands that are not numbers, and
    /// the type of a `<=>` outcome, are for core to say: not known
    /// here, and not reported.
    fn binary_type(&mut self, operator: BinaryOperator, left: &Expr, right: &Expr) -> Type {
        let integer = |ty: &Type| matches!(ty, Type::Scalar(Scalar::I64));
        let number = |ty: &Type| matches!(ty, Type::Scalar(Scalar::I64 | Scalar::F64));
        match operator {
            BinaryOperator::And
            | BinaryOperator::Or
            | BinaryOperator::Equal
            | BinaryOperator::NotEqual
            | BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual => BOOL,
            BinaryOperator::Compare => Type::Unknown,
            BinaryOperator::Divide if number(&left.ty) && number(&right.ty) => {
                Type::Scalar(Scalar::F64)
            }
            BinaryOperator::Add
            | BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::FloorDivide
            | BinaryOperator::Modulo
                if number(&left.ty) && number(&right.ty) =>
            {
                if integer(&left.ty) && integer(&right.ty) {
                    Type::Scalar(Scalar::I64)
                } else {
                    Type::Scalar(Scalar::F64)
                }
            }
            _ => Type::Unknown,
        }
    }

    // ---- What a place expects ----

    /// The expression `value` in a place that expects `expected`: as it
    /// is when its type is the one expected, widened when it is an
    /// `i64` where an `f64` is expected (EXPR-5: "Numeric widening
    /// follows the admitted destination relationship"), and reported
    /// otherwise. `what` names the place: `the function's result`.
    fn fit(&mut self, value: Expr, expected: &Type, what: &str) -> Expr {
        if is_integer(&value.ty) && accepts_float(expected) {
            let at = value.at;
            return Expr {
                kind: ExprKind::Widen(Box::new(value)),
                ty: Type::Scalar(Scalar::F64),
                at,
            };
        }
        self.conforms(&value.ty, expected, value.at, what);
        value
    }

    /// Reports a value of type `ty` in a place that expects `expected`
    /// when it is no value of that type.
    fn conforms(&mut self, ty: &Type, expected: &Type, at: Location, what: &str) {
        if admits(expected, ty) {
            return;
        }
        self.diagnostics.emit(local_error(
            Category::Type,
            codes::local::TYPE_MISMATCH,
            at,
            format!("this is `{ty}`, and {what} is `{expected}`"),
            "EXPR-5",
        ));
    }
}

/// `bool`.
const BOOL: Type = Type::Scalar(Scalar::Bool);

/// True when nothing can be said against a value of this type: it is
/// not known, or it is `any`, whose typing is for a later pass
/// (TYPE-8).
fn open(ty: &Type) -> bool {
    matches!(ty, Type::Unknown | Type::Scalar(Scalar::Any))
}

fn is_integer(ty: &Type) -> bool {
    matches!(ty, Type::Scalar(Scalar::I64))
}

/// True when an `i64` is widened in a place that expects `expected`: an
/// `f64`, or an `f64?`.
fn accepts_float(expected: &Type) -> bool {
    match expected {
        Type::Scalar(Scalar::F64) => true,
        Type::Nullable(inner) => accepts_float(inner),
        _ => false,
    }
}

/// True when a value of type `ty` is a value of `expected`: the same
/// type as far as each is known, or a `T` where a `T?` is expected
/// (TYPE-14: "either a T or null").
fn admits(expected: &Type, ty: &Type) -> bool {
    if open(expected) || open(ty) || agree(expected, ty) {
        return true;
    }
    match expected {
        Type::Nullable(inner) => admits(inner, ty),
        _ => false,
    }
}

/// True when `node` is the integer literal 2^63, which is one more
/// than the greatest `i64`.
fn is_least_magnitude(node: &SyntaxNode) -> bool {
    node.kind() == SyntaxKind::Literal
        && tokens(node).next().is_some_and(|token| {
            token.kind() == SyntaxKind::Int
                && decode_integer(token.text()) == Ok(i64::MIN.unsigned_abs())
        })
}

/// The type a value that is not `null` is expected to have: of `T?`,
/// `T` (TYPE-14: "either a T or null").
fn not_null(expected: Option<&Type>) -> Option<&Type> {
    match expected {
        Some(Type::Nullable(inner)) => not_null(Some(inner)),
        other => other,
    }
}

/// The identifier a name expression uses: of `module.name`, the name.
fn name_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    let identifiers = |node: &SyntaxNode| {
        tokens(node)
            .filter(|token| token.kind() == SyntaxKind::Ident)
            .collect::<Vec<_>>()
    };
    match child(node, SyntaxKind::QualifiedName) {
        Some(qualified) => identifiers(&qualified).into_iter().nth(1),
        None => identifiers(node).into_iter().next(),
    }
}

/// The type of a temporal literal, by the parts it writes (LEX-11:
/// "Offset-free date/time is civil, not an implicit UTC instant. A
/// zoned datetime requires an explicit offset"). `None` for one the
/// lexer has reported.
fn temporal_type(text: &str) -> Option<Scalar> {
    let literal = decode_temporal(text).ok()?;
    let zone = literal.zone.is_some();
    Some(match (literal.date, literal.time, literal.offset) {
        (Some(_), None, _) => Scalar::Date,
        (Some(_), Some(_), None) => Scalar::CivilDatetime,
        (Some(_), Some(_), Some(_)) if zone => Scalar::ZonedDatetime,
        (Some(_), Some(_), Some(_)) => Scalar::Datetime,
        (None, Some(_), _) if zone => Scalar::ZonedTime,
        (None, Some(_), _) => Scalar::Time,
        (None, None, _) => Scalar::Timezone,
    })
}

/// The operator a token is, between two operands.
fn binary_operator(kind: SyntaxKind) -> Option<BinaryOperator> {
    Some(match kind {
        SyntaxKind::Plus => BinaryOperator::Add,
        SyntaxKind::Minus => BinaryOperator::Subtract,
        SyntaxKind::Star => BinaryOperator::Multiply,
        SyntaxKind::Slash => BinaryOperator::Divide,
        SyntaxKind::SlashSlash => BinaryOperator::FloorDivide,
        SyntaxKind::Percent => BinaryOperator::Modulo,
        SyntaxKind::EqEq => BinaryOperator::Equal,
        SyntaxKind::BangEq => BinaryOperator::NotEqual,
        SyntaxKind::Lt => BinaryOperator::Less,
        SyntaxKind::LtEq => BinaryOperator::LessEqual,
        SyntaxKind::Gt => BinaryOperator::Greater,
        SyntaxKind::GtEq => BinaryOperator::GreaterEqual,
        SyntaxKind::Spaceship => BinaryOperator::Compare,
        SyntaxKind::AmpAmp => BinaryOperator::And,
        SyntaxKind::PipePipe => BinaryOperator::Or,
        _ => return None,
    })
}

/// The operator of a compound assignment: `+` for `+=`. `None` for `=`
/// and for a token that is no assignment.
fn compound(kind: SyntaxKind) -> Option<BinaryOperator> {
    Some(match kind {
        SyntaxKind::PlusEq => BinaryOperator::Add,
        SyntaxKind::MinusEq => BinaryOperator::Subtract,
        SyntaxKind::StarEq => BinaryOperator::Multiply,
        SyntaxKind::SlashEq => BinaryOperator::Divide,
        SyntaxKind::SlashSlashEq => BinaryOperator::FloorDivide,
        SyntaxKind::PercentEq => BinaryOperator::Modulo,
        _ => return None,
    })
}
