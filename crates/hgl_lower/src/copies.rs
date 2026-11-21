//! Where a value is copied.
//!
//! A `var`, a `state` and a `cache` are places: the list, the
//! dictionary or the struct that is no value ([`by_value`]) one
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
//! The language's side of it is FN-13 (a retained result keeps its own
//! contents) and FN-15 (a `var` parameter is the place given).
//!
//! This module reads one body and says, of each `let` and each loop,
//! whether the place its value is read from is changed while the name
//! is in scope. Where it is, the name is bound to a copy; where it is
//! not, the name is a reference into the place, and what is read
//! through it is read from that place.

use std::collections::{HashMap, HashSet};

use hgl_analyser::hir::{Body, Callee, Expr, ExprKind, Local, Node, Stmt, walk_statement_nodes};
use hgl_analyser::{Detail, LocalKind, NamedKind, Program, Scalar, Type};

use crate::types::inherited;

/// What one body's `let`s and loop names are bound to.
#[derive(Clone, Default)]
pub struct Copies {
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
    /// The references that may also be, or hold, what is held
    /// elsewhere and is no place of this body: a parameter, a constant
    /// of a module, what a native function gives of one
    /// ([`Copies::elsewhere`]). `let x = if flag { mine } else { p }`
    /// refers into `mine`, and may be `p`.
    elsewhere: HashSet<usize>,
}

impl Copies {
    /// Reads `body`: which of its `let`s and loops take a copy, and what
    /// each other name refers into.
    pub fn of(program: &Program, body: &Body) -> Copies {
        let mut copies = Copies::default();
        copies.block(program, &body.locals, &body.statements, &[]);
        copies
    }

    /// True where the `let` or the loop that binds `local` takes a
    /// copy of what it reads.
    pub fn copied(&self, local: usize) -> bool {
        self.copied.contains(&local)
    }

    /// The places `value` is read from: the `var`, `state` or `cache`
    /// it names, or is a part of, or that a name it reads refers into.
    /// Empty for a value that is read from no place: a literal, what a
    /// function of the program gives, a parameter, a constant. What it
    /// may also be that is no place of the body, a parameter say, is
    /// not among them: [`Copies::elsewhere`] says whether there is any.
    /// An expression of any other form may hold a part of what it is
    /// made of, `rows + more` the lists `rows` holds: its places are
    /// those of each part.
    pub fn places(&self, program: &Program, locals: &[Local], value: &Expr) -> Vec<usize> {
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
            // What a function of the program gives is its own.
            ExprKind::Call { .. }
            | ExprKind::Literal(_)
            | ExprKind::Constant { .. }
            | ExprKind::Endpoint { .. }
            | ExprKind::EndpointOf(_)
            | ExprKind::GenericValue { .. }
            | ExprKind::EnumMember { .. }
            | ExprKind::Unread
            | ExprKind::Block { value: None, .. } => Vec::new(),
            _ => {
                let mut parts = Vec::new();
                children(value, &mut parts);
                self.all_places(program, locals, parts.into_iter())
            }
        }
    }

    /// True where `value` may be, or may hold, a value that is no place
    /// of this body and is not its own: a parameter, a constant of a
    /// module, an input's value, what a native function gives of one,
    /// a `let` of one. Only what is known to be the body's own is not:
    /// a place of it, a literal, what a function of the program gives,
    /// a copy, and a value of a type nothing changes where it is. Any
    /// form not known is taken to be held elsewhere.
    pub fn elsewhere(&self, program: &Program, locals: &[Local], value: &Expr) -> bool {
        if !changed_in_place(program, &value.ty, 0) {
            return false;
        }
        // Whether any of some parts is held elsewhere. `&mut dyn` is a
        // borrowed iterator of any kind, so one closure takes them all.
        let any = |values: &mut dyn Iterator<Item = &Expr>| {
            let mut found = false;
            for part in values {
                found |= self.elsewhere(program, locals, part);
            }
            found
        };
        match &value.kind {
            ExprKind::Local(local) => {
                if is_place(locals, *local)
                    || self.copied.contains(local)
                    || self.own.contains(local)
                {
                    false
                } else if self.references.contains_key(local) {
                    self.elsewhere.contains(local)
                } else {
                    true
                }
            }
            ExprKind::Literal(_) | ExprKind::EnumMember { .. } => false,
            ExprKind::Field { base: inner, .. }
            | ExprKind::Index { base: inner, .. }
            | ExprKind::Widen(inner)
            | ExprKind::Payload { result: inner, .. }
            | ExprKind::Tagged { payload: inner, .. }
            | ExprKind::Raised { result: inner, .. }
            | ExprKind::OrNull { result: inner, .. }
            | ExprKind::NullRead { value: inner, .. } => self.elsewhere(program, locals, inner),
            ExprKind::Block { value, .. } => value
                .as_ref()
                .is_some_and(|last| self.elsewhere(program, locals, last)),
            ExprKind::If {
                then, otherwise, ..
            } => {
                self.elsewhere(program, locals, then)
                    || otherwise
                        .as_ref()
                        .is_some_and(|other| self.elsewhere(program, locals, other))
            }
            // A field left out takes its default, which may be a
            // constant of a module (see `fresh`).
            ExprKind::Construct { .. } if !fresh_defaults(program, value) => true,
            ExprKind::Construct { arguments, .. } | ExprKind::Delta { arguments } => {
                any(&mut arguments.iter().map(|given| &given.value))
            }
            ExprKind::Sequence(entries) | ExprKind::Tuple(entries) => any(&mut entries.iter()),
            ExprKind::Pairs(entries) => {
                any(&mut entries.iter().flat_map(|(key, entry)| [key, entry]))
            }
            ExprKind::Call {
                callee, arguments, ..
            } => {
                !gives_its_own(program, callee)
                    && (arguments.is_empty()
                        || any(&mut arguments.iter().map(|given| &given.value)))
            }
            _ => true,
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
                        if self.elsewhere(program, locals, value) {
                            self.elsewhere.insert(*local);
                        }
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
                        let elsewhere = self.elsewhere(program, locals, sequence);
                        for name in names {
                            if elsewhere {
                                self.elsewhere.insert(*name);
                            }
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
                Stmt::Declare { value: None, .. } | Stmt::Unread { .. } => {}
                // RUN-6: the entries of a run's configuration, each a
                // value of its field's type.
                Stmt::Inject { config, .. } => {
                    for entry in config {
                        self.nested(program, locals, &entry.value, loops);
                    }
                }
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
pub fn fresh(program: &Program, value: &Expr) -> bool {
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
        ExprKind::Construct { arguments, .. } => {
            fresh_defaults(program, value)
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

/// True where the construction `value` leaves out no field that may
/// hold what is changed where it is: a field left out takes its
/// default, which may be a constant of a module, the one value every
/// read of it has.
fn fresh_defaults(program: &Program, value: &Expr) -> bool {
    let ExprKind::Construct {
        module,
        declaration,
        arguments,
        ..
    } = &value.kind
    else {
        return false;
    };
    match program.modules[*module].details.get(*declaration) {
        Some(Detail::Struct(of)) => of.members.iter().enumerate().all(|(index, member)| {
            arguments
                .iter()
                .any(|argument| argument.parameter == Some(index))
                || !changed_in_place(program, &member.ty, 0)
        }),
        _ => false,
    }
}

/// True for a local that is a place: a `var`, a `var` parameter, a
/// `state` or a `cache`.
pub fn is_place(locals: &[Local], local: usize) -> bool {
    matches!(
        locals[local].kind,
        LocalKind::Var | LocalKind::State | LocalKind::Cache
    )
}

/// The place a written place is inside: `rows` for `rows[0].orders`.
pub fn place_of(locals: &[Local], place: &Expr) -> Option<usize> {
    match place.place_root().kind {
        ExprKind::Local(local) if is_place(locals, local) => Some(local),
        _ => None,
    }
}

/// True where what a call of `callee` gives is its own: a function of
/// the program returns nothing that something else holds, since it
/// copies what it was given before it returns it. A native function of
/// core's may give a part of what it is given.
fn gives_its_own(program: &Program, callee: &Callee) -> bool {
    // A call of what is not known, with no declaration, may be one that
    // gives a part of what it is given.
    callee.declaration().is_some_and(|(module, declaration)| {
        program.modules[module]
            .bodies
            .get(declaration)
            .is_some_and(Option::is_some)
    })
}

/// True where `statements` change the place `place`: assign it or a
/// part of it, or give it or a part of it to a `var` parameter.
pub fn changes(program: &Program, locals: &[Local], statements: &[Stmt], place: usize) -> bool {
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
            // The signature of what is called, where it is known. A call
            // of what is not known may take any argument as a place.
            let signature = callee.declaration().and_then(|(module, declaration)| {
                match program.modules[module].details.get(declaration) {
                    Some(Detail::Callable(signature)) => Some(signature),
                    _ => None,
                }
            });
            for argument in arguments {
                let var = match (signature, argument.parameter) {
                    (Some(signature), Some(parameter)) => signature
                        .parameters
                        .get(parameter)
                        .is_none_or(|parameter| parameter.var),
                    _ => true,
                };
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
            ..
        } => {
            into.push(condition);
            into.push(then);
            into.extend(otherwise.iter().map(|other| &**other));
        }
        ExprKind::Unary { operand: inner, .. }
        | ExprKind::DeferredNegation { operand: inner, .. }
        | ExprKind::Widen(inner)
        | ExprKind::ParentList(inner)
        | ExprKind::EnumFrom { value: inner, .. }
        | ExprKind::DeferredEnumFrom(inner)
        | ExprKind::Field { base: inner, .. }
        | ExprKind::DeferredMember { base: inner, .. }
        | ExprKind::Tagged { payload: inner, .. }
        | ExprKind::TagTest { value: inner, .. }
        | ExprKind::Payload { result: inner, .. }
        | ExprKind::ErrorPart { base: inner, .. }
        | ExprKind::Raised { result: inner, .. }
        | ExprKind::OrNull { result: inner, .. }
        | ExprKind::NullRead { value: inner, .. }
        | ExprKind::Capture(inner)
        | ExprKind::Captured { capture: inner, .. }
        | ExprKind::ChildEndpoint { child: inner, .. }
        | ExprKind::ChildEndpointOf(inner)
        | ExprKind::CheckedChild { read: inner, .. }
        | ExprKind::CheckedWindow(inner)
        | ExprKind::CheckedDelta(inner)
        | ExprKind::Publications(inner) => into.push(inner),
        ExprKind::Binary { left, right, .. }
        | ExprKind::DeferredBinary { left, right, .. }
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
            into.extend(config.iter().map(|entry| &entry.value));
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
        | ExprKind::DeferredPresent(_)
        | ExprKind::EndpointOf(_)
        | ExprKind::GenericValue { .. }
        | ExprKind::EnumMember { .. }
        | ExprKind::StaticCallable(_)
        | ExprKind::Unread => {}
    }
}

/// True for a type whose value may be, or may hold, one that a place
/// changes where it is: a list, a dictionary and a struct that is no
/// value ([`by_value`]), and a nullable, a tuple, a set, a result or a
/// struct with one inside it. A generic parameter and `any` may hold
/// anything. A scalar, an enum and a struct that is a value are copied,
/// and nothing in them is changed where it is.
pub fn changed_in_place(program: &Program, ty: &Type, depth: usize) -> bool {
    // A struct may hold itself inside a list, which is changed in place.
    if depth > 8 {
        return true;
    }
    let any = |types: &[&Type]| {
        types
            .iter()
            .any(|ty| changed_in_place(program, ty, depth + 1))
    };
    match ty {
        Type::List { .. } | Type::Dict { .. } => true,
        Type::Scalar(scalar) => *scalar == Scalar::Any,
        Type::Named {
            kind: NamedKind::Enum | NamedKind::Native,
            ..
        }
        | Type::Error
        | Type::ErrorContext => false,
        Type::Named {
            module,
            declaration,
            kind: NamedKind::Struct,
            ..
        } => !by_value(program, *module, *declaration),
        Type::Nullable(inner) | Type::Set(inner) => any(&[inner]),
        Type::Tuple(elements) => elements
            .iter()
            .any(|element| changed_in_place(program, element, depth + 1)),
        Type::Result { value, error } => any(&[value, error]),
        _ => true,
    }
}

/// True for the struct `declaration` of `module` where it is a value:
/// copied where it is given or kept, and given a new value where a
/// field of it is assigned. The owner's rule (the compiler's issue
/// 151): "The copy by value exception is for trivial structures (think
/// sizeof(i64))", built as at most two fields, each a scalar other than
/// a `str`, a `bytes` or an `any`, or an enum. A struct with generic
/// parameters is not one, nor is a struct that another inherits from: a
/// value of the other may stand for it. Any other struct a `var`, a
/// `state` or a `cache` holds is changed where it is, as a list is.
pub fn by_value(program: &Program, module: usize, declaration: usize) -> bool {
    let Some(Detail::Struct(read)) = program.modules[module].details.get(declaration) else {
        return false;
    };
    read.generics.is_empty()
        && !inherited(program, module, declaration)
        && read.members.len() <= 2
        && read.members.iter().all(|member| match &member.ty {
            Type::Scalar(scalar) => !matches!(scalar, Scalar::Str | Scalar::Bytes | Scalar::Any),
            Type::Named {
                kind: NamedKind::Enum,
                ..
            } => true,
            _ => false,
        })
}
