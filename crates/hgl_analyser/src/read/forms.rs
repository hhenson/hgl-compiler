//! Reads type forms according to what each position admits.

use hgl_diagnostics::Category;
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};
use hgl_source::Span;

use crate::constant::{Fault, Typed};
use crate::syntax::{has_token, name_token, tokens};
use crate::types::{Constant, ConstantValue, Member, NativeType, Scalar, Type, WrittenType};
use crate::{codes, error};

use super::{Position, Reader, is_const, is_type, text_of, type_child};

impl Reader<'_> {
    /// Reads a type that stands in no other type, once.
    pub(super) fn outermost(&mut self, node: &SyntaxNode) -> Type {
        let span = node.span();
        if let Some(ty) = self.read.get(&span.start) {
            return ty.clone();
        }
        let position = self.position(node);
        let ty = self.type_(node, position);
        self.record(span, ty)
    }

    pub(super) fn record(&mut self, span: Span, ty: Type) -> Type {
        self.read.insert(span.start, ty.clone());
        self.written.push(WrittenType {
            at: self.at(span),
            ty: ty.clone(),
        });
        ty
    }

    /// What the place `node` is written in admits, from the node around
    /// it.
    fn position(&self, node: &SyntaxNode) -> Position {
        let Some(parent) = node.parent() else {
            return Position::OPEN;
        };
        match parent.kind() {
            SyntaxKind::Param => {
                let pack = has_token(&parent, SyntaxKind::Ellipsis);
                if self.value_function {
                    // FN-1: a `const fn` "computes ordinary values": its
                    // parameter types are ordinary value types.
                    Position {
                        pack,
                        ..Position::value("a parameter of a value function", "FN-1")
                    }
                } else if pack {
                    // FN-8: "`...T` captures the positional ones: all of
                    // type T, or each of its own type when T is a
                    // declared type pack." Each one captured is an
                    // input, so `...signal` is a pack of signals.
                    Position {
                        signal: true,
                        ..Position::PACK
                    }
                } else if is_const(&parent) {
                    // FN-1: "Parameter-level `const` instead fixes
                    // configuration."
                    Position::value("a `const` parameter", "FN-1")
                } else {
                    // A temporal input, with a default or without one
                    // (UNIT-8).
                    Position::INPUT
                }
            }
            SyntaxKind::PackType => Position::PACK,
            SyntaxKind::ReturnType => {
                // The result of the declaration itself, not of an
                // anonymous function inside it.
                let of_declaration = parent.parent().is_some_and(|declaration| {
                    matches!(
                        declaration.kind(),
                        SyntaxKind::FnDecl | SyntaxKind::NativeFnDecl
                    )
                });
                if self.value_function && of_declaration {
                    Position::value("the result of a value function", "FN-1")
                } else {
                    Position::PLAIN
                }
            }
            SyntaxKind::StructMember => Position::FIELD,
            // UNIT-13: a constant "names one ordinary value of the type
            // T".
            SyntaxKind::ConstantDecl => Position::value("the type of a constant", "UNIT-13"),
            // NATIVE-6: a property is "a name and an ordinary value
            // type".
            SyntaxKind::NativeProperty => {
                Position::value("a property of a native type", "NATIVE-6")
            }
            SyntaxKind::StructBases => Position::PLAIN,
            // GEN-13: "An explicit `const N: V` declares a value
            // parameter of ordinary type V."
            SyntaxKind::GenericParam => Position::value("a generic value parameter", "GEN-13"),
            // GEN-9: "Property clauses apply to explicit concrete
            // domains", written `properties<value_type, ...>`.
            SyntaxKind::OperatorProperties => {
                Position::value("the domain of an operator's properties", "GEN-9")
            }
            // TYPE-12: `signal` is the whole type of a function's
            // temporal parameter, and neither a local's annotation nor
            // an anonymous function's parameter. Which other types
            // those may have is for the pass that knows the phase.
            SyntaxKind::LocalDecl
            | SyntaxKind::StateDecl
            | SyntaxKind::CacheDecl
            | SyntaxKind::LambdaParam => Position::PLAIN,
            _ => Position::OPEN,
        }
    }

    // ---- Reading a type ----

    /// Reads the type `node` is, checking what it is made of against
    /// what `position` admits. It calls itself for the types inside:
    /// as deep as the type is nested, which the parser bounds.
    pub(super) fn type_(&mut self, node: &SyntaxNode, position: Position) -> Type {
        match node.kind() {
            SyntaxKind::ScalarType => tokens(node)
                .next()
                .and_then(|word| Scalar::from_keyword(word.kind()))
                .map_or(Type::Unknown, Type::Scalar),
            SyntaxKind::ErrorType => Type::Error,
            SyntaxKind::SignalType => {
                self.signal(node, position);
                Type::Signal
            }
            SyntaxKind::NullableType => self.nullable(node, position),
            SyntaxKind::NamedType => {
                let ty = self.named(node, position);
                self.capability_position(node, &ty);
                ty
            }
            SyntaxKind::PackMemberType | SyntaxKind::IndexExpr => self.pack_member(node),
            SyntaxKind::TupleType => Type::Tuple(
                node.children()
                    .filter(|element| is_type(element.kind()))
                    .map(|element| self.type_(&element, position.part()))
                    .collect(),
            ),
            SyntaxKind::ListType => self.list(node, position),
            // The grammar: `set_type = "set", "<", value_type, ">"`.
            SyntaxKind::SetType => {
                if let Some(element) = type_child(node) {
                    self.key(&element, "a set's element");
                }
                Type::Set(Box::new(
                    self.inner(node, Position::value("a set's element", "TYPE-9")),
                ))
            }
            SyntaxKind::DictType => self.dict(node, position),
            SyntaxKind::RollingWindowType => self.rolling_window(node, position),
            SyntaxKind::EndpointType => {
                self.endpoint(node);
                Type::Endpoint(Box::new(self.inner(node, Position::PLAIN)))
            }
            SyntaxKind::AtomicType => {
                self.ordinary(node, position, false);
                // TYPE-9: "`atomic<V>` stops expansion at a complete
                // ordinary V. For non-composite S, `atomic<S>`
                // normalizes to S".
                Type::atomic(self.inner(node, Position::value("an atomic payload", "TYPE-9")))
            }
            SyntaxKind::ResultType => self.result(node),
            SyntaxKind::DeltaType => self.delta(node),
            _ => Type::Unknown,
        }
    }

    /// The one type inside `node`, read for `position`.
    pub(super) fn inner(&mut self, node: &SyntaxNode, position: Position) -> Type {
        type_child(node).map_or(Type::Unknown, |inner| self.type_(&inner, position))
    }

    /// Reports `node`, which is no ordinary value type, where only one
    /// may stand. `signal` is true for `signal`.
    fn ordinary(&mut self, node: &SyntaxNode, position: Position, signal: bool) {
        let Some((what, place_rule)) = position.value else {
            return;
        };
        // TYPE-12 says where `signal` may not stand. For the others, and
        // under a `?`, the place's own rule asks for an ordinary value.
        let rule = if signal && place_rule != "TYPE-14" {
            "TYPE-12"
        } else {
            place_rule
        };
        self.diagnostics.emit(
            error(
                Category::Type,
                codes::TYPE_NOT_ORDINARY,
                self.at(node.span()),
                format!(
                    "`{}` is not an ordinary value type, and {what} is one",
                    text_of(node)
                ),
                rule,
            )
            .with_note("`rolling_window`, `atomic` and `signal` are not ordinary value types"),
        );
    }

    /// TYPE-12: "`signal` is a complete temporal input constraint with
    /// no readable payload; it is not a result, nested shape, field, or
    /// const value."
    fn signal(&mut self, node: &SyntaxNode, position: Position) {
        if position.value.is_some() {
            self.ordinary(node, position, true);
        } else if !position.signal {
            self.diagnostics.emit(
                error(
                    Category::Type,
                    codes::SIGNAL_POSITION,
                    self.at(node.span()),
                    "`signal` is the whole type of an input, and nothing else".to_string(),
                    "TYPE-12",
                )
                .with_note("it is not a result, a field, or a part of another type"),
            );
        }
    }

    /// TIME-8: "A capability type is written only as the type of a
    /// parameter of a `const fn` or of a native value function."
    fn capability_position(&mut self, node: &SyntaxNode, ty: &Type) {
        if !self.program.is_capability_type(ty) {
            return;
        }
        let parameter = node
            .parent()
            .is_some_and(|parent| parent.kind() == SyntaxKind::Param);
        if parameter && self.value_function {
            return;
        }
        self.diagnostics.emit(
            error(
                Category::Injectable,
                codes::CAPABILITY_AS_VALUE,
                self.at(node.span()),
                format!(
                    "`{ty}` is a capability's type, and is written only as the type of a \
                     parameter of a `const fn` or of a native value function"
                ),
                "TIME-8",
            )
            .with_note("a node has a capability by `inject`, and does not keep one"),
        );
    }

    /// The read-only properties a native type declares (NATIVE-6): "each
    /// a name and an ordinary value type, written as a struct's typed
    /// fields are."
    pub(super) fn native_type(&mut self, node: &SyntaxNode, declared: &str) -> NativeType {
        let mut properties: Vec<Member> = Vec::new();
        for property in node
            .children()
            .filter(|property| property.kind() == SyntaxKind::NativeProperty)
        {
            // A property's name is its first token. It may be a reserved
            // word (LEX-9).
            let Some(name) = tokens(&property).next() else {
                continue;
            };
            let ty =
                type_child(&property).map_or(Type::Unknown, |written| self.outermost(&written));
            // "Written as a struct's typed fields are": two of one name
            // reject, as two fields do (TYPE-4), and the first stands.
            if let Some(first) = properties
                .iter()
                .find(|earlier| earlier.name == name.text())
            {
                self.diagnostics.emit(
                    error(
                        Category::Type,
                        codes::FIELD_DUPLICATE,
                        self.at(name.span()),
                        format!("`{declared}` has two properties named `{}`", name.text()),
                        "NATIVE-6",
                    )
                    .with_related(first.at, "first declared here"),
                );
                continue;
            }
            properties.push(Member {
                name: name.text().to_string(),
                at: self.at(name.span()),
                ty,
                has_default: false,
                default: None,
            });
        }
        NativeType { properties }
    }

    /// TYPE-12: "`endpoint<T>` is the endpoint of a temporal value of T,
    /// for a function that takes the endpoint and not its payload
    /// (TIME-9). It is the whole type of a parameter of a native value
    /// function, and of nothing else." FN-1 has that parameter the
    /// exception to a value function's ordinary value types.
    fn endpoint(&mut self, node: &SyntaxNode) {
        let parameter = node
            .parent()
            .filter(|parent| parent.kind() == SyntaxKind::Param);
        let native = parameter
            .as_ref()
            .and_then(|parameter| {
                parameter
                    .ancestors()
                    .find(|ancestor| ancestor.kind() == SyntaxKind::NativeFnDecl)
            })
            .is_some();
        if native && self.value_function {
            return;
        }
        self.diagnostics.emit(
            error(
                Category::Type,
                codes::ENDPOINT_POSITION,
                self.at(node.span()),
                "`endpoint<T>` is the whole type of a parameter of a native value function, \
                 and nothing else"
                    .to_string(),
                "TYPE-12",
            )
            .with_note("any other function takes the value: write the type `T`"),
        );
    }

    /// TYPE-14: a set's element and a dictionary's key are never null
    /// and take no `?`.
    fn key(&mut self, node: &SyntaxNode, what: &str) {
        if node.kind() == SyntaxKind::NullableType {
            self.diagnostics.emit(
                error(
                    Category::Type,
                    codes::NULLABLE_KEY,
                    self.at(node.span()),
                    format!("{what} is never null and takes no `?`"),
                    "TYPE-14",
                )
                .with_note("a list's element and a dictionary's value may be nullable"),
            );
        }
    }

    /// `T?` (TYPE-14, TYPE-15).
    ///
    /// TYPE-14: "`T?` denotes an ordinary value that is either a T or
    /// null, where T is an admitted ordinary value type ... Where T is
    /// no ordinary value type ... `T?` is admitted only as the whole
    /// type of a function's temporal parameter, whose argument may be
    /// omitted (UNIT-8), or of a struct field, which is then optional
    /// (TYPE-4). It is no result type, local's annotation, or part of
    /// another type."
    fn nullable(&mut self, node: &SyntaxNode, position: Position) -> Type {
        let Some(base) = type_child(node) else {
            return Type::Unknown;
        };
        let base_position = match position.value {
            // Where the place around it asks for an ordinary value
            // type already, that place is the one named.
            Some(_) => position,
            // The whole type of an input or a field: any type it may
            // have.
            None if position.omissible => Position {
                omissible: false,
                ..position
            },
            // Anywhere else the whole of what `?` is applied to is an
            // ordinary value type: `list<signal>?` is no more one than
            // `signal?`.
            None => Position::value("what `?` is applied to", "TYPE-14"),
        };
        Type::Nullable(Box::new(self.type_(&base, base_position)))
    }

    /// `list<T>`, `list<T, size>` (TYPE-10).
    fn list(&mut self, node: &SyntaxNode, position: Position) -> Type {
        let element = self.inner(node, position.part());
        // The size is the child after the element: `unbounded`, or an
        // expression.
        let size = node
            .children()
            .find(|part| !is_type(part.kind()) && part.kind() != SyntaxKind::Error)
            .map(|size| self.list_size(&size));
        Type::List {
            element: Box::new(element),
            size,
        }
    }

    /// TYPE-10: "fixed sizes are nonnegative constants. `unbounded` is
    /// the size sentinel."
    fn list_size(&mut self, node: &SyntaxNode) -> Constant {
        if node.kind() == SyntaxKind::UnboundedSize {
            return Constant {
                text: "unbounded".to_string(),
                value: ConstantValue::Unbounded,
            };
        }
        let count = Type::Scalar(Scalar::I64);
        let read = self.typed_constant(node, Some(&count));
        let problem = match &read {
            Ok(Typed {
                value: ConstantValue::Int(count),
                ..
            }) if *count < 0 => Some("a list's size is not negative".to_string()),
            Ok(read) => self
                .other_kind(node, read, &[Scalar::I64])
                .map(|wrong| format!("a list's size is a count, and {wrong}")),
            Err(_) => None,
        };
        if let Some(problem) = problem {
            self.diagnostics.emit(error(
                Category::Type,
                codes::LIST_SIZE,
                self.at(node.span()),
                problem,
                "TYPE-10",
            ));
        }
        // A size reported, or one whose reading was, says nothing more
        // (ERR-9).
        let value = match read {
            Ok(read) => crate::constant::fit(read, Some(&count), self.program)
                .unwrap_or(ConstantValue::Unknown),
            Err(Fault::NotConstant) => ConstantValue::Unevaluated,
            Err(_) => ConstantValue::Unknown,
        };
        self.note_values_in(node);
        if let ConstantValue::Parameter { at, .. } = &value {
            self.note_list_size(*at);
        }
        Constant {
            text: text_of(node),
            value,
        }
    }

    /// What a constant read as `read` is, where its type is none of
    /// `kinds`: `` `F` is a constant of type `f64` `` for a name, and
    /// `this is a float` for any other. `None` where it is of one of
    /// them, or its type is not known.
    fn other_kind(&self, node: &SyntaxNode, read: &Typed, kinds: &[Scalar]) -> Option<String> {
        if matches!(read.value, ConstantValue::Null) {
            return Some("this is null".to_string());
        }
        let ty = read.ty()?;
        match ty {
            Type::Scalar(scalar) if kinds.contains(scalar) => return None,
            Type::Unknown | Type::Parameter { .. } | Type::PackMember { .. } => return None,
            _ => {}
        }
        // A lone name: a generic value parameter, or a constant of a
        // module (UNIT-13).
        if matches!(node.kind(), SyntaxKind::NameExpr | SyntaxKind::NamedType)
            && let Some(name) = name_token(node)
        {
            return Some(format!("`{}` is a constant of type `{ty}`", name.text()));
        }
        Some(format!(
            "this is {}",
            crate::constant::kind_of(&read.value, ty)
        ))
    }

    /// `dict<K, V>`: `dict_type = "dict", "<", value_type, ",", type, ">"`.
    fn dict(&mut self, node: &SyntaxNode, position: Position) -> Type {
        let mut parts = node.children().filter(|part| is_type(part.kind()));
        let key = parts.next().map_or(Type::Unknown, |key| {
            self.key(&key, "a dictionary's key");
            self.type_(&key, Position::value("a dictionary's key", "TYPE-9"))
        });
        let value = parts
            .next()
            .map_or(Type::Unknown, |value| self.type_(&value, position.part()));
        Type::Dict {
            key: Box::new(key),
            value: Box::new(value),
        }
    }

    /// `result<T, E>`: both payloads are value types, and "result and
    /// error envelopes are ordinary atomic values" (TYPE-13).
    fn result(&mut self, node: &SyntaxNode) -> Type {
        let position = Position::value("a result's payload", "TYPE-13");
        let mut parts = node.children().filter(|part| is_type(part.kind()));
        let mut next = |reader: &mut Self| {
            parts
                .next()
                .map_or(Type::Unknown, |part| reader.type_(&part, position))
        };
        let value = next(self);
        let error = next(self);
        Type::Result {
            value: Box::new(value),
            error: Box::new(error),
        }
    }

    // ---- Windows (TYPE-11) ----

    /// `rolling_window<V, maximum>` or `rolling_window<V, maximum, minimum>`.
    fn rolling_window(&mut self, node: &SyntaxNode, position: Position) -> Type {
        self.ordinary(node, position, false);
        let value = self.inner(node, Position::value("a window's value", "TYPE-11"));
        let mut sizes = node
            .children()
            .filter(|part| !is_type(part.kind()) && part.kind() != SyntaxKind::Error);
        let Some(maximum_node) = sizes.next() else {
            // The parser has reported the missing size.
            return Type::Unknown;
        };
        let minimum_node = sizes.next();
        // A size is a tick count or a duration: either is read as it is.
        let maximum_read = self.typed_constant(&maximum_node, None);
        let minimum_read = minimum_node
            .as_ref()
            .map(|node| self.typed_constant(node, None));
        let maximum = self.held(&maximum_node, maximum_read.clone(), None, None);
        let minimum = minimum_node
            .as_ref()
            .zip(minimum_read.clone())
            .map(|(node, read)| self.held(node, read, None, None));
        // GEN-13: a generic parameter named in a size is used as a value.
        for node in std::iter::once(&maximum_node).chain(minimum_node.as_ref()) {
            self.note_values_in(node);
        }
        // `zip` pairs the two when both are there.
        self.window_sizes(
            (&maximum_node, &maximum, &maximum_read),
            minimum_node
                .as_ref()
                .zip(minimum.as_ref())
                .zip(minimum_read.as_ref())
                .map(|((node, size), read)| (node, size, read)),
        );
        // "Omitted minimum equals maximum": a minimum written as the
        // maximum is the same window as one with none.
        let minimum = minimum.filter(|minimum| *minimum != maximum);
        Type::RollingWindow {
            value: Box::new(value),
            maximum,
            minimum,
        }
    }

    /// TYPE-11: "Its maximum and minimum have one constant kind: tick
    /// count or duration ... Omitted minimum equals maximum ... Tick sizes
    /// are positive; duration maximum is positive and minimum may be
    /// zero ... Minimum cannot exceed maximum."
    fn window_sizes(
        &mut self,
        (maximum_node, maximum, maximum_read): (&SyntaxNode, &Constant, &Result<Typed, Fault>),
        minimum: Option<(&SyntaxNode, &Constant, &Result<Typed, Fault>)>,
    ) {
        let maximum_kind = self.size_kind(maximum_node, maximum_read);
        let minimum_kind = match minimum {
            Some((node, _, read)) => self.size_kind(node, read),
            None => Some(SizeKind::Unknown),
        };
        let minimum = minimum.map(|(node, size, _)| (node, size));
        // A size of the wrong kind has been reported; its bounds mean
        // nothing.
        let (Some(maximum_kind), Some(minimum_kind)) = (maximum_kind, minimum_kind) else {
            return;
        };
        if let Some((node, _)) = minimum {
            let mixed = matches!(
                (maximum_kind, minimum_kind),
                (SizeKind::Ticks, SizeKind::Time) | (SizeKind::Time, SizeKind::Ticks)
            );
            if mixed {
                self.diagnostics.emit(
                    error(
                        Category::Type,
                        codes::ROLLING_WINDOW_SIZE_KIND,
                        self.at(node.span()),
                        format!(
                            "a window's sizes are of one kind: the maximum is {} and the \
                             minimum is {}",
                            maximum_kind.describe(),
                            minimum_kind.describe()
                        ),
                        "TYPE-11",
                    )
                    .with_related(self.at(maximum_node.span()), "the maximum"),
                );
                return;
            }
        }

        // The bounds of what is known.
        match maximum.value {
            ConstantValue::Int(ticks) if ticks <= 0 => {
                self.bounds(maximum_node, "a window holds at least one tick");
            }
            ConstantValue::Duration(length) if length <= 0 => {
                self.bounds(maximum_node, "a window's duration is more than zero");
            }
            _ => {}
        }
        let Some((minimum_node, minimum)) = minimum else {
            return;
        };
        match (&maximum.value, &minimum.value) {
            (_, ConstantValue::Int(ticks)) if *ticks <= 0 => {
                self.bounds(minimum_node, "a window's minimum is at least one tick");
            }
            (_, ConstantValue::Duration(length)) if *length < 0 => {
                self.bounds(minimum_node, "a window's minimum duration is not negative");
            }
            (ConstantValue::Int(maximum), ConstantValue::Int(minimum))
            | (ConstantValue::Duration(maximum), ConstantValue::Duration(minimum))
                if minimum > maximum =>
            {
                self.bounds(minimum_node, "a window's minimum cannot exceed its maximum");
            }
            _ => {}
        }
    }

    fn bounds(&mut self, node: &SyntaxNode, message: &str) {
        self.diagnostics.emit(error(
            Category::Type,
            codes::ROLLING_WINDOW_SIZE_BOUNDS,
            self.at(node.span()),
            message.to_string(),
            "TYPE-11",
        ));
    }

    /// The kind of one size of a window; `None`, with a report, when it
    /// is neither a tick count nor a duration.
    fn size_kind(&mut self, node: &SyntaxNode, read: &Result<Typed, Fault>) -> Option<SizeKind> {
        let Ok(read) = read else {
            return Some(SizeKind::Unknown);
        };
        let wrong = match read.ty() {
            Some(Type::Scalar(Scalar::I64)) => return Some(SizeKind::Ticks),
            Some(Type::Scalar(Scalar::Duration)) => return Some(SizeKind::Time),
            _ => match &read.value {
                ConstantValue::Unbounded => "this is `unbounded`".to_string(),
                _ => match self.other_kind(node, read, &[Scalar::I64, Scalar::Duration]) {
                    Some(wrong) => wrong,
                    None => return Some(SizeKind::Unknown),
                },
            },
        };
        self.diagnostics.emit(error(
            Category::Type,
            codes::ROLLING_WINDOW_SIZE_KIND,
            self.at(node.span()),
            format!("a window's size is a tick count or a duration, and {wrong}"),
            "TYPE-11",
        ));
        None
    }

    /// `delta<T>` as it is written (TYPE-16): what the table gives for
    /// T, or the delta itself where it is a type of its own or T is not
    /// yet known. "`signal`: None: `delta.unsupported_shape`".
    fn delta(&mut self, node: &SyntaxNode) -> Type {
        // What the delta is of, under a `?`.
        let mut of = type_child(node);
        while let Some(inner) = of
            .as_ref()
            .filter(|of| of.kind() == SyntaxKind::NullableType)
        {
            of = type_child(inner);
        }
        let none = of.filter(|of| of.kind() == SyntaxKind::SignalType);
        if let Some(of) = none {
            self.diagnostics.emit(
                error(
                    Category::Shape,
                    codes::DELTA_UNSUPPORTED_SHAPE,
                    self.at(node.span()),
                    format!("`{}` has no delta", text_of(&of)),
                    "TYPE-16",
                )
                .with_note("a `signal` has no payload, and publishes no value of its own"),
            );
            return Type::Unknown;
        }
        Type::delta(self.inner(node, Position::PLAIN))
    }
}

/// The two kinds a window's size may be (TYPE-11), and not known yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SizeKind {
    /// A count of ticks.
    Ticks,
    /// A duration.
    Time,
    /// A generic parameter or an expression this pass does not
    /// evaluate.
    Unknown,
}

impl SizeKind {
    fn describe(self) -> &'static str {
        match self {
            SizeKind::Ticks => "a tick count",
            SizeKind::Time => "a duration",
            SizeKind::Unknown => "not known",
        }
    }
}
