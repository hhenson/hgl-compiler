//! The read of a collection's child, and the endpoint a child is. Part
//! of the typing pass ([`super`]): a node's hooks read a temporal
//! input's children here.
//!
//! TIME-1: "A child of a collection (TIME-9), an element, a field or an
//! entry, is an endpoint whether or not it is valid: `parts[1]` is given
//! to `valid` and passed on as a connection while it is invalid. In a
//! hook, a read of a child's value, or of its `delta_value`, requires
//! that child's admission: `all_valid(parts)` admits every child,
//! `valid(parts[1])` or `valid(quote.bid)` that one, and `valid(parts)`
//! none (TIME-4). Where the read names its child when the program is
//! compiled, at a constant index or a field, one that is not admitted
//! rejects (`payload.unguarded`). Elsewhere, `parts[i]`, it is checked
//! when it runs: a child that is not valid fails it (`value.unset_read`),
//! or gives an `err` where its place takes a `result` (ERR-14)."
//!
//! A child's `delta_value` is an endpoint operation on it, and is judged
//! where the operation is read ([`Typer::child_operation`]).

use hgl_diagnostics::{Category, Location};
use hgl_parser::{SyntaxKind, SyntaxNode};

use super::{BOOL, Fact, Index, Typer, constant_index, not_null, nullable, tokens};
use crate::hir::{EndpointOperation, Expr, ExprKind, Hook, LocalId, Value};
use crate::types::{ConstantValue, Detail, NamedKind, Scalar, Type};
use crate::{codes, error};

/// A child of a temporal input that the program names when it is
/// compiled: the input, and each step from it to the child in turn.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ChildPath {
    /// The temporal input.
    pub(super) input: LocalId,
    /// The steps from it, outermost first. None for the input itself.
    pub(super) steps: Vec<Step>,
}

impl ChildPath {
    /// The collection the child is of: the path less its last step.
    fn parent(&self) -> ChildPath {
        let mut steps = self.steps.clone();
        steps.pop();
        ChildPath {
            input: self.input,
            steps,
        }
    }
}

/// One step from a collection to a child of it, named as a constant.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Step {
    /// A struct's field, by its position among the struct's members.
    Field(usize),
    /// A list's or a tuple's position, or a dictionary's integer key.
    Position(i64),
    /// A dictionary's key that is a literal of another kind.
    Key(Value),
    /// A dictionary's key that is a member of an enum: its module, its
    /// declaration and its position among the members.
    Member(usize, usize, usize),
}

/// What a child of a temporal input is, as the program names it.
pub(super) enum Child {
    /// Named when the program is compiled: each step is a field or a
    /// constant.
    Named(ChildPath),
    /// Known only when the program runs: a step's index is no
    /// constant, `parts[i]`.
    Runtime,
}

/// True where the temporal form of `ty` has children valid on their own
/// (TIME-9): "a list, a tuple, a struct and a dictionary". An
/// `atomic<V>` holds whole values, and has none (TYPE-9).
fn collection(ty: &Type) -> bool {
    matches!(
        ty,
        Type::List { .. }
            | Type::Tuple(_)
            | Type::Dict { .. }
            | Type::Named {
                kind: NamedKind::Struct,
                ..
            }
    )
}

impl Typer<'_> {
    /// True in a hook that reads a temporal input's payload where it is
    /// admitted: a `when`, and `stop` (TIME-4, TIME-5). `start` reads no
    /// input at all, and a `state`'s or a `cache`'s initializer none
    /// either, which is reported where the input is named.
    pub(super) fn reads_children(&self) -> bool {
        matches!(self.hook, Some(Hook::When | Hook::Stop)) && !self.initializing && !self.caching
    }

    /// The child of a temporal input of the node that `value` is: an
    /// element, a field or an entry, through collections alone and at
    /// any depth. `None` for anything else, the input itself among them.
    pub(super) fn child_of(&self, value: &Expr) -> Option<Child> {
        let (_, _, steps) = self.steps_of(value)?;
        match steps {
            Some(steps) if steps.is_empty() => None,
            Some(steps) => {
                let input = self.input_of(value)?;
                Some(Child::Named(ChildPath { input, steps }))
            }
            None => Some(Child::Runtime),
        }
    }

    /// The temporal input `value` is rooted at, through fields and
    /// indices.
    fn input_of(&self, value: &Expr) -> Option<LocalId> {
        self.steps_of(value).map(|(input, _, _)| input)
    }

    /// The input `value` is rooted at, where it is written, and the
    /// steps from it, `None` where one is known only when the program
    /// runs.
    fn steps_of(&self, value: &Expr) -> Option<(LocalId, Location, Option<Vec<Step>>)> {
        match &value.kind {
            ExprKind::Local(local) if self.is_temporal_input(*local) => {
                Some((*local, value.at, Some(Vec::new())))
            }
            ExprKind::Field { base, member }
                if matches!(
                    base.ty,
                    Type::Named {
                        kind: NamedKind::Struct,
                        ..
                    }
                ) =>
            {
                let (input, at, steps) = self.steps_of(base)?;
                Some((
                    input,
                    at,
                    steps.map(|mut steps| {
                        steps.push(Step::Field(*member));
                        steps
                    }),
                ))
            }
            ExprKind::Index { base, index } if collection(&base.ty) => {
                let (input, at, steps) = self.steps_of(base)?;
                let step = self.step(&base.ty, index);
                let steps = match (steps, step) {
                    (Some(mut steps), Some(step)) => {
                        steps.push(step);
                        Some(steps)
                    }
                    _ => None,
                };
                Some((input, at, steps))
            }
            _ => None,
        }
    }

    /// The step `index` of a collection of type `of` names, where it is
    /// a constant: a literal, a module's constant (UNIT-13), or literals
    /// under a minus or joined by arithmetic, as a tuple's index may be
    /// (EXPR-7). `None` where it is known only when the program runs:
    /// one that reads a local, a parameter, a `const` parameter among
    /// them, or a call.
    fn step(&self, of: &Type, index: &Expr) -> Option<Step> {
        let constant = |module: usize, declaration: usize| match crate::read::constant_value(
            self.program,
            module,
            declaration,
            &self.constant_values,
        ) {
            ConstantValue::Int(value) => Index::Known(value),
            _ => Index::Unevaluated,
        };
        if let Index::Known(value) = constant_index(index, &constant) {
            return Some(Step::Position(value));
        }
        if !matches!(of, Type::Dict { .. }) {
            return None;
        }
        match &index.kind {
            ExprKind::Literal(Value::Null) => None,
            ExprKind::Literal(value) => Some(Step::Key(value.clone())),
            ExprKind::EnumMember {
                module,
                declaration,
                member,
            } => Some(Step::Member(*module, *declaration, *member)),
            _ => None,
        }
    }

    /// True where `node`, of type `ty`, is the collection a child is
    /// read from: `parts` of `parts[1]`, `quote` of `quote.bid`. What is
    /// read is then the child, and that read is judged where the child
    /// is (`child_read`): the collection is not read whole.
    pub(super) fn continues_chain(&self, node: &SyntaxNode, ty: &Type) -> bool {
        let Some(parent) = node.parent() else {
            return false;
        };
        if parent.children().next().as_ref() != Some(node) {
            return false;
        }
        match parent.kind() {
            SyntaxKind::IndexExpr => {
                matches!(ty, Type::List { .. } | Type::Tuple(_) | Type::Dict { .. })
            }
            SyntaxKind::MemberExpr => {
                let Type::Named {
                    module,
                    declaration,
                    kind: NamedKind::Struct,
                    ..
                } = ty
                else {
                    return false;
                };
                let Some(Detail::Struct(read)) =
                    self.program.modules[*module].details.get(*declaration)
                else {
                    return false;
                };
                // The member's name is the last token (LEX-9).
                tokens(&parent).last().is_some_and(|name| {
                    read.members.iter().any(|member| member.name == name.text())
                })
            }
            _ => false,
        }
    }

    /// `value`, written at `node`, as a read in a hook: where it reads a
    /// child of a temporal input, the child's admission is judged.
    ///
    /// TIME-1: "Where the read names its child when the program is
    /// compiled, at a constant index or a field, one that is not
    /// admitted rejects (`payload.unguarded`). Elsewhere, `parts[i]`, it
    /// is checked when it runs", which is what
    /// [`ExprKind::CheckedChild`] records. TIME-4: "A child's guard is a
    /// test of `all_valid` of its input, or of `valid` of the child as
    /// the read names it, `valid(parts[1])` for `parts[1]`".
    pub(super) fn child_read(&mut self, node: &SyntaxNode, value: Expr) -> Expr {
        if !self.reads_children() || self.continues_chain(node, &value.ty) {
            return value;
        }
        // A child named as an endpoint, `parts[1]` of `valid(parts[1])`,
        // is not read.
        let named = self
            .steps_of(&value)
            .is_some_and(|(_, at, _)| self.naming == Some(at));
        if named {
            return value;
        }
        match self.child_of(&value) {
            None => value,
            Some(Child::Runtime) => {
                let (ty, at) = (value.ty.clone(), value.at);
                Expr {
                    kind: ExprKind::CheckedChild {
                        read: Box::new(value),
                        result: false,
                    },
                    ty,
                    at,
                }
            }
            Some(Child::Named(path)) => {
                self.admit_child(path, node, value.at);
                value
            }
        }
    }

    /// Reports the read of the child `path`, written at `node` and
    /// standing at `at`, where nothing admits it (TIME-1: "one that is
    /// not admitted rejects (`payload.unguarded`)").
    fn admit_child(&mut self, path: ChildPath, node: &SyntaxNode, at: Location) {
        let admitted = self.present.contains(&Fact::ChildValid(path.clone()))
            || self.present.contains(&Fact::AllValid(path.parent()));
        if admitted {
            return;
        }
        let written = node.text().to_string();
        let parent = node
            .children()
            .next()
            .map_or_else(String::new, |base| base.text().to_string());
        self.diagnostics.emit(
            error(
                Category::Phase,
                codes::PAYLOAD_UNGUARDED,
                at,
                format!(
                    "`{written}` may not be valid here: a child is read where it is admitted, \
                     and nothing admits this one"
                ),
                "TIME-1",
            )
            .with_note(format!(
                "`all_valid({parent})` admits every child, and `valid({written})` this one; \
                 `valid({parent})` is true once any child is, and admits none"
            )),
        );
        // One report for one child on a path.
        self.present.push(Fact::ChildValid(path));
    }

    /// The child an endpoint operation, or an `endpoint<T>` parameter,
    /// is given at `node`, where it is written as one: an element, a
    /// field or an entry of a temporal input of the node, `parts[1]`,
    /// `quote.bid` or `parts[i]` (TIME-9). `None` where it is not so
    /// written, and nothing has been read. `Some(None)` where it is so
    /// written and is no child, `quote.bid` of an `atomic<Quote>`: what
    /// it reads has been read and reported, and the caller reports that
    /// it is no endpoint where nothing else has been.
    ///
    /// The child is named and not read: an index in it is read, as any
    /// value is.
    pub(super) fn child_endpoint(&mut self, node: &SyntaxNode) -> Option<Option<Expr>> {
        // "Parentheses select grouping" (EXPR-1).
        let mut named = node.clone();
        while named.kind() == SyntaxKind::ParenExpr
            && let Some(inner) = named.children().next()
        {
            named = inner;
        }
        if !matches!(named.kind(), SyntaxKind::IndexExpr | SyntaxKind::MemberExpr) {
            return None;
        }
        let mut root = named.clone();
        while matches!(root.kind(), SyntaxKind::IndexExpr | SyntaxKind::MemberExpr) {
            root = root.children().next()?;
        }
        if root.kind() != SyntaxKind::NameExpr {
            return None;
        }
        let local = self.local_of(&root)?;
        if !self.is_temporal_input(local) {
            return None;
        }
        let before = self.naming.replace(self.at(&root));
        let child = self.expression(&named, None);
        self.naming = before;
        Some(self.child_of(&child).is_some().then_some(child))
    }

    /// The operation `operation`, written at `at`, on the child `given`
    /// names (TIME-9, END-8): "A child of a collection is an endpoint
    /// whether or not it is valid: `valid`, `modified`, `delta_value`
    /// and `last_modified` take it, and `all_valid` where it is a
    /// collection". `found` is what [`Typer::child_endpoint`] found.
    pub(super) fn child_operation(
        &mut self,
        operation: EndpointOperation,
        at: Location,
        given: &SyntaxNode,
        found: Option<Expr>,
        reported: usize,
    ) -> (ExprKind, Type) {
        let ty = match operation {
            EndpointOperation::LastModified => nullable(Type::Scalar(Scalar::Datetime)),
            EndpointOperation::DeltaValue => Type::Unknown,
            _ => BOOL,
        };
        let Some(child) = found else {
            // What it reads has been reported, where it is wrong.
            if self.diagnostics.len() == reported {
                self.diagnostics.emit(
                    error(
                        Category::Phase,
                        codes::ENDPOINT_ARGUMENT,
                        self.at(given),
                        format!(
                            "`{}` takes a temporal input of `{}`, or a child of one: an \
                             element, a field or an entry of a list, a tuple, a struct or a \
                             dictionary",
                            operation.name(),
                            self.declaration.name
                        ),
                        "TIME-9",
                    )
                    .with_note("an `atomic<V>` holds whole values, and has no child"),
                );
            }
            return (ExprKind::Unread, ty);
        };
        self.child_in_start(at, &child);
        // TIME-9: "`all_valid` takes only a collection; another endpoint
        // is rejected (`endpoint.argument`)".
        if operation == EndpointOperation::AllValid
            && !collection(&child.ty)
            && !matches!(child.ty, Type::Parameter { .. } | Type::Unknown)
        {
            self.diagnostics.emit(
                error(
                    Category::Phase,
                    codes::ENDPOINT_ARGUMENT,
                    child.at,
                    format!("`all_valid` takes a collection, and `{}` is none", child.ty),
                    "TIME-9",
                )
                .with_note("a collection is a list, a tuple, a struct or a dictionary"),
            );
        }
        // TIME-1: "a read of a child's value, or of its `delta_value`,
        // requires that child's admission". One known only when it runs
        // is checked then.
        if operation == EndpointOperation::DeltaValue
            && let Some(path) = self.tested_child(&child)
        {
            let mut named = given.clone();
            while named.kind() == SyntaxKind::ParenExpr
                && let Some(inner) = named.children().next()
            {
                named = inner;
            }
            self.admit_child(path, &named, child.at);
        }
        let ty = match operation {
            // END-3: "`delta_value(x)` returns what `x` published this
            // cycle, a `delta<T>` for an endpoint of T". TYPE-16: "`ref<V>`,
            // `signal`: None".
            EndpointOperation::DeltaValue => {
                if matches!(not_null(Some(&child.ty)), Some(Type::Ref(_) | Type::Signal)) {
                    self.diagnostics.emit(
                        error(
                            Category::Shape,
                            codes::DELTA_UNSUPPORTED_SHAPE,
                            at,
                            format!("`{}` has no delta", child.ty),
                            "TYPE-16",
                        )
                        .with_note(
                            "a `ref` designates an endpoint and a `signal` has no payload: \
                             neither publishes a value of its own",
                        ),
                    );
                    Type::Unknown
                } else {
                    Type::delta(child.ty.clone())
                }
            }
            _ => ty,
        };
        (
            ExprKind::ChildEndpoint {
                operation,
                child: Box::new(child),
            },
            ty,
        )
    }

    /// Reports `child`, given to an endpoint at `at`, where that is in
    /// `start` (TIME-5: `start` "gives none to an endpoint operation":
    /// it runs before any input has a value).
    pub(super) fn child_in_start(&mut self, at: Location, child: &Expr) {
        if self.hook == Some(Hook::Start)
            && let Some(input) = self.input_of(child)
        {
            let name = self.locals[input].name.clone();
            self.input_lifecycle(at, &name);
        }
    }

    /// The child a test of `child`, an endpoint operation's argument,
    /// proves valid, where it is named when the program is compiled.
    pub(super) fn tested_child(&self, child: &Expr) -> Option<ChildPath> {
        match self.child_of(child)? {
            Child::Named(path) => Some(path),
            Child::Runtime => None,
        }
    }
}

/// What a test of `valid` or `all_valid` of a child proves where it
/// holds: the child is valid, which admits its read (TIME-1), and so is
/// the input it is of, a collection valid "where any element, field or
/// entry is valid" (TIME-9). `all_valid` also admits each of the child's
/// own children.
pub(super) fn child_facts(operation: EndpointOperation, path: ChildPath) -> Vec<Fact> {
    let mut facts = vec![Fact::Valid(path.input), Fact::ChildValid(path.clone())];
    if operation == EndpointOperation::AllValid {
        facts.push(Fact::AllValid(path));
    }
    facts
}

/// What `all_valid` of a temporal input proves where it holds: each of
/// its children is valid, "`all_valid(parts)` admits every child"
/// (TIME-1).
pub(super) fn all_valid_fact(input: LocalId) -> Fact {
    Fact::AllValid(ChildPath {
        input,
        steps: Vec::new(),
    })
}
