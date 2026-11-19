//! The type annotations the hgraph target writes: a scalar as Python's
//! type, a temporal value as hgraph's time-series type (`TS`, `TSS`,
//! `TSL`, `TSB`, `TSD`, `TSW`, `REF`, `SIGNAL`), and a node's or a
//! graph's signature. Part of the transpiler, the last stage: `emit`
//! writes a module with them.

use super::*;

impl Emitter<'_> {
    /// The annotation of a parameter or a result of a temporal
    /// function: `TS[int]` for a temporal `i64`, `int` for a `const`
    /// one. `None`, reported, for a type not written yet.
    pub(super) fn annotation(&mut self, ty: &Type, temporal: bool, at: Location) -> Option<String> {
        // TYPE-9: temporalization maps "sets to membership". A
        // temporal `set<T>` is hgraph's `TSS[T]`, whose tick
        // is the set's delta, and a `const` one a `frozenset`.
        if let Type::Set(element) = ty {
            let element = self.annotation(element, false, at)?;
            return Some(if temporal {
                self.hgraph = true;
                format!("_hg.TSS[{element}]")
            } else {
                format!("_py.frozenset[{element}]")
            });
        }
        if temporal && let Type::List { element, size } = ty {
            return self.list_annotation(ty, element, size.as_ref(), at);
        }
        // TYPE-9: temporalization maps "dictionaries to keyed
        // children": hgraph's `TSD`, a child for each key, each child
        // temporalized in turn. Its key is a value, a scalar or an enum.
        if temporal && let Type::Dict { key, value } = ty {
            let written = self.annotation(key, false, at)?;
            // A held key is an object to hgraph (`held`).
            if held(key) {
                self.unsupported(at, &format!("the type `{ty}`"));
                return None;
            }
            let child = self.child_annotation(ty, value, at)?;
            self.hgraph = true;
            return Some(format!("_hg.TSD[{written}, {child}]"));
        }
        if temporal && let Type::Tuple(elements) = ty {
            return self.tuple_annotation(ty, elements, at);
        }
        if let Type::RollingWindow {
            value,
            maximum,
            minimum,
        } = ty
        {
            return self.window_annotation(ty, value, (maximum, minimum.as_ref()), temporal, at);
        }
        // A list, a tuple or a dictionary that is a node's configuration
        // is given to hgraph inside an object it keeps as it is
        // (`held`): to hgraph it is an object.
        if !temporal && held(ty) {
            return Some("_py.object".to_string());
        }
        // TYPE-14: "`T?` denotes an ordinary value that is either a T
        // or null". hgraph has a value that holds one `T` or is unset
        // where the type under `TS[...]` is `T | None`, and a tick of
        // it may be the unset one: a published null.
        if let Type::Nullable(inner) = ty {
            // TYPE-14: on a type that is no ordinary value the `?`
            // marks an input that may be left out, and is no part of
            // what is published (`Type::omission_removed`): the input
            // is the type under it, and one left out is never valid
            // (UNIT-8). A window that may be left out is not written
            // yet.
            let under = ty.omission_removed();
            if temporal && !matches!(under, Type::Nullable(_) | Type::RollingWindow { .. }) {
                return self.annotation(under, true, at);
            }
            // As a temporal value only a scalar or an enum may be
            // unset this way: it publishes whole. A set publishes its
            // changes, which hgraph's `TS` of a set that may be unset
            // would not gather, so a nullable set is not written.
            let whole = scalar(inner).is_some()
                || matches!(
                    &**inner,
                    Type::Named {
                        kind: NamedKind::Enum,
                        ..
                    }
                );
            if temporal && !whole {
                self.unsupported(at, &format!("the type `{ty}`"));
                return None;
            }
            let inner = self.annotation(inner, false, at)?;
            return Some(if temporal {
                self.hgraph = true;
                format!("_hg.TS[{inner} | None]")
            } else {
                format!("{inner} | None")
            });
        }
        // TYPE-12: "`ref<T>` designates an endpoint, not an ordinary
        // payload type": hgraph's `REF` of the temporal T, whose value
        // is a reference to an output. TIME-10: "A reference ticks on
        // changed designation, not every target tick", as a `REF`
        // input ticks when the reference it is given changes, and not
        // when what it points at publishes.
        if temporal && let Type::Ref(target) = ty {
            let target = self.annotation(target, true, at)?;
            self.hgraph = true;
            return Some(format!("_hg.REF[{target}]"));
        }
        // TYPE-12: "`signal` is a complete temporal input constraint
        // with no readable payload": hgraph's `SIGNAL`, an input bound
        // to any output, which ticks when it does and holds no value.
        if temporal && matches!(ty, Type::Signal) {
            self.hgraph = true;
            return Some("_hg.SIGNAL".to_string());
        }
        // TYPE-9: "`atomic<V>` stops expansion at a complete ordinary
        // V": hgraph's `TS[V]`, whose tick is the whole value. Its V is
        // written as a value is; a held list, tuple or dictionary is
        // not written as a temporal payload yet.
        if let Type::Atomic(payload) = ty {
            let payload_written = self.annotation(payload, false, at)?;
            if !temporal {
                return Some(payload_written);
            }
            if held(payload) {
                self.unsupported(at, &format!("the type `{ty}`"));
                return None;
            }
            self.hgraph = true;
            return Some(format!("_hg.TS[{payload_written}]"));
        }
        if let Type::Named {
            module,
            declaration,
            kind: NamedKind::Struct,
            ..
        } = ty
        {
            return self.struct_annotation(ty, (*module, *declaration), temporal, at);
        }
        let python = match (scalar(ty), ty) {
            (Some(python), _) => {
                self.datetime |= python.starts_with("_dt.");
                self.natives |= python.starts_with("_core.");
                python.to_string()
            }
            // An enum is a scalar to hgraph, as its Python `Enum` is.
            (
                None,
                Type::Named {
                    module,
                    declaration,
                    kind: NamedKind::Enum,
                    ..
                },
            ) => self.type_name(*module, *declaration),
            // ERR-2 and ERR-4: a `result` and an `error` are no
            // composite, and publish whole (TYPE-9): the provider's
            // classes, `Ok` and `Err` both a `Result`.
            (None, Type::Error) => {
                self.natives = true;
                "_core.Error".to_string()
            }
            // ERR-4: what an error's `.context` holds, the provider's
            // `ErrorContext`.
            (None, Type::ErrorContext) => {
                self.natives = true;
                "_core.ErrorContext".to_string()
            }
            (None, Type::Result { .. }) => {
                self.natives = true;
                "_core.Result".to_string()
            }
            _ => {
                self.unsupported(at, &format!("the type `{ty}`"));
                return None;
            }
        };
        if temporal {
            self.hgraph = true;
            Some(format!("_hg.TS[{python}]"))
        } else {
            Some(python)
        }
    }

    /// The annotation of `ty`, as `annotation` writes it, or `_py.object`
    /// where it is not written: that is reported, and the piece is left
    /// out.
    pub(super) fn annotation_or_object(
        &mut self,
        ty: &Type,
        temporal: bool,
        at: Location,
    ) -> String {
        self.annotation(ty, temporal, at)
            .unwrap_or_else(|| "_py.object".to_string())
    }

    /// The temporal annotation of `child`, a field, a position or a
    /// keyed child of the temporal composite `ty` (TYPE-9).
    pub(super) fn child_annotation(
        &mut self,
        ty: &Type,
        child: &Type,
        at: Location,
    ) -> Option<String> {
        // A reference as a child, and a field that may be left out
        // because its type is no ordinary value (TYPE-14), are not
        // written yet. Nor is a struct with no field, a `TS` of its class
        // (`struct_annotation`): the delta of a composite that holds one
        // would give it a struct's delta, which it does not take.
        let under = child.omission_removed();
        let omittable = matches!(child, Type::Nullable(_)) && !matches!(under, Type::Nullable(_));
        if matches!(under, Type::Ref(_)) || omittable || empty_struct(self.program, child) {
            self.unsupported(at, &format!("the type `{ty}`"));
            return None;
        }
        self.annotation(child, true, at)
    }

    /// The annotation of a temporal list `ty` of `element`, of `size`.
    ///
    /// TYPE-9: temporalization maps "lists to child positions". A list
    /// of a fixed size N (TYPE-10: "fixed sizes are nonnegative integer
    /// constants") is hgraph's `TSL` of N children, each temporalized
    /// in turn, whose tick is a `dict` of each position that changed:
    /// TYPE-16's `dict<i64, delta<V>>`.
    ///
    /// TYPE-10: "`list<T>` denotes a growing shape": hgraph's unbounded
    /// `TSL`, `Size[-1]` (hgraph 0.9.5), a child for each position it
    /// holds, each temporalized in turn, valid on its own, whose tick is
    /// a `TSLDelta` of each position that changed, those that lost their
    /// value and the new size where it moved: TYPE-16's list delta.
    pub(super) fn list_annotation(
        &mut self,
        ty: &Type,
        element: &Type,
        size: Option<&Constant>,
        at: Location,
    ) -> Option<String> {
        match size.map(|size| &size.value) {
            // TYPE-10 allows a list of no position, and TIME-9 has a
            // published empty list valid, but hgraph 0.9.3's `TSL` of
            // none publishes nothing ("unresolved element type").
            Some(ConstantValue::Int(0)) => {
                self.unsupported(at, &format!("the type `{ty}`"));
                None
            }
            Some(ConstantValue::Int(count)) => {
                let child = self.child_annotation(ty, element, at)?;
                self.hgraph = true;
                Some(format!("_hg.TSL[{child}, _hg.Size[{count}]]"))
            }
            None => {
                let child = self.child_annotation(ty, element, at)?;
                self.hgraph = true;
                Some(format!("_hg.TSL[{child}, _hg.Size[-1]]"))
            }
            _ => {
                self.unsupported(at, &format!("the type `{ty}`"));
                None
            }
        }
    }

    /// The annotation of the window `ty`, of `value`, with its maximum
    /// and its minimum, `None` where none is written.
    ///
    /// TYPE-11: "`rolling_window<V, Max, Min>` is a rolling window of V.
    /// Its maximum and minimum have one constant kind: tick count or
    /// duration ... Omitted minimum equals maximum". It is hgraph's
    /// `TSW`, whose sizes are a count of ticks or a `timedelta`, and
    /// which takes one size as both its maximum and its minimum, as
    /// TYPE-11 does. TYPE-16: a window's delta is "V: the value that
    /// arrived", and a tick of a `TSW` is one value added to it.
    ///
    /// hgraph's window holds values that are scalars to it, so V is a
    /// scalar or an enum here. A window of a nullable, of a struct and
    /// of a collection is reported, and so is a size not known as the
    /// module is written: one that names a generic parameter.
    fn window_annotation(
        &mut self,
        ty: &Type,
        value: &Type,
        (maximum, minimum): (&Constant, Option<&Constant>),
        temporal: bool,
        at: Location,
    ) -> Option<String> {
        // TYPE-12: a window is no ordinary value, so it is only ever a
        // temporal one.
        if !temporal || !whole_scalar(value) {
            self.unsupported(at, &format!("the type `{ty}`"));
            return None;
        }
        let value = self.annotation(value, false, at)?;
        let mut sizes = Vec::new();
        for size in std::iter::once(maximum).chain(minimum) {
            let written = match &size.value {
                ConstantValue::Int(ticks) => ticks.to_string(),
                ConstantValue::Duration(microseconds) => {
                    self.datetime = true;
                    format!("_dt.timedelta(microseconds={microseconds})")
                }
                _ => {
                    self.unsupported(at, &format!("the type `{ty}`"));
                    return None;
                }
            };
            sizes.push(format!("_hg.WindowSize[{written}]"));
        }
        self.hgraph = true;
        Some(format!("_hg.TSW[{value}, {}]", sizes.join(", ")))
    }

    /// True where a node may read `ty` as an input or as its own `out`
    /// for each window it holds, itself or at any depth (`windows_in`);
    /// otherwise reported at `at`.
    ///
    /// FN-12: "Fixed-size lists and windows retain their own
    /// shape/readiness contracts", and TYPE-11: "window publications
    /// obey core's contract". Core has no such contract yet: it does
    /// not say whether a window is valid before it holds its minimum,
    /// how a duration is held, or whether a value leaves a window of a
    /// duration as time passes with none arriving. Only a window of
    /// ticks whose minimum is one tick is the same under every reading:
    /// it is valid from the first value that arrives, and a value
    /// leaves it only as another arrives, as hgraph's `TSW` has it. A
    /// window inside a struct, a tuple, a list or a dictionary decides
    /// that one's validity too (TIME-9).
    pub(super) fn readable_window(&mut self, ty: &Type, at: Location) -> bool {
        for window in windows_in(self.program, ty) {
            let Type::RollingWindow {
                maximum, minimum, ..
            } = &window
            else {
                continue;
            };
            // "Omitted minimum equals maximum."
            let minimum = minimum.as_ref().unwrap_or(maximum);
            if !matches!(minimum.value, ConstantValue::Int(1)) {
                self.unsupported(
                    at,
                    &format!("the readiness of `{window}`, which core leaves undefined,"),
                );
                return false;
            }
        }
        true
    }

    /// The annotation of the struct `ty`, declared at `(module,
    /// declaration)`: its class as a value (`struct_class`), and as a
    /// temporal value hgraph's bundle, `TSB`.
    ///
    /// TYPE-9: "Temporalization recursively maps tuples/structs to
    /// child fields". The bundle's schema has a field for each field
    /// the struct has, a parent's first, each temporalized in turn:
    /// `bid: f64` is a `TS[float]` of it. A generic struct's field has
    /// the type its arguments give it (TYPE-4), so a `Box<i64>`'s
    /// `value: T` is a `TS[int]`. The provider makes the schema once
    /// for the struct's class, beside it (`_core.schema`), so that
    /// every annotation of the struct names one schema; a generic
    /// struct's class is one for each application.
    ///
    /// An abstract struct's bundle has the fields it has itself, which
    /// every struct below it has too: a field is read and changed by
    /// its port, its delta has "a property for each field" (TYPE-16),
    /// and a whole value, of a struct below it, sets those fields
    /// (`whole_shape`). A struct's connection is given where one of an
    /// abstract struct it inherits from is expected as a bundle of the
    /// abstract struct's fields (`narrowed`). As a value it may be of
    /// any struct below it, with fields its bundle has not, so a value
    /// of it, and its temporal value read whole (`port_value`), are not
    /// written yet.
    pub(super) fn struct_annotation(
        &mut self,
        ty: &Type,
        (module, declaration): (usize, usize),
        temporal: bool,
        at: Location,
    ) -> Option<String> {
        let program = self.program;
        let is_abstract = inherited(program, module, declaration);
        let class = match self.struct_class(ty) {
            Some(class) if temporal || !is_abstract => class,
            _ => {
                self.unsupported(at, &format!("the type `{ty}`"));
                return None;
            }
        };
        if !temporal {
            return Some(class);
        }
        let Some(fields) = fields_of(program, ty) else {
            self.unsupported(at, &format!("the type `{ty}`"));
            return None;
        };
        // A struct with no field: hgraph's bundle with no field is never
        // valid and never ticks, where TIME-9 has "a published empty
        // collection" valid and all valid. Its one value is published
        // whole, as a `TS` of its class (`empty_struct`). An abstract
        // one's value would be of a struct below it.
        if fields.is_empty() {
            if is_abstract {
                self.unsupported(at, &format!("the type `{ty}`"));
                return None;
            }
            self.hgraph = true;
            return Some(format!("_hg.TS[{class}]"));
        }
        let mut written = Vec::new();
        for (member, field) in &fields {
            let field = self.child_annotation(ty, field, at)?;
            written.push(format!("{}: {field}", string(&name(&member.name))));
        }
        self.hgraph = true;
        self.natives = true;
        Some(format!(
            "_hg.TSB[_core.schema({class}, {{{}}})]",
            written.join(", ")
        ))
    }

    /// The class of a value of the struct `ty`: `Quote`, and for a
    /// generic struct the class for its arguments, `_core.applied(Box,
    /// "i64")`. TYPE-3: "Generic applications are invariant by origin
    /// and complete canonical arguments" (see `construct`). `None` for a
    /// type that is no struct, and where an argument is not known as
    /// the module is written.
    pub(super) fn struct_class(&mut self, ty: &Type) -> Option<String> {
        let Type::Named {
            module,
            declaration,
            arguments,
            kind: NamedKind::Struct,
            ..
        } = ty
        else {
            return None;
        };
        let class = self.type_name(*module, *declaration);
        if arguments.is_empty() {
            return Some(class);
        }
        let texts: Vec<String> = arguments
            .iter()
            .map(|argument| canonical_argument(self.program, argument))
            .collect::<Option<_>>()?;
        self.natives = true;
        Some(format!(
            "_core.applied({class}, {})",
            string(&texts.join(", "))
        ))
    }

    /// The annotation of a temporal tuple `ty` of `elements`.
    ///
    /// TYPE-9: "Temporalization recursively maps tuples/structs to
    /// child fields". TYPE-10: "A tuple retains positional identity and
    /// is not a fixed list alias". A tuple is hgraph's bundle with no
    /// name, `TSB["_0": ..., "_1": ...]`, a field for each position,
    /// each temporalized in turn: hgraph's `TSL` holds children of one
    /// type, and a bundle's fields are each valid on their own, of a
    /// type of their own. A bundle with no name is matched by its
    /// fields, as a tuple type is by its elements. Its tick is a `dict`
    /// of each position that changed, which the program reads as
    /// TYPE-16's `tuple<delta<A>?, delta<B>?>` (`positions`), and
    /// hgraph's bundle takes a tuple, a `None` where a position does
    /// not change, as that delta.
    pub(super) fn tuple_annotation(
        &mut self,
        ty: &Type,
        elements: &[Type],
        at: Location,
    ) -> Option<String> {
        if elements.is_empty() {
            self.unsupported(at, &format!("the type `{ty}`"));
            return None;
        }
        let mut fields = Vec::new();
        for (index, element) in elements.iter().enumerate() {
            let child = self.child_annotation(ty, element, at)?;
            fields.push(format!("{}: {child}", string(&position(index))));
        }
        self.hgraph = true;
        Some(format!("_hg.TSB[{}]", fields.join(", ")))
    }

    /// The parameters of a temporal function as Python writes them, and
    /// its result's annotation.
    pub(super) fn temporal_signature(
        &mut self,
        declaration: &Declaration,
        signature: &Signature,
    ) -> (Vec<String>, Option<String>) {
        let mut parameters = Vec::new();
        for parameter in &signature.parameters {
            // A `const` parameter's default is written where a call
            // leaves it out. A temporal input's is "wired as a constant
            // of its default" (UNIT-8), which is not written yet.
            if parameter.pack.is_some() {
                self.unsupported(parameter.at, "a pack of a temporal function");
            } else if parameter.has_default && !parameter.constant {
                self.unsupported(parameter.at, "a default of a temporal input");
            }
            let annotation =
                self.annotation_or_object(&parameter.ty, !parameter.constant, parameter.at);
            parameters.push(format!("{}: {annotation}", name(&parameter.name)));
        }
        let result = signature
            .result
            .as_ref()
            .and_then(|result| self.annotation(result, true, declaration.at));
        (parameters, result)
    }

    /// Reports each input of a node or a source that holds a window,
    /// itself or within, whose readiness is not known
    /// (`readable_window`): the node reads it, if only by requiring it
    /// valid (TIME-3). A graph only wires one.
    pub(super) fn window_inputs(&mut self, signature: &Signature) {
        for parameter in &signature.parameters {
            if !parameter.constant {
                self.readable_window(&parameter.ty, parameter.at);
            }
        }
    }

    /// Reports a node whose result holds a window (`windows_in`) and
    /// that may publish to it more than once in one evaluation: hgraph's
    /// `TSW` takes one value in a cycle and fails at a second, where
    /// TIME-7 and END-5 have the last write be what is published.
    ///
    /// A `return` with a value ends the evaluation (FN-7), so returns
    /// alone publish once. A write through `out`, to it or to a part of
    /// it, may be followed by another: it is written only where it is
    /// the node's one place that publishes, and stands in no loop.
    pub(super) fn window_published_once(&mut self, signature: &Signature, body: &Body) {
        use hgl_analyser::hir::{Node, walk_statement_nodes};
        let program = self.program;
        if signature
            .result
            .as_ref()
            .is_none_or(|result| windows_in(program, result).is_empty())
        {
            return;
        }
        let locals = &body.locals;
        let out =
            |local: usize| locals[local].kind == LocalKind::Inject && locals[local].name == "out";
        // Each place in `statements` that publishes, and whether it is
        // a write through `out`.
        let writes = |statements: &[Stmt]| {
            let mut found: Vec<(Location, bool)> = Vec::new();
            walk_statement_nodes(statements, &mut |node| match node {
                Node::Statement(Stmt::Assign { local, at, .. }) if out(*local) => {
                    found.push((*at, true));
                }
                Node::Statement(Stmt::AssignPlace { place, at, .. }) => {
                    if matches!(place.place_root().kind, ExprKind::Local(local) if out(local)) {
                        found.push((*at, true));
                    }
                }
                Node::Statement(Stmt::Return {
                    value: Some(_), at, ..
                }) => found.push((*at, false)),
                _ => {}
            });
            found
        };
        let all = writes(&body.statements);
        let mut looped: Vec<Location> = Vec::new();
        walk_statement_nodes(&body.statements, &mut |node| {
            if let Node::Statement(Stmt::For { body, .. } | Stmt::While { body, .. }) = node {
                looped.extend(writes(body).into_iter().map(|(at, _)| at));
            }
        });
        let again = all
            .iter()
            .find(|(at, through_out)| *through_out && (all.len() > 1 || looped.contains(at)));
        if let Some((at, _)) = again {
            self.unsupported(
                *at,
                "a window that one evaluation may publish to more than once",
            );
        }
    }
}

/// The annotation of a node's own output, `_output`, whose result is
/// annotated `result`. hgraph names the output's type after its
/// input's: `TS_OUT[int]` for `TS[int]`, `TSS_OUT[int]` for a set,
/// `TSB_OUT[...]` for a struct's bundle, `TSD_OUT` for a dictionary's
/// and `TSW_OUT` for a window's. Only the outermost is renamed: a child
/// stays as it is. hgraph has no `TSL_OUT`: a list's is its `TSL` given
/// to `TS_OUT`, which takes a time-series type as it is.
pub(super) fn output_annotation(result: &str) -> String {
    if result.starts_with("_hg.TSL[") {
        return format!("_hg.TS_OUT[{result}]");
    }
    let kind = ["TSS", "TSB", "TSD", "TSW"]
        .into_iter()
        .find(|kind| result.starts_with(&format!("_hg.{kind}[")))
        .unwrap_or("TS");
    result.replacen(&format!("{kind}["), &format!("{kind}_OUT["), 1)
}

/// The Python type of a scalar this target writes, `int` for `i64`,
/// reached through `_py`, the target's name for Python's built-ins, or
/// `_core`, the provider's.
///
/// TYPE-1: "`bool`, signed 64-bit `i64`, binary64 `f64`, Unicode
/// `str`, and octet-sequence `bytes` retain distinct identities": each
/// is a Python type of its own. Python's `int` has no bound, so core's
/// arithmetic holds an `i64` to its range (NUM-2). A Python `bytes`
/// holds octets, each 0 to 255, and orders them as unsigned octets.
pub(super) fn scalar(ty: &Type) -> Option<&'static str> {
    match ty {
        Type::Scalar(Scalar::Bool) => Some("_py.bool"),
        Type::Scalar(Scalar::I64) => Some("_py.int"),
        Type::Scalar(Scalar::F64) => Some("_py.float"),
        Type::Scalar(Scalar::Str) => Some("_py.str"),
        Type::Scalar(Scalar::Bytes) => Some("_py.bytes"),
        // TYPE-17: a tenor is the provider's `Tenor`, by its counts.
        Type::Scalar(Scalar::Tenor) => Some("_core.Tenor"),
        // TYPE-2: "named `timezone`, resolved `zoned_datetime`, and
        // date-independent `zoned_time`" are the provider's classes,
        // each with its zone by name.
        Type::Scalar(Scalar::Timezone) => Some("_core.Timezone"),
        Type::Scalar(Scalar::ZonedDatetime) => Some("_core.ZonedDatetime"),
        Type::Scalar(Scalar::ZonedTime) => Some("_core.ZonedTime"),
        // TYPE-2: a `duration` is elapsed time in whole microseconds,
        // which is what a `timedelta` holds.
        Type::Scalar(Scalar::Duration) => Some("_dt.timedelta"),
        // TYPE-2: a `datetime` is an instant in UTC. hgraph's instants
        // are `datetime`s in UTC that name no zone, so one is written as
        // that. A `civil_datetime` is a local date and time, which is a
        // `datetime` that names no zone too: HGL's types keep the two
        // apart, and nothing in Python has to.
        Type::Scalar(Scalar::Datetime | Scalar::CivilDatetime) => Some("_dt.datetime"),
        Type::Scalar(Scalar::Date) => Some("_dt.date"),
        Type::Scalar(Scalar::Time) => Some("_dt.time"),
        _ => None,
    }
}
