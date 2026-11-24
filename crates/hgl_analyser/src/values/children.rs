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

use super::fit::{not_null, nullable, window_value};
use super::node::RefRead;
use super::{BOOL, Fact, Typer};
use crate::hir::{
    EndpointOperation, Expr, ExprKind, Hook, LabelValue, LocalId, PackEndpointSelection, PackKey,
    ValueCategory,
};
use crate::syntax::{tokens, ungrouped};
use crate::types::{ConstantValue, Detail, NamedKind, Scalar, Type};
use crate::{codes, error};

/// A child of a temporal input that the program names when it is
/// compiled: the input, and each step from it to the child in turn.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ChildPath {
    /// The temporal input.
    pub(super) input: LocalId,
    /// Which physical input of a pack the path starts at.
    entry: Option<PackKey>,
    /// The steps from it, outermost first. None for the input itself.
    pub(super) steps: Vec<Step>,
}

impl ChildPath {
    /// The root of one selected input, rather than the whole pack.
    pub(super) fn pack_entry(input: LocalId, entry: PackKey) -> Self {
        Self {
            input,
            entry: Some(entry),
            steps: Vec::new(),
        }
    }

    /// The collection the child is of: the path less its last step.
    fn parent(&self) -> ChildPath {
        let mut steps = self.steps.clone();
        steps.pop();
        ChildPath {
            input: self.input,
            entry: self.entry.clone(),
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
    /// A dictionary's key of another kind, by its value: a string, a
    /// bool, a float or a member of the key's enum.
    Key(LabelValue),
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

/// The input and path captured by an inferred `let` (ERR-14). A path
/// with a runtime index still has a known root.
#[derive(Clone)]
pub(super) struct ChildOrigin {
    input: LocalId,
    entry: Option<PackKey>,
    steps: Option<Vec<Step>>,
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

/// True where a value of `ty`, a temporal input or a child of one, is
/// read whole only where its `all_valid` holds. TIME-1: "A read of a
/// struct, a tuple or a fixed list whole, `return quote`, requires
/// `all_valid` of it ... A dictionary and an unsized list hold what they
/// hold: a read of one whole requires its `valid` and no more." An
/// `atomic<V>` holds whole values, and has no child (TYPE-9).
pub(super) fn read_whole_all_valid(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Tuple(_)
            | Type::List { size: Some(_), .. }
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
        let ChildOrigin {
            input,
            entry,
            steps,
        } = self.child_origin(value)?;
        match steps {
            Some(steps) => Some(Child::Named(ChildPath {
                input,
                entry,
                steps,
            })),
            None => Some(Child::Runtime),
        }
    }

    /// The origin of an endpoint binding, if the initializer names a
    /// child. A direct input is read as usual.
    pub(super) fn child_origin(&self, value: &Expr) -> Option<ChildOrigin> {
        let (origin, _) = self.steps_of(value)?;
        if origin.steps.as_ref().is_some_and(Vec::is_empty) {
            return None;
        }
        Some(origin)
    }

    /// The temporal input `value` is rooted at, through fields and
    /// indices.
    pub(super) fn input_of(&self, value: &Expr) -> Option<LocalId> {
        self.steps_of(value).map(|(origin, _)| origin.input)
    }

    /// The input `value` is rooted at, where it is written, and the
    /// steps from it, `None` where one is known only when the program
    /// runs.
    fn steps_of(&self, value: &Expr) -> Option<(ChildOrigin, Location)> {
        match &value.kind {
            // A `T?` is no collection, though a test against null has
            // refined the read to its T (ERR-12): it publishes whole.
            ExprKind::Local(local)
                if self.is_temporal_input(*local) && collection(&self.locals[*local].ty) =>
            {
                Some((
                    ChildOrigin {
                        input: *local,
                        entry: None,
                        steps: Some(Vec::new()),
                    },
                    value.at,
                ))
            }
            ExprKind::PackEntry { .. } | ExprKind::DeferredPackRead => {
                self.packed_child_root(value)
            }
            ExprKind::BoundChild(local) => self
                .bound_children
                .get(local)
                .map(|origin| (origin.clone(), value.at)),
            ExprKind::Field { base, member }
                if matches!(
                    base.ty,
                    Type::Named {
                        kind: NamedKind::Struct,
                        ..
                    }
                ) =>
            {
                let (mut origin, at) = self.steps_of(base)?;
                if let Some(steps) = &mut origin.steps {
                    steps.push(Step::Field(*member));
                }
                Some((origin, at))
            }
            // A window is no collection (TIME-9): its value is read as a
            // list (TYPE-11), and a position of that list is no child.
            ExprKind::Index { base, index } if collection(&base.ty) && !self.reads_window(base) => {
                let (mut origin, at) = self.steps_of(base)?;
                let step = self.step(&base.ty, index);
                origin.steps = match (origin.steps, step) {
                    (Some(mut steps), Some(step)) => {
                        steps.push(step);
                        Some(steps)
                    }
                    _ => None,
                };
                Some((origin, at))
            }
            _ => None,
        }
    }

    /// A structural entry has children of its own (FN-19, TIME-1).
    /// Use the input's endpoint type before any hook payload projection.
    fn packed_child_root(&self, value: &Expr) -> Option<(ChildOrigin, Location)> {
        let written = self.syntax_of(value)?;
        let mut parts = written.children();
        let root = ungrouped(parts.next()?);
        let input = self.local_of(&root)?;
        let (key, ty) = match &value.kind {
            ExprKind::PackEntry { pack, key } => {
                (key.clone(), self.packed_input_type(*pack, Some(key))?)
            }
            ExprKind::DeferredPackRead => {
                // A homogeneous pack can already expose its child shape
                // where the declaration is read, before any call.
                let ty = self.packed_input_type(input, None)?;
                let index = self.read_constant(&parts.next()?, None).ok()?;
                let key = match index {
                    ConstantValue::Int(position) => {
                        PackKey::Position(usize::try_from(position).ok()?)
                    }
                    ConstantValue::Str(name) => PackKey::Name(name),
                    _ => return None,
                };
                (key, ty)
            }
            _ => return None,
        };
        collection(ty).then(|| {
            (
                ChildOrigin {
                    input,
                    entry: Some(key),
                    steps: Some(Vec::new()),
                },
                self.at(&root),
            )
        })
    }

    /// The selected temporal entry's endpoint type before a hook
    /// projects it to its payload (TYPE-9, TYPE-11).
    pub(super) fn packed_input_type(&self, pack: LocalId, key: Option<&PackKey>) -> Option<&Type> {
        if !self.is_temporal_input(pack) {
            return None;
        }
        match key {
            Some(key) => self
                .pack_inputs
                .as_ref()?
                .iter()
                .find(|entry| entry.parameter == pack && entry.key == *key)
                .map(|entry| &entry.ty),
            None => Some(&self.signature.parameters.get(pack)?.ty),
        }
    }

    fn packed_read_type(&self, value: &Expr) -> Option<&Type> {
        match &value.kind {
            ExprKind::PackEntry { pack, key } => self.packed_input_type(*pack, Some(key)),
            ExprKind::DeferredPackRead => {
                let base = ungrouped(self.syntax_of(value)?.children().next()?);
                self.packed_input_type(self.local_of(&base)?, None)
            }
            _ => None,
        }
    }

    /// True where `value` reads a child that is a window, of an input
    /// or of `out`: a field, a position or an entry whose declared type
    /// is a window, whatever its read is typed as (TYPE-11: a list).
    fn reads_window(&self, value: &Expr) -> bool {
        let window = |ty: &Type| matches!(ty, Type::RollingWindow { .. });
        match &value.kind {
            ExprKind::BoundChild(local) => matches!(
                &self.locals[*local].ty,
                Type::Endpoint(payload) if window(payload)
            ),
            ExprKind::Field { base, member } => self
                .program
                .field_types(&base.ty)
                .and_then(|fields| fields.get(*member).cloned())
                .is_some_and(|field| window(&field)),
            ExprKind::Index { base, index } => match &base.ty {
                Type::List { element: child, .. } | Type::Dict { value: child, .. } => {
                    window(child)
                }
                Type::Tuple(elements) => match self.step(&base.ty, index) {
                    Some(Step::Position(position)) => usize::try_from(position)
                        .ok()
                        .and_then(|position| elements.get(position))
                        .is_some_and(window),
                    _ => false,
                },
                _ => false,
            },
            _ => false,
        }
    }

    /// True where `value` is the read, in a hook, of a window's value:
    /// a temporal input or `out` whose type is a window, or a child of
    /// either named when the program is compiled that is one, each read
    /// as the `list<V>` it holds (TYPE-11). One known only when it runs
    /// is a [`ExprKind::CheckedChild`].
    pub(super) fn window_read(&self, value: &Expr) -> bool {
        if !self.reads_children() {
            return false;
        }
        match &value.kind {
            ExprKind::Local(local) => {
                (self.is_temporal_input(*local) || Some(*local) == self.out)
                    && matches!(
                        self.locals[*local].ty.omission_removed(),
                        Type::RollingWindow { .. }
                    )
            }
            ExprKind::PackEntry { .. } | ExprKind::DeferredPackRead => self
                .packed_read_type(value)
                .is_some_and(|ty| matches!(ty.omission_removed(), Type::RollingWindow { .. })),
            ExprKind::Field { .. } | ExprKind::Index { .. } | ExprKind::BoundChild(_) => {
                self.reads_window(value)
                    && (self.rooted_at_out(value)
                        || matches!(self.child_of(value), Some(Child::Named(_))))
            }
            _ => false,
        }
    }

    /// A whole read admitted by TIME-1 whose contents can still have a
    /// gap or a partial dictionary entry (ERR-14).
    pub(super) fn checked_whole_read(&self, value: &Expr) -> bool {
        let can_be_partial =
            |ty: &Type| matches!(ty, Type::Dict { .. } | Type::List { size: None, .. });
        if !self.reads_children() || !can_be_partial(&value.ty) {
            return false;
        }
        match &value.kind {
            ExprKind::Local(local) => {
                (self.is_temporal_input(*local) || Some(*local) == self.out)
                    && can_be_partial(self.locals[*local].ty.omission_removed())
            }
            ExprKind::PackEntry { .. } | ExprKind::DeferredPackRead => self
                .packed_read_type(value)
                .is_some_and(|ty| can_be_partial(ty.omission_removed())),
            ExprKind::Field { .. } | ExprKind::Index { .. } | ExprKind::BoundChild(_) => {
                self.rooted_at_out(value) || matches!(self.child_of(value), Some(Child::Named(_)))
            }
            _ => false,
        }
    }

    /// True where `value` is `out` or a child of it, through fields and
    /// indices (TIME-7).
    fn rooted_at_out(&self, value: &Expr) -> bool {
        match &value.kind {
            ExprKind::Local(local) => Some(*local) == self.out,
            ExprKind::Field { base, .. } | ExprKind::Index { base, .. } => self.rooted_at_out(base),
            _ => false,
        }
    }

    /// The step `index` of a collection of type `of` names, where it is
    /// a constant: a literal, a module's constant (UNIT-13), or literals
    /// under a minus or joined by arithmetic, as a tuple's index may be
    /// (EXPR-7). `None` where it is known only when the program runs:
    /// one that reads a local, a parameter, a `const` parameter among
    /// them, or a call.
    fn step(&self, of: &Type, index: &Expr) -> Option<Step> {
        // The value the index comes to, read as every constant is
        // (`crate::constant`), as a value of the key's type.
        let key = match of {
            Type::Dict { key, .. } => (**key).clone(),
            _ => Type::Scalar(Scalar::I64),
        };
        let written = self.syntax_of(index)?;
        let value = self.read_constant(&written, Some(&key)).ok()?;
        match super::label(self.program, &value, &key)? {
            LabelValue::Int(value) => Some(Step::Position(value)),
            key if matches!(of, Type::Dict { .. }) => Some(Step::Key(key)),
            _ => None,
        }
    }

    /// True where `node`, of type `ty`, is the collection a child is
    /// read from: `parts` of `parts[1]`, `quote` of `quote.bid`. What is
    /// read is then the child, and that read is judged where the child
    /// is (`child_read`): the collection is not read whole.
    pub(super) fn continues_chain(&self, node: &SyntaxNode, ty: &Type) -> bool {
        // "Parentheses select grouping" (EXPR-1): `(parts)[1]` reads a
        // child of `parts` as `parts[1]` does.
        let mut current = node.clone();
        let mut parent = node.parent();
        while let Some(grouping) = parent
            .as_ref()
            .filter(|parent| parent.kind() == SyntaxKind::ParenExpr)
        {
            current = grouping.clone();
            parent = grouping.parent();
        }
        let Some(parent) = parent else {
            return false;
        };
        if parent.children().next().as_ref() != Some(&current) {
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
            .is_some_and(|(_, at)| self.naming == Some(at));
        if named || (self.binding_child == Some(value.at) && self.child_origin(&value).is_some()) {
            return value;
        }
        // FN-16: "A `ref` parameter and its children, `xs[i]` and
        // `quote.bid`, are endpoints, not values ... A read of the value
        // ... of one rejects". EXPR-7: "A structural projection of a
        // `ref` parameter selects its child and reads no payload": what
        // is read here is the child's value.
        if let Some(input) = self
            .input_of(&value)
            .filter(|&input| self.is_reference(input))
            && self.child_of(&value).is_some()
        {
            let written = node.text().to_string();
            self.ref_read(input, Some(&written), RefRead::Value, value.at);
            return value;
        }
        // TYPE-11: a window's value, "read in a hook ... is a `list<V>` of
        // the values held, oldest first", a child's among them. TIME-7:
        // "`inject out` exposes that same result for reads": a window
        // that is a child of `out` is read as its list too, and fails
        // when it runs where it is not all valid, as an input's does.
        if matches!(value.ty, Type::RollingWindow { .. }) && self.rooted_at_out(&value) {
            return Expr {
                ty: window_value(&value.ty),
                ..value
            };
        }
        match self.child_of(&value) {
            None => value,
            Some(Child::Runtime) => {
                let (ty, at) = (window_value(&value.ty), value.at);
                Expr {
                    kind: ExprKind::CheckedChild {
                        read: Box::new(value),
                        result: false,
                    },
                    ty,
                    at,
                    category: ValueCategory::Unsettled,
                }
            }
            Some(Child::Named(path)) => {
                let whole = read_whole_all_valid(&value.ty);
                self.admit_child(path, node, value.at, whole);
                Expr {
                    ty: window_value(&value.ty),
                    ..value
                }
            }
        }
    }

    /// Reports the read of the child `path`, written at `node` and
    /// standing at `at`, where nothing admits it (TIME-1: "one that is
    /// not admitted rejects (`payload.unguarded`)"). Where `whole`, the
    /// child is a struct, a tuple or a fixed list read whole, which only
    /// its own `all_valid` admits (TIME-1: "`return rows[0]`, for `rows:
    /// list<Quote, 2>`, requires `all_valid(rows[0])`, which
    /// `all_valid(rows)`, one level deep, does not give").
    fn admit_child(&mut self, path: ChildPath, node: &SyntaxNode, at: Location, whole: bool) {
        let written = node.text().to_string();
        let parent = node.children().next();
        let all_valid_parent = parent.as_ref().map_or_else(
            || "`all_valid` of its parent".to_string(),
            |base| format!("`all_valid({})`", base.text()),
        );
        if whole {
            if self.present.contains(&Fact::AllValid(path.clone())) {
                return;
            }
            self.diagnostics.emit(
                error(
                    Category::Phase,
                    codes::PAYLOAD_UNGUARDED,
                    at,
                    format!(
                        "`{written}` is read whole here, and may not be all valid: a struct, a \
                         tuple or a fixed list is read whole where its `all_valid` holds"
                    ),
                    "TIME-1",
                )
                .with_note(format!(
                    "`all_valid({written})` admits it; {all_valid_parent} is one level deep, \
                     and `valid({written})` is true once any of its children is"
                )),
            );
            // One report for one child on a path.
            self.present.push(Fact::ChildValid(path.clone()));
            self.present.push(Fact::AllValid(path));
            return;
        }
        let admitted = self.present.contains(&Fact::ChildValid(path.clone()))
            || self.present.contains(&Fact::AllValid(path.parent()));
        if admitted {
            return;
        }
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
            .with_note(parent.map_or_else(
                || {
                    format!(
                        "{all_valid_parent} admits every child, and `valid({written})` this one"
                    )
                },
                |base| {
                    format!(
                        "{all_valid_parent} admits every child, and `valid({written})` this one; \
                         `valid({})` is true once any child is, and admits none",
                        base.text()
                    )
                },
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
        let named = super::endpoint_argument(node);
        let mut root = named.clone();
        while matches!(
            root.kind(),
            SyntaxKind::IndexExpr | SyntaxKind::MemberExpr | SyntaxKind::ParenExpr
        ) {
            root = root.children().next()?;
        }
        if root.kind() != SyntaxKind::NameExpr {
            return None;
        }
        let local = self.local_of(&root)?;
        let projection = matches!(named.kind(), SyntaxKind::IndexExpr | SyntaxKind::MemberExpr);
        if !self.bound_children.contains_key(&local)
            && !(projection && self.is_temporal_input(local))
        {
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
        // TIME-9: "`all_valid` takes only a collection or a window;
        // another endpoint is rejected (`endpoint.argument`)".
        if operation == EndpointOperation::AllValid
            && !collection(&child.ty)
            && !matches!(
                child.ty,
                Type::RollingWindow { .. } | Type::Parameter { .. } | Type::Unknown
            )
        {
            self.diagnostics.emit(
                error(
                    Category::Phase,
                    codes::ENDPOINT_ARGUMENT,
                    child.at,
                    format!(
                        "`all_valid` takes a collection or a window, and `{}` is neither",
                        child.ty
                    ),
                    "TIME-9",
                )
                .with_note(
                    "a collection is a list, a tuple, a struct or a dictionary; a window is a \
                     `rolling_window`",
                ),
            );
        }
        // FN-16: a child of a `ref` parameter is an endpoint, and "the
        // endpoint operations take them (TIME-9), `delta_value` aside".
        let reference = self
            .input_of(&child)
            .filter(|&input| self.is_reference(input));
        if operation == EndpointOperation::DeltaValue
            && let Some(input) = reference
        {
            let written = super::endpoint_argument(given).text().to_string();
            self.ref_read(input, Some(&written), RefRead::Delta, child.at);
            return (
                ExprKind::ChildEndpoint {
                    operation,
                    child: Box::new(child),
                },
                Type::Unknown,
            );
        }
        // TIME-1: "a read of a child's value, or of its `delta_value`,
        // requires that child's admission". One known only when it runs
        // is checked then.
        if operation == EndpointOperation::DeltaValue
            && let Some(path) = self.tested_child(&child)
        {
            let named = super::endpoint_argument(given);
            self.admit_child(path, &named, child.at, false);
        }
        let ty = match operation {
            // END-3: "`delta_value(x)` returns what `x` published this
            // cycle, a `delta<T>` for an endpoint of T". TYPE-16:
            // "`signal`: None".
            EndpointOperation::DeltaValue => {
                if matches!(not_null(Some(&child.ty)), Some(Type::Signal)) {
                    self.diagnostics.emit(
                        error(
                            Category::Shape,
                            codes::DELTA_UNSUPPORTED_SHAPE,
                            at,
                            format!("`{}` has no delta", child.ty),
                            "TYPE-16",
                        )
                        .with_note("a `signal` has no payload, and publishes no value of its own"),
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
    let mut facts = Vec::new();
    // A guard on one pack entry proves nothing about the other inputs.
    if path.entry.is_none() {
        facts.push(Fact::Valid(path.input));
    }
    facts.push(Fact::ChildValid(path.clone()));
    if operation == EndpointOperation::AllValid {
        facts.push(Fact::AllValid(path));
    }
    facts
}

/// What a direct-entry guard proves. An unread key defers admission to
/// the instance, without proving validity for the entire pack (GEN-7).
pub(super) fn pack_guard_facts(
    operation: EndpointOperation,
    entry: PackEndpointSelection,
) -> Vec<Fact> {
    match entry.key {
        Some(key) => child_facts(operation, ChildPath::pack_entry(entry.pack, key)),
        None => vec![Fact::DeferredPackGuard {
            pack: entry.pack,
            all_valid: operation == EndpointOperation::AllValid,
        }],
    }
}

/// What `all_valid` of a temporal input proves where it holds: each of
/// its children is valid, "`all_valid(parts)` admits every child"
/// (TIME-1).
pub(super) fn all_valid_fact(input: LocalId) -> Fact {
    Fact::AllValid(ChildPath {
        input,
        entry: None,
        steps: Vec::new(),
    })
}
