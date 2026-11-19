//! Where a value is copied.
//!
//! A `var`, a `state` and a `cache` are places: the list, the
//! dictionary or the struct that is no value (`emit::by_value`) one
//! holds is changed where it is, and never copied for a change. A `let`, a
//! parameter and a loop's name are references: each names a value
//! something else may hold. So a copy is made where a reference is
//! made to what a place holds and the place then changes, and where a
//! reference is kept. The owner's rule, on the compiler's issue 151:
//!
//! > all var mutations are mutations, not copies. The copies only occur
//! > when: 1. the value is passed to a function and the function also
//! > recieves a var reference to the item or items it contains,
//! > othewise they are safe to pass through. 2. let assignment as for
//! > any of the structures.
//!
//! and, of a `let` and a parameter: "It is only when the list could be
//! modified in the scope of the let / parameter that we need to make a
//! copy".
//!
//! This module reads one body and says, of each `let` and each loop,
//! whether the place its value is read from is changed while the name
//! is in scope. Where it is, the name is bound to a copy; where it is
//! not, the name is a reference into the place, and what is read
//! through it is read from that place.

use std::collections::{HashMap, HashSet};

use hgl_analyser::hir::{Body, Callee, Expr, ExprKind, Local, Node, Stmt, walk_statement_nodes};
use hgl_analyser::{Detail, LocalKind, Program};

use crate::emit::changed_in_place;

/// What one body's `let`s and loop names are bound to.
#[derive(Default)]
pub(crate) struct Copies {
    /// The `let`s, and the first name of each loop, that are bound to
    /// a copy: the place read is changed while the name is in scope.
    copied: HashSet<usize>,
    /// The names that are references into places, each with the places
    /// it may refer into: a `let` of what a place holds, and a loop's
    /// name over it, where the place is not changed in the name's
    /// scope.
    references: HashMap<usize, Vec<usize>>,
    /// The `let`s that hold a value of their own: one bound to what
    /// nothing else holds ([`fresh`]).
    own: HashSet<usize>,
}

impl Copies {
    /// Reads `body`.
    pub(crate) fn of(program: &Program, body: &Body) -> Copies {
        let mut copies = Copies::default();
        copies.block(program, &body.locals, &body.statements, &[]);
        copies
    }

    /// True where the `let` or the loop that binds `local` takes a
    /// copy of what it reads.
    pub(crate) fn copied(&self, local: usize) -> bool {
        self.copied.contains(&local)
    }

    /// True where `value` is the function's own, and is gone when the
    /// function returns: what it returns of it is then held by nothing
    /// else, and is returned as it is. It is read from a `var` the
    /// body declares, or through a `let` that holds a value of its
    /// own or refers into such a `var`. What is read from a parameter,
    /// a `var` one among them, is the caller's, and what is read from
    /// a `state` or a `cache` stays the node's.
    ///
    /// `parameters` is how many of the locals are parameters: they
    /// come first.
    pub(crate) fn own(&self, locals: &[Local], parameters: usize, value: &Expr) -> bool {
        let own_place = |local: usize| local >= parameters && locals[local].kind == LocalKind::Var;
        match &value.kind {
            ExprKind::Local(local) => {
                if is_place(locals, *local) {
                    own_place(*local)
                } else if self.copied.contains(local) || self.own.contains(local) {
                    true
                } else {
                    self.references
                        .get(local)
                        .is_some_and(|places| places.iter().all(|&place| own_place(place)))
                }
            }
            ExprKind::Field { base: inner, .. }
            | ExprKind::Index { base: inner, .. }
            | ExprKind::Widen(inner)
            | ExprKind::Payload { result: inner, .. } => self.own(locals, parameters, inner),
            ExprKind::Block {
                value: Some(last), ..
            } => self.own(locals, parameters, last),
            ExprKind::If {
                then, otherwise, ..
            } => {
                self.own(locals, parameters, then)
                    && otherwise
                        .as_ref()
                        .is_none_or(|otherwise| self.own(locals, parameters, otherwise))
            }
            _ => false,
        }
    }

    /// The places `value` is read from: the `var`, `state` or `cache`
    /// it names, or is a part of, or that a name it reads refers into.
    /// Empty for a value that is read from no place: one written out,
    /// what a call gives, a parameter.
    pub(crate) fn places(&self, program: &Program, locals: &[Local], value: &Expr) -> Vec<usize> {
        match &value.kind {
            ExprKind::Local(local) => {
                if is_place(locals, *local) {
                    vec![*local]
                } else {
                    self.references.get(local).cloned().unwrap_or_default()
                }
            }
            ExprKind::Field { base: inner, .. }
            | ExprKind::Index { base: inner, .. }
            | ExprKind::Widen(inner)
            | ExprKind::Payload { result: inner, .. } => self.places(program, locals, inner),
            ExprKind::Block {
                value: Some(value), ..
            } => self.places(program, locals, value),
            ExprKind::If {
                then, otherwise, ..
            } => {
                let mut places = self.places(program, locals, then);
                for other in otherwise
                    .iter()
                    .flat_map(|other| self.places(program, locals, other))
                {
                    if !places.contains(&other) {
                        places.push(other);
                    }
                }
                places
            }
            // A value written out holds what it is written with: a
            // struct made of a place's list refers into the place.
            ExprKind::Construct { arguments, .. } | ExprKind::Delta { arguments } => {
                self.all_places(program, locals, arguments.iter().map(|given| &given.value))
            }
            ExprKind::Sequence(entries) | ExprKind::Tuple(entries) => {
                self.all_places(program, locals, entries.iter())
            }
            ExprKind::Pairs(entries) => self.all_places(
                program,
                locals,
                entries.iter().flat_map(|(key, entry)| [key, entry]),
            ),
            ExprKind::Tagged { payload, .. } => self.places(program, locals, payload),
            // A native function of core's may give a part of what it
            // is given, an entry of a dictionary say: what it gives is
            // read from wherever its arguments are.
            ExprKind::Call {
                callee, arguments, ..
            } if !gives_its_own(program, callee) => {
                let mut places = Vec::new();
                for argument in arguments {
                    for place in self.places(program, locals, &argument.value) {
                        if !places.contains(&place) {
                            places.push(place);
                        }
                    }
                }
                places
            }
            _ => Vec::new(),
        }
    }

    /// The places any of `values` is read from, each once.
    fn all_places<'a>(
        &self,
        program: &Program,
        locals: &[Local],
        values: impl Iterator<Item = &'a Expr>,
    ) -> Vec<usize> {
        let mut places = Vec::new();
        for value in values {
            for place in self.places(program, locals, value) {
                if !places.contains(&place) {
                    places.push(place);
                }
            }
        }
        places
    }

    /// Reads the statements of one block. `loops` is the bodies of the
    /// loops the block is inside, the innermost last.
    fn block(
        &mut self,
        program: &Program,
        locals: &[Local],
        statements: &[Stmt],
        loops: &[&[Stmt]],
    ) {
        for (position, statement) in statements.iter().enumerate() {
            let rest = &statements[position + 1..];
            match statement {
                Stmt::Declare {
                    local,
                    value: Some(value),
                } => {
                    self.nested(program, locals, value, loops);
                    if is_place(locals, *local) {
                        continue;
                    }
                    let places = self.places(program, locals, value);
                    if places.is_empty() {
                        if fresh(program, value) {
                            self.own.insert(*local);
                        }
                        continue;
                    }
                    // The name is in scope to the end of its block,
                    // and again each time a loop it is inside comes
                    // round: a change anywhere in such a loop comes
                    // before a later read of the name's earlier
                    // binding only where the name is rebound first,
                    // which this does not follow.
                    let changed = places.iter().any(|&place| {
                        changes(program, locals, rest, place)
                            || loops
                                .iter()
                                .any(|body| changes(program, locals, body, place))
                    });
                    if changed {
                        self.copied.insert(*local);
                    } else {
                        self.references.insert(*local, places);
                    }
                }
                Stmt::For {
                    names,
                    sequence,
                    body,
                    ..
                } => {
                    self.nested(program, locals, sequence, loops);
                    let places = self.places(program, locals, sequence);
                    // A loop runs over the value its sequence had when
                    // the loop began: it takes a copy where its own
                    // body changes the place.
                    let changed = places
                        .iter()
                        .any(|&place| changes(program, locals, body, place));
                    if changed {
                        if let Some(first) = names.first() {
                            self.copied.insert(*first);
                        }
                    } else if !places.is_empty() {
                        for name in names {
                            self.references.insert(*name, places.clone());
                        }
                    }
                    let mut inside = loops.to_vec();
                    inside.push(body);
                    self.block(program, locals, body, &inside);
                }
                Stmt::While {
                    condition, body, ..
                } => {
                    if let Some(condition) = condition {
                        self.nested(program, locals, condition, loops);
                    }
                    let mut inside = loops.to_vec();
                    inside.push(body);
                    self.block(program, locals, body, &inside);
                }
                // A hook is evaluated on its own: a name of one is out
                // of scope before another runs.
                Stmt::Hook {
                    condition, body, ..
                } => {
                    if let Some(condition) = condition {
                        self.nested(program, locals, condition, &[]);
                    }
                    self.block(program, locals, body, &[]);
                }
                Stmt::AssertRaises { body, .. } => self.block(program, locals, body, loops),
                Stmt::Switch {
                    selector,
                    cases,
                    default,
                    ..
                } => {
                    self.nested(program, locals, selector, loops);
                    for case in cases {
                        self.nested(program, locals, &case.label, loops);
                        self.block(program, locals, &case.body, loops);
                    }
                    if let Some(default) = default {
                        self.block(program, locals, default, loops);
                    }
                }
                Stmt::Declare { value: None, .. } | Stmt::Inject { .. } | Stmt::Unread { .. } => {}
                Stmt::Return { value, .. } => {
                    if let Some(value) = value {
                        self.nested(program, locals, value, loops);
                    }
                }
                Stmt::Assign { value, .. } | Stmt::Expr(value) => {
                    self.nested(program, locals, value, loops);
                }
                Stmt::Assert { condition, .. } => self.nested(program, locals, condition, loops),
                Stmt::AssignPlace { place, value, .. } => {
                    self.nested(program, locals, place, loops);
                    self.nested(program, locals, value, loops);
                }
                Stmt::Yield { time, value, .. } => {
                    self.nested(program, locals, time, loops);
                    self.nested(program, locals, value, loops);
                }
            }
        }
    }

    /// Reads the blocks inside `value`: a block's statements are a
    /// scope of their own, as an `if`'s are.
    fn nested(&mut self, program: &Program, locals: &[Local], value: &Expr, loops: &[&[Stmt]]) {
        match &value.kind {
            ExprKind::Block {
                statements,
                value: last,
            } => {
                self.block(program, locals, statements, loops);
                if let Some(last) = last {
                    self.nested(program, locals, last, loops);
                }
            }
            _ => {
                // Every other expression: the blocks directly inside
                // it, each read once.
                let mut inner: Vec<&Expr> = Vec::new();
                children(value, &mut inner);
                for part in inner {
                    self.nested(program, locals, part, loops);
                }
            }
        }
    }
}

/// True where nothing else holds what `value` gives: a value written
/// out, and what a function of the program returns, which copies what
/// it was given before it returns it. A value of a type that nothing
/// changes where it is ([`changed_in_place`]), a scalar or a struct of
/// two scalars, counts as one too: nothing of it is copied.
///
/// A name does not: a `let`, a parameter and a loop's name are
/// references, and what a place holds is the place's. Nor does what a
/// native function of core's gives, which may be a part of what it was
/// given or of the run's store, nor a constant of a module, whose one
/// value every read of it has.
pub(crate) fn fresh(program: &Program, value: &Expr) -> bool {
    if !changed_in_place(program, &value.ty, 0) {
        return true;
    }
    match &value.kind {
        ExprKind::Sequence(entries) | ExprKind::Tuple(entries) => {
            entries.iter().all(|entry| fresh(program, entry))
        }
        ExprKind::Pairs(entries) => entries
            .iter()
            .all(|(key, entry)| fresh(program, key) && fresh(program, entry)),
        // A field left out takes its default, which may be a constant
        // of a module: the one value every read of it has. A struct
        // written with fewer fields than it has is fresh only where
        // none of those left out can hold a collection.
        ExprKind::Construct {
            module,
            declaration,
            arguments,
        } => {
            let complete = match program.modules[*module].details.get(*declaration) {
                Some(Detail::Struct(of)) => of.members.iter().enumerate().all(|(index, member)| {
                    arguments
                        .iter()
                        .any(|argument| argument.parameter == Some(index))
                        || !changed_in_place(program, &member.ty, 0)
                }),
                _ => false,
            };
            complete
                && arguments
                    .iter()
                    .all(|argument| fresh(program, &argument.value))
        }
        ExprKind::Delta { arguments } => arguments
            .iter()
            .all(|argument| fresh(program, &argument.value)),
        ExprKind::Tagged { payload: inner, .. } | ExprKind::Widen(inner) => fresh(program, inner),
        ExprKind::Block {
            value: Some(last), ..
        } => fresh(program, last),
        ExprKind::If {
            then, otherwise, ..
        } => {
            fresh(program, then)
                && otherwise
                    .as_ref()
                    .is_none_or(|otherwise| fresh(program, otherwise))
        }
        ExprKind::Call { callee, .. } => gives_its_own(program, callee),
        _ => false,
    }
}

/// True for a local that is a place: a `var`, a `var` parameter, a
/// `state` or a `cache`.
pub(crate) fn is_place(locals: &[Local], local: usize) -> bool {
    matches!(
        locals[local].kind,
        LocalKind::Var | LocalKind::State | LocalKind::Cache
    )
}

/// The place a written place is inside: `rows` for `rows[0].orders`.
pub(crate) fn place_of(locals: &[Local], place: &Expr) -> Option<usize> {
    match &place.kind {
        ExprKind::Local(local) if is_place(locals, *local) => Some(*local),
        ExprKind::Field { base, .. } | ExprKind::Index { base, .. } => place_of(locals, base),
        _ => None,
    }
}

/// True where what a call of `callee` gives is its own: a function of
/// the program returns nothing that something else holds, since it
/// copies what it was given before it returns it. A native function of
/// core's may give a part of what it is given.
fn gives_its_own(program: &Program, callee: &Callee) -> bool {
    match callee {
        Callee::Function {
            module,
            declaration,
            ..
        }
        | Callee::Candidate {
            module,
            declaration,
            ..
        } => program.modules[*module]
            .bodies
            .get(*declaration)
            .is_some_and(Option::is_some),
        // A call of what is not known: it may be one that gives a part
        // of what it is given.
        Callee::Required { .. } | Callee::Unread => false,
    }
}

/// True where `statements` change the place `place`: assign it or a
/// part of it, or give it or a part of it to a `var` parameter.
pub(crate) fn changes(
    program: &Program,
    locals: &[Local],
    statements: &[Stmt],
    place: usize,
) -> bool {
    let mut changed = false;
    walk_statement_nodes(statements, &mut |node| match node {
        Node::Statement(Stmt::Assign { local, .. }) => changed |= *local == place,
        Node::Statement(Stmt::AssignPlace { place: written, .. }) => {
            changed |= place_of(locals, written) == Some(place);
        }
        Node::Expression(Expr {
            kind: ExprKind::Call {
                callee, arguments, ..
            },
            ..
        }) => {
            let (Callee::Function {
                module,
                declaration,
                ..
            }
            | Callee::Candidate {
                module,
                declaration,
                ..
            }) = callee
            else {
                return;
            };
            let Some(Detail::Callable(signature)) =
                program.modules[*module].details.get(*declaration)
            else {
                return;
            };
            for argument in arguments {
                let var = argument
                    .parameter
                    .and_then(|parameter| signature.parameters.get(parameter))
                    .is_some_and(|parameter| parameter.var);
                if var && place_of(locals, &argument.value) == Some(place) {
                    changed = true;
                }
            }
        }
        _ => {}
    });
    changed
}

/// The expressions directly inside `value`, a block's statements left
/// out: each is read as a scope of its own.
fn children<'a>(value: &'a Expr, into: &mut Vec<&'a Expr>) {
    match &value.kind {
        ExprKind::If {
            condition,
            then,
            otherwise,
        } => {
            into.push(condition);
            into.push(then);
            into.extend(otherwise.iter().map(|other| &**other));
        }
        ExprKind::Unary { operand: inner, .. }
        | ExprKind::Widen(inner)
        | ExprKind::EnumFrom { value: inner, .. }
        | ExprKind::Field { base: inner, .. }
        | ExprKind::Tagged { payload: inner, .. }
        | ExprKind::TagTest { value: inner, .. }
        | ExprKind::Payload { result: inner, .. }
        | ExprKind::ErrorPart { base: inner, .. }
        | ExprKind::Propagate(inner)
        | ExprKind::Capture(inner)
        | ExprKind::Captured { capture: inner, .. }
        | ExprKind::ChildEndpoint { child: inner, .. }
        | ExprKind::ChildEndpointOf(inner)
        | ExprKind::CheckedChild { read: inner, .. }
        | ExprKind::Publications(inner) => into.push(inner),
        ExprKind::Binary { left, right, .. }
        | ExprKind::Index {
            base: left,
            index: right,
        }
        | ExprKind::TraceCompare { left, right, .. } => {
            into.push(left);
            into.push(right);
        }
        ExprKind::Call { arguments, .. }
        | ExprKind::Construct { arguments, .. }
        | ExprKind::Delta { arguments } => {
            into.extend(arguments.iter().map(|argument| &argument.value));
        }
        ExprKind::Error { message, code } => {
            into.push(message);
            into.extend(code.iter().map(|code| &**code));
        }
        ExprKind::Tuple(entries) | ExprKind::Sequence(entries) => into.extend(entries.iter()),
        ExprKind::Pairs(entries) => {
            for (key, entry) in entries {
                into.push(key);
                into.push(entry);
            }
        }
        ExprKind::Eval { call, config, .. } => {
            into.push(call);
            into.extend(config.iter().map(|config| &**config));
        }
        ExprKind::Trace(cells) => {
            for cell in cells {
                into.extend([&cell.time, &cell.value].into_iter().flatten());
            }
        }
        ExprKind::Block { .. }
        | ExprKind::Literal(_)
        | ExprKind::Constant { .. }
        | ExprKind::Endpoint { .. }
        | ExprKind::Local(_)
        | ExprKind::EndpointOf(_)
        | ExprKind::GenericValue { .. }
        | ExprKind::EnumMember { .. }
        | ExprKind::Unread => {}
    }
}
