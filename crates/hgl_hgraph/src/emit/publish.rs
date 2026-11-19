//! What the hgraph target writes for a value a node publishes (TIME-7):
//! a whole value as hgraph's composite takes it, a delta as hgraph's
//! own, a null as hgraph's `Null`, and a constant connection of a
//! graph. Part of the transpiler, the last stage: `emit` writes a
//! module with them.

use super::*;

impl Emitter<'_> {
    /// What an assignment to `out` and a `yield` publish for `value`,
    /// written `written` (TIME-7): the runtime keeps what is published
    /// and the hook or the source goes on after it, so a copy where
    /// something else holds the value (`copied`), given as `bundled`
    /// gives it, and a null as hgraph's `Null` (`publishes_null`).
    pub(super) fn kept_publication(
        &mut self,
        function: &Function,
        value: &Expr,
        written: String,
    ) -> String {
        let written = self.copied(function, value, written);
        let written = self.bundled(function, value, written);
        if function.publishes_null(value) {
            self.published(written)
        } else {
            written
        }
    }

    /// What a node publishes for `value`, written `written`: a whole
    /// struct published to a temporal struct is given as each of its
    /// fields, as hgraph's bundle takes it (TYPE-9), and anything else,
    /// a struct's delta among them, as it is. A struct with a field
    /// that is a temporal struct too is not written whole yet.
    pub(super) fn bundled(&mut self, function: &Function, value: &Expr, written: String) -> String {
        let result = function
            .signature
            .and_then(|signature| signature.result.as_ref());
        if !matches!(function.mode, Mode::Node | Mode::Source) {
            return written;
        }
        // TYPE-16: a list's delta has `updated`, `removed` and
        // `size_modified`. An unsized list is a whole tuple here
        // (`list_annotation`): the provider applies the delta to the
        // list the node last published, its `_output` (see
        // `list_applied`), and publishes the whole list made.
        if let Some(result) = result
            && growing_list(result)
            && result != &value.ty
        {
            // A source's generator has no `_output` to apply it to.
            if function.mode == Mode::Node
                && matches!(&value.ty, Type::Delta(of) if growing_list(of))
            {
                self.natives = true;
                return format!("_core.list_applied(_output, {written})");
            }
            let unwritten = if function.mode == Mode::Source {
                "a delta of an unsized list yielded by a source"
            } else {
                "a delta published to an unsized list"
            };
            self.unsupported(value.at, unwritten);
            return written;
        }
        // TYPE-16: "A `list<V, N>` keeps its bounds: ... one in a delta
        // made as the program runs fails the write that publishes it".
        if let Some(Type::List {
            size: Some(size), ..
        }) = result
            && let ConstantValue::Int(count) = size.value
            && matches!(&value.ty, Type::Delta(of) if matches!(**of, Type::List { .. }))
        {
            self.natives = true;
            return format!("_core.list_within({written}, {count})");
        }
        // TIME-7: a node's whole dictionary is "a complete value of the
        // result": it replaces what the dictionary held, and each key it
        // has not is removed from what the node last published, its
        // `_output` (see `dict_replaced`).
        if function.mode == Mode::Node && result == Some(&value.ty) && whole_dictionary(&value.ty) {
            self.natives = true;
            return format!("_core.dict_replaced(_output, {written})");
        }
        match result {
            // A struct with no field publishes its one value, for a
            // whole value and for its delta alike (`empty_struct`).
            Some(result) if empty_struct(self.program, result) => {
                self.bundle_of(result, value.at, written)
            }
            Some(result) if whole_value(result, value) => self.bundle_of(result, value.at, written),
            _ => written,
        }
    }

    /// `written`, an ordinary value of `ty` known where the graph is
    /// built, wired as a constant connection (FN-3: "an ordinary value
    /// in a branch is then wired as a constant").
    ///
    /// hgraph's `const` infers a `TS` of the whole value, and a composite's
    /// connection is no `TS` (TYPE-9): a struct's and a tuple's is a
    /// bundle, a fixed list's a `TSL`, a set's a `TSS`, a dictionary's a
    /// `TSD`. So a composite's constant is told its time-series type, and
    /// is given as that type publishes a whole value (`bundle_of`): a
    /// struct's fields, a tuple's positions, a list's elements. A set and
    /// a dictionary of values that publish whole are given as they are,
    /// which a constant's one publication takes whole: it has no earlier
    /// members or keys, and a null entry is hgraph's `Null`. A dictionary
    /// of composites is reported.
    pub(super) fn constant_connection(
        &mut self,
        ty: &Type,
        at: Location,
        written: String,
    ) -> String {
        let composite = matches!(
            ty,
            Type::Named {
                kind: NamedKind::Struct,
                ..
            } | Type::Tuple(_)
                | Type::List { .. }
                | Type::Set(_)
                | Type::Dict { .. }
        );
        if !composite {
            return format!("_hg.const({written})");
        }
        let Some(tp) = self.annotation(ty, true, at) else {
            return "None".to_string();
        };
        let whole = match ty {
            // A dictionary's composite value would be given its child's
            // update, a struct's fields, which is not written here.
            Type::Dict { value, .. } if !publishes_whole(value) => {
                self.unsupported(
                    at,
                    "an ordinary dictionary of composites as a branch's connection",
                );
                return "None".to_string();
            }
            // A null entry is published as hgraph's `Null`: `None` would
            // leave its child unset.
            Type::Dict { value, .. } if is_nullable(value) => {
                self.natives = true;
                format!("{{_key: _core.published(_entry) for _key, _entry in ({written}).items()}}")
            }
            Type::Set(_) | Type::Dict { .. } => written,
            _ => self.bundle_of(ty, at, written),
        };
        format!("_hg.const({whole}, tp={tp})")
    }

    /// `value`, written `written` in a graph, where a connection of the
    /// temporal struct `ty` is expected: the connection as it is, or,
    /// where it is of a struct that inherits from the abstract `ty`, a
    /// bundle of `ty` made of the fields `ty` has, each the value's own
    /// field. TYPE-3: "A struct is a value where an abstract struct it
    /// inherits from is expected". hgraph binds a bundle to the schema it
    /// was made with alone, and the abstract struct's bundle has its own
    /// fields (`struct_annotation`).
    pub(super) fn narrowed(
        &mut self,
        function: &Function,
        ty: &Type,
        value: &Expr,
        written: String,
    ) -> String {
        // A connection given across a reference (TYPE-12, TIME-10)
        // between two struct types, one inheriting from the other,
        // would be narrowed on the far side of the reference
        // (`Type::followed`), which is not written yet.
        let (expected, given) = (ty.followed(), value.ty.followed());
        let crosses = expected != *ty || given != value.ty;
        if function.mode == Mode::Graph
            && crosses
            && expected != given
            && matches!(
                expected,
                Type::Named {
                    kind: NamedKind::Struct,
                    ..
                }
            )
            && self.connects(function, value)
        {
            self.unsupported(
                value.at,
                "a struct connection given across a reference to a struct it inherits from",
            );
            return written;
        }
        let Type::Named {
            module,
            declaration,
            kind: NamedKind::Struct,
            ..
        } = ty
        else {
            return written;
        };
        let below =
            matches!(value.ty, Type::Named { kind: NamedKind::Struct, .. } if value.ty != *ty);
        if function.mode != Mode::Graph || !below || !self.connects(function, value) {
            return written;
        }
        let fields: Vec<String> = match self.program.modules[*module].details.get(*declaration) {
            Some(Detail::Struct(read)) => read
                .members
                .iter()
                .map(|member| name(&member.name))
                .collect(),
            _ => Vec::new(),
        };
        let Some(bundle) = self.annotation(ty, true, value.at) else {
            return written;
        };
        self.natives = true;
        format!("_core.narrowed({written}, {bundle}, {})", tuple(&fields))
    }

    /// What publishes `written`, a whole value of the type `ty`, to a
    /// part of `out` of that type or as a lifted node's result (TIME-7):
    /// a whole value as `bundle_of` gives it, and a null as hgraph's
    /// `Null` where `ty` publishes it whole. EXPR-3: "The value is of
    /// the field's type", so a part takes a whole value and no
    /// `delta<T>`, which the analyser rejects; a lifted function's
    /// result is a value. `at` is where what is not written yet is
    /// reported.
    pub(super) fn published_to(&mut self, ty: &Type, at: Location, written: String) -> String {
        if publishes_whole_null(ty) {
            return self.published(written);
        }
        self.bundle_of(ty, at, written)
    }

    /// `written`, a whole value of `ty`, as a temporal value of `ty`
    /// publishes it: a struct's fields as hgraph's bundle takes them
    /// (TYPE-9), and any other value as it is.
    pub(super) fn bundle_of(&mut self, ty: &Type, at: Location, written: String) -> String {
        // A struct with no field is a `TS` of its class: what it publishes
        // is its one value, made by the provider of whatever is published,
        // a whole value or a delta, which has no property.
        if empty_struct(self.program, ty)
            && let Some(class) = self.struct_class(ty)
        {
            self.natives = true;
            return format!("_core.only_value({written}, {class})");
        }
        match ty {
            // An unsized list's tick is the whole list, as a tuple that
            // is made of it: hgraph keeps no list the program goes on
            // changing.
            Type::List { size: None, .. } => return format!("_py.tuple({written})"),
            // A fixed list's `TSL` takes a tuple of its elements, each
            // one's update as that position's own (TYPE-9): an element
            // that publishes whole and is not null is set by it.
            Type::List { element, .. } if whole_scalar(element) => {
                return format!("_py.tuple({written})");
            }
            _ => {}
        }
        let simple = match ty {
            Type::Named {
                module,
                declaration,
                kind: NamedKind::Struct,
                ..
            } => !inherited(self.program, *module, *declaration),
            // `whole_shape` reports a fixed list with an element that
            // is composite or nullable, and a dictionary, whose `TSD`
            // takes its keys' changes: a whole dictionary also removes
            // each key it has not, which hgraph is not told here.
            Type::Tuple(_) | Type::List { .. } | Type::Dict { .. } => false,
            _ => return written,
        };
        let Some(shape) = self.whole_shape(ty, at) else {
            return written;
        };
        self.natives = true;
        // A struct of its own fields, each of which publishes whole, is
        // published by the fields its class has.
        if simple && shape.flat {
            return format!("_core.bundled({written})");
        }
        format!("_core.bundled({written}, {})", shape.text)
    }

    /// How a whole value of `ty`, a part of what a temporal struct or
    /// tuple publishes, is given to hgraph (`bundle_of`), as the
    /// provider's `bundled` reads it: `None` for a part that publishes
    /// whole, a `dict` of each field's for a struct, a tuple of each
    /// position's for a tuple, and `[None]` for a fixed list of
    /// elements that publish whole. `None`, reported, for a part not
    /// written yet.
    ///
    /// TIME-7: a complete value is "published whole". TYPE-9 maps a
    /// struct's and a tuple's parts "to child fields", so a whole value
    /// sets each part: a struct's or a tuple's field by its own parts, a
    /// fixed list by a tuple of its elements, and a set as the members
    /// it has now, hgraph's `TSS` removing those it no longer has. A
    /// field of an abstract struct is a field of the struct below it
    /// that the value is, read by its name. A dictionary's `TSD` would
    /// take its keys' changes, and not remove a key the value has not,
    /// and a fixed list with an element that is null or composite is
    /// not written whole yet.
    pub(super) fn whole_shape(&mut self, ty: &Type, at: Location) -> Option<Shape> {
        match ty {
            Type::Named {
                kind: NamedKind::Struct,
                ..
            } => {
                let Some(fields) = fields_of(self.program, ty) else {
                    self.unsupported(at, &format!("a whole `{ty}` published"));
                    return None;
                };
                let mut flat = true;
                let mut written = Vec::new();
                for (member, field) in &fields {
                    let part = self.whole_shape(field, at)?;
                    flat &= part.leaf;
                    written.push(format!("{}: {}", string(&name(&member.name)), part.text));
                }
                Some(Shape {
                    text: format!("{{{}}}", written.join(", ")),
                    leaf: false,
                    flat,
                })
            }
            Type::Tuple(elements) => {
                let mut flat = true;
                let mut written = Vec::new();
                for element in elements {
                    let part = self.whole_shape(element, at)?;
                    flat &= part.leaf;
                    written.push(part.text);
                }
                Some(Shape {
                    text: format!("({},)", written.join(", ")),
                    leaf: false,
                    flat,
                })
            }
            Type::List {
                element,
                size: Some(_),
            } if whole_scalar(element) => Some(Shape {
                text: "[None]".to_string(),
                leaf: false,
                flat: true,
            }),
            Type::List { .. } => {
                self.unsupported(
                    at,
                    "a whole list published where an element is temporal composite or nullable",
                );
                None
            }
            // A node's whole dictionary of values that publish whole is
            // written (`bundled`); one of composites is not.
            Type::Dict { value, .. } if !publishes_whole(value) => {
                self.unsupported(at, "a whole dictionary of composites");
                None
            }
            Type::Dict { .. } => {
                self.unsupported(at, "a whole dictionary published to a temporal dictionary");
                None
            }
            Type::Parameter { .. } | Type::Unknown => {
                self.unsupported(at, &format!("a whole `{ty}` published"));
                None
            }
            _ => Some(Shape {
                text: "None".to_string(),
                leaf: true,
                flat: true,
            }),
        }
    }

    /// The delta of a temporal value of `ty` as the program reads it,
    /// given `delta`, what hgraph gives for it (END-3, TYPE-16). It is the
    /// one reading of a delta, for a direct input, a child of one at any
    /// depth (END-8) and a struct that holds a tuple: hgraph's own where
    /// it is the program's, a fixed list's `dict` of each position that
    /// changed, its `updated` (see `list_delta`), and a dictionary's
    /// `dict` of each key changed, with `REMOVE` for each removed (see
    /// `dict_delta`); and where `ty` holds a tuple, whose bundle gives a
    /// `dict` by each position's field, the tuple of each position's
    /// delta, null where it did not change (`positions`, `delta_shape`).
    /// An unsized list ticks whole here (`list_annotation`), and does not
    /// say which positions changed: its delta is reported.
    pub(super) fn language_delta(&mut self, ty: &Type, delta: String, at: Location) -> String {
        if growing_list(ty) {
            self.unsupported(at, "the delta of an unsized list");
            return "None".to_string();
        }
        // TYPE-16: a struct's delta has "a property for each field": one
        // with no field has none, and its delta is the empty one.
        if empty_struct(self.program, ty) {
            self.natives = true;
            return "_core.struct_delta()".to_string();
        }
        match self.delta_shape(ty, at) {
            Some(shape) => {
                self.natives = true;
                format!("_core.positions({delta}, {shape})")
            }
            None => delta,
        }
    }

    /// How hgraph's delta of a temporal value of `ty` is read where it
    /// holds a tuple (TYPE-16), as the provider's `positions` reads it:
    /// a tuple of each position's for a tuple, a `dict` of each field
    /// that holds one for a struct. `None` where `ty` holds no tuple, and
    /// hgraph's delta is the program's as it is. A list or a dictionary
    /// of what holds a tuple is reported: its delta is not read yet.
    fn delta_shape(&mut self, ty: &Type, at: Location) -> Option<String> {
        match ty {
            Type::Tuple(elements) => {
                let parts: Vec<String> = elements
                    .iter()
                    .map(|element| {
                        self.delta_shape(element, at)
                            .unwrap_or_else(|| "None".to_string())
                    })
                    .collect();
                Some(format!("({},)", parts.join(", ")))
            }
            Type::Named {
                kind: NamedKind::Struct,
                ..
            } => {
                let fields = fields_of(self.program, ty)?;
                let parts: Vec<String> = fields
                    .iter()
                    .filter_map(|(member, field)| {
                        let part = self.delta_shape(field, at)?;
                        Some(format!("{}: {part}", string(&name(&member.name))))
                    })
                    .collect();
                (!parts.is_empty()).then(|| format!("{{{}}}", parts.join(", ")))
            }
            Type::List { element: inner, .. } | Type::Dict { value: inner, .. } => {
                if self.delta_shape(inner, at).is_some() {
                    self.unsupported(at, "the delta of a list or a dictionary that holds a tuple");
                }
                None
            }
            _ => None,
        }
    }

    /// What publishes `value`, of a nullable type: the value, or
    /// hgraph's `Null` where it is null. To hgraph a node that returns
    /// `None` publishes nothing, and one that returns `Null` publishes
    /// a value that is unset (TIME-7: "`return null` for an `i64?`
    /// result [publishes] a null").
    pub(super) fn published(&mut self, value: String) -> String {
        self.natives = true;
        format!("_core.published({value})")
    }
}

/// How a whole value of a type is given to hgraph, part by part, as
/// the provider's `bundled` reads it (`Emitter::whole_shape`).
pub(super) struct Shape {
    /// The provider's text for it: `None`, a `dict`, a tuple or `[None]`.
    text: String,
    /// True for a part that publishes whole, as it is.
    leaf: bool,
    /// True where each of its own parts is a leaf.
    flat: bool,
}

/// True where `statements` return a value of `ty` itself, at any depth.
pub(super) fn returns_whole(statements: &[Stmt], ty: &Type) -> bool {
    let mut found = false;
    hgl_analyser::hir::walk_statement_nodes(statements, &mut |node| {
        if let hgl_analyser::hir::Node::Statement(Stmt::Return {
            value: Some(value), ..
        }) = node
        {
            found |= value.ty == *ty;
        }
    });
    found
}
