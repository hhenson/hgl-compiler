//! Writes constructors, field and delta properties, and access to their places.

use hgl_analyser::hir::{Argument, Expr, ExprKind, Value};
use hgl_analyser::{Detail, NamedKind, Type};
use hgl_lower::calls::{DefaultValue, applied_field_default, field_default};
use hgl_lower::types::inherited;

use crate::emit::Emitter;
use crate::emit::calls::{changes, may_fail};
use crate::emit::expressions::reads_an_element;
use crate::emit::names::{self, identifier};
use crate::emit::statements::Scope;
use crate::runtime;

use super::collection_of;
use super::layouts::property_type;

/// One value a constructor of a struct or a delta is given, in the
/// order HGL evaluates it (TYPE-5, EXPR-2).
struct Given {
    /// The position it fills among what the C++ is made of.
    slot: usize,
    /// The C++ that makes it.
    text: String,
    /// True where it may fail or do anything ([`may_fail`]).
    fails: bool,
    /// True where it runs statements of its own ([`changes`]).
    changes: bool,
    /// True where it reads what a local or a constant holds ([`refers`]).
    refers: bool,
    /// The C++ type it is held as where it is held: its own, never `auto`
    /// of what was read, which for an element of a `list<bool>` is the
    /// vector's reference to its bit.
    ty: String,
}

impl Emitter<'_> {
    /// `Struct(field: value, ...)` (TYPE-5): "Its arguments are named,
    /// each for a field the struct has ... A field with a default ... may
    /// be left out, and then has that default. An optional field with no
    /// default may be left out too, and is then null ... Written arguments
    /// evaluate once in source order; failure prevents later arguments.
    /// Defaults are evaluated only when omitted."
    pub(in crate::emit) fn construct(
        &mut self,
        scope: &mut Scope,
        value: &Expr,
        arguments: &[Argument],
    ) -> String {
        let program = self.program;
        let Type::Named {
            module,
            declaration,
            kind: NamedKind::Struct,
            ..
        } = &value.ty
        else {
            return self.missing(value.at, "this construction");
        };
        let Some(Detail::Struct(read)) = program.modules[*module].details.get(*declaration) else {
            return self.missing(value.at, "this construction");
        };
        let Some(ty) = self.written_type(&value.ty, value.at) else {
            return "0".to_string();
        };
        let Some(field_types) = program.field_types(&value.ty) else {
            return self.missing(value.at, "this construction");
        };
        let mut given = Vec::new();
        for argument in arguments {
            let Some(slot) = argument.parameter.filter(|&slot| slot < read.members.len()) else {
                return self.missing(argument.value.at, "an argument given to no field");
            };
            let Some(written) = self.given(scope, slot, &argument.value) else {
                return "0".to_string();
            };
            given.push(written);
        }
        // Each field left out, in the order of the fields: its default, or
        // null.
        for (slot, member) in read.members.iter().enumerate() {
            if given.iter().any(|written| written.slot == slot) {
                continue;
            }
            let field_type = &field_types[slot];
            let Some(slot_type) = self.written_type(field_type, value.at) else {
                return "0".to_string();
            };
            let default = member
                .default
                .and_then(|field| field_default(program, field).map(|default| (field, default)));
            let text = match default {
                Some((field, declared)) => {
                    let key = (*module, *declaration, usize::MAX - slot);
                    if self.expanding.contains(&key) {
                        return self.missing(declared.value.at, "a default that leaves itself out");
                    }
                    let Some(default) =
                        applied_field_default(program, field, &value.ty, field_type)
                    else {
                        self.unsupported(
                            declared.value.at,
                            "a default of a generic struct's field",
                        );
                        return "0".to_string();
                    };
                    self.expanding.push(key);
                    let made = self.field_default(&default, field_type);
                    self.expanding.pop();
                    let Some(made) = made else {
                        return "0".to_string();
                    };
                    Given {
                        slot,
                        text: made,
                        fails: may_fail(&default),
                        changes: changes(&default),
                        refers: false,
                        ty: slot_type,
                    }
                }
                None if hgl_lower::types::is_nullable(field_type)
                    || matches!(field_type, Type::Atomic(payload) if hgl_lower::types::is_nullable(payload)) =>
                {
                    Given {
                        slot,
                        text: "::std::nullopt".to_string(),
                        fails: false,
                        changes: false,
                        refers: false,
                        ty: slot_type,
                    }
                }
                None => {
                    return self.missing(
                        value.at,
                        &format!("a construction that leaves out `{}`", member.name),
                    );
                }
            };
            given.push(text);
        }
        self.made_in_order(scope, (&ty, &ty), read.members.len(), given, ("{", "}"))
    }

    /// What `argument`, given to `slot`, is written as; `None` where its
    /// type is not written yet, which is reported.
    fn given(&mut self, scope: &mut Scope, slot: usize, argument: &Expr) -> Option<Given> {
        let text = self.expression(scope, argument);
        let ty = self.written_type(&argument.ty, argument.at)?;
        Some(Given {
            slot,
            text,
            fails: self.may_fail_here(argument),
            changes: changes(argument),
            refers: self.refers(argument),
            ty,
        })
    }

    /// TYPE-5: write the applied default in its own scope (UNIT-7).
    /// Empty collections and null also have the field type available here.
    fn field_default(&mut self, value: &DefaultValue<'_>, field_type: &Type) -> Option<String> {
        let empty = matches!(&value.kind, ExprKind::Sequence(entries) if entries.is_empty())
            || matches!(&value.kind, ExprKind::Pairs(entries) if entries.is_empty());
        if empty {
            // TYPE-4: "optional fields may have non-null defaults": the
            // empty collection, which an optional field then holds, made
            // as the collection and not as the optional.
            let ty = self.written_type(collection_of(field_type), value.at)?;
            return Some(format!("{ty}{{}}"));
        }
        if matches!(value.kind, ExprKind::Literal(Value::Null)) {
            return Some("::std::nullopt".to_string());
        }
        self.write_default(value)
    }

    /// A value of the C++ type `ty`, made by `made` (the type itself for
    /// an aggregate, or a function that gives one) of `slots` values,
    /// `open` and `close` around them, each given by one of `given`, which
    /// are in the order HGL evaluates them. Where more than one of them
    /// may fail, or one runs statements, which may change what another
    /// reads, each is first held in that order, as a value of its own where
    /// one runs statements: C++ evaluates them in the order of the slots.
    #[allow(clippy::too_many_arguments)]
    fn made_in_order(
        &mut self,
        scope: &mut Scope,
        (ty, made): (&str, &str),
        slots: usize,
        given: Vec<Given>,
        (open, close): (&str, &str),
    ) -> String {
        let changing = given.iter().any(|value| value.changes);
        let failing = given.iter().filter(|value| value.fails).count();
        let mut filled = vec![String::new(); slots];
        if !changing && failing <= 1 {
            for value in given {
                filled[value.slot] = value.text;
            }
            return format!("{made}{open}{}{close}", filled.join(", "));
        }
        let mut steps = String::new();
        for value in given {
            let name = names::temporary("field", self.temporary());
            let held = if value.refers && !changing {
                format!("const {}&", value.ty)
            } else {
                format!("const {}", value.ty)
            };
            steps.push_str(&format!("{held} {name} = {}; ", value.text));
            filled[value.slot] = name;
        }
        scope.acted = true;
        format!(
            "[&]() -> {ty} {{ {steps}return {made}{open}{}{close}; }}()",
            filled.join(", ")
        )
    }

    /// `base.member`: a field of a struct (TYPE-3), of an abstract one
    /// through the function of its name, or a property of a delta that
    /// is a type of its own (TYPE-16): a set's `added` and `removed`, a
    /// dictionary's `updated` and `removed`, a list's `updated`,
    /// `removed` and `size_modified`, a nullable's `value`, which is the
    /// optional itself, and a struct's of the field's name.
    pub(in crate::emit) fn field(
        &mut self,
        scope: &mut Scope,
        value: &Expr,
        base: &Expr,
        member: usize,
    ) -> String {
        let program = self.program;
        match &base.ty {
            Type::Named {
                module,
                declaration,
                kind: NamedKind::Struct,
                ..
            } => {
                let Some(field) = hgl_lower::types::field_of(program, &base.ty, member) else {
                    return self.missing(value.at, "this field");
                };
                let name = identifier(&field.name);
                let abstract_ = inherited(program, *module, *declaration);
                let base = self.expression(scope, base);
                if abstract_ {
                    format!("({base}).{name}()")
                } else {
                    format!("({base}).{name}")
                }
            }
            Type::Delta(of) => {
                let ns = runtime::NAMESPACE;
                // A struct's delta's property is of its field's name.
                let field = match &**of {
                    Type::Named {
                        kind: NamedKind::Struct,
                        ..
                    } => match hgl_lower::types::field_of(program, of, member) {
                        Some(field) => Some(identifier(&field.name)),
                        None => return self.missing(value.at, "this property of a delta"),
                    },
                    _ => None,
                };
                let written = self.expression(scope, base);
                match (&**of, member) {
                    (Type::Set(_), 0) => format!("({written}).added"),
                    (Type::Set(_) | Type::Dict { .. }, 1) => format!("({written}).removed"),
                    (Type::Dict { .. }, 0) => format!("({written}).updated"),
                    // A list's delta keeps its changes in order of their
                    // positions; HGL's `updated` and `removed` are a
                    // dictionary and a set, made where they are read.
                    (Type::List { .. }, 0) => format!("::{ns}::updated_of({written})"),
                    (Type::List { .. }, 1) => format!("::{ns}::removed_of({written})"),
                    (Type::List { .. }, 2) => format!("({written}).size_modified"),
                    // "the property `value`, a `delta<V>?`": the optional
                    // the delta holds.
                    (Type::Nullable(_), 0) => format!("({written}).value"),
                    _ => match field {
                        Some(field) => format!("({written}).{field}"),
                        None => self.missing(value.at, "this property of a delta"),
                    },
                }
            }
            _ => self.missing(value.at, "this property"),
        }
    }

    /// `delta<T>(property: value, ...)` (TYPE-16): "A property left out is
    /// empty, or null for a struct's field, a nullable's `value`, and a
    /// list's `size_modified`." The properties are evaluated once, in the
    /// order written (EXPR-2).
    pub(in crate::emit) fn delta(
        &mut self,
        scope: &mut Scope,
        value: &Expr,
        arguments: &[Argument],
    ) -> String {
        let program = self.program;
        let Type::Delta(of) = &value.ty else {
            return self.missing(value.at, "this delta");
        };
        let Some(ty) = self.written_type(&value.ty, value.at) else {
            return "0".to_string();
        };
        // Each property's type, and what it is when it is left out.
        let empty: Vec<(String, String)> = match &**of {
            Type::Set(element) => {
                let Some(set) = self.written_type(&Type::Set(element.clone()), value.at) else {
                    return "0".to_string();
                };
                vec![
                    (set.clone(), format!("{set}{{}}")),
                    (set.clone(), format!("{set}{{}}")),
                ]
            }
            Type::Dict { key, value: held } => {
                let updated = Type::Dict {
                    key: key.clone(),
                    value: Box::new(Type::delta((**held).clone())),
                };
                let (Some(updated), Some(removed)) = (
                    self.written_type(&updated, value.at),
                    self.written_type(&Type::Set(key.clone()), value.at),
                ) else {
                    return "0".to_string();
                };
                vec![
                    (updated.clone(), format!("{updated}{{}}")),
                    (removed.clone(), format!("{removed}{{}}")),
                ]
            }
            Type::List { element, .. } => {
                let position = Box::new(Type::Scalar(hgl_analyser::Scalar::I64));
                let updated = Type::Dict {
                    key: position.clone(),
                    value: Box::new(Type::delta((**element).clone())),
                };
                let (Some(updated), Some(removed)) = (
                    self.written_type(&updated, value.at),
                    self.written_type(&Type::Set(position), value.at),
                ) else {
                    return "0".to_string();
                };
                vec![
                    (updated.clone(), format!("{updated}{{}}")),
                    (removed.clone(), format!("{removed}{{}}")),
                    (
                        "::std::optional<::hgraph::Int>".to_string(),
                        "::std::nullopt".to_string(),
                    ),
                ]
            }
            Type::Nullable(_) => vec![(ty.clone(), String::new())],
            Type::Named {
                kind: NamedKind::Struct,
                ..
            } => {
                let Some(fields) = program.field_types(of) else {
                    return self.missing(value.at, "this delta");
                };
                let mut properties = Vec::new();
                for field in &fields {
                    let Some(property) = self.written_type(&property_type(field), value.at) else {
                        return "0".to_string();
                    };
                    properties.push((property, "::std::nullopt".to_string()));
                }
                properties
            }
            _ => return self.missing(value.at, "this delta"),
        };
        let mut given = Vec::new();
        for argument in arguments {
            let Some(slot) = argument.parameter.filter(|&slot| slot < empty.len()) else {
                return self.missing(argument.value.at, "an argument given to no property");
            };
            let Some(written) = self.given(scope, slot, &argument.value) else {
                return "0".to_string();
            };
            given.push(written);
        }
        for (slot, (property, left_out)) in empty.iter().enumerate() {
            if !given.iter().any(|written| written.slot == slot) {
                given.push(Given {
                    slot,
                    text: left_out.clone(),
                    fails: false,
                    changes: false,
                    refers: false,
                    ty: property.clone(),
                });
            }
        }
        match &**of {
            // `delta<i64?>(value: 7)` holds 7, and `delta<i64?>()` is a
            // published null.
            Type::Nullable(_) => {
                let text = given.pop().map(|value| value.text).unwrap_or_default();
                format!("{ty}{{{text}}}")
            }
            Type::List { .. } => {
                let made = format!(
                    "::{}::list_delta_of<{}>",
                    runtime::NAMESPACE,
                    match self.written_type(of, value.at) {
                        Some(list) => list,
                        None => return "0".to_string(),
                    }
                );
                let slots = empty.len();
                self.made_in_order(scope, (&ty, &made), slots, given, ("(", ")"))
            }
            _ => {
                let slots = empty.len();
                self.made_in_order(scope, (&ty, &ty), slots, given, ("{", "}"))
            }
        }
    }

    /// The C++ place a field of the struct `base` is (EXPR-3), `base`
    /// being its place written: `point.x`. A field of an abstract struct
    /// is not written yet as a place.
    pub(in crate::emit) fn field_place(
        &self,
        base_type: &Type,
        base: String,
        member: usize,
    ) -> Option<String> {
        let Type::Named {
            module,
            declaration,
            kind: NamedKind::Struct,
            ..
        } = base_type
        else {
            return None;
        };
        if inherited(self.program, *module, *declaration) {
            return None;
        }
        let field = hgl_lower::types::field_of(self.program, base_type, member)?;
        Some(format!("{base}.{}", identifier(&field.name)))
    }

    /// `convert<list<B>>(values)` (core's OPS-6): a list of structs taken
    /// as a list of an abstract struct `B` they inherit from, "the same
    /// elements".
    pub(in crate::emit) fn parent_list(
        &mut self,
        scope: &mut Scope,
        value: &Expr,
        inner: &Expr,
    ) -> String {
        let Type::List { element, .. } = &value.ty else {
            return self.missing(value.at, "this conversion");
        };
        let Some(element) = self.written_type(element, value.at) else {
            return "0".to_string();
        };
        let written = self.expression(scope, inner);
        format!(
            "::{}::parent_list<{element}>({written})",
            runtime::NAMESPACE
        )
    }

    /// The place `place` given to a `var` parameter, as the steps that
    /// hold each list index it has, in order, and the place written with
    /// the indices held (FN-15, EXPR-2): a call with an argument that runs
    /// statements takes the place where it is called, the list it names
    /// as it is then, and each index as it was when its argument was
    /// evaluated. `None` for a place not written yet this way, a
    /// dictionary's entry among them.
    pub(in crate::emit) fn held_place(
        &mut self,
        scope: &mut Scope,
        place: &Expr,
    ) -> Option<(String, String)> {
        match &place.kind {
            ExprKind::Local(local) => {
                scope.read = true;
                Some((String::new(), identifier(&scope.body.locals[*local].name)))
            }
            ExprKind::Field { base, member } => {
                let (steps, written) = self.held_place(scope, base)?;
                Some((steps, self.field_place(&base.ty, written, *member)?))
            }
            ExprKind::Index { base, index } => match &base.ty {
                Type::Tuple(_) => {
                    let ExprKind::Literal(Value::Int(position)) = index.kind else {
                        return None;
                    };
                    let (steps, written) = self.held_place(scope, base)?;
                    Some((steps, format!("::std::get<{position}>({written})")))
                }
                Type::List { .. } => {
                    let (mut steps, written) = self.held_place(scope, base)?;
                    // EXPR-2: the list's own place, an element or an
                    // entry, is checked before this index is evaluated;
                    // the place is taken again at the call.
                    if reads_an_element(base) {
                        steps.push_str(&format!("static_cast<void>({written}); "));
                    }
                    let at = self.expression(scope, index);
                    let held = names::temporary("index", self.temporary());
                    steps.push_str(&format!("const ::hgraph::Int {held} = {at}; "));
                    Some((
                        steps,
                        format!("::{}::item({written}, {held})", runtime::NAMESPACE),
                    ))
                }
                _ => None,
            },
            _ => None,
        }
    }
}
