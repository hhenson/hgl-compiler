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
//! * `let`, `var`, assignment and `return` (EXPR-3, EXPR-5, FN-7);
//! * calls of value functions and constructions of structs, each
//!   argument given to its parameter or field (EXPR-2, TYPE-5);
//! * members of enums, fields of struct values, and the elements of
//!   lists and dictionaries (TYPE-7, EXPR-7, ERR-6).
//!
//! A form it does not read yet is left [`ExprKind::Unread`], with no
//! type and no report: control flow, operators given configuration, a
//! call of what is no value function or struct. A type that is not
//! known is reported once, where it was found, and never again where it
//! is used (ERR-9).

use std::collections::HashMap;

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_lexer::literal::{
    decode_duration, decode_float, decode_integer, decode_string, decode_temporal,
};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};
use hgl_source::FileId;

use crate::declared::{agree, names_one_of, names_parameter, substitute_within};
use crate::hir::{
    self, BinaryOperator, Body, Callee, Expr, ExprKind, Local, LocalId, Stmt, UnaryOperator, Value,
};
use crate::program::{
    Binding, Declaration, DeclarationKind, LocalKind, Module, OperatorRef, Program, Role,
};
use crate::read::{child, has_token, is_type, tokens};
use crate::types::{
    Argument, Constant, ConstantValue, Detail, GenericKind, GenericParameter, Pack, Scalar,
    Signature, Type,
};
use crate::{codes, error, local_error};

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
                // FN-1: a `const fn`. GEN-4: "an `impl const fn` a value
                // candidate", whose body is a value function's.
                (
                    DeclarationKind::Function(Role::Value)
                    | DeclarationKind::Implementation(Role::Value, _),
                    Some(Detail::Callable(signature)),
                ) => Typer::new(
                    program,
                    module,
                    declaration,
                    signature,
                    &bindings,
                    &written,
                    diagnostics,
                )
                .body(),
                _ => None,
            };
            bodies.push(body);
        }
        program.modules[index].bodies = bodies;
    }
}

/// Reads one value function.
struct Typer<'a> {
    program: &'a Program,
    /// The module the function is declared in.
    module: &'a Module,
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
        program: &'a Program,
        module: &'a Module,
        declaration: &'a Declaration,
        signature: &'a Signature,
        bindings: &'a HashMap<(FileId, u32), &'a Binding>,
        written: &'a HashMap<(FileId, u32), &'a Type>,
        diagnostics: &'a mut Diagnostics,
    ) -> Typer<'a> {
        let mut typer = Typer {
            program,
            module,
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
                let at = self.at(node);
                // FN-7: "A value return yields an ordinary value." One
                // with nothing written yields none.
                if let (None, Some(result)) = (&value, &self.signature.result)
                    && !open(result)
                {
                    self.diagnostics.emit(error(
                        Category::Type,
                        codes::TYPE_MISMATCH,
                        at,
                        format!("this returns no value, and the function's result is `{result}`"),
                        "FN-7",
                    ));
                }
                // "`return value` [rejects] in a function that declares
                // no result."
                if let (Some(value), None) = (&value, &self.signature.result) {
                    self.diagnostics.emit(error(
                        Category::Type,
                        codes::TYPE_MISMATCH,
                        value.at,
                        "this returns a value, and the function declares no result".to_string(),
                        "FN-7",
                    ));
                }
                Stmt::Return { value, at }
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
        // Where the assignment's operator is written: `+=`.
        let operator_at = tokens(node)
            .find(|token| compound(token.kind()).is_some())
            .map_or(self.at(node), |token| {
                Location::new(self.file, token.span())
            });
        let at = self.at(node);
        let target = self.locals[local].ty.clone();
        if !matches!(
            self.locals[local].kind,
            LocalKind::Var | LocalKind::State | LocalKind::Cache
        ) {
            self.diagnostics.emit(
                error(
                    Category::Name,
                    codes::BINDING_IMMUTABLE,
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
                let result = self.binary_type(operator, &left, &right, operator_at);
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
            SyntaxKind::CallExpr => self.call(node, expected),
            SyntaxKind::TypedConstructor => self.typed_constructor(node),
            SyntaxKind::UndecidedConstructor => {
                // GEN-14: the resolver decided what the node is. One
                // that is comparisons is for a later part.
                let constructor = self
                    .module
                    .decisions
                    .iter()
                    .any(|decision| decision.at == at && decision.constructor == Some(true));
                if constructor {
                    self.typed_constructor(node)
                } else {
                    (ExprKind::Unread, Type::Unknown)
                }
            }
            SyntaxKind::MemberExpr => self.member(node),
            SyntaxKind::ImpliedMember => self.implied_member(node, expected),
            SyntaxKind::IndexExpr => self.index(node),
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
                        self.diagnostics.emit(error(
                            Category::Type,
                            codes::TYPE_MISMATCH,
                            self.at(node),
                            format!("`null` is no value of `{expected}`: write `{expected}?`"),
                            "TYPE-14",
                        ));
                        (ExprKind::Literal(Value::Null), Type::Unknown)
                    }
                    // What the place expects is not known.
                    Some(_) => (ExprKind::Literal(Value::Null), Type::Unknown),
                    // "`null` has the nullable type its place expects,
                    // and rejects where nothing gives it one."
                    None => {
                        self.diagnostics.emit(error(
                            Category::Type,
                            codes::NULL_CONTEXT,
                            self.at(node),
                            "`null` has the type its place expects, and nothing here gives \
                             one: write the type"
                                .to_string(),
                            "TYPE-14",
                        ));
                        (ExprKind::Literal(Value::Null), Type::Unknown)
                    }
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
                error(
                    Category::Name,
                    codes::BINDING_UNASSIGNED,
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
        // True when nothing is expected of the tuple, which is not the
        // same as what is expected not being known.
        let unexpected = expected.is_none();
        let expected = match not_null(expected) {
            Some(Type::Tuple(expected)) if expected.len() == items.len() => Some(expected),
            _ => None,
        };
        let elements: Vec<Expr> = items
            .iter()
            .enumerate()
            .map(|(index, item)| match expected {
                Some(expected) => {
                    let expected = &expected[index];
                    let element = self.expression(item, Some(expected));
                    self.fit(element, expected, "this element of the tuple")
                }
                // Nothing is expected of the tuple, and so of its
                // elements: `(null, 1)` gives its `null` no type.
                None if unexpected => self.expression(item, None),
                // What the place expects is not known, or is no tuple
                // of this length: what an element is expected to be is
                // not known either, and nothing is reported of that.
                None => self.expression(item, Some(&Type::Unknown)),
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
        // True when nothing is expected of the sequence, which is not
        // the same as what is expected not being known.
        let unexpected = expected.is_none();
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
            let elements: Vec<Expr> = items
                .iter()
                .map(|item| {
                    let value = self.expression(item, Some(element));
                    self.fit(value, element, "an element")
                })
                .collect();
            // TYPE-10: "Size participates in identity." A list of a
            // size that is known has that many entries.
            if let Type::List {
                size: Some(size), ..
            } = expected
                && let ConstantValue::Int(size) = size.value
                && i64::try_from(elements.len()) != Ok(size)
            {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    self.at(node),
                    format!(
                        "this has {} {}, and a `{expected}` has {size}",
                        elements.len(),
                        if elements.len() == 1 {
                            "entry"
                        } else {
                            "entries"
                        }
                    ),
                    "TYPE-10",
                ));
            }
            return (ExprKind::Sequence(elements), expected.clone());
        }
        // From here the place gives the entries no type. Where it
        // expects a type that is known and holds no sequence, an `i64`,
        // the sequence is no value of it. It is then a list of what is
        // not known, which the place reports as no `i64`, and its
        // entries are not judged as well. A dictionary's entries are for
        // the pass that reads dictionaries.
        let misplaced = expected
            .is_some_and(|expected| !open(expected) && !matches!(expected, Type::Dict { .. }));
        let unjudged = if misplaced {
            Type::List {
                element: Box::new(Type::Unknown),
                size: None,
            }
        } else {
            Type::Unknown
        };
        // An entry that takes its type from its place, `null`, says
        // nothing of what the sequence holds: the others are read
        // first, and it is then read as an element of their type.
        // Where what the place expects is not known, what an entry is
        // expected to be is not known either: the `[]` of `[[]]` is
        // then not reported.
        let of_entry = if unexpected {
            None
        } else {
            Some(&Type::Unknown)
        };
        let mut read: Vec<Option<Expr>> = items
            .iter()
            .map(|item| (!takes_its_type(item)).then(|| self.expression(item, of_entry)))
            .collect();
        let known: Vec<&Type> = read.iter().flatten().map(|entry| &entry.ty).collect();
        // EXPR-9: "Entries of two types reject, and so do entries that
        // give no type: `[]`, `[null]`." Where what the place expects
        // is not known, neither is the sequence, and nothing is
        // reported.
        let Some(&first) = known.first() else {
            if unexpected {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::SEQUENCE_CONTEXT,
                    self.at(node),
                    "nothing here says what this sequence holds: write its type".to_string(),
                    "EXPR-9",
                ));
            }
            let elements = items
                .iter()
                .map(|item| self.expression(item, Some(&Type::Unknown)))
                .collect();
            return (ExprKind::Sequence(elements), unjudged);
        };
        let numbers = |ty: &Type| matches!(ty, Type::Scalar(Scalar::I64 | Scalar::F64));
        let of = if known.iter().all(|ty| numbers(ty))
            && known.iter().any(|ty| **ty == Type::Scalar(Scalar::F64))
        {
            Type::Scalar(Scalar::F64)
        } else {
            first.clone()
        };
        // "`[1, null]` [is] a `list<i64?>`": a `null` among the values
        // makes their type nullable.
        let of = if items.iter().any(is_null) && !matches!(of, Type::Nullable(_)) {
            Type::Nullable(Box::new(of))
        } else {
            of
        };
        // Where what the place expects is not known, it may be what
        // gives entries of two types their one type: a `list<Base>` for
        // an `A` and a `B` that inherit from it. Such entries are not
        // judged here, and the sequence has no type. Where the place
        // is known and holds no sequence, the place reports it.
        let disagree = read.iter().flatten().any(|entry| {
            !(self.admits(&of, &entry.ty) || is_integer_literal(entry) && accepts_float(&of))
        });
        if !unexpected && disagree {
            let elements = items
                .iter()
                .enumerate()
                .map(|(position, item)| match read[position].take() {
                    Some(entry) => entry,
                    None => self.expression(item, Some(&Type::Unknown)),
                })
                .collect();
            return (ExprKind::Sequence(elements), unjudged);
        }
        let elements: Vec<Expr> = items
            .iter()
            .enumerate()
            .map(|(position, item)| {
                let entry = match read[position].take() {
                    Some(entry) => entry,
                    None => self.expression(item, Some(&of)),
                };
                self.fit(entry, &of, "an element, as the first is,")
            })
            .collect();
        let ty = Type::List {
            element: Box::new(of),
            size: None,
        };
        (ExprKind::Sequence(elements), ty)
    }

    /// `-value`, `!value`. NUM-1: "Unary negation preserves the numeric
    /// type." `!` is of a bool (EXPR-6; OPS-5: "`&&`, `||`, and `!` take
    /// bools").
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
                // NUM-1: "Unary negation preserves the numeric type."
                // OPS-1: `-x` of a `duration` is a `duration`.
                let ty = match &operand.ty {
                    Type::Scalar(Scalar::I64 | Scalar::F64 | Scalar::Duration) => {
                        operand.ty.clone()
                    }
                    ty if unjudged_operand(ty) => Type::Unknown,
                    ty => {
                        self.diagnostics.emit(error(
                            Category::Operator,
                            codes::OPERATOR_OPERAND,
                            Location::new(self.file, token.span()),
                            format!("`-` takes no `{ty}`"),
                            "OPS-1",
                        ));
                        Type::Unknown
                    }
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
        let operator_at = tokens(node)
            .find(|token| binary_operator(token.kind()).is_some())
            .map_or(self.at(node), |token| {
                Location::new(self.file, token.span())
            });
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
            // TYPE-7: `.member` "names a member of the enum type its
            // position expects": beside an operand, that operand's.
            // "Parentheses select grouping" (EXPR-1): `(.buy)` is
            // `.buy`.
            // A `null` beside an operand is of that operand's type when
            // it is nullable; beside any other, what it is of is not
            // known here.
            _ if takes_its_type(&right) => {
                let left = self.expression(&left, None);
                let expected = beside(&right, &left.ty);
                let right = self.expression(&right, Some(&expected));
                (left, right)
            }
            _ if takes_its_type(&left) => {
                let right = self.expression(&right, None);
                let expected = beside(&left, &right.ty);
                let left = self.expression(&left, Some(&expected));
                (left, right)
            }
            _ => (self.expression(&left, None), self.expression(&right, None)),
        };
        let ty = self.binary_type(operator, &left, &right, operator_at);
        let kind = ExprKind::Binary {
            operator,
            left: Box::new(left),
            right: Box::new(right),
        };
        (kind, ty)
    }

    /// The type of `left operator right`, by core's contracts. `at` is
    /// where the operator is written.
    ///
    /// NUM-1: "For i64 operands, `+`, `-`, `*`, `//`, and `%` return
    /// i64. Unconfigured mixed i64/f64 and f64 arithmetic returns f64;
    /// `/` defaults to f64." OPS-1: "The symbol operators admit the
    /// operand types of the first table below ... Every other pairing
    /// is a source rejection." OPS-2: which types are ordered, and that
    /// equality is between two values of one type.
    ///
    /// An operand whose type is not known, a generic parameter, whose
    /// operators its constraints say, and a nullable one, whose payload
    /// needs its guard, are not judged here.
    fn binary_type(
        &mut self,
        operator: BinaryOperator,
        left: &Expr,
        right: &Expr,
        at: Location,
    ) -> Type {
        use BinaryOperator as Op;
        let comparison = matches!(
            operator,
            Op::Equal | Op::NotEqual | Op::Less | Op::LessEqual | Op::Greater | Op::GreaterEqual
        );
        // What the operator gives when nothing can be said of its
        // operands. The type of a `<=>` outcome, core's `CmpResult`, is
        // not declared in HGL source yet, and is not known here.
        let unjudged = if comparison || matches!(operator, Op::And | Op::Or) {
            BOOL
        } else {
            Type::Unknown
        };
        if matches!(operator, Op::And | Op::Or)
            || unjudged_operand(&left.ty)
            || unjudged_operand(&right.ty)
        {
            return unjudged;
        }
        let found = match operator {
            Op::Equal | Op::NotEqual => {
                // "Two values of one type", a number with a number
                // included (NUM-2).
                let one_type = self.admits(&left.ty, &right.ty) || self.admits(&right.ty, &left.ty);
                (one_type || (is_number(&left.ty) && is_number(&right.ty))).then_some(BOOL)
            }
            _ => match (scalar_of(&left.ty), scalar_of(&right.ty)) {
                (Some(left), Some(right)) => operator_result(operator, left, right),
                _ => None,
            },
        };
        match found {
            Some(_) if matches!(operator, Op::Compare) => Type::Unknown,
            Some(ty) => ty,
            None => {
                self.diagnostics.emit(error(
                    Category::Operator,
                    codes::OPERATOR_OPERAND,
                    at,
                    format!(
                        "`{}` takes no `{}` and `{}`",
                        operator.symbol(),
                        left.ty,
                        right.ty
                    ),
                    if comparison || matches!(operator, Op::Compare) {
                        "OPS-2"
                    } else {
                        "OPS-1"
                    },
                ));
                unjudged
            }
        }
    }

    // ---- Calls and constructions ----

    /// The declarations a name is bound to, when it names declarations
    /// of a module and not a local.
    fn declarations(&self, name: &SyntaxToken) -> Option<(usize, &'a [usize])> {
        match self.bindings.get(&(self.file, name.span().start))? {
            Binding::Declarations {
                module,
                declarations,
            } => Some((*module, declarations.as_slice())),
            Binding::Local { .. } => None,
        }
    }

    /// `callee(arguments)`: a call of a value function, or the
    /// construction of a struct.
    ///
    /// FN-4: "runtime/value evaluation chooses the value definition."
    /// A name left for the prelude, an operator, and a callee that is
    /// itself an expression are for later parts: their arguments are
    /// read, and the call has no type.
    fn call(&mut self, node: &SyntaxNode, expected: Option<&Type>) -> (ExprKind, Type) {
        let (Some(callee), Some(list)) = (node.children().next(), child(node, SyntaxKind::ArgList))
        else {
            return (ExprKind::Unread, Type::Unknown);
        };
        let found = match callee.kind() {
            SyntaxKind::NameExpr => name_token(&callee)
                .and_then(|name| self.declarations(&name).map(|found| (name, found))),
            _ => None,
        };
        let Some((name, (module, declarations))) = found else {
            return self.unread_call(&list);
        };
        let program = self.program;
        let kind_of = |declaration: usize| program.modules[module].declarations[declaration].kind;
        let value_function = declarations.iter().copied().find(|&declaration| {
            matches!(
                kind_of(declaration),
                DeclarationKind::Function(Role::Value)
                    | DeclarationKind::NativeFunction(Role::Value)
            )
        });
        if let Some(declaration) = value_function {
            return self.call_function(&name, &list, module, declaration);
        }
        let structure = declarations
            .iter()
            .copied()
            .find(|&declaration| kind_of(declaration) == DeclarationKind::Struct);
        if let Some(declaration) = structure {
            // GEN-2: a generic struct "infers them from constructor
            // fields and expected type".
            let given = match not_null(expected) {
                Some(Type::Named {
                    module: expected_module,
                    declaration: expected_declaration,
                    arguments,
                    ..
                }) if (*expected_module, *expected_declaration) == (module, declaration) => {
                    Some(arguments.clone())
                }
                _ => None,
            };
            return self.construct(&name, &list, module, declaration, given);
        }
        let operator = declarations
            .iter()
            .copied()
            .find(|&declaration| kind_of(declaration) == DeclarationKind::Operator);
        if let Some(declaration) = operator {
            let operator = OperatorRef {
                module,
                declaration,
            };
            return self.call_operator(&name, &list, operator, expected);
        }
        let temporal = declarations.iter().any(|&declaration| {
            matches!(
                kind_of(declaration),
                DeclarationKind::Function(Role::Temporal)
                    | DeclarationKind::NativeFunction(Role::Temporal)
            )
        });
        if temporal {
            // FN-3: "A runtime body calls only value functions, and so
            // does a value function." FN-4: "A bad selected signature
            // is an error, not fallback to the other role."
            self.diagnostics.emit(error(
                Category::FunctionKind,
                codes::CALL_ROLE,
                Location::new(self.file, name.span()),
                format!(
                    "`{}` is a temporal function, and a value function calls value functions",
                    name.text()
                ),
                "FN-3",
            ));
        }
        self.unread_call(&list)
    }

    /// A call this pass does not read: its arguments are read for what
    /// they report themselves, with nothing expected of them.
    fn unread_call(&mut self, list: &SyntaxNode) -> (ExprKind, Type) {
        let arguments = arguments_of(list)
            .into_iter()
            .map(|(_, value)| hir::Argument {
                parameter: None,
                value: self.expression(&value, Some(&Type::Unknown)),
            })
            .collect();
        let kind = ExprKind::Call {
            callee: Callee::Unread,
            arguments,
        };
        (kind, Type::Unknown)
    }

    /// A call of the value function `declaration` of `module`.
    fn call_function(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        module: usize,
        declaration: usize,
    ) -> (ExprKind, Type) {
        let callee = Callee::Function {
            module,
            declaration,
        };
        let program = self.program;
        let Some(Detail::Callable(signature)) = program.modules[module].details.get(declaration)
        else {
            return self.unread_call(list);
        };
        // What a generic function's parameters stand for is found by
        // the pass that resolves generics: a type that names one is not
        // known here.
        let known = |ty: &Type| {
            if names_parameter(ty) {
                Type::Unknown
            } else {
                ty.clone()
            }
        };
        let slots: Vec<Slot> = signature
            .parameters
            .iter()
            .map(|parameter| Slot {
                name: &parameter.name,
                ty: known(&parameter.ty),
                // UNIT-8: "A parameter whose type is written `T?` needs
                // no default to be omitted."
                required: !parameter.has_default && !matches!(parameter.ty, Type::Nullable(_)),
                pack: parameter.pack,
            })
            .collect();
        let arguments = self.bind(name, list, &slots, Takes::Parameters);
        let ty = signature.result.as_ref().map_or(Type::Unknown, known);
        (ExprKind::Call { callee, arguments }, ty)
    }

    /// A call of the operator `operator` by its name.
    ///
    /// GEN-5: "Match candidates against the requested output before
    /// inputs, then check their conditions. Select exactly one
    /// most-specific admitted candidate ... No match rejects; tied
    /// most-specific candidates are ambiguous ... A call in a value
    /// function or a runtime body selects among the value candidates."
    /// FN-3: "Any body calls an operator; in a runtime body or a value
    /// function the call selects a value candidate."
    ///
    /// The arguments are given to the operator's public parameters
    /// (GEN-4: the operator "owns public names/order"), and a candidate
    /// is admitted when each fits the parameter the candidate has in
    /// that place. A candidate that fits with no integer literal
    /// widened is taken before one that needs it.
    ///
    /// What is not read here: a call of an operator that has a pack, and
    /// one that has a generic value candidate. Which of two candidates
    /// is the more specific is for the pass that resolves generics
    /// (GEN-12). Such a call has no type.
    fn call_operator(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        operator: OperatorRef,
        expected: Option<&Type>,
    ) -> (ExprKind, Type) {
        let program = self.program;
        let Some(Detail::Callable(public)) = program.modules[operator.module]
            .details
            .get(operator.declaration)
        else {
            return self.unread_call(list);
        };
        // The operator's value candidates, wherever they are declared.
        // GEN-5: "Library membership and registration order grant no
        // priority."
        let mut candidates: Vec<(usize, usize, &Signature)> = Vec::new();
        for (module, declared) in program.modules.iter().enumerate() {
            for (declaration, candidate) in declared.declarations.iter().enumerate() {
                if candidate.kind.candidate() != Some((Role::Value, Some(operator))) {
                    continue;
                }
                if let Some(Detail::Callable(signature)) = declared.details.get(declaration) {
                    candidates.push((module, declaration, signature));
                }
            }
        }
        let unread = public
            .parameters
            .iter()
            .any(|parameter| parameter.pack.is_some())
            || candidates
                .iter()
                .any(|(_, _, signature)| !signature.generics.is_empty());
        if unread {
            return self.unread_call(list);
        }

        // EXPR-2: "Named arguments use the selected public callable's
        // parameter names." What a call may leave out is the
        // candidate's to say (GEN-6: "A candidate may give any of its
        // inputs a default"), so nothing is required here.
        let slots: Vec<Slot> = public
            .parameters
            .iter()
            .map(|parameter| Slot {
                name: &parameter.name,
                ty: Type::Unknown,
                required: false,
                pack: None,
            })
            .collect();
        let reported = self.diagnostics.len();
        let assigned = self.assign(name, list, &slots, Takes::Parameters);
        // Each argument, read with nothing expected of it: what it
        // reports itself is reported here, once.
        let given: Vec<Given> = assigned
            .into_iter()
            .map(|(slot, node)| Given {
                slot,
                by_place: typed_by_place(&node),
                value: self.expression(&node, Some(&Type::Unknown)),
                node,
            })
            .collect();
        // An argument given to no parameter, or rejected itself, has
        // been reported, and one whose type is not known says nothing
        // of the candidates: the call is then not judged (ERR-9).
        let faulty = self.diagnostics.len() != reported;
        let unknown = given
            .iter()
            .any(|given| !given.by_place && open(&given.value.ty));
        if faulty || unknown {
            return (unselected(given), Type::Unknown);
        }

        // GEN-5: "the requested output before inputs". EXPR-5: an
        // integer literal is an `f64` where one is expected; it selects
        // as the `i64` it is where a candidate takes one.
        let wanted = expected.filter(|expected| !open(expected));
        let mut selected: Vec<(usize, usize, &Signature)> = Vec::new();
        for widening in [false, true] {
            for &(module, declaration, signature) in &candidates {
                if self.admits_call(signature, public, &given, wanted, widening) {
                    selected.push((module, declaration, signature));
                }
            }
            if !selected.is_empty() {
                break;
            }
        }
        let at = Location::new(self.file, name.span());
        let &[(module, declaration, signature)] = selected.as_slice() else {
            // GEN-5: "the requested output before inputs". Where what
            // the place expects is not known, it may be what tells tied
            // candidates apart: the `make()` of `[make()]` given to a
            // `list<i64>`. The tie is then not reported, and the call
            // has no type.
            if selected.len() > 1 && expected.is_some_and(open) {
                return (unselected(given), Type::Unknown);
            }
            let takes: Vec<String> = given
                .iter()
                .map(|given| {
                    if given.by_place {
                        format!("`{}`", given.node.text().to_string().trim())
                    } else {
                        format!("`{}`", given.value.ty)
                    }
                })
                .collect();
            let takes = if takes.is_empty() {
                "no argument".to_string()
            } else {
                takes.join(", ")
            };
            let diagnostic = if selected.is_empty() {
                let message = if candidates.is_empty() {
                    format!(
                        "operator `{}` has no value candidate, and a value function selects \
                         among those",
                        name.text()
                    )
                } else {
                    format!(
                        "no value candidate of operator `{}` takes {takes}{}",
                        name.text(),
                        match wanted {
                            Some(wanted) => format!(" and gives `{wanted}`"),
                            None => String::new(),
                        }
                    )
                };
                local_error(
                    Category::Operator,
                    codes::local::OPERATOR_NO_CANDIDATE,
                    at,
                    message,
                    "GEN-5",
                )
            } else {
                let mut diagnostic = local_error(
                    Category::Operator,
                    codes::local::OPERATOR_AMBIGUOUS,
                    at,
                    format!(
                        "{} value candidates of operator `{}` take {takes} equally",
                        selected.len(),
                        name.text()
                    ),
                    "GEN-5",
                );
                for (module, declaration, _) in &selected {
                    let candidate = &program.modules[*module].declarations[*declaration];
                    diagnostic = diagnostic.with_related(candidate.at, "this candidate");
                }
                diagnostic
            };
            self.diagnostics.emit(diagnostic);
            return (unselected(given), Type::Unknown);
        };

        // Each argument is of the type the selected candidate's
        // parameter has.
        let arguments = given
            .into_iter()
            .map(|given| {
                let Some(index) = given.slot else {
                    return hir::Argument {
                        parameter: None,
                        value: given.value,
                    };
                };
                let parameter = &signature.parameters[index];
                let place = format!("the parameter `{}`", parameter.name);
                let value = if given.by_place {
                    // Read again, as the parameter's type: the
                    // candidate was admitted because it fits.
                    let value = self.expression(&given.node, Some(&parameter.ty));
                    self.fit(value, &parameter.ty, &place)
                } else {
                    self.fit(given.value, &parameter.ty, &place)
                };
                hir::Argument {
                    parameter: Some(index),
                    value,
                }
            })
            .collect();
        let kind = ExprKind::Call {
            callee: Callee::Candidate {
                operator: (operator.module, operator.declaration),
                module,
                declaration,
            },
            arguments,
        };
        (kind, signature.result.clone().unwrap_or(Type::Unknown))
    }

    /// True when the candidate `signature`, of an operator whose public
    /// signature is `public`, admits a call that gives it `given` and of
    /// which `wanted` is expected (GEN-5). With `widening`, an integer
    /// literal is admitted where an `f64` is taken (EXPR-5).
    fn admits_call(
        &mut self,
        signature: &Signature,
        public: &Signature,
        given: &[Given],
        wanted: Option<&Type>,
        widening: bool,
    ) -> bool {
        let result_fits = match (wanted, &signature.result) {
            (Some(wanted), Some(result)) => self.admits(wanted, result),
            (Some(_), None) => false,
            (None, _) => true,
        };
        if !result_fits || signature.parameters.len() != public.parameters.len() {
            return false;
        }
        for (index, parameter) in signature.parameters.iter().enumerate() {
            let admitted = match given.iter().find(|given| given.slot == Some(index)) {
                // What the call leaves out has a default, the
                // candidate's or the operator's, or is written `T?`
                // (UNIT-8).
                None => {
                    parameter.has_default
                        || public.parameters[index].has_default
                        || matches!(parameter.ty, Type::Nullable(_))
                }
                Some(given) if given.by_place => {
                    self.fits_place(&given.node, &parameter.ty, widening)
                }
                Some(given) => {
                    self.admits(&parameter.ty, &given.value.ty)
                        || widening
                            && is_integer_literal(&given.value)
                            && accepts_float(&parameter.ty)
                }
            };
            if !admitted {
                return false;
            }
        }
        true
    }

    /// True when the expression at `node`, which is read as the type of
    /// its place, is a value of `ty`. It is read as one, and what that
    /// reading reports is taken back: the caller asks, and reports for
    /// itself. Without `widening`, it is no value of `ty` when an
    /// integer literal among its entries had to be widened to be one:
    /// `[1]` is then a `list<i64>` and no `list<f64>`.
    ///
    /// The expression has been read once already, with nothing expected
    /// of it, so a reading here changes nothing but the diagnostics.
    fn fits_place(&mut self, node: &SyntaxNode, ty: &Type, widening: bool) -> bool {
        let reported = self.diagnostics.len();
        let value = self.expression(node, Some(ty));
        let value = self.fit(value, ty, "its place");
        let fits = self.diagnostics.len() == reported;
        self.diagnostics.truncate(reported);
        fits && (widening || !widens_an_entry(&value))
    }

    /// `Name<arguments>(fields)`: the construction of a generic struct
    /// with its arguments written.
    fn typed_constructor(&mut self, node: &SyntaxNode) -> (ExprKind, Type) {
        // `Name<...>(...)` or `alias.Name<...>(...)`: the name is the
        // last identifier, and the type is recorded where the whole of
        // it starts.
        let start = match child(node, SyntaxKind::QualifiedName) {
            Some(qualified) => Some(qualified.span().start),
            None => name_token(node).map(|name| name.span().start),
        };
        let (Some(name), Some(start), Some(list)) =
            (name_token(node), start, child(node, SyntaxKind::ArgList))
        else {
            return (ExprKind::Unread, Type::Unknown);
        };
        // The type the `read` pass gave `Name<arguments>`.
        let written = self.written.get(&(self.file, start)).copied();
        match written {
            Some(Type::Named {
                module,
                declaration,
                arguments,
                ..
            }) if self.program.modules[*module].declarations[*declaration].kind
                == DeclarationKind::Struct =>
            {
                self.construct(&name, &list, *module, *declaration, Some(arguments.clone()))
            }
            // A generic function given its arguments (GEN-8) is for a
            // later part.
            _ => self.unread_call(&list),
        }
    }

    /// The construction of the struct `declaration` of `module`. `given`
    /// is its generic arguments, when they are written or expected.
    ///
    /// TYPE-5: "An ordinary constructor supplies complete required
    /// data." TYPE-3: "abstract values are not constructible."
    fn construct(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        module: usize,
        declaration: usize,
        given: Option<Vec<Argument>>,
    ) -> (ExprKind, Type) {
        let program = self.program;
        let declared = &program.modules[module].declarations[declaration];
        let Some(Detail::Struct(read)) = program.modules[module].details.get(declaration) else {
            return self.unread_call(list);
        };
        if has_token(&declared.syntax, SyntaxKind::KwAbstract) {
            self.diagnostics.emit(
                error(
                    Category::Type,
                    codes::STRUCT_ABSTRACT,
                    Location::new(self.file, name.span()),
                    format!(
                        "`{}` is an abstract struct and is not constructed",
                        declared.name
                    ),
                    "TYPE-3",
                )
                .with_related(declared.at, "declared here"),
            );
        }
        // Arguments of the wrong number or kind have been reported
        // where they are written (GEN-2, GEN-14): the struct made is
        // then not known, and nothing more is reported of it (ERR-9).
        let reported = given
            .as_ref()
            .is_some_and(|given| !applies(&read.generics, given));
        let given = given.filter(|_| !reported);
        // With none given, a generic struct's arguments are found from
        // the fields it is given.
        let inferring = given.is_none() && !read.generics.is_empty();
        // The struct's own parameters. A field's type that still names
        // one once the arguments are in place has been given none: with
        // no arguments given, or where a pack's place is not filled in.
        // It is then not known. A parameter of the function being read
        // is a type like any other.
        let own: Vec<Location> = read.generics.iter().map(|generic| generic.at).collect();
        // Each field's type, with the generic arguments in place when
        // they are given.
        let map: Vec<(Location, &Argument)> = match &given {
            Some(given) => fixed(&read.generics).zip(given).collect(),
            None => Vec::new(),
        };
        let slots: Vec<Slot> = read
            .members
            .iter()
            .map(|member| {
                let ty = substitute_within(&member.ty, &map);
                Slot {
                    name: &member.name,
                    // TYPE-4: "A field is optional when its type is
                    // nullable ... or an `atomic<...>` of a nullable
                    // type", the generic arguments in place: the
                    // `value: T` of a `Box<i64?>` is.
                    required: !member.has_default && !is_optional(&ty),
                    ty: placed(ty, &own),
                    pack: None,
                }
            })
            .collect();
        // With the arguments given, each field's type is known and the
        // values are read against it.
        let (arguments, arguments_of_type) = match given {
            Some(given) => (self.bind(name, list, &slots, Takes::Fields), given),
            None if !inferring => (self.bind(name, list, &slots, Takes::Fields), Vec::new()),
            // GEN-2: "infers them from constructor fields". The value
            // of a field whose type names a parameter not yet found
            // says what the parameter is: `1` for `first: T`, `[1]`
            // for `values: list<T>`. Such values are read first, in
            // order, with nothing expected of them, but for `null` and
            // `.member`, which take their type from the field and say
            // nothing of it.
            None => {
                let assigned = self.assign(name, list, &slots, Takes::Fields);
                let mut found: Vec<Option<Argument>> = vec![None; read.generics.len()];
                let mut values: Vec<Option<Expr>> = vec![None; assigned.len()];
                for (position, (slot, node)) in assigned.iter().enumerate() {
                    let Some(index) = *slot else {
                        continue;
                    };
                    let pattern = &read.members[index].ty;
                    let unfound: Vec<Location> = read
                        .generics
                        .iter()
                        .zip(&found)
                        .filter(|(_, found)| found.is_none())
                        .map(|(generic, _)| generic.at)
                        .collect();
                    if takes_its_type(node) || !mentions(pattern, &unfound) {
                        continue;
                    }
                    let value = self.expression(node, Some(&Type::Unknown));
                    infer(pattern, &value.ty, &read.generics, &mut found);
                    values[position] = Some(value);
                }
                let found: Vec<Argument> = found
                    .into_iter()
                    .map(|found| found.unwrap_or(Argument::Type(Type::Unknown)))
                    .collect();
                // Every field given is of its type with the arguments
                // found in place: `Pair(1, "x")` gives `second: T` a
                // `str` where `T` is `i64`, and `Pair(1, null)` a
                // `null`.
                let map: Vec<(Location, &Argument)> = fixed(&read.generics).zip(&found).collect();
                let arguments = assigned
                    .iter()
                    .enumerate()
                    .map(|(position, (slot, node))| {
                        let Some(index) = *slot else {
                            return self.argument(None, node, None, Takes::Fields);
                        };
                        let ty = substitute_within(&read.members[index].ty, &map);
                        let slot = Slot {
                            ty: placed(ty, &own),
                            ..slots[index]
                        };
                        match values[position].take() {
                            Some(value) => {
                                let place = format!("the field `{}`", slot.name);
                                hir::Argument {
                                    parameter: Some(index),
                                    value: self.fit(value, &slot.ty, &place),
                                }
                            }
                            None => self.argument(Some(index), node, Some(&slot), Takes::Fields),
                        }
                    })
                    .collect();
                (arguments, found)
            }
        };
        let ty = if reported {
            Type::Unknown
        } else {
            Type::Named {
                name: declared.name.clone(),
                module,
                declaration,
                arguments: arguments_of_type,
            }
        };
        let kind = ExprKind::Construct {
            module,
            declaration,
            arguments,
        };
        (kind, ty)
    }

    /// Gives each argument of `list` to one of `slots`: the parameters
    /// of a function, or the fields of a struct, as `takes` says.
    ///
    /// EXPR-2: "Positional arguments precede named ones; reject
    /// duplicates, unknown names, missing required arguments, and wrong
    /// types. Named arguments use the selected public callable's
    /// parameter names." TYPE-5: a constructor's "arguments are named,
    /// each for a field the struct has, a parent's included, in any
    /// order; it takes no positional argument."
    ///
    /// The arguments come back in the order written, each with the slot
    /// it is given to: `None` for one given to a pack, or to nothing.
    /// Their values are not read here.
    fn assign(
        &mut self,
        callee: &SyntaxToken,
        list: &SyntaxNode,
        slots: &[Slot],
        takes: Takes,
    ) -> Vec<(Option<usize>, SyntaxNode)> {
        let what = takes.noun();
        let callee_at = Location::new(self.file, callee.span());
        let has_pack = |pack: Pack| slots.iter().any(|slot| slot.pack == Some(pack));
        // Which slots have been given a value.
        let mut filled = vec![false; slots.len()];
        let mut named_seen = false;
        // Whether a constructor was given an argument with no name. Which
        // field it was meant for is not known, so no field is then
        // reported as missing (ERR-9).
        let mut unnamed_field = false;
        let mut arguments = Vec::new();
        for (name, value) in arguments_of(list) {
            let at = self.at(&value);
            let slot = match &name {
                // TYPE-5: "it takes no positional argument."
                None if takes == Takes::Fields => {
                    unnamed_field = true;
                    self.diagnostics.emit(error(
                        Category::Type,
                        codes::ARGUMENT_UNEXPECTED,
                        at,
                        format!(
                            "a struct's fields are given by name: `{}(field: value)`",
                            callee.text()
                        ),
                        "TYPE-5",
                    ));
                    None
                }
                None => {
                    if named_seen {
                        self.diagnostics.emit(error(
                            Category::Type,
                            codes::ARGUMENT_ORDER,
                            at,
                            "an argument with no name follows a named one: positional \
                             arguments come first"
                                .to_string(),
                            "EXPR-2",
                        ));
                    }
                    // The first slot not yet filled that is no pack; a
                    // positional pack takes what the others leave
                    // (FN-8).
                    let next = (0..slots.len())
                        .find(|&index| !filled[index] && slots[index].pack.is_none());
                    if next.is_none() && !has_pack(Pack::Positional) {
                        let fixed = slots.iter().filter(|slot| slot.pack.is_none()).count();
                        self.diagnostics.emit(error(
                            Category::Type,
                            codes::ARGUMENT_UNEXPECTED,
                            at,
                            format!(
                                "`{}` has {fixed} {what}{}, and this is one more",
                                callee.text(),
                                if fixed == 1 { "" } else { "s" }
                            ),
                            "EXPR-2",
                        ));
                    }
                    next
                }
                Some(name) => {
                    named_seen = true;
                    let name_at = Location::new(self.file, name.span());
                    let found = slots
                        .iter()
                        .position(|slot| slot.pack.is_none() && slot.name == name.text());
                    match found {
                        Some(index) if filled[index] => {
                            self.diagnostics.emit(error(
                                Category::Name,
                                codes::ARGUMENT_DUPLICATE,
                                name_at,
                                format!("`{}` is given twice", name.text()),
                                "EXPR-2",
                            ));
                            None
                        }
                        Some(index) => Some(index),
                        // A named pack takes the names the others
                        // leave (FN-8).
                        None if has_pack(Pack::Named) => None,
                        None => {
                            self.diagnostics.emit(error(
                                Category::Name,
                                codes::ARGUMENT_UNKNOWN,
                                name_at,
                                format!("`{}` has no {what} `{}`", callee.text(), name.text()),
                                "EXPR-2",
                            ));
                            None
                        }
                    }
                }
            };
            if let Some(index) = slot {
                filled[index] = true;
            }
            arguments.push((slot, value));
        }
        for (index, slot) in slots.iter().enumerate() {
            if slot.required && slot.pack.is_none() && !filled[index] && !unnamed_field {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::ARGUMENT_MISSING,
                    callee_at,
                    format!(
                        "`{}` is given no `{}`, a {what} with no default",
                        callee.text(),
                        slot.name
                    ),
                    "EXPR-2",
                ));
            }
        }
        arguments
    }

    /// Gives each argument of `list` to one of `slots` and reads its
    /// value, which is of the slot's type.
    fn bind(
        &mut self,
        callee: &SyntaxToken,
        list: &SyntaxNode,
        slots: &[Slot],
        takes: Takes,
    ) -> Vec<hir::Argument> {
        self.assign(callee, list, slots, takes)
            .into_iter()
            .map(|(slot, value)| {
                let expected = slot.map(|index| &slots[index]);
                self.argument(slot, &value, expected, takes)
            })
            .collect()
    }

    /// The argument written at `node`, given to `slot`. One given to a
    /// pack, or to nothing, is read for what it reports itself.
    fn argument(
        &mut self,
        parameter: Option<usize>,
        node: &SyntaxNode,
        slot: Option<&Slot>,
        takes: Takes,
    ) -> hir::Argument {
        let value = match slot {
            Some(slot) => {
                let value = self.expression(node, Some(&slot.ty));
                let place = format!("the {} `{}`", takes.noun(), slot.name);
                self.fit(value, &slot.ty, &place)
            }
            None => self.expression(node, Some(&Type::Unknown)),
        };
        hir::Argument { parameter, value }
    }

    // ---- Members and elements ----

    /// `base.member`: a member of an enum, or a field of a struct
    /// value. Any other base is for a later part: the payload of a
    /// nullable or a result needs its guard (ERR-3, ERR-12).
    fn member(&mut self, node: &SyntaxNode) -> (ExprKind, Type) {
        // The member's name is the last token: any word (LEX-9).
        let (Some(base), Some(name)) = (node.children().next(), tokens(node).last()) else {
            return (ExprKind::Unread, Type::Unknown);
        };
        let program = self.program;
        // `Side.buy`: the base names an enum.
        if base.kind() == SyntaxKind::NameExpr
            && let Some(enum_name) = name_token(&base)
            && let Some((module, declarations)) = self.declarations(&enum_name)
            && let Some(&declaration) = declarations.iter().find(|&&declaration| {
                program.modules[module].declarations[declaration].kind == DeclarationKind::Enum
            })
        {
            return self.enum_member(&name, module, declaration);
        }
        let base = self.expression(&base, None);
        let Type::Named {
            module,
            declaration,
            arguments,
            ..
        } = &base.ty
        else {
            return (ExprKind::Unread, Type::Unknown);
        };
        let Some(Detail::Struct(read)) = program.modules[*module].details.get(*declaration) else {
            return (ExprKind::Unread, Type::Unknown);
        };
        let Some(member) = read
            .members
            .iter()
            .position(|member| member.name == name.text())
        else {
            self.diagnostics.emit(error(
                Category::Name,
                codes::MEMBER_UNKNOWN,
                Location::new(self.file, name.span()),
                format!("`{}` has no field `{}`", base.ty, name.text()),
                "EXPR-7",
            ));
            return (ExprKind::Unread, Type::Unknown);
        };
        // The field's type, with the value's generic arguments in
        // place. Where a pack's place is not filled in, it is not
        // known.
        let map: Vec<(Location, &Argument)> = fixed(&read.generics).zip(arguments).collect();
        let ty = substitute_within(&read.members[member].ty, &map);
        let own: Vec<Location> = read.generics.iter().map(|generic| generic.at).collect();
        let ty = placed(ty, &own);
        let kind = ExprKind::Field {
            base: Box::new(base),
            member,
        };
        (kind, ty)
    }

    /// The member `name` of the enum `declaration` of `module`.
    fn enum_member(
        &mut self,
        name: &SyntaxToken,
        module: usize,
        declaration: usize,
    ) -> (ExprKind, Type) {
        let program = self.program;
        let declared = &program.modules[module].declarations[declaration];
        let ty = Type::Named {
            name: declared.name.clone(),
            module,
            declaration,
            arguments: Vec::new(),
        };
        let member = match program.modules[module].details.get(declaration) {
            Some(Detail::Enum(read)) => read
                .members
                .iter()
                .position(|member| member.name == name.text()),
            _ => None,
        };
        match member {
            Some(member) => {
                let kind = ExprKind::EnumMember {
                    module,
                    declaration,
                    member,
                };
                (kind, ty)
            }
            None => {
                self.diagnostics.emit(
                    error(
                        Category::Name,
                        codes::MEMBER_UNKNOWN,
                        Location::new(self.file, name.span()),
                        format!("`{}` has no member `{}`", declared.name, name.text()),
                        "TYPE-7",
                    )
                    .with_related(declared.at, "declared here"),
                );
                // It is of the enum still: one report.
                (ExprKind::Unread, ty)
            }
        }
    }

    /// `.member` (TYPE-7): "names a member of the enum type its
    /// position expects and rejects where none is."
    fn implied_member(&mut self, node: &SyntaxNode, expected: Option<&Type>) -> (ExprKind, Type) {
        let Some(name) = tokens(node).last() else {
            return (ExprKind::Unread, Type::Unknown);
        };
        match not_null(expected) {
            Some(Type::Named {
                module,
                declaration,
                ..
            }) if self.program.modules[*module].declarations[*declaration].kind
                == DeclarationKind::Enum =>
            {
                self.enum_member(&name, *module, *declaration)
            }
            // What the place expects is not known.
            Some(expected) if open(expected) => (ExprKind::Unread, Type::Unknown),
            expected => {
                let place = match expected {
                    Some(expected) => format!("a `{expected}` is expected here"),
                    None => "nothing here says which".to_string(),
                };
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::ENUM_CONTEXT,
                    self.at(node),
                    format!(
                        "`.{}` is a member of the enum its place expects, and {place}",
                        name.text()
                    ),
                    "TYPE-7",
                ));
                (ExprKind::Unread, Type::Unknown)
            }
        }
    }

    /// `base[index]`: an element of a list or a tuple, or the value a
    /// dictionary has for a key (EXPR-7; ERR-6: "Ordinary/atomic
    /// dictionary brackets ... are strict point-in-time reads"). Any
    /// other base is for a later part.
    fn index(&mut self, node: &SyntaxNode) -> (ExprKind, Type) {
        let mut parts = node.children();
        let (Some(base), Some(index)) = (parts.next(), parts.next()) else {
            return (ExprKind::Unread, Type::Unknown);
        };
        let base = self.expression(&base, None);
        let (index, ty) = match &base.ty {
            Type::List { element, .. } => {
                let integer = Type::Scalar(Scalar::I64);
                let index = self.expression(&index, Some(&integer));
                (
                    self.fit(index, &integer, "the index of a list"),
                    (**element).clone(),
                )
            }
            // EXPR-7: "A tuple's index is a constant within its length:
            // it selects the element's type."
            Type::Tuple(elements) => {
                let integer = Type::Scalar(Scalar::I64);
                let index = self.expression(&index, Some(&integer));
                // EXPR-7: "by an `i64` index". An index of another type,
                // `pair[0 / 1]` or `pair[0 == 0]`, is reported as that,
                // and is not evaluated.
                let is_integer = self.admits(&integer, &index.ty);
                let index = self.fit(index, &integer, "the index of a tuple");
                let element = match constant_index(&index) {
                    _ if !is_integer => Some(&Type::Unknown),
                    Index::Known(value) => usize::try_from(value)
                        .ok()
                        .and_then(|position| elements.get(position)),
                    // A constant this pass does not evaluate: the
                    // element's type is then not known, and nothing is
                    // reported of it.
                    Index::Unevaluated => Some(&Type::Unknown),
                    Index::NotConstant => None,
                };
                let ty = match element {
                    Some(element) => element.clone(),
                    None => {
                        self.diagnostics.emit(error(
                            Category::Type,
                            codes::TUPLE_INDEX,
                            index.at,
                            format!(
                                "a tuple's index is a constant, here one of 0 to {}: it \
                                 selects the element's type",
                                elements.len().saturating_sub(1)
                            ),
                            "EXPR-7",
                        ));
                        Type::Unknown
                    }
                };
                (index, ty)
            }
            Type::Dict { key, value } => {
                let index = self.expression(&index, Some(key));
                (
                    self.fit(index, key, "the key of the dictionary"),
                    (**value).clone(),
                )
            }
            _ => (self.expression(&index, Some(&Type::Unknown)), Type::Unknown),
        };
        let kind = ExprKind::Index {
            base: Box::new(base),
            index: Box::new(index),
        };
        (kind, ty)
    }

    // ---- What a place expects ----

    /// The expression `value` in a place that expects `expected`: as it
    /// is when its type is the one expected, widened when it is an
    /// `i64` where an `f64` is expected (EXPR-5: "Numeric widening
    /// follows the admitted destination relationship"), and reported
    /// otherwise. `what` names the place: `the function's result`.
    fn fit(&mut self, value: Expr, expected: &Type, what: &str) -> Expr {
        if is_integer_literal(&value) && accepts_float(expected) {
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
        if self.admits(expected, ty) {
            return;
        }
        self.diagnostics.emit(error(
            Category::Type,
            codes::TYPE_MISMATCH,
            at,
            format!("this is `{ty}`, and {what} is `{expected}`"),
            "EXPR-5",
        ));
    }

    /// True when a value of type `ty` is a value of `expected`: the same
    /// type as far as each is known; a `T` where a `T?` is expected
    /// (TYPE-14: "either a T or null"); or a struct where an abstract
    /// struct it inherits from is expected.
    fn admits(&self, expected: &Type, ty: &Type) -> bool {
        if open(expected) || open(ty) || agree(expected, ty) {
            return true;
        }
        match expected {
            Type::Nullable(inner) => self.admits(inner, ty),
            Type::Named { .. } => self.inherits(ty, expected),
            _ => false,
        }
    }

    /// True when the struct type `ty` inherits from `expected`, through
    /// any number of parents, each with the generic arguments the
    /// struct below gives it.
    ///
    /// TYPE-6: "A field whose type is an abstract struct holds every
    /// struct that inherits from it": a value of a struct is a value of
    /// the abstract structs it inherits from. TYPE-3: "Generic
    /// applications are invariant", so a `Child : Base<i64>` is no
    /// `Base<f64>`.
    fn inherits(&self, ty: &Type, expected: &Type) -> bool {
        // The parents still to look at.
        let mut to_do = vec![ty.clone()];
        // The applications of each struct whose parents have been
        // looked at. A struct reached by two lines of parents is looked
        // at once for each application, so that a ladder of diamonds
        // is walked in step with its length.
        let mut followed: HashMap<(usize, usize), Vec<Vec<Argument>>> = HashMap::new();
        // Structs that inherit from one another have been reported;
        // the count ends the walk for them.
        let mut budget = 100_000;
        while let Some(current) = to_do.pop() {
            let Type::Named {
                module,
                declaration,
                arguments,
                ..
            } = &current
            else {
                continue;
            };
            let Some(Detail::Struct(read)) =
                self.program.modules[*module].details.get(*declaration)
            else {
                continue;
            };
            let applications = followed.entry((*module, *declaration)).or_default();
            if applications.contains(arguments) {
                continue;
            }
            applications.push(arguments.clone());
            // Each generic parameter before a type pack, with the
            // argument it is given.
            let map: Vec<(Location, &Argument)> = read
                .generics
                .iter()
                .take_while(|generic| !matches!(generic.kind, GenericKind::Pack { .. }))
                .map(|generic| generic.at)
                .zip(arguments)
                .collect();
            // The struct's own parameters: one a parent still names
            // once the arguments are in place has been given none here.
            let own: Vec<Location> = read.generics.iter().map(|generic| generic.at).collect();
            for parent in &read.parents {
                let parent = substitute_within(parent, &map);
                // Where the struct passes a type pack on to its parent,
                // the pack is not put in place here: the parent may be
                // the application expected, and nothing is reported of
                // it (ERR-9). A parameter of the function being read
                // is a type like any other, and is compared.
                let may_be = names_one_of(&parent, &own) && same_declaration(expected, &parent);
                if may_be || agree(expected, &parent) {
                    return true;
                }
                if budget == 0 {
                    return false;
                }
                budget -= 1;
                to_do.push(parent);
            }
        }
        false
    }
}

/// `bool`.
const BOOL: Type = Type::Scalar(Scalar::Bool);

/// True when an operator's contract is not applied to an operand of
/// this type here: it is not known, it is a generic parameter, whose
/// operators its constraints say, or it is nullable, and its payload
/// needs its guard first.
fn unjudged_operand(ty: &Type) -> bool {
    open(ty) || matches!(ty, Type::Parameter { .. } | Type::Nullable(_))
}

/// True for `i64` and `f64`.
fn is_number(ty: &Type) -> bool {
    matches!(ty, Type::Scalar(Scalar::I64 | Scalar::F64))
}

/// The scalar a type is, when it is one.
fn scalar_of(ty: &Type) -> Option<Scalar> {
    match ty {
        Type::Scalar(scalar) => Some(*scalar),
        _ => None,
    }
}

/// What an arithmetic or ordering operator gives for two scalars, by
/// core's contracts; `None` for a pairing they do not admit.
///
/// This table stands in for core's operator declarations, which are in
/// no HGL source yet. An operator is to be resolved as any call is, from
/// the candidates its operator declares (GEN-4), and the table goes when
/// core declares them (owner's note of 2026-11-15).
///
/// NUM-1 for numbers. OPS-1's table for the others: "An operation that
/// commutes admits its operands in either order; one that does not
/// admits the order shown." OPS-2: the ordered types are `str`, `bool`,
/// `date`, `time`, `datetime`, `civil_datetime` and `duration`, and the
/// numbers.
fn operator_result(operator: BinaryOperator, left: Scalar, right: Scalar) -> Option<Type> {
    use BinaryOperator as Op;
    use Scalar::{
        Bool, CivilDatetime, Date, Datetime, Duration, F64, I64, Str, Time, ZonedDatetime,
    };
    let number = |scalar: Scalar| matches!(scalar, I64 | F64);
    let instant =
        |scalar: Scalar| matches!(scalar, Datetime | Date | CivilDatetime | ZonedDatetime);
    let scalar = |scalar: Scalar| Some(Type::Scalar(scalar));
    // The result of arithmetic on two numbers.
    let numeric = || {
        if left == I64 && right == I64 {
            scalar(I64)
        } else {
            scalar(F64)
        }
    };
    match operator {
        Op::Add => match (left, right) {
            _ if number(left) && number(right) => numeric(),
            (Str, Str) => scalar(Str),
            (Duration, Duration) => scalar(Duration),
            (at, Duration) | (Duration, at) if instant(at) => scalar(at),
            (Date, Time) | (Time, Date) => scalar(CivilDatetime),
            _ => None,
        },
        Op::Subtract => match (left, right) {
            _ if number(left) && number(right) => numeric(),
            (Duration, Duration) => scalar(Duration),
            (at, Duration) if instant(at) => scalar(at),
            (Datetime, Datetime) | (Date, Date) | (CivilDatetime, CivilDatetime) => {
                scalar(Duration)
            }
            _ => None,
        },
        Op::Multiply => match (left, right) {
            _ if number(left) && number(right) => numeric(),
            (Duration, other) | (other, Duration) if number(other) => scalar(Duration),
            (Str, I64) | (I64, Str) => scalar(Str),
            _ => None,
        },
        Op::Divide => match (left, right) {
            _ if number(left) && number(right) => scalar(F64),
            (Duration, other) if number(other) => scalar(Duration),
            (Duration, Duration) => scalar(F64),
            _ => None,
        },
        Op::FloorDivide => match (left, right) {
            _ if number(left) && number(right) => numeric(),
            (Duration, Duration) => scalar(I64),
            _ => None,
        },
        Op::Modulo => (number(left) && number(right)).then(numeric).flatten(),
        Op::Less | Op::LessEqual | Op::Greater | Op::GreaterEqual | Op::Compare => {
            let ordered = matches!(
                left,
                Str | Bool | Date | Time | Datetime | CivilDatetime | Duration
            );
            ((number(left) && number(right)) || (left == right && ordered)).then_some(BOOL)
        }
        // Equality and the connectives are not read from this table.
        Op::Equal | Op::NotEqual | Op::And | Op::Or => Some(BOOL),
    }
}

/// One argument of an operator's call, before a candidate is selected.
struct Given {
    /// The public parameter it is given to, when it is given to one.
    slot: Option<usize>,
    /// Where it is written.
    node: SyntaxNode,
    /// True when it is read as the type of the parameter it is given
    /// to: see [`typed_by_place`].
    by_place: bool,
    /// Its value, read with nothing expected of it.
    value: Expr,
}

/// The call of an operator for which no candidate is selected: its
/// arguments as they were read, and no callee.
fn unselected(given: Vec<Given>) -> ExprKind {
    ExprKind::Call {
        callee: Callee::Unread,
        arguments: given
            .into_iter()
            .map(|given| hir::Argument {
                parameter: None,
                value: given.value,
            })
            .collect(),
    }
}

/// True when an integer literal was widened among the entries of a
/// sequence or a tuple written out, at any depth. A literal widened
/// inside a call among the entries is that call's, and is not counted.
fn widens_an_entry(value: &Expr) -> bool {
    match &value.kind {
        ExprKind::Widen(_) => true,
        ExprKind::Sequence(entries) | ExprKind::Tuple(entries) => {
            entries.iter().any(widens_an_entry)
        }
        _ => false,
    }
}

/// True when an argument is read as the type of the parameter it is
/// given to: `null` and `.member`, which have no type of their own
/// (TYPE-14, TYPE-7), and a sequence or a tuple written out, whose
/// entries are read as the parameter's (EXPR-5, EXPR-9), so that `[1]`
/// is a `list<f64>` or a `set<i64>` where a candidate takes one.
fn typed_by_place(node: &SyntaxNode) -> bool {
    match node.kind() {
        SyntaxKind::ParenExpr => node
            .children()
            .next()
            .is_some_and(|inner| typed_by_place(&inner)),
        SyntaxKind::SequenceExpr | SyntaxKind::TupleExpr => true,
        _ => takes_its_type(node),
    }
}

/// What the index of a tuple is, as far as this pass can say.
enum Index {
    /// A constant with this value.
    Known(i64),
    /// A constant whose value another pass finds: one that names a
    /// generic value parameter, divides, or leaves the range of an
    /// `i64`.
    Unevaluated,
    /// No constant: it reads a local, a parameter or a call.
    NotConstant,
}

/// The value of a tuple's index (EXPR-7: "A tuple's index is a constant
/// within its length"): an integer literal, or literals under a minus
/// and joined by `+`, `-` and `*`, as in `pair[1 - 1]`.
fn constant_index(index: &Expr) -> Index {
    match &index.kind {
        ExprKind::Literal(Value::Int(value)) => Index::Known(*value),
        ExprKind::GenericValue { .. } => Index::Unevaluated,
        ExprKind::Unary {
            operator: UnaryOperator::Negate,
            operand,
        } => match constant_index(operand) {
            // `checked_neg` gives `None` where the result does not fit.
            Index::Known(value) => value.checked_neg().map_or(Index::Unevaluated, Index::Known),
            other => other,
        },
        ExprKind::Binary {
            operator,
            left,
            right,
        } => match (constant_index(left), constant_index(right)) {
            (Index::NotConstant, _) | (_, Index::NotConstant) => Index::NotConstant,
            (Index::Known(left), Index::Known(right)) => {
                let value = match operator {
                    BinaryOperator::Add => left.checked_add(right),
                    BinaryOperator::Subtract => left.checked_sub(right),
                    BinaryOperator::Multiply => left.checked_mul(right),
                    // Division and remainder follow core's numeric
                    // contract, which this pass does not evaluate.
                    _ => None,
                };
                value.map_or(Index::Unevaluated, Index::Known)
            }
            _ => Index::Unevaluated,
        },
        _ => Index::NotConstant,
    }
}

/// What an argument may be given to: a parameter of a function, or a
/// field of a struct.
struct Slot<'a> {
    /// Its name.
    name: &'a str,
    /// Its type; not known when it names a generic parameter that
    /// nothing has given an argument.
    ty: Type,
    /// True when a call that gives it nothing is rejected.
    required: bool,
    /// The pack it is, when it is one (FN-8).
    pack: Option<Pack>,
}

/// What takes the arguments of a call.
///
/// `#[derive(Clone, Copy, PartialEq)]` asks the compiler to write the
/// code that copies one of these and compares two of them.
#[derive(Clone, Copy, PartialEq)]
enum Takes {
    /// The parameters of a function: positional arguments, then named
    /// ones (EXPR-2).
    Parameters,
    /// The fields of a struct: named arguments only (TYPE-5).
    Fields,
}

impl Takes {
    /// The word a message uses for one of them.
    fn noun(self) -> &'static str {
        match self {
            Takes::Parameters => "parameter",
            Takes::Fields => "field",
        }
    }
}

/// The arguments of an argument list, in order: each with its name,
/// when it has one, and its value.
fn arguments_of(list: &SyntaxNode) -> Vec<(Option<SyntaxToken>, SyntaxNode)> {
    list.children()
        .filter(|argument| argument.kind() == SyntaxKind::Arg)
        .filter_map(|argument| {
            let value = argument.children().next()?;
            // `name: value`: the name is the first token, any word
            // (LEX-9).
            let name = has_token(&argument, SyntaxKind::Colon)
                .then(|| tokens(&argument).next())
                .flatten();
            Some((name, value))
        })
        .collect()
}

/// Where each generic parameter before a type pack is declared: the
/// ones an argument is given to place for place (GEN-2).
fn fixed(generics: &[GenericParameter]) -> impl Iterator<Item = Location> + '_ {
    generics
        .iter()
        .take_while(|generic| !matches!(generic.kind, GenericKind::Pack { .. }))
        .map(|generic| generic.at)
}

/// True when `given` are arguments the generic parameters `generics`
/// can be given: as many, where there is no type pack, and each a type
/// or a value as its parameter is (GEN-2, GEN-14).
fn applies(generics: &[GenericParameter], given: &[Argument]) -> bool {
    let packed = generics
        .iter()
        .any(|generic| matches!(generic.kind, GenericKind::Pack { .. }));
    (packed || generics.len() == given.len())
        && generics
            .iter()
            .zip(given)
            .all(|(generic, given)| match (&generic.kind, given) {
                (GenericKind::Value(_), Argument::Type(ty)) => matches!(ty, Type::Unknown),
                (GenericKind::Type | GenericKind::Plain, Argument::Constant(_)) => false,
                _ => true,
            })
}

/// Finds what generic parameters stand for from a field's type,
/// `pattern`, and the type of the value the field is given, `actual`:
/// `T` is `i64` when `list<T>` is given a `list<i64>`, and `N` is 2
/// when `list<i64, N>` is given a `list<i64, 2>`. `found` has an entry
/// for each of `generics`; one already found is left as it is, and a
/// type that is not known finds nothing.
fn infer(
    pattern: &Type,
    actual: &Type,
    generics: &[GenericParameter],
    found: &mut [Option<Argument>],
) {
    match (pattern, actual) {
        (_, Type::Unknown) => {}
        (Type::Parameter { at, .. }, actual) => {
            find(*at, Argument::Type(actual.clone()), generics, found);
        }
        (Type::Nullable(pattern), Type::Nullable(actual)) => {
            infer(pattern, actual, generics, found)
        }
        // A `T` given where a `T?` is expected (TYPE-14).
        (Type::Nullable(pattern), actual) => infer(pattern, actual, generics, found),
        (
            Type::Named {
                module,
                declaration,
                arguments,
                ..
            },
            Type::Named {
                module: actual_module,
                declaration: actual_declaration,
                arguments: actual_arguments,
                ..
            },
        ) if (module, declaration) == (actual_module, actual_declaration) => {
            for pair in arguments.iter().zip(actual_arguments) {
                match pair {
                    (Argument::Type(pattern), Argument::Type(actual)) => {
                        infer(pattern, actual, generics, found);
                    }
                    (Argument::Constant(pattern), Argument::Constant(actual)) => {
                        infer_constant(pattern, actual, generics, found);
                    }
                    _ => {}
                }
            }
        }
        (Type::Tuple(patterns), Type::Tuple(actuals)) if patterns.len() == actuals.len() => {
            for (pattern, actual) in patterns.iter().zip(actuals) {
                infer(pattern, actual, generics, found);
            }
        }
        (
            Type::List { element, size },
            Type::List {
                element: actual,
                size: actual_size,
            },
        ) => {
            infer(element, actual, generics, found);
            if let (Some(size), Some(actual_size)) = (size, actual_size) {
                infer_constant(size, actual_size, generics, found);
            }
        }
        (Type::Set(pattern), Type::Set(actual))
        | (Type::Ref(pattern), Type::Ref(actual))
        | (Type::Atomic(pattern), Type::Atomic(actual))
        | (Type::Delta(pattern), Type::Delta(actual)) => infer(pattern, actual, generics, found),
        (
            Type::Dict { key, value },
            Type::Dict {
                key: actual_key,
                value: actual_value,
            },
        ) => {
            infer(key, actual_key, generics, found);
            infer(value, actual_value, generics, found);
        }
        (
            Type::Rolling {
                value,
                maximum,
                minimum,
            },
            Type::Rolling {
                value: actual,
                maximum: actual_maximum,
                minimum: actual_minimum,
            },
        ) => {
            infer(value, actual, generics, found);
            infer_constant(maximum, actual_maximum, generics, found);
            // TYPE-11: "Omitted minimum equals maximum."
            if let Some(minimum) = minimum {
                let actual_minimum = actual_minimum.as_ref().unwrap_or(actual_maximum);
                infer_constant(minimum, actual_minimum, generics, found);
            }
        }
        (
            Type::Result { value, error },
            Type::Result {
                value: actual_value,
                error: actual_error,
            },
        ) => {
            infer(value, actual_value, generics, found);
            infer(error, actual_error, generics, found);
        }
        _ => {}
    }
}

/// [`infer`], for a size or a constant argument: a generic value
/// parameter written there is the constant given in its place.
fn infer_constant(
    pattern: &Constant,
    actual: &Constant,
    generics: &[GenericParameter],
    found: &mut [Option<Argument>],
) {
    if let ConstantValue::Parameter { at, .. } = &pattern.value {
        find(*at, Argument::Constant(actual.clone()), generics, found);
    }
}

/// Records that the generic parameter declared at `at` stands for
/// `argument`, unless something has been found for it already.
fn find(
    at: Location,
    argument: Argument,
    generics: &[GenericParameter],
    found: &mut [Option<Argument>],
) {
    let position = generics.iter().position(|generic| generic.at == at);
    if let Some(position) = position
        && found[position].is_none()
    {
        found[position] = Some(argument);
    }
}

/// TYPE-4: "A field is optional when its type is nullable, `T?`, or an
/// `atomic<...>` of a nullable type, `atomic<T?>`."
fn is_optional(ty: &Type) -> bool {
    match ty {
        Type::Nullable(_) => true,
        Type::Atomic(inner) => matches!(**inner, Type::Nullable(_)),
        _ => false,
    }
}

/// True when nothing can be said against a value of this type: it is
/// not known, or it is `any`, whose typing is for a later pass
/// (TYPE-8).
fn open(ty: &Type) -> bool {
    matches!(ty, Type::Unknown | Type::Scalar(Scalar::Any))
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

/// A type once generic arguments are put in place of the parameters
/// declared at `own`: not known when it still names one of them, which
/// has then been given no argument.
fn placed(ty: Type, own: &[Location]) -> Type {
    if mentions(&ty, own) {
        Type::Unknown
    } else {
        ty
    }
}

/// True when `ty` names one of the generic parameters declared at
/// `own`: as a type, or as a value in a size or a constant argument.
fn mentions(ty: &Type, own: &[Location]) -> bool {
    names_one_of(ty, own) || names_value_of(ty, own)
}

/// True when a size or a constant argument inside `ty` is a generic
/// value parameter declared at one of `own`: the `N` of `list<i64, N>`.
fn names_value_of(ty: &Type, own: &[Location]) -> bool {
    let named = |constant: &Constant| matches!(&constant.value, ConstantValue::Parameter { at, .. } if own.contains(at));
    let inner = |ty: &Type| names_value_of(ty, own);
    match ty {
        Type::Named { arguments, .. } => arguments.iter().any(|argument| match argument {
            Argument::Type(ty) => inner(ty),
            Argument::Constant(constant) => named(constant),
        }),
        Type::Tuple(elements) => elements.iter().any(inner),
        Type::List { element, size } => size.as_ref().is_some_and(named) || inner(element),
        Type::Set(ty) | Type::Ref(ty) | Type::Atomic(ty) | Type::Delta(ty) | Type::Nullable(ty) => {
            inner(ty)
        }
        Type::Dict { key, value } => inner(key) || inner(value),
        Type::Rolling {
            value,
            maximum,
            minimum,
        } => named(maximum) || minimum.as_ref().is_some_and(named) || inner(value),
        Type::Result { value, error } => inner(value) || inner(error),
        Type::Scalar(_) | Type::Parameter { .. } | Type::Signal | Type::Error | Type::Unknown => {
            false
        }
    }
}

/// What is expected of an expression that takes its type from its
/// place, `node`, beside an operand of type `other`: for `.member`, the
/// operand's type; for `null`, the operand's type when it is nullable.
fn beside(node: &SyntaxNode, other: &Type) -> Type {
    if is_implied_member(node) || matches!(other, Type::Nullable(_)) {
        other.clone()
    } else {
        Type::Unknown
    }
}

/// True when an expression is the literal `null`, in parentheses or not.
fn is_null(node: &SyntaxNode) -> bool {
    takes_its_type(node) && !is_implied_member(node)
}

/// EXPR-5: "An integer literal is an `f64` where one is expected." True
/// when an expression is one, under a minus or not.
fn is_integer_literal(value: &Expr) -> bool {
    match &value.kind {
        ExprKind::Literal(Value::Int(_)) => true,
        ExprKind::Unary {
            operator: UnaryOperator::Negate,
            operand,
        } => is_integer_literal(operand),
        _ => false,
    }
}

/// True when an expression is `.member`, in parentheses or not.
fn is_implied_member(node: &SyntaxNode) -> bool {
    match node.kind() {
        SyntaxKind::ParenExpr => node
            .children()
            .next()
            .is_some_and(|inner| is_implied_member(&inner)),
        SyntaxKind::ImpliedMember => true,
        _ => false,
    }
}

/// True when an expression takes its type from its place and has none
/// of its own: `null`, and `.member`, in parentheses or not.
fn takes_its_type(node: &SyntaxNode) -> bool {
    match node.kind() {
        SyntaxKind::ParenExpr => node
            .children()
            .next()
            .is_some_and(|inner| takes_its_type(&inner)),
        SyntaxKind::ImpliedMember => true,
        SyntaxKind::Literal => tokens(node).any(|token| token.kind() == SyntaxKind::KwNull),
        _ => false,
    }
}

/// True when two types are applications of one struct, whatever their
/// generic arguments.
fn same_declaration(left: &Type, right: &Type) -> bool {
    match (left, right) {
        (
            Type::Named {
                module,
                declaration,
                ..
            },
            Type::Named {
                module: other_module,
                declaration: other_declaration,
                ..
            },
        ) => (module, declaration) == (other_module, other_declaration),
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
