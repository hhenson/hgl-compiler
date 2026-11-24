//! `base.member`, `.member` and `base[index]`: a member of an enum, a
//! field of a struct, a part of a value, and an element of a list, a
//! tuple or a dictionary (TYPE-7, EXPR-7, ERR-6, ERR-12). Part of the
//! typing pass ([`super`]).

use hgl_diagnostics::{Category, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};

use super::constants::constant_text;
use super::dependent_packs::MemberFault;
use super::fit::{hook_payload_type, not_null, open, whole};
use super::forms::received_type;
use super::forwarded_packs::BoundPack;
use super::inference::{fixed, placed, type_pack};
use super::results::{captured_output, proof_binding};
use super::{Fact, Typer};
use crate::constant::Fault;
use crate::declared::substitute_within;
use crate::hir::{Body, Expr, ExprKind, Hook, PackKey, Value, ValueCategory};
use crate::program::DeclarationKind;
use crate::syntax::{name_token, tokens, ungrouped};
use crate::types::{Argument, Constant, ConstantValue, Detail, NamedKind, Scalar, Type};
use crate::{codes, error};

enum PackEntryKey {
    Known(Option<PackKey>, Constant),
    Deferred,
    Rejected,
}

/// EXPR-7 and GEN-7: the type of an indexed parameter is known only in
/// the instance. Reread the bracket and any call it supplies there.
pub(crate) fn has_dependent_index(body: &Body) -> bool {
    let mut dependent = false;
    super::forwarded_packs::walk_nested_calls(&body.statements, &mut |value| {
        if let ExprKind::Index { base, .. } = &value.kind
            && matches!(base.ty, Type::Parameter { .. } | Type::PackMember { .. })
        {
            dependent = true;
        }
    });
    dependent
}

impl<'a> Typer<'a> {
    // ---- Members and elements ----

    /// `base.member`: a member of an enum, or a field of a struct
    /// value. A result and a `T?` are read for the T they hold (ERR-12,
    /// ERR-15). Any other base is for a later part.
    pub(super) fn member(&mut self, node: &SyntaxNode) -> (ExprKind, Type) {
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
        // ERR-7: a capture is "read through two connections".
        let captured = match &base.kind {
            ExprKind::Local(local) | ExprKind::DeferredPresent(local) => {
                self.captures.get(local).cloned()
            }
            ExprKind::Capture(call) => Some(captured_output(program, call)),
            _ => None,
        };
        if let Some(output) = captured {
            return self.captured(base, &name, output);
        }
        let at = Location::new(self.file, name.span());
        let variant_proven = self.payload_proven(&base, name.text());
        let deferred_graph = self.graph
            && !matches!(base.kind, ExprKind::Unread)
            && not_null(Some(&base.ty)).is_some_and(|ty| match ty {
                Type::Unknown => true,
                // ERR-3: a lexical guard proves a variant only when the
                // result settles to an ordinary value.
                Type::Result { .. } if matches!(name.text(), "value" | "error") => {
                    variant_proven && self.category(&base) == ValueCategory::Unsettled
                }
                // ERR-15 cannot take an implicit success until its type settles.
                Type::Result { .. } => !whole(ty),
                _ => false,
            });
        if deferred_graph || self.deferred_pack_selection(&base).is_some() {
            return (
                ExprKind::DeferredMember {
                    base: Box::new(base),
                    name: name.text().to_owned(),
                    at,
                    variant_proven,
                },
                Type::Unknown,
            );
        }
        self.member_value(base, name.text(), at, variant_proven)
    }

    /// Replay a projection when its input becomes concrete. A saved
    /// ERR-3 guard can prove a variant only of an ordinary result.
    pub(super) fn settle_member(
        &mut self,
        base: &Expr,
        name: &str,
        at: Location,
        mut variant_proven: bool,
    ) -> Option<(ExprKind, Type)> {
        if !whole(&base.ty) {
            return None;
        }
        if variant_proven && matches!(base.ty, Type::Result { .. }) {
            match self.category(base) {
                ValueCategory::Unsettled => return None,
                ValueCategory::Connection => variant_proven = false,
                ValueCategory::Value => {}
            }
        }
        Some(self.member_value(base.clone(), name, at, variant_proven))
    }

    /// Resolve a member against a known value, retaining its implicit reads.
    fn member_value(
        &mut self,
        mut base: Expr,
        name: &str,
        at: Location,
        variant_proven: bool,
    ) -> (ExprKind, Type) {
        let program = self.program;
        // What the member is read from, in the form that place takes it:
        // each form taken is read again for its member.
        loop {
            match &base.ty {
                // ERR-3: `.value` and `.error` "read from a result ...
                // are the result's own".
                Type::Result { value, error } if matches!(name, "value" | "error") => {
                    let sides = ((**value).clone(), (**error).clone());
                    // An implicit read cannot transfer a binding's fact.
                    let variant_proven = variant_proven && proof_binding(&base).is_some();
                    return self.payload(base, name, at, sides, variant_proven);
                }
                // ERR-15: "What a member or an index is read from ...
                // expect[s] the T too": `checked(quote).bid` is the bid of
                // the success, or raises the failure.
                Type::Result { .. } => {
                    let read = self.received(base, |_| false);
                    if matches!(read.ty, Type::Result { .. }) {
                        return (ExprKind::Unread, Type::Unknown);
                    }
                    base = read;
                }
                // ERR-4: a part of an error, and of its context.
                Type::Error => return self.error_part(base, name, at),
                Type::ErrorContext => return self.context_part(base, name, at),
                // ERR-12: "A `T?` value given where a T is expected is
                // read as its value": a member of a nullable's payload
                // reads it, where no test against null has proved it a
                // T. ERR-4: "A nullable code or context read where a
                // value is expected is read as ERR-12 says."
                Type::Nullable(inner) if has_members(inner) => {
                    base = self.received(base, has_members);
                }
                // TYPE-16: a set's delta has "the properties `added` and
                // `removed`, each a `set<K>`".
                Type::Delta(of) => {
                    let of = of.clone();
                    return self.delta_property(base, name, at, &of);
                }
                _ => break,
            }
        }
        // TYPE-9: "`atomic<V>` stops expansion at a complete ordinary
        // V": an `atomic<Quote>` holds whole quotes, and its field is
        // the field of each, itself an atomic of the field's type, as
        // complete as the quote it is read from.
        let (whole, atomic) = match &base.ty {
            Type::Atomic(payload) => (&**payload, true),
            other => (other, false),
        };
        let Type::Named {
            module,
            declaration,
            arguments,
            kind,
            ..
        } = whole
        else {
            return (ExprKind::Unread, Type::Unknown);
        };
        if atomic && *kind != NamedKind::Struct {
            return (ExprKind::Unread, Type::Unknown);
        }
        // NATIVE-6: "`value.property` reads one as an ordinary value."
        if let Some(Detail::Native(native)) = program.modules[*module].details.get(*declaration) {
            let Some(member) = native
                .properties
                .iter()
                .position(|property| property.name == name)
            else {
                self.diagnostics.emit(error(
                    Category::Name,
                    codes::MEMBER_UNKNOWN,
                    at,
                    format!("`{}` has no property `{name}`", base.ty),
                    "NATIVE-6",
                ));
                return (ExprKind::Unread, Type::Unknown);
            };
            let ty = native.properties[member].ty.clone();
            let kind = ExprKind::Field {
                base: Box::new(base),
                member,
            };
            return (kind, ty);
        }
        let Some(Detail::Struct(read)) = program.modules[*module].details.get(*declaration) else {
            return (ExprKind::Unread, Type::Unknown);
        };
        let Some(member) = read.members.iter().position(|member| member.name == name) else {
            self.diagnostics.emit(error(
                Category::Name,
                codes::MEMBER_UNKNOWN,
                at,
                format!("`{}` has no field `{name}`", base.ty),
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
        let ty = if atomic {
            // TYPE-9: an inferred field may become a scalar. Where
            // even its shape is open, wait to decide whether it is atomic.
            match &ty {
                Type::Unknown => Type::Unknown,
                Type::Nullable(payload) | Type::Delta(payload) if **payload == Type::Unknown => {
                    Type::Unknown
                }
                _ => Type::atomic(ty),
            }
        } else {
            ty
        };
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
            kind: NamedKind::Enum,
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
    pub(super) fn implied_member(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Type>,
    ) -> (ExprKind, Type) {
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
    /// dictionary brackets ... are strict point-in-time reads"), or a
    /// pack entry under a constant key (FN-19).
    pub(super) fn index(&mut self, node: &SyntaxNode) -> (ExprKind, Type) {
        let mut parts = node.children();
        let (Some(base), Some(index)) = (parts.next(), parts.next()) else {
            return (ExprKind::Unread, Type::Unknown);
        };
        let root = ungrouped(base.clone());
        if let Some(named) = self.local_of(&root).filter(|&local| self.names_pack(local)) {
            let pack = self.pack_origin(named);
            if self.bound_packs.contains_key(&pack) {
                return self.bound_pack_entry(pack, named, node, &index);
            }
            let (kind, ty) = self.pack_entry(pack, named, &index);
            let at = self.at(node);
            let input = self.is_temporal_input(pack);
            let child = self.reads_children() && self.continues_chain(node, &ty);
            if !matches!(kind, ExprKind::Unread)
                && input
                && !child
                && self.naming != Some(self.at(&root))
            {
                // FN-19 selects one input. In a hook its payload has
                // the same admission and lifecycle requirements (TIME-1,
                // TIME-4, TIME-5) as a non-packed input.
                let key = match &kind {
                    ExprKind::PackEntry { key, .. } => Some(key.clone()),
                    _ => self.endpoint_key(node),
                };
                // Check entry admission even when the shape is unfitted. A
                // dominating child guard on that entry leaves the projection's
                // admission to the instance, which can see the actual child.
                let guarded_projection = matches!(kind, ExprKind::DeferredPackRead)
                    && ty == Type::Unknown
                    && self.pack_inputs.is_none()
                    && self.reads_children()
                    && super::children::following_projection(node).is_some()
                    && self.present.iter().any(|fact| {
                        matches!(fact, Fact::DeferredChildGuard { pack: guarded, key: entry }
                            if *guarded == pack && key.as_ref() == Some(entry))
                    });
                // An unresolved key or shape can affect what admits the
                // read. Its instance decides it where both are known.
                if !guarded_projection
                    && (key.is_some() || !matches!(self.hook, Some(Hook::When | Hook::Stop)))
                {
                    self.pack_input_read(
                        pack,
                        key,
                        node,
                        super::children::read_whole_all_valid(&ty),
                    );
                }
                self.signal_read(pack, at, Some((&ty, node)));
            }
            let ty = if input && self.hook.is_some() {
                hook_payload_type(&ty)
            } else {
                ty
            };
            return (kind, ty);
        }
        let base = self.expression(&base, None);
        // ERR-15, ERR-12: what an index is read from expects the T: a
        // result's, and a nullable list's, tuple's or dictionary's.
        let base = self.received(base, index_collection);
        self.check_index_base(&base.ty, base.at);
        let (index, ty) = match &base.ty {
            Type::List { .. } | Type::Dict { .. } => {
                let (expected, place) =
                    index_operand(&base.ty).expect("a list or dictionary index");
                let index = self.expression(&index, Some(expected));
                let ty = index_result(&base.ty, None)
                    .cloned()
                    .unwrap_or(Type::Unknown);
                (self.fit(index, expected, place), ty)
            }
            Type::Tuple(_) => {
                let (integer, place) = index_operand(&base.ty).expect("a tuple index");
                let mut value = self.expression(&index, Some(integer));
                let ty = self.tuple_index(&base.ty, &mut value, Some(&index), true);
                (self.fit(value, integer, place), ty)
            }
            _ => (self.expression(&index, Some(&Type::Unknown)), Type::Unknown),
        };
        let kind = ExprKind::Index {
            base: Box::new(base),
            index: Box::new(index),
        };
        (kind, ty)
    }

    /// FN-19 and GEN-17: read an entry under a compile-time key. A
    /// specialization supplies the pack's actual keys and fitted types.
    fn pack_entry(&mut self, pack: usize, named: usize, index: &SyntaxNode) -> (ExprKind, Type) {
        let (wanted, key) = match self.pack_entry_key(index) {
            PackEntryKey::Known(wanted, key) => (wanted, key),
            PackEntryKey::Deferred => return self.deferred_pack_entry(pack),
            PackEntryKey::Rejected => return (ExprKind::Unread, Type::Unknown),
        };
        let unread = (ExprKind::Unread, Type::Unknown);
        let Some(inputs) = &self.pack_inputs else {
            return self.deferred_pack_entry(pack);
        };
        let entry = wanted.as_ref().and_then(|key| {
            inputs
                .values
                .iter()
                .find(|input| input.parameter == pack && input.key == *key)
        });
        let Some(entry) = entry else {
            self.pack_member_fault(
                self.at(index),
                &MemberFault::Missing {
                    pack: self.locals[named].name.clone(),
                    key,
                },
            );
            return unread;
        };
        (
            ExprKind::PackEntry {
                pack,
                key: entry.key.clone(),
            },
            entry.ty.clone(),
        )
    }

    /// A computed binding reads the local initialized by the selected pass.
    fn bound_pack_entry(
        &mut self,
        pack: usize,
        named: usize,
        node: &SyntaxNode,
        index: &SyntaxNode,
    ) -> (ExprKind, Type) {
        let (wanted, key) = match self.pack_entry_key(index) {
            PackEntryKey::Known(wanted, key) => (wanted, key),
            PackEntryKey::Deferred => return (ExprKind::DeferredPackRead, Type::Unknown),
            PackEntryKey::Rejected => return (ExprKind::Unread, Type::Unknown),
        };
        let Some(BoundPack::Entries(entries)) = self.bound_packs.get(&pack) else {
            return (ExprKind::DeferredPackRead, Type::Unknown);
        };
        let local = wanted.and_then(|key| {
            entries
                .iter()
                .find_map(|(entry, local)| (*entry == key).then_some(*local))
        });
        if let Some(local) = local {
            let value = self.read_local(local, node);
            (value.kind, value.ty)
        } else {
            self.pack_member_fault(
                self.at(index),
                &MemberFault::Missing {
                    pack: self.locals[named].name.clone(),
                    key,
                },
            );
            (ExprKind::Unread, Type::Unknown)
        }
    }

    fn pack_entry_key(&mut self, index: &SyntaxNode) -> PackEntryKey {
        let at = self.at(index);
        let text = constant_text(index);
        let source = match self.typed_in(self.file, index, None) {
            Ok(read)
                if read.ty().is_none_or(|ty| {
                    !whole(ty) || matches!(ty, Type::Scalar(Scalar::Str | Scalar::I64))
                }) =>
            {
                read.value
            }
            // CTRL-16: the analyser sees a range body before the
            // target can supply the constant of each unrolled pass.
            Err(Fault::NotConstant) if self.range_pack_key(index) => {
                return PackEntryKey::Deferred;
            }
            Err(Fault::Reported | Fault::Failed) => return PackEntryKey::Rejected,
            _ => {
                let key = Constant {
                    text,
                    value: ConstantValue::Unknown,
                };
                self.pack_member_fault(at, &MemberFault::KeyKind { key });
                return PackEntryKey::Rejected;
            }
        };
        let value = source.substituted(&|at| {
            self.instance
                .iter()
                .chain(&self.bound)
                .find_map(|(declared, given)| match given {
                    Argument::Constant(given) if *declared == at => Some(given.value.clone()),
                    _ => None,
                })
        });
        // GEN-17 reports the written key. A generic parameter used as
        // the key instead names the value this instance supplies.
        let text = if matches!(source, ConstantValue::Parameter { .. }) {
            value.key().unwrap_or(text)
        } else {
            text
        };
        let wanted = match &value {
            ConstantValue::Str(name) => Some(PackKey::Name(name.clone())),
            ConstantValue::Int(position) => usize::try_from(*position).ok().map(PackKey::Position),
            ConstantValue::Unknown => return PackEntryKey::Rejected,
            // UNIT-13: a call can compute a module constant. The
            // constant reader holds that value by its declaration even
            // when it cannot work it out for this lookup.
            _ if named_key(&value) => return PackEntryKey::Deferred,
            _ if self.pack_inputs.is_none()
                && self
                    .signature
                    .generics
                    .iter()
                    .any(|generic| value.names_one_of(&[generic.at])) =>
            {
                return PackEntryKey::Deferred;
            }
            _ => {
                self.pack_member_fault(
                    at,
                    &MemberFault::KeyKind {
                        key: Constant { text, value },
                    },
                );
                return PackEntryKey::Rejected;
            }
        };
        PackEntryKey::Known(wanted, Constant { text, value })
    }

    /// A graph range's constant binding can be part of an FN-19 key
    /// (CTRL-16). The ordinary expression reader still checks its type.
    fn range_pack_key(&mut self, index: &SyntaxNode) -> bool {
        if !index.descendants().any(|part| {
            part.kind() == SyntaxKind::NameExpr
                && self
                    .local_of(&part)
                    .is_some_and(|local| self.unrolled_range_bindings.contains(&local))
        }) {
            return false;
        }
        let value = self.expression(index, None);
        matches!(value.ty, Type::Scalar(Scalar::Str | Scalar::I64))
            && self.is_fixed(&value, &self.unrolled_range_bindings)
    }

    /// The entry awaits either its concrete pack or its constant key.
    fn deferred_pack_entry(&self, pack: usize) -> (ExprKind, Type) {
        let parameter = &self.signature.parameters[pack];
        let ty = if type_pack(parameter, &self.signature.generics).is_some() {
            Type::Unknown
        } else {
            parameter.ty.clone()
        };
        (ExprKind::DeferredPackRead, ty)
    }

    /// EXPR-7: brackets read a list, tuple or dictionary. A base whose
    /// payload shape is known must be one of those collections.
    pub(super) fn check_index_base(&mut self, ty: &Type, at: Location) {
        // Its wrapper can affect how the payload is read (TYPE-9,
        // ERR-12, ERR-15). The payload decides whether it has an index.
        let mut shape = ty;
        while let Type::Atomic(payload)
        | Type::Nullable(payload)
        | Type::Result { value: payload, .. } = shape
        {
            shape = payload;
        }
        if open(shape)
            || matches!(shape, Type::Parameter { .. } | Type::PackMember { .. })
            || index_collection(shape)
        {
            return;
        }
        let message = if matches!(shape, Type::Set(_)) {
            format!(
                "this is `{shape}`, and a set has no element at an index: brackets read a \
                 list, a tuple, or a dictionary"
            )
        } else {
            format!("this is `{shape}`. Brackets read a list, a tuple, or a dictionary")
        };
        self.diagnostics.emit(error(
            Category::Type,
            codes::TYPE_MISMATCH,
            at,
            message,
            "EXPR-7",
        ));
    }

    /// EXPR-7: a tuple index is a constant within its length. Read it
    /// as UNIT-13 reads other constants and retain the folded position.
    /// Type settlement can recover the type without reporting until
    /// the instance checks its destinations.
    pub(super) fn tuple_index(
        &mut self,
        base: &Type,
        index: &mut Expr,
        written: Option<&SyntaxNode>,
        report: bool,
    ) -> Type {
        let Type::Tuple(elements) = base else {
            return Type::Unknown;
        };
        let (integer, _) = index_operand(base).expect("a tuple index");
        // A wrong index type is reported by the normal fit.
        if !self.admits(integer, &index.ty) {
            return Type::Unknown;
        }
        let found = match index.kind {
            ExprKind::Literal(Value::Int(position)) => Ok(ConstantValue::Int(position)),
            _ => written
                .cloned()
                .or_else(|| self.syntax_of(index))
                .map_or(Err(Fault::Reported), |node| {
                    self.read_constant(&node, Some(integer))
                }),
        };
        match found {
            Ok(ConstantValue::Int(position)) => {
                if let Some(ty) = tuple_element(self.program, elements, position) {
                    index.kind = ExprKind::Literal(Value::Int(position));
                    index.ty = integer.clone();
                    return ty;
                }
            }
            Err(Fault::NotConstant) => {}
            // A value not known yet, a failed constant or a reported
            // fault does not establish the type of the element.
            _ => return Type::Unknown,
        }
        if report {
            self.diagnostics.emit(error(
                Category::Type,
                codes::TUPLE_INDEX,
                index.at,
                if elements.iter().any(|ty| matches!(ty, Type::Parameter { at, .. }
                    if super::requirements::type_pack_bounds(self.program, *at).is_some())) {
                    "the index must select an element for every admitted tuple-pack length; its element is not determined here".to_string()
                } else {
                    format!(
                        "a tuple's index is a constant, here one of 0 to {}: it selects the element's type",
                        elements.len().saturating_sub(1)
                    )
                },
                "EXPR-7",
            ));
        }
        Type::Unknown
    }
}

/// An FN-19 key can name a module constant that the constant reader
/// has not evaluated, directly or in arithmetic or string concatenation.
fn named_key(value: &ConstantValue) -> bool {
    match value {
        ConstantValue::Named { .. } => true,
        ConstantValue::Unary(_, operand) => named_key(operand),
        ConstantValue::Binary(_, left, right) => named_key(left) || named_key(right),
        _ => false,
    }
}

/// EXPR-7: the collection kinds read by brackets, including through a `T?`.
pub(super) fn index_collection(ty: &Type) -> bool {
    matches!(ty, Type::List { .. } | Type::Tuple(_) | Type::Dict { .. })
}

/// ERR-12 and ERR-15: the type an index sees after the same implicit
/// reads as `Typer::received`. Settlement uses it before inserting reads.
pub(super) fn indexed_payload(ty: &Type) -> &Type {
    received_type(ty, index_collection)
}

/// EXPR-7: what type a bracket index must have for each collection.
pub(super) fn index_operand(ty: &Type) -> Option<(&Type, &'static str)> {
    const INTEGER: &Type = &Type::Scalar(Scalar::I64);
    match ty {
        Type::List { .. } => Some((INTEGER, "the index of a list")),
        Type::Tuple(_) => Some((INTEGER, "the index of a tuple")),
        Type::Dict { key, .. } => Some((key, "the key of the dictionary")),
        _ => None,
    }
}

/// EXPR-7: the result of indexing an already selected collection type.
pub(super) fn index_result(ty: &Type, position: Option<i64>) -> Option<&Type> {
    match ty {
        Type::List { element, .. } => Some(element),
        Type::Dict { value, .. } => Some(value),
        Type::Tuple(elements) => elements.get(usize::try_from(position?).ok()?),
        _ => None,
    }
}

/// GEN-7, GEN-10 and EXPR-7: a pack marker denotes a sequence, never
/// one tuple element. A guaranteed member retains its source and key for
/// specialization; a position that can cross a variable boundary is unknown.
fn tuple_element(program: &crate::Program, elements: &[Type], position: i64) -> Option<Type> {
    let mut remaining = u64::try_from(position).ok()?;
    for ty in elements {
        if let Type::Parameter { at, name } = ty
            && let Some((minimum, maximum)) = super::requirements::type_pack_bounds(program, *at)
        {
            if remaining < minimum {
                let position = i64::try_from(remaining).ok()?;
                return Some(Type::PackMember {
                    pack: *at,
                    name: name.clone(),
                    key: Constant {
                        text: position.to_string(),
                        value: ConstantValue::Int(position),
                    },
                });
            }
            if maximum != Some(minimum) {
                return None;
            }
            remaining -= minimum;
        } else if remaining == 0 {
            return Some(ty.clone());
        } else {
            remaining -= 1;
        }
    }
    None
}

/// True when a value of `ty` has members that `value.member` reads: a
/// struct's fields, a native type's properties, a result's payloads, an
/// error's and its context's parts, and a delta's properties. An
/// `atomic<S>` under a `?` is an input that may be omitted, and is not
/// one (TYPE-14).
fn has_members(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Named {
            kind: NamedKind::Struct | NamedKind::Native,
            ..
        } | Type::Result { .. }
            | Type::Error
            | Type::ErrorContext
            | Type::Delta(_)
    )
}
