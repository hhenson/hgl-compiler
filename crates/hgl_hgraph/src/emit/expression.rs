//! An expression as the hgraph target writes it: a literal, a name, a
//! field, an element, an operator, a construction, a delta, a trace, and
//! in a hook a read of a temporal input's port or of a child of one
//! (TIME-1). Part of the transpiler, the last stage: `emit` writes a
//! module with them.

use super::*;

impl Emitter<'_> {
    /// An expression as Python. One with no Python here yet is reported
    /// and written `None`.
    pub(super) fn expression(&mut self, function: &Function, value: &Expr) -> String {
        // In a graph a value may be a connection, and Python's `and`,
        // `not` and `if` would act on the port: only a name, a literal,
        // a member of an enum, a constant, a call and a widening are
        // written there.
        if function.mode == Mode::Graph
            && !matches!(
                value.kind,
                ExprKind::Literal(_)
                    | ExprKind::Local(_)
                    | ExprKind::Call { .. }
                    | ExprKind::Widen(_)
                    | ExprKind::EnumMember { .. }
                    | ExprKind::Constant { .. }
                    | ExprKind::Capture(_)
                    | ExprKind::Captured { .. }
                    | ExprKind::TagTest { .. }
            )
            && !self.connected_field(function, value)
            && !self.connected_element(function, value)
            && !(matches!(value.kind, ExprKind::Tuple(_)) && self.connects(function, value))
        {
            self.unsupported(value.at, "this expression in a graph");
            return "None".to_string();
        }
        // What a call that assigns places has evaluated already is held
        // under a name of its own, and is not evaluated again (FN-15).
        if let Some((_, held)) = self.held.iter().find(|(at, _)| *at == value.at) {
            return held.clone();
        }
        match &value.kind {
            ExprKind::Literal(literal) => self.literal(literal, value.at),
            // UNIT-13: a constant is read by a call of the function
            // that gives its value, of its own module or another's.
            ExprKind::Constant {
                module,
                declaration,
            } => format!("{}()", self.function_name(*module, *declaration)),
            ExprKind::Local(local) => {
                // In a hook a temporal input is read through its
                // payload: in a `when`, and in `stop` (TIME-5). So is
                // `out` read whole: a struct's value is made from its
                // bundle's fields (TYPE-9), as an input's is. The port
                // holds a value of the declared type (`port_type`).
                if matches!(function.mode, Mode::Node | Mode::Stop)
                    && let Some(port) = self.input_port(function, value)
                {
                    let ty = function.port_type(value);
                    if matches!(ty, Type::RollingWindow { .. }) {
                        return self.window_read(value, &port);
                    }
                    let read = self.port_value(ty, &port, value.at);
                    if function.is_out(*local) {
                        return self.published_read(ty, &port, read);
                    }
                    return read;
                }
                let read = self.place(function, *local);
                if function.mode == Mode::Lifecycle && function.is_input(*local) {
                    // The analyser rejects it (`input.lifecycle`).
                    self.unsupported(value.at, "a temporal input read in `start`");
                    read
                } else {
                    // What a place holds is read as it is: a copy is
                    // made where a reference to it is kept, and not
                    // where it is read (see `kept`).
                    read
                }
            }
            ExprKind::Endpoint {
                operation,
                inputs: Some(inputs),
            } if inputs.len() == 1 => {
                let endpoint = &function.locals[inputs[0]];
                // At stop, validity guards payload reads of inputs and out,
                // and both expose their last publication time (TIME-5, TIME-7).
                let given = match function.mode {
                    Mode::Lifecycle => false,
                    Mode::Stop => {
                        matches!(
                            operation,
                            EndpointOperation::Valid
                                | EndpointOperation::AllValid
                                | EndpointOperation::LastModified
                        ) || (endpoint.kind != LocalKind::Inject
                            && matches!(
                                operation,
                                EndpointOperation::IsActive
                                    | EndpointOperation::Activate
                                    | EndpointOperation::Passivate
                            ))
                    }
                    _ => true,
                };
                if !given {
                    self.unsupported(value.at, "this endpoint read in `start` or `stop`");
                }
                let input = endpoint_name(endpoint);
                // FN-16: "the endpoint operations take" a `ref`
                // parameter, and `out` of a `ref` result, each hgraph's
                // `REF`. TIME-10: "`valid` of a `ref` parameter is
                // whether a reference is present"; core's END-10, its
                // `modified` "whether the parameter was given a
                // designation", and its `last_modified` "when the
                // parameter was last given a designation", which are the
                // `REF`'s own.
                if function.designates(inputs[0]) {
                    // `out` of a `ref` result in a `when`: what it
                    // designates is the provider's `Designation`'s, which
                    // writes it as the evaluation ends.
                    let kept = function.designating() && function.is_out(inputs[0]);
                    return match operation {
                        EndpointOperation::Invalidate if kept => {
                            "_designation.invalidate()".to_string()
                        }
                        EndpointOperation::Valid if kept => "_designation.valid".to_string(),
                        EndpointOperation::Modified if kept => "_designation.modified".to_string(),
                        EndpointOperation::LastModified if kept => {
                            "_designation.last_modified".to_string()
                        }
                        EndpointOperation::Valid => format!("{input}.valid"),
                        EndpointOperation::Modified => format!("{input}.modified"),
                        EndpointOperation::LastModified => {
                            format!("({input}.last_modified_time if {input}.valid else None)")
                        }
                        EndpointOperation::Invalidate => format!("{input}.invalidate()"),
                        EndpointOperation::Activate => format!("{input}.make_active()"),
                        EndpointOperation::Passivate => format!("{input}.make_passive()"),
                        EndpointOperation::IsActive => format!("{input}.active"),
                        other => {
                            self.unsupported(
                                value.at,
                                &format!("`{}` of a `ref` parameter or result", other.name()),
                            );
                            "None".to_string()
                        }
                    };
                }
                // The endpoint of what holds a window, itself or within
                // (`windows_in`). hgraph's `TSW` keeps what it holds
                // when it is invalidated, where END-4 has "Invalidation
                // clears validity, modification, payload, and delta".
                if !windows_in(self.program, &endpoint.ty).is_empty()
                    && matches!(operation, EndpointOperation::Invalidate)
                {
                    self.unsupported(value.at, "the invalidation of a window");
                }
                match operation {
                    // TIME-9, END-7: "`valid` of a collection is true where any
                    // element, field or entry is valid", as hgraph's
                    // `valid` of a `TSL`, a `TSB` and a `TSD` is but where
                    // a list is in the value (`valid_test`).
                    EndpointOperation::Valid => self.valid_test(&endpoint.ty, &input),
                    // TIME-9: "`all_valid` is true where every one is, by
                    // its own `valid`, one level deep": hgraph's
                    // `all_valid`, but where a part holds a list.
                    EndpointOperation::AllValid => self.all_valid_test(&endpoint.ty, &input),
                    // END-4: "Invalidation clears validity,
                    // modification, payload, and delta." hgraph's own
                    // `modified` of an input in the cycle it is
                    // invalidated is true to a node that injects
                    // something, so it is held to the input being
                    // valid.
                    EndpointOperation::Modified => {
                        let valid = self.valid_test(&endpoint.ty, &input);
                        format!("({input}.modified and {valid})")
                    }
                    // END-4: "Invalidation clears validity,
                    // modification, payload, and delta."
                    // hgraph's unbounded `TSL` keeps its positions when
                    // it is invalidated, which the next delta read would
                    // name removed: the provider empties each list first.
                    // hgraph's `TSD` and `TSS` keep their keys and members
                    // too, which a delta that names no change would leave
                    // in what it makes valid (TIME-7): the provider
                    // empties each, at any depth of a struct, a tuple and
                    // a fixed list (`emptied_shape`).
                    EndpointOperation::Invalidate => {
                        match emptied_shape(self.program, &endpoint.ty) {
                            Some(shape) => {
                                self.natives = true;
                                format!("_core.invalidated({input}, {shape})")
                            }
                            None => format!("{input}.invalidate()"),
                        }
                    }
                    // TIME-9: input activity controls delivery of notifications.
                    EndpointOperation::Activate => format!("{input}.make_active()"),
                    EndpointOperation::Passivate => format!("{input}.make_passive()"),
                    EndpointOperation::IsActive => format!("{input}.active"),
                    // END-3: what the input published this cycle, read as
                    // the program reads a delta of its type
                    // (`language_delta`).
                    EndpointOperation::DeltaValue => {
                        let ty = endpoint.ty.clone();
                        if duration_window(&ty) {
                            // A window of a duration's (`window_delta`).
                            self.natives = true;
                            format!("_core.window_delta({input})")
                        } else {
                            self.language_delta(&ty, format!("{input}.delta_value"), value.at)
                        }
                    }
                    // END-6: an invalid endpoint has no timestamp, even
                    // if it held a value before being invalidated.
                    EndpointOperation::LastModified => {
                        let valid = self.valid_test(&endpoint.ty, &input);
                        format!("({input}.last_modified_time if {valid} else None)")
                    }
                    other => {
                        self.unsupported(value.at, &format!("`{}`", other.name()));
                        "None".to_string()
                    }
                }
            }
            // END-8: "A child of a collection is an endpoint whether or
            // not it is valid: `valid`, `modified`, `delta_value` and
            // `last_modified` take it, and `all_valid` where it is a
            // collection". It is hgraph's own child of the input.
            ExprKind::ChildEndpoint { operation, child } => {
                self.child_operation(function, *operation, child, value.at)
            }
            // TIME-1: "Elsewhere, `parts[i]`, it is checked when it
            // runs: a child that is not valid fails it
            // (`value.unset_read`), or gives an `err` where its place
            // takes a `result` (ERR-14)."
            // A window bound to a generic type parameter (`window_read`).
            ExprKind::CheckedChild { .. } if matches!(value.ty, Type::RollingWindow { .. }) => {
                self.unsupported(value.at, GENERIC_WINDOW_READ);
                "None".to_string()
            }
            ExprKind::CheckedChild { read, result } => {
                self.checked_child(function, read, *result, value.at)
            }
            // TYPE-11: a read of a window's value "before the window is
            // `all_valid` ... gives an `err` of an `error` with that code
            // where its place takes a `result<list<V>, error>`" (ERR-14).
            ExprKind::CheckedWindow(read) => self.window_result(function, read, value.at),
            // TIME-9: `scheduled()` as a bool, where it sets no
            // subscription (TIME-3: "Selectors under negation or
            // disjunction leave the default in place"): what woke the
            // node, as `node` holds it where the node is entered. The
            // analyser rejects it in `start` and `stop`
            // (`scheduled.lifecycle`), where no wake is due.
            ExprKind::Endpoint {
                operation: EndpointOperation::Scheduled,
                ..
            } => match (function.mode, &self.scheduled) {
                (Mode::Node, Some(woke)) => woke.clone(),
                _ => {
                    self.unsupported(value.at, "`scheduled()` outside a `when`");
                    "None".to_string()
                }
            },
            // The language widens a literal and no connection (EXPR-5),
            // so what is widened here is known where the graph is
            // built. Python's `float` would act on a port.
            ExprKind::Widen(inner)
                if function.mode == Mode::Graph && self.connects(function, inner) =>
            {
                self.unsupported(value.at, "a widening of a connection");
                "None".to_string()
            }
            ExprKind::Widen(inner) => {
                format!("_py.float({})", self.expression(function, inner))
            }
            // Core's OPS-6: `convert<list<B>>(values)`.
            ExprKind::ParentList(_) => {
                self.unsupported(
                    value.at,
                    "a list of structs converted to a list of their parent",
                );
                "None".to_string()
            }
            ExprKind::Unary {
                operator: UnaryOperator::Not,
                operand,
            } => format!("(not {})", self.expression(function, operand)),
            ExprKind::Binary {
                operator: operator @ (BinaryOperator::And | BinaryOperator::Or),
                left,
                right,
            } => {
                let word = if *operator == BinaryOperator::And {
                    "and"
                } else {
                    "or"
                };
                let left = self.expression(function, left);
                let right = self.expression(function, right);
                format!("({left} {word} {right})")
            }
            // A test against `null` is the language's own, and is
            // recorded as it is written (ERR-12).
            ExprKind::Binary {
                operator: operator @ (BinaryOperator::Equal | BinaryOperator::NotEqual),
                left,
                right,
            } if is_null(left) || is_null(right) => {
                let tested = if is_null(left) { right } else { left };
                let tested = self.expression(function, tested);
                if *operator == BinaryOperator::Equal {
                    format!("({tested} is None)")
                } else {
                    format!("({tested} is not None)")
                }
            }
            ExprKind::Call {
                callee, arguments, ..
            } => self.call(function, callee, arguments, value),
            // A block that is only its value, and an `if` between two
            // such.
            ExprKind::Block { statements, value } if statements.is_empty() => match value {
                Some(value) => self.expression(function, value),
                None => "None".to_string(),
            },
            ExprKind::If {
                condition,
                then,
                otherwise: Some(otherwise),
            } => {
                let condition = self.expression(function, condition);
                let then = self.expression(function, then);
                let otherwise = self.expression(function, otherwise);
                format!("({then} if {condition} else {otherwise})")
            }
            // CTRL-10: "With no `else` it is as if its `else` were null".
            ExprKind::If {
                condition,
                then,
                otherwise: None,
            } => {
                let condition = self.expression(function, condition);
                let then = self.expression(function, then);
                format!("({then} if {condition} else None)")
            }
            ExprKind::Tuple(entries)
                if function.mode == Mode::Graph && self.connects(function, value) =>
            {
                self.connected_tuple(function, value, entries)
            }
            ExprKind::Tuple(entries) => {
                let entries: Vec<String> = entries
                    .iter()
                    .map(|entry| self.expression(function, entry))
                    .collect();
                format!("({},)", entries.join(", "))
            }
            ExprKind::Sequence(entries) => {
                let entries: Vec<String> = entries
                    .iter()
                    .map(|entry| self.expression(function, entry))
                    .collect();
                // EXPR-9: "The use site and expected type constrain
                // element types, keys, ordering, uniqueness": a
                // sequence that is a set is one in Python too, and
                // "Empty brackets have no entries": `[]` where a
                // dictionary is expected is an empty one.
                let made = match &value.ty {
                    Type::Nullable(inner) => inner,
                    ty => ty,
                };
                let is_set = matches!(made, Type::Set(_));
                if matches!(made, Type::Dict { .. }) {
                    "{}".to_string()
                } else if is_set {
                    format!(
                        "_py.frozenset(({}{}))",
                        entries.join(", "),
                        if entries.len() == 1 { "," } else { "" }
                    )
                } else {
                    format!("[{}]", entries.join(", "))
                }
            }
            // EXPR-9: `[key: value, ...]`, a dictionary. Python evaluates
            // each key and then its value, in the order written.
            ExprKind::Pairs(entries) => {
                let entries: Vec<String> = entries
                    .iter()
                    .map(|(key, entry)| {
                        let key = self.expression(function, key);
                        let entry = self.expression(function, entry);
                        format!("{key}: {entry}")
                    })
                    .collect();
                format!("{{{}}}", entries.join(", "))
            }
            ExprKind::Delta { arguments } => self.delta(function, value, arguments),
            // TST-17, RUN-3: `run(...)` in an integration test is the
            // eval's run in real time.
            ExprKind::Eval {
                call,
                config,
                real_time,
            } => self.eval(function, call, &value.ty, config.as_deref(), *real_time),
            // TST-16: "A trace given to a `const fn` is an ordinary
            // value there: ... a `list<tuple<datetime, delta<T>>>`, one
            // tuple for each publication". The harness makes it of the
            // dense trace an eval gives.
            ExprKind::Publications(trace) => {
                let trace = self.expression(function, trace);
                format!("_harness.publications({trace})")
            }
            ExprKind::Trace(cells) => self.trace(function, cells, &value.ty),
            ExprKind::TraceCompare {
                left,
                right,
                negated,
            } => {
                // TST-4: "Harness comparison is intrinsic: compare
                // presence, exact present data, and shape/length". The
                // harness compares, and not Python's `==`: hgraph's set
                // delta takes a removed element for an added one.
                let left = self.expression(function, left);
                let right = self.expression(function, right);
                let not = if *negated { "not " } else { "" };
                format!("{not}_harness.same({left}, {right})")
            }
            // TYPE-7: a member of an enum, however it was written,
            // read from its `Enum` by its name.
            ExprKind::EnumMember {
                module,
                declaration,
                member,
            } => {
                let written = match self.program.modules[*module].details.get(*declaration) {
                    Some(Detail::Enum(read)) => read.members.get(*member),
                    _ => None,
                };
                let Some(written) = written else {
                    self.unsupported(value.at, "this member");
                    return "None".to_string();
                };
                let member = string(&member_name(&written.name));
                format!("{}[{member}]", self.type_name(*module, *declaration))
            }
            // TYPE-7: "`Side(1)` is the member whose number is 1, and
            // `Side("buy")` the member of that exact name ... a value
            // that is none fails where it is evaluated."
            ExprKind::EnumFrom {
                module,
                declaration,
                value,
            } => {
                self.natives = true;
                let of = self.type_name(*module, *declaration);
                let value = self.expression(function, value);
                format!("_core.member({of}, {value})")
            }
            ExprKind::Construct {
                module,
                declaration,
                arguments,
            } => self.construct(function, (*module, *declaration), arguments, value),
            // TIME-1: a child of a temporal input read in a hook, where it
            // is admitted, is read through its own endpoint, hgraph's
            // child of the input (`child_port`), as one read when it runs
            // is: never through its siblings, which may not be valid.
            ExprKind::Field { .. } | ExprKind::Index { .. }
                if matches!(function.mode, Mode::Node | Mode::Stop)
                    && self.reads_child(function, value) =>
            {
                // TYPE-11: a window child's value is read as a `list<V>`
                // (`window_read`), which the child's port is not.
                let window_child = window_child(self.program, value);
                match self.child_port(function, value) {
                    Some(port) if window_child => self.window_read(value, &port),
                    Some(port) => self.part_value(&value.ty, &port, value.at),
                    None => {
                        self.unsupported(value.at, "this read of a child");
                        "None".to_string()
                    }
                }
            }
            // TYPE-3: a struct's fields are public, and read by name.
            ExprKind::Field { base, member } => {
                // CAP-1: a property of a capability "is read as an
                // ordinary value". It is the provider's to read, as a
                // native function is: `_core.Clock_evaluation_time`.
                if let Type::Named {
                    name: native,
                    module,
                    declaration,
                    kind: NamedKind::Native,
                    ..
                } = &base.ty
                {
                    let program = self.program;
                    let property = match program.modules[*module].details.get(*declaration) {
                        Some(Detail::Native(read)) => read.properties.get(*member),
                        _ => None,
                    };
                    let read = property.and_then(|property| {
                        self.native_function(*module, format!("{native}_{}", property.name))
                            .map(|read| (property, read))
                    });
                    return match read {
                        Some((property, read)) => {
                            self.natives = true;
                            let base = self.expression(function, base);
                            self.native_result(Some(&property.ty), format!("_core.{read}({base})"))
                        }
                        _ => {
                            self.unsupported(value.at, "this property of a native type");
                            "None".to_string()
                        }
                    };
                }
                let delta = DeltaOf::of(&base.ty);
                // TYPE-16: a nullable's delta has one property, `value`,
                // "null for a published null". hgraph's delta of that
                // is `Null`.
                if matches!(delta, Some(DeltaOf::Nullable)) && *member == 0 {
                    self.natives = true;
                    let base = self.expression(function, base);
                    return format!("_core.unpublished({base})");
                }
                // TYPE-16: a dictionary's delta has `updated` and
                // `removed`, read through the provider from hgraph's
                // `dict` of both (see `delta`).
                // TYPE-16: a list's delta has `updated`, `removed` and
                // `size_modified` (see `list_delta`), for `list<V>` and
                // `list<V, N>` alike: hgraph 0.9.8's `TSL`, bounded or
                // not, reports a position that lost its value as
                // `REMOVE`, and a fixed list's delta read has no size.
                if let Some(DeltaOf::List) = delta {
                    let read = match *member {
                        0 => "updated",
                        1 => "removed",
                        _ => "size_modified",
                    };
                    self.natives = true;
                    let base = self.expression(function, base);
                    return format!("_core.{read}({base})");
                }
                if matches!(delta, Some(DeltaOf::Dict)) {
                    let read = if *member == 0 { "updated" } else { "removed" };
                    self.natives = true;
                    let base = self.expression(function, base);
                    return format!("_core.{read}({base})");
                }
                // TYPE-16: a struct's delta has "a property for each
                // field ... null where the field did not change". It is
                // hgraph's delta of a bundle, a `dict` of the fields
                // that changed, and a field is read through the
                // provider.
                if let Some(DeltaOf::Struct(of)) = delta
                    && let Some(field) = field_of(self.program, of, *member)
                {
                    self.natives = true;
                    let base = self.expression(function, base);
                    return format!("_core.changed({base}, {})", string(&name(&field.name)));
                }
                // TYPE-9: an `atomic<V>` of a struct holds the whole
                // struct, whose fields are read as the struct's.
                let whole = payload_of(&base.ty);
                // In a graph that field of an atomic struct's connection
                // is hgraph's `getattr_`, told the field's type: the
                // provider's class does not declare one.
                if function.mode == Mode::Graph
                    && matches!(base.ty, Type::Atomic(_))
                    && let Some(field) = field_of(self.program, whole, *member)
                {
                    let Some(of) = self.annotation(&value.ty, false, value.at) else {
                        return "None".to_string();
                    };
                    let base = self.expression(function, base);
                    self.hgraph = true;
                    return format!(
                        "_hg.getattr_[_hg.SCALAR: {of}]({base}, {})",
                        string(&name(&field.name))
                    );
                }
                // In a hook a field of a temporal struct input is
                // read through the field's own port (TYPE-9: structs
                // map "to child fields"): `quote.bid.value`.
                if matches!(function.mode, Mode::Node | Mode::Stop)
                    && let Some(port) = self.input_port(function, value)
                {
                    return self.part_value(&value.ty, &port, value.at);
                }
                let Some(field) = field_of(self.program, whole, *member) else {
                    // A property of a delta.
                    self.unsupported(value.at, "this member");
                    return "None".to_string();
                };
                let base = self.expression(function, base);
                // In a graph it is a field of a bundle's connection,
                // the child's port, taken by its name: a field may be
                // named as a port's own attribute is, `value`.
                if function.mode == Mode::Graph {
                    return format!("{base}[{}]", string(&name(&field.name)));
                }
                format!("{base}.{}", name(&field.name))
            }
            // EXPR-7: "Brackets read an element of a list or a tuple by
            // an `i64` index, and the value a dictionary has for a
            // key." ERR-6: a dictionary's brackets "are strict
            // point-in-time reads; absence fails".
            ExprKind::Index { base, index } if function.mode == Mode::Graph => {
                // TYPE-9: an element of a list's or a dictionary's
                // connection is that child's own connection, taken from
                // hgraph's `TSL` by a position known where the graph is
                // built, and from its `TSD` by a key that may be a
                // connection: a key that ticks selects another child.
                // TIME-1: "A child of a collection ... is an endpoint
                // whether or not it is valid". hgraph's `getitem_` of an
                // unbounded `TSL` fails the run where the list no longer
                // holds the position, so the provider's node selects it,
                // by a position known where the graph is built or one that
                // ticks: a connection that is not valid while the list does
                // not hold the position, as a dictionary's absent key's is.
                if let Type::List {
                    element,
                    size: None,
                } = &base.ty
                {
                    if !publishes_whole(element) {
                        self.unsupported(value.at, "an element of an unsized list of composites");
                        return "None".to_string();
                    }
                    let connected = self.connects(function, index);
                    let base = self.expression(function, base);
                    let index = self.expression(function, index);
                    let index = if connected {
                        index
                    } else {
                        format!("_hg.const({index}, tp=_hg.TS[_py.int])")
                    };
                    self.natives = true;
                    return format!("_core.position_in({base}, {index})");
                }
                if matches!(base.ty, Type::List { .. }) && self.connects(function, index) {
                    self.unsupported(value.at, "a list's element at a position that ticks");
                    return "None".to_string();
                }
                // ERR-6: a dictionary's brackets "are strict
                // point-in-time reads; absence fails". hgraph's own
                // child selection is silent where the key is absent, so
                // the provider's node selects it and fails there, with
                // the code `value.missing_key`. It publishes where the
                // key ticks or the selected child does. A key known
                // where the graph is built is a constant connection of
                // the key's type.
                if let Type::Dict { key, value: child } = &base.ty {
                    if !publishes_whole(child) {
                        self.unsupported(value.at, "an element of a dictionary of composites");
                        return "None".to_string();
                    }
                    let connected = self.connects(function, index);
                    let Some(key) = self.annotation(key, false, value.at) else {
                        return "None".to_string();
                    };
                    let base = self.expression(function, base);
                    let index = self.expression(function, index);
                    let index = if connected {
                        index
                    } else {
                        format!("_hg.const({index}, tp=_hg.TS[{key}])")
                    };
                    self.natives = true;
                    return format!("_core.entry_of({base}, {index})");
                }
                // TYPE-9: a tuple's position is its own connection, the
                // field of its position (`tuple_annotation`), under a
                // constant index (EXPR-3).
                if matches!(base.ty, Type::Tuple(_)) {
                    let base = self.expression(function, base);
                    let key = self.position_key(function, index);
                    return format!("{base}[{key}]");
                }
                let base = self.expression(function, base);
                let index = self.expression(function, index);
                format!("{base}[{index}]")
            }
            // In a hook a position of a temporal tuple is read through
            // the position's own port (TYPE-9), as a field is.
            ExprKind::Index { base, .. }
                if matches!(function.mode, Mode::Node | Mode::Stop)
                    && matches!(base.ty, Type::Tuple(_))
                    && let Some(port) = self.input_port(function, value) =>
            {
                self.part_value(&value.ty, &port, value.at)
            }
            ExprKind::Index { base, index } => {
                let Some(read) = element_read(&base.ty) else {
                    self.unsupported(value.at, "this element");
                    return "None".to_string();
                };
                self.natives = true;
                let base = self.expression(function, base);
                let index = self.expression(function, index);
                format!("_core.{read}({base}, {index})")
            }
            // ERR-2: "`ok(value)` constructs a success and
            // `err(value)` a failure."
            ExprKind::Tagged { success, payload } => {
                self.natives = true;
                let payload = self.expression(function, payload);
                let tag = if *success { "Ok" } else { "Err" };
                format!("_core.{tag}({payload})")
            }
            // ERR-3: `is_ok(value)` and `is_err(value)` give a `bool`.
            // "In composition a test of a connection is an ordinary
            // lifted call (FN-6): it gives a connection of `bool` and
            // no fact": the provider's node, which publishes the test
            // on each tick of the result.
            ExprKind::TagTest {
                success,
                value: tested,
            } => {
                self.natives = true;
                let lifted = function.mode == Mode::Graph && self.connects(function, tested);
                let tested = self.expression(function, tested);
                let tag = if *success { "ok" } else { "err" };
                if lifted {
                    self.hgraph = true;
                    return format!("_core.lifted_is_{tag}({tested})");
                }
                let tag = if *success { "Ok" } else { "Err" };
                format!("_py.isinstance({tested}, _core.{tag})")
            }
            // ERR-3: "`binding.value` is the T and `binding.error` the
            // E", where a guard has proved which the binding is.
            ExprKind::Payload { success, result } => {
                let result = self.expression(function, result);
                let part = if *success { "value" } else { "error" };
                format!("{result}.{part}")
            }
            // ERR-4: `error(message: text, code: optional_code)`, the
            // code null when it is left out.
            ExprKind::Error { message, code } => {
                self.natives = true;
                // EXPR-2: arguments are evaluated in the order
                // written, and Python evaluates keyword arguments in
                // theirs. With `code` written first, a `?` in it is
                // met before one in `message`.
                let code_first = code
                    .as_ref()
                    .is_some_and(|code| code.at.span.start < message.at.span.start);
                let mut written = Vec::new();
                if !code_first {
                    written.push(format!("message={}", self.expression(function, message)));
                }
                written.push(match code {
                    Some(code) => format!("code={}", self.expression(function, code)),
                    None => "code=None".to_string(),
                });
                if code_first {
                    written.push(format!("message={}", self.expression(function, message)));
                }
                format!("_core.Error({})", written.join(", "))
            }
            ExprKind::ErrorPart { base, part } => {
                let base = self.expression(function, base);
                // ERR-4: the provider's `Error` has its context, an
                // `ErrorContext` or `None`, as `context`.
                let part = match part {
                    ErrorPart::Message => "message",
                    ErrorPart::Code => "code",
                    ErrorPart::Context => "context",
                    ErrorPart::Node => "node",
                    ErrorPart::Phase => "phase",
                    ErrorPart::EvaluationTime => "evaluation_time",
                };
                format!("{base}.{part}")
            }
            // ERR-5: "Postfix `?` evaluates once. Success yields T. In
            // a value function returning `result<U,E>`, failure
            // returns the same E as Err. In when, only
            // `result<T,error>` may propagate to node failure."
            ExprKind::Propagate(result) => {
                let called = match function.mode {
                    Mode::Value => {
                        self.propagates = true;
                        "propagated"
                    }
                    Mode::Node => "succeeded",
                    // The analyser rejects a `?` anywhere else
                    // (`result.propagation_phase`).
                    _ => {
                        self.unsupported(value.at, "a `?` here");
                        return "None".to_string();
                    }
                };
                self.natives = true;
                let result = self.expression(function, result);
                format!("_core.{called}({result})")
            }
            ExprKind::Capture(call) => self.capture(function, call),
            // ERR-7: "`.value` retains the normal output type": the
            // child's own output. `.error` is "a connection of `error`":
            // hgraph's stream of the child's failures, each made an
            // `error` by the provider's node.
            ExprKind::Captured { capture, error } => {
                let capture = self.expression(function, capture);
                if *error {
                    self.natives = true;
                    format!("_core.captured_error({capture}.exception)")
                } else {
                    format!("{capture}.out")
                }
            }
            // TIME-3: "In that handler's body `modified([])`, as a bool,
            // is true, at a wake that lands with an input's tick
            // included: what the header selected is taken as nothing
            // modified. Elsewhere, in another body or under a negation
            // or a disjunction, it is true in a cycle where no temporal
            // input was modified." The selection in a header is the
            // node's (`node`). Elsewhere each input is asked what
            // `modified(input)` asks of it: a publication, and no
            // invalidation, which "never satisfies a modification test".
            // `start` and `stop` are no cycle of the node, and the rule
            // does not say what it is there.
            ExprKind::Endpoint {
                operation: EndpointOperation::Modified,
                inputs: Some(inputs),
            } if inputs.is_empty() => {
                if function.mode != Mode::Node {
                    self.unsupported(value.at, "`modified([])` in `start` or `stop`");
                    return "None".to_string();
                }
                if self.empty_selected {
                    return "True".to_string();
                }
                let modified: Vec<String> = (0..function.locals.len())
                    .filter(|&local| function.is_input(local))
                    .map(|local| {
                        let one = Expr {
                            kind: ExprKind::Endpoint {
                                operation: EndpointOperation::Modified,
                                inputs: Some(vec![local]),
                            },
                            ty: Type::Scalar(Scalar::Bool),
                            at: value.at,
                        };
                        self.expression(function, &one)
                    })
                    .collect();
                match modified.as_slice() {
                    // A node with no temporal input.
                    [] => "True".to_string(),
                    _ => format!("(not ({}))", modified.join(" or ")),
                }
            }
            _ => {
                self.unsupported(value.at, "this expression");
                "None".to_string()
            }
        }
    }

    /// `Quote(bid: 1.0, ask: 2.0)`: a value of the struct `of`, a
    /// module and a declaration of it, with each field by its name.
    ///
    /// TYPE-5: "Its arguments are named, each for a field the struct
    /// has, a parent's included, in any order ... A field with a
    /// default, its own or one a struct below gives it, may be left
    /// out, and then has that default. An optional field with no
    /// default may be left out too, and is then null ... Written
    /// arguments evaluate once in source order; failure prevents later
    /// arguments. Defaults are evaluated only when omitted."
    ///
    /// The arguments are written in the order the source has them,
    /// which is the order Python evaluates them in, and then the
    /// default of each field left out.
    ///
    /// TYPE-3: "Generic applications are invariant by origin and
    /// complete canonical arguments". A value of a generic struct is
    /// made of the class for its arguments, `_core.applied(Box,
    /// "i64")`: a `Box<i64>` is then no `Box<f64>` where the two meet
    /// as values of a parent. `whole` is the construction, whose type
    /// says the arguments. Where they are not known as the module is
    /// written, in a function generic in them, the construction is
    /// reported.
    fn construct(
        &mut self,
        function: &Function,
        of: (usize, usize),
        arguments: &[Argument],
        whole: &Expr,
    ) -> String {
        let at = whole.at;
        let program = self.program;
        let Some(Detail::Struct(read)) = program.modules[of.0].details.get(of.1) else {
            self.unsupported(at, "this construction");
            return "None".to_string();
        };
        let mut written = Vec::new();
        for argument in arguments {
            let field = argument.parameter.and_then(|field| read.members.get(field));
            let Some(field) = field else {
                self.unsupported(argument.value.at, "this argument of a construction");
                continue;
            };
            let value = self.expression(function, &argument.value);
            written.push(format!("{}={value}", name(&field.name)));
        }
        for (index, field) in read.members.iter().enumerate() {
            let given = arguments
                .iter()
                .any(|argument| argument.parameter == Some(index));
            if given {
                continue;
            }
            let value = match field.default {
                Some(default) => self.field_default(default, at),
                // The analyser has held the construction to every field
                // that is not optional (TYPE-5).
                None => "None".to_string(),
            };
            written.push(format!("{}={value}", name(&field.name)));
        }
        // The class for the construction's arguments (`struct_class`).
        // A struct with no generic parameter is its own class.
        let applied = match &whole.ty {
            Type::Named { arguments, .. } if !arguments.is_empty() => self.struct_class(&whole.ty),
            _ if read.generics.is_empty() => Some(self.type_name(of.0, of.1)),
            _ => None,
        };
        let applied = applied.unwrap_or_else(|| {
            self.unsupported(
                at,
                "a generic struct made where its arguments are not known",
            );
            self.type_name(of.0, of.1)
        });
        format!("{applied}({})", written.join(", "))
    }

    /// The default the field `default` writes, where the construction
    /// at `at` leaves the field out. It is read from the analysed
    /// program, as a parameter's is (see `left_out`): a default names
    /// no field and no local (UNIT-7), so it means the same wherever it
    /// is written.
    fn field_default(&mut self, default: FieldRef, at: Location) -> String {
        let Some(value) = field_default(self.program, default) else {
            self.unsupported(at, "a default of a field left out here");
            return "None".to_string();
        };
        // A default that constructs its own struct with that same
        // field left out would be written inside itself without end.
        let key = (default.module, default.declaration, default.field);
        if self.expanding.contains(&key) {
            self.unsupported(value.at, "a default that leaves itself out");
            return "None".to_string();
        }
        let cold = Function::cold(Mode::Value);
        self.expanding.push(key);
        let written = self.expression(&cold, value);
        self.expanding.pop();
        written
    }

    fn literal(&mut self, literal: &Value, at: Location) -> String {
        match literal {
            Value::Int(value) => value.to_string(),
            Value::Float(value) => float(*value),
            Value::Bool(true) => "True".to_string(),
            Value::Bool(false) => "False".to_string(),
            Value::Str(text) => string(text),
            Value::Null => "None".to_string(),
            // LEX-13: a duration literal is a count of whole microseconds,
            // which the analyser has made of its parts. OPS-1: its
            // operators are the provider's functions on that count.
            Value::Duration(microseconds) => {
                self.datetime = true;
                format!("_dt.timedelta(microseconds={microseconds})")
            }
            Value::Temporal(parts) => self.temporal(parts, at),
            // LEX-28, TYPE-17: a tenor literal is its counts, which the
            // analyser has made of its parts: the provider's `Tenor`.
            // TEN-1: its operators are the provider's functions.
            Value::Tenor(Tenor::Moves { months, days }) => {
                self.natives = true;
                format!("_core.Tenor({months}, {days})")
            }
            Value::Tenor(Tenor::BusinessDays(count)) => {
                self.natives = true;
                format!("_core.Tenor(business={count})")
            }
        }
    }

    /// A temporal literal, by the parts it writes (LEX-11), which are
    /// what its type goes by. TYPE-2: a `date`, a `time`, a
    /// `civil_datetime` and a `datetime` are Python's `date`, `time` and
    /// `datetime`; a `timezone`, a `zoned_time` and a `zoned_datetime`
    /// are the provider's, made by its functions, which check the zone
    /// (LEX-12: "The selected run provider checks exact catalog
    /// membership and offset consistency").
    fn temporal(&mut self, parts: &Temporal, at: Location) -> String {
        if let Some(zone) = &parts.zone {
            self.natives = true;
            let zone = string(zone);
            return match (parts.date, parts.time, parts.offset) {
                (None, None, _) => format!("_core.timezone({zone})"),
                (None, Some(time), _) => {
                    self.datetime = true;
                    format!("_core.zoned_time(_dt.time({}), {zone})", time_fields(time))
                }
                (Some(date), Some(time), Some(minutes)) => {
                    if !in_calendar(date, time, minutes) {
                        self.unsupported(at, "an instant outside the years 1 to 9999");
                        return "None".to_string();
                    }
                    self.datetime = true;
                    format!(
                        "_core.zoned_datetime(_dt.datetime({}, {}), {minutes}, {zone})",
                        date_fields(date),
                        time_fields(time)
                    )
                }
                // LEX-11: a zone follows an offset or a time, or stands
                // alone, and the lexer reads no other.
                _ => {
                    self.unsupported(at, "a zone after a date with no time or no offset");
                    "None".to_string()
                }
            };
        }
        self.datetime = true;
        match (parts.date, parts.time, parts.offset) {
            (Some(date), None, _) => format!("_dt.date({})", date_fields(date)),
            (None, Some(time), _) => format!("_dt.time({})", time_fields(time)),
            (Some(date), Some(time), offset) => {
                let written = format!("_dt.datetime({}, {})", date_fields(date), time_fields(time));
                match offset {
                    // A local date and time, and an instant written in
                    // UTC: the parts are the value's own.
                    None | Some(0) => written,
                    // TYPE-2: "Instants/durations use signed 64-bit
                    // microseconds", and an instant here is a Python
                    // `datetime`, which holds the years 1 to 9999 only.
                    // One written near either end with an offset may be
                    // outside them in UTC: it is not written.
                    Some(minutes) if !in_calendar(date, time, minutes) => {
                        self.unsupported(at, "an instant outside the years 1 to 9999");
                        "None".to_string()
                    }
                    // An instant written east or west of UTC is that
                    // many minutes from what its parts say. The
                    // provider works it out, with Python's calendar.
                    Some(minutes) => {
                        self.natives = true;
                        format!("_core.instant({written}, {minutes})")
                    }
                }
            }
            // The lexer gives a literal a date, a time or a zone.
            (None, None, _) => {
                self.unsupported(at, "a temporal literal with no date and no time");
                "None".to_string()
            }
        }
    }

    /// What `value` designates, a designation (`designates`) given to a
    /// `ref` result by a `return` or `out =` (FN-16), as the provider's
    /// `Designation` is told it (`Designating`). `None`, reported, for
    /// one not written yet.
    pub(super) fn designation(&mut self, function: &Function, value: &Expr) -> Option<Designating> {
        let port = |emitter: &Self, local: usize| {
            emitter
                .designated
                .iter()
                .find(|(designated, _)| *designated == local)
                .map(|(_, port)| string(port))
        };
        match (
            designated(function.locals, function.signature, value),
            &value.kind,
        ) {
            (Some((local, Designated::Whole)), _) if function.is_reference(local) => {
                Some(Designating::Sampled(endpoint_name(&function.locals[local])))
            }
            (Some((local, Designated::Whole)), _) => port(self, local).map(Designating::Followed),
            (Some((local, Designated::Position)), ExprKind::ChildEndpointOf(child)) => {
                let ExprKind::Index { index, .. } = &child.kind else {
                    return None;
                };
                let port = port(self, local)?;
                let index = self.expression(function, index);
                Some(Designating::Followed(format!("{port}, {index}")))
            }
            _ => {
                // hgraph gives a `REF` input of a list no reference of
                // its child; a dictionary's absent key and an unsized
                // list's position it does not hold have no port to
                // refer to.
                let rooted_at_reference = matches!(
                    &value.kind,
                    ExprKind::ChildEndpointOf(child)
                        if matches!(child.place_root().kind, ExprKind::Local(local) if function.is_reference(local))
                );
                self.unsupported(
                    value.at,
                    if rooted_at_reference {
                        "a child of a `ref` parameter designated"
                    } else {
                        "a designation of a child that is no position of a fixed list"
                    },
                );
                None
            }
        }
    }

    /// The endpoint of `child`, a child of a temporal input of the node
    /// (TIME-1), as hgraph has it: its own child of the input's,
    /// `parts[1]` as `_core.position_of(parts, 1)`, `quote.bid` as
    /// `quote["bid"]`, and a dictionary's entry as `_core.entry_port`,
    /// one that is not valid where the key is absent, named or read
    /// alike. A list's position is hgraph's child of its `TSL`, fixed or
    /// unbounded. `None` where hgraph has no child for it.
    fn child_port(&mut self, function: &Function, child: &Expr) -> Option<String> {
        match &child.kind {
            ExprKind::Local(local) if function.is_input(*local) || function.is_out(*local) => {
                Some(endpoint_name(&function.locals[*local]))
            }
            ExprKind::Field { base, member } => {
                let field = field_of(self.program, &base.ty, *member)?;
                let base = self.child_port(function, base)?;
                Some(format!("{base}[{}]", string(&name(&field.name))))
            }
            // TYPE-9: a tuple's position is the field of its position
            // (`tuple_annotation`), under a constant index.
            ExprKind::Index { base, index } if matches!(base.ty, Type::Tuple(_)) => {
                let key = self.position_key(function, index);
                let base = self.child_port(function, base)?;
                Some(format!("{base}[{key}]"))
            }
            ExprKind::Index { base, index } => {
                let of = match &base.ty {
                    // TIME-1: a position an unsized list does not hold is
                    // a child that is not valid, as an absent key's is.
                    Type::List { size: None, .. } => "list_position",
                    Type::List { .. } => "position_of",
                    // An absent key's entry is a child that is not valid
                    // (TIME-1), read or named alike.
                    Type::Dict { .. } => "entry_port",
                    _ => return None,
                };
                let base = self.child_port(function, base)?;
                let index = self.expression(function, index);
                self.natives = true;
                Some(format!("_core.{of}({base}, {index})"))
            }
            _ => None,
        }
    }

    /// True where `value` reads a child of a temporal input of the node or
    /// of its `out`, an element of a list, a field, a tuple's position or a
    /// dictionary's entry, at any depth, each step one hgraph has a child
    /// for (`child_port`).
    fn reads_child(&self, function: &Function, value: &Expr) -> bool {
        fn within(emitter: &Emitter<'_>, function: &Function, value: &Expr) -> bool {
            match &value.kind {
                // The input, or `out`, is a collection (TIME-9): an
                // `atomic<V>` and a `T?` have no child, as the analyser
                // has it. TIME-7: `out` read holds what it
                // published, each part by its own port.
                ExprKind::Local(local) => {
                    (function.is_input(*local) || function.is_out(*local))
                        && matches!(
                            function.locals[*local].ty,
                            Type::List { .. }
                                | Type::Tuple(_)
                                | Type::Dict { .. }
                                | Type::Named {
                                    kind: NamedKind::Struct,
                                    ..
                                }
                        )
                }
                // An `atomic<V>`'s field is computed from the whole value
                // (TYPE-9), and is no child.
                ExprKind::Field { base, member } => {
                    matches!(
                        base.ty,
                        Type::Named {
                            kind: NamedKind::Struct,
                            ..
                        }
                    ) && field_of(emitter.program, &base.ty, *member).is_some()
                        && within(emitter, function, base)
                }
                // A window is no collection (TIME-9): a position of the
                // list its value is read as (TYPE-11) is no child of it.
                ExprKind::Index { base, .. } => {
                    matches!(
                        base.ty,
                        Type::List { .. } | Type::Tuple(_) | Type::Dict { .. }
                    ) && !window_child(emitter.program, base)
                        && within(emitter, function, base)
                }
                _ => false,
            }
        }
        matches!(value.kind, ExprKind::Field { .. } | ExprKind::Index { .. })
            && within(self, function, value)
    }

    /// TIME-9's `valid` of the endpoint `port`, of a temporal value of
    /// `ty`, where it is not hgraph's own: `_core.valid_of`, where a list
    /// is in the value (`validity_shape`). `None` where hgraph's `valid`
    /// is the language's.
    pub(super) fn precise_valid(&mut self, ty: &Type, port: &str) -> Option<String> {
        let shape = validity_shape(self.program, ty)?;
        self.natives = true;
        Some(format!("_core.valid_of({port}, {shape})"))
    }

    /// TIME-9's `valid` of the endpoint `port`, of a temporal value of
    /// `ty`: "true where any element, field or entry is valid, and for a
    /// published empty list or dictionary" (END-7). hgraph's `valid`, but
    /// where a list is in the value (`precise_valid`).
    pub(super) fn valid_test(&mut self, ty: &Type, port: &str) -> String {
        self.precise_valid(ty, port)
            .unwrap_or_else(|| format!("{port}.valid"))
    }

    /// TIME-9's `all_valid` of the endpoint `port`, of a temporal value of
    /// `ty`: "true where every one is, by its own `valid`, one level
    /// deep", an optional child aside. hgraph's `all_valid` counts each
    /// part by hgraph's `valid`, which is the language's where the part
    /// holds no list (`valid_test`); where one does, or where a child is
    /// optional, the provider counts them.
    pub(super) fn all_valid_test(&mut self, ty: &Type, port: &str) -> String {
        // TIME-9: "`all_valid` is true where every one is, by its own
        // `valid`, one level deep, an optional child aside: one whose type
        // is `T?`, or a field that TYPE-4 makes optional, is not asked".
        // hgraph's `all_valid` asks every child, so where one is optional
        // the provider asks the others (`all_valid_of`), each by its own
        // `valid` (`validity_shape`). Where every child is optional none
        // is asked, and `all_valid` is the collection's `valid`: true of
        // a valid collection, and, as TIME-9 proposes, false of one that
        // is not valid.
        match asked_children(self.program, ty) {
            Asked::Every => {}
            Asked::None => return self.valid_test(ty, port),
            Asked::Some(required) => {
                let parts: Vec<String> = required
                    .iter()
                    .map(|(index, part)| {
                        // The child's port: `"bid"` for a struct's field,
                        // `"_0"` for a tuple's position (`tuple_annotation`).
                        let key = match ty {
                            Type::Tuple(_) => position(*index),
                            _ => field_of(self.program, ty, *index)
                                .map_or_else(String::new, |member| name(&member.name)),
                        };
                        let shape = validity_shape(self.program, part);
                        format!(
                            "{}: {}",
                            string(&key),
                            shape.unwrap_or_else(|| "None".to_string())
                        )
                    })
                    .collect();
                self.natives = true;
                return format!(
                    "_core.all_valid_of({port}, (\"fields\", {{{}}}))",
                    parts.join(", ")
                );
            }
        }
        let parts_hold_lists = match ty {
            Type::List { element, .. } => validity_shape(self.program, element).is_some(),
            other => validity_shape(self.program, other).is_some(),
        };
        match validity_shape(self.program, ty) {
            Some(shape) if parts_hold_lists => {
                self.natives = true;
                format!("_core.all_valid_of({port}, {shape})")
            }
            _ => format!("{port}.all_valid"),
        }
    }

    /// The argument a provider's function of a child's endpoint is given
    /// after the port where a list is in the child, its `validity_shape`,
    /// `, ("list", None)`: what its `valid` is read by. Empty where
    /// hgraph's `valid` is the language's.
    fn child_shape(&self, ty: &Type) -> String {
        validity_shape(self.program, ty).map_or(String::new(), |shape| format!(", {shape}"))
    }

    /// Reports `argument`, a property of a delta of a list or a
    /// dictionary that the program makes, where its value holds an
    /// unsized list's delta (TYPE-16): hgraph applies such a nested
    /// `TSLDelta` as it is, and fails the run for a negative size, which
    /// the provider counts only for a list whose port the node's own
    /// output has (`published_into`), and a position or a key the
    /// delta adds has none yet. A struct's and a tuple's are counted.
    fn nested_list_delta(&mut self, argument: &Argument) {
        if holds_growing_list(self.program, &argument.value.ty) {
            self.unsupported(
                argument.value.at,
                "an unsized list's delta inside the delta of a list or a dictionary",
            );
        }
    }

    /// `operation` of the child `child` names, an endpoint whether or not
    /// it is valid (END-8), written at `at`.
    fn child_operation(
        &mut self,
        function: &Function,
        operation: EndpointOperation,
        child: &Expr,
        at: Location,
    ) -> String {
        // At stop, validity and the last publication time are read, as
        // an input's are (TIME-5).
        let given = match function.mode {
            Mode::Node => true,
            Mode::Stop => matches!(
                operation,
                EndpointOperation::Valid
                    | EndpointOperation::AllValid
                    | EndpointOperation::LastModified
            ),
            _ => false,
        };
        if !given {
            self.unsupported(at, "this endpoint read in `start` or `stop`");
            return "None".to_string();
        }
        // hgraph's `REF` input holds one reference, and gives none of a
        // child (FN-16: "A `ref` parameter and its children ... are
        // endpoints").
        if matches!(child.place_root().kind, ExprKind::Local(local) if function.designates(local)) {
            self.unsupported(at, "an endpoint of a child of a `ref` parameter or result");
            return "None".to_string();
        }
        let Some(port) = self.child_port(function, child) else {
            self.unsupported(at, "an endpoint of this child");
            return "None".to_string();
        };
        self.natives = true;
        match operation {
            EndpointOperation::Valid => self.valid_test(&child.ty, &port),
            EndpointOperation::AllValid => self.all_valid_test(&child.ty, &port),
            // END-4, END-6: a child that is not valid is not modified and
            // has no time it was last modified, nor a delta to read, by
            // `valid` as the language has it where a list is in the child
            // (`validity_shape`), whose shape the provider is given.
            EndpointOperation::Modified => {
                let shape = self.child_shape(&child.ty);
                format!("_core.child_modified({port}{shape})")
            }
            // END-8: a child's delta is read as a direct input's of its
            // type is (`language_delta`).
            EndpointOperation::DeltaValue => {
                // A window of a duration's, of a child that is valid
                // (`window_delta`).
                if duration_window(&child.ty) {
                    format!("_core.checked_child({port}, _core.window_delta)")
                } else {
                    let shape = self.child_shape(&child.ty);
                    self.language_delta(&child.ty, format!("_core.child_delta({port}{shape})"), at)
                }
            }
            EndpointOperation::LastModified => {
                let shape = self.child_shape(&child.ty);
                format!("_core.child_last_modified({port}{shape})")
            }
            other => {
                self.unsupported(at, &format!("`{}` of a child", other.name()));
                "None".to_string()
            }
        }
    }

    /// The read `read` of a child known only when it runs (TIME-1),
    /// written at `at`: its value, read from its endpoint, or a failure
    /// with `value.unset_read` where it is not valid; where `result`,
    /// `ok` of the value or `err` of that failure's error (ERR-14).
    fn checked_child(
        &mut self,
        function: &Function,
        read: &Expr,
        result: bool,
        at: Location,
    ) -> String {
        self.natives = true;
        let Some(port) = self.child_port(function, read) else {
            self.unsupported(at, "this read of a child");
            return "None".to_string();
        };
        // A window's value is read as the `list<V>` it holds, and fails
        // where the window is not all valid (TYPE-11, `window_value`).
        // ERR-14: so does "a read of a window's value before it is
        // `all_valid`" take its form from its place: where it takes a
        // `result`, `checked_child_result` gives that failure's `err`.
        let value = if matches!(read.ty, Type::RollingWindow { .. }) {
            self.window_value("_port")
        } else {
            self.part_value(&read.ty, "_port", at)
        };
        let of = if result {
            "checked_child_result"
        } else {
            "checked_child"
        };
        format!("_core.{of}({port}, lambda _port: {value})")
    }

    /// The port of a node's temporal input that `value` reads, where it
    /// is the input or a field of a temporal struct port, as hgraph
    /// names it: `quote`, `quote["bid"]`. `None` for anything else, an
    /// `atomic<V>`'s field among them, which is read from its value.
    pub(super) fn input_port(&self, function: &Function, value: &Expr) -> Option<String> {
        match &value.kind {
            ExprKind::Local(local) if function.is_input(*local) || function.is_out(*local) => {
                Some(endpoint_name(&function.locals[*local]))
            }
            ExprKind::Field { base, member } => {
                // An atomic input is one port of the whole value, read
                // in a hook as that value (TYPE-9), and has no port of
                // a field. The `?` of `atomic<V>?` marks an input that
                // may be left out (TYPE-14): its port is the atomic's.
                if let ExprKind::Local(local) = base.kind
                    && matches!(
                        function.locals[local].ty.omission_removed(),
                        Type::Atomic(_)
                    )
                {
                    return None;
                }
                let field = field_of(self.program, &base.ty, *member)?;
                let base = self.input_port(function, base)?;
                Some(format!("{base}[{}]", string(&name(&field.name))))
            }
            // A tuple's position, under a constant index, is the field
            // of its position (`tuple_annotation`).
            ExprKind::Index { base, index } if matches!(base.ty, Type::Tuple(_)) => {
                let index = constant_position(index)?;
                let base = self.input_port(function, base)?;
                Some(format!("{base}[{}]", string(&position(index))))
            }
            _ => None,
        }
    }

    /// The value of a window, an input, `out` or a child of either, read
    /// in a hook through its endpoint `port`. TYPE-11: "That value, read
    /// in a hook ... is a `list<V>` of the values held, oldest first, and
    /// the read before `all_valid` fails with `value.unset_read`", as
    /// core's END-9 has it. hgraph's `TSW` holds them, and reads them
    /// before it is all valid too: the provider's `window_values` fails
    /// that read.
    pub(super) fn window_value(&mut self, port: &str) -> String {
        self.natives = true;
        format!("_core.window_values({port})")
    }

    /// The read `read` of a window's value where its place takes
    /// `result<list<V>, error>` (TYPE-11, ERR-14), written at `at`: the
    /// window an input, `out` or a child of either named when the program
    /// is compiled, read through its endpoint. The provider gives `ok`
    /// of the list it holds, or, where it is not all valid, `err` of an
    /// `error` whose code is `value.unset_read`
    /// (`_core.window_result`).
    fn window_result(&mut self, function: &Function, read: &Expr, at: Location) -> String {
        if matches!(read.ty, Type::RollingWindow { .. }) {
            self.unsupported(read.at, GENERIC_WINDOW_READ);
            return "None".to_string();
        }
        let Some(port) = self.child_port(function, read) else {
            self.unsupported(at, "this read of a window");
            return "None".to_string();
        };
        self.natives = true;
        format!("_core.window_result({port})")
    }

    /// The read `value` of a window, an input, `out` or a child of
    /// either, through its endpoint `port` (`window_value`). The analyser
    /// types a window's read as the `list<V>` it holds (TYPE-11); one
    /// typed as the window itself is a generic type parameter's read in
    /// an instance where the parameter stands for a window. The body was
    /// checked once, its `T` read as a `T` (GEN-7), and the rules do not
    /// say what the read of a `T` that is a window gives, nor that such a
    /// binding rejects: it is reported.
    fn window_read(&mut self, value: &Expr, port: &str) -> String {
        if matches!(value.ty, Type::RollingWindow { .. }) {
            self.unsupported(value.at, GENERIC_WINDOW_READ);
            return "None".to_string();
        }
        self.window_value(port)
    }

    /// What the port `port`, of a temporal value of `ty`, holds, read in
    /// a hook. A struct's bundle holds a value of the struct, made from
    /// each field's port in turn (TYPE-9), and a tuple's a tuple of each
    /// position's; a list's and a dictionary's children hold its
    /// elements; any other port holds its `value`. The read is at `at`,
    /// where what is not written yet is reported.
    pub(super) fn port_value(&mut self, ty: &Type, port: &str, at: Location) -> String {
        self.port_value_at(ty, port, 0, at)
    }

    /// `port_value` of a port read inside `depth` comprehensions, whose
    /// names `_child{depth}` and `_key{depth}` are its own.
    fn port_value_at(&mut self, ty: &Type, port: &str, depth: usize, at: Location) -> String {
        match ty {
            // A value that holds a window, a struct's or a list's, is not
            // read whole yet; a window input itself is (`window_value`).
            Type::RollingWindow { .. } => {
                self.unsupported(at, "a window's value inside another value");
                return "None".to_string();
            }
            // TYPE-9: "lists to child positions". A list's value is each
            // child's, in order: hgraph's own tuple of them, fixed or
            // unbounded, where each child publishes whole, made a Python
            // `list` (`list_value`). TIME-1's table: a position of an
            // unsized list "that holds no value, a gap or one removed
            // (TYPE-16), reads in the whole value ... as that type's value
            // for 'not set' where it has one", NaN for an `f64`; an
            // optional element reads null, "as an optional child that is
            // not valid is in any whole value (TYPE-15)", of a fixed list
            // too; any other fails the read, and so does a fixed list
            // that is not all valid.
            Type::List { element, size } if publishes_whole(element) => {
                self.natives = true;
                let hole = match (&**element, size) {
                    (Type::Nullable(_), _) => ", None",
                    (Type::Scalar(Scalar::F64), None) => ", _py.float(\"nan\")",
                    _ => "",
                };
                return format!("_core.list_value({port}{hole})");
            }
            // An element of composites is read as a part is
            // (`part_value_at`): each of its own required parts where it is
            // valid, and an optional one null where it is not.
            Type::List { element, .. } => {
                let child = format!("_child{depth}");
                let read = self.part_value_at(element, &child, depth + 1, at);
                return format!("[{read} for {child} in {port}]");
            }
            // TYPE-9: "dictionaries to keyed children". A dictionary's
            // value has each key the `TSD` has, with its child's value.
            Type::Dict { value, .. } if publishes_whole(value) => {
                return format!("_py.dict({port}.value)");
            }
            Type::Dict { value, .. } => {
                let (key, child) = (format!("_key{depth}"), format!("_child{depth}"));
                let read = self.port_value_at(value, &child, depth + 1, at);
                return format!("{{{key}: {read} for {key}, {child} in {port}.items()}}");
            }
            // A set's value is a `frozenset`, as a `const` one is: hgraph
            // gives a `set`, which its `TSS` takes back as members added,
            // and a whole set published replaces what the set held
            // (TIME-7).
            Type::Set(_) => return format!("_py.frozenset({port}.value)"),
            // TYPE-9: tuples map "to child fields": a tuple of each
            // position's value, from the field of its position
            // (`tuple_annotation`).
            Type::Tuple(elements) => {
                let reads: Vec<String> = elements
                    .iter()
                    .enumerate()
                    .map(|(index, element)| {
                        let child = format!("{port}[{}]", string(&position(index)));
                        self.part_value_at(element, &child, depth, at)
                    })
                    .collect();
                return format!("({},)", reads.join(", "));
            }
            _ => {}
        }
        let (
            Some(fields),
            Type::Named {
                module,
                declaration,
                ..
            },
        ) = (fields_of(self.program, ty), ty)
        else {
            return format!("{port}.value");
        };
        // An abstract struct's value is of a struct below it, which its
        // bundle does not say (`struct_annotation`).
        let class = match self.struct_class(ty) {
            Some(class) if !inherited(self.program, *module, *declaration) => class,
            _ => {
                self.unsupported(at, &format!("a whole value of the temporal `{ty}`"));
                return "None".to_string();
            }
        };
        let fields: Vec<String> = fields
            .iter()
            .map(|(member, field_ty)| {
                let field = name(&member.name);
                let child = format!("{port}[{}]", string(&field));
                let read = self.part_value_at(field_ty, &child, depth, at);
                format!("{field}={read}")
            })
            .collect();
        format!("{class}({})", fields.join(", "))
    }

    /// What a part of a temporal struct or tuple holds, the port `port`
    /// of a field or a position of the type `ty`, read alone, `quote.ask`,
    /// or as its composite is read whole, `let held = quote`.
    ///
    /// A part that has not published has no value to give, and none is
    /// made up. TIME-1: "A read of a struct, a tuple or a fixed list
    /// whole, `return quote`, requires `all_valid` of it ... In the value
    /// it reads, an optional child (TIME-9) that is not valid is null".
    /// The analyser holds a whole read it sees to `all_valid`; one checked
    /// when it runs, `rows[i]`, "that is not all valid fails it
    /// (`value.unset_read`)" (ERR-14). So this target reads a required
    /// part that is not valid as that failure, with the catalogued
    /// `value.unset_read` ("Executed operation requires an unset
    /// ordinary payload"), and an optional part that is not valid as
    /// null. A fixed list of required elements that publish whole is held
    /// to each of them (`all_valid`); any other part to its own `valid`,
    /// and a struct's, a tuple's or a list's parts each in turn.
    pub(super) fn part_value(&mut self, ty: &Type, port: &str, at: Location) -> String {
        self.part_value_at(ty, port, 0, at)
    }

    /// `read`, the whole of the node's `out` read at its port `port`, of
    /// the result's type `ty`, where it is valid.
    ///
    /// TIME-7: "`inject out` exposes that same result for reads", read
    /// in a `when` and in `stop`. Until the node has published, and after
    /// `invalidate(out)`, `out` holds no payload, and nothing is made up
    /// for it: the read fails with the catalogue's `value.unset_read`,
    /// "Executed operation requires an unset ordinary payload", an
    /// execution failure (ERR-1) a capture takes (ERR-7). It fails where
    /// `valid(out)` is false, by the same test (`valid_test`). A part of
    /// `out` read alone is held to its own `valid` (`part_value`).
    fn published_read(&mut self, ty: &Type, port: &str, read: String) -> String {
        let ready = self.valid_test(ty, port);
        self.natives = true;
        format!(
            "({read} if {ready} else _core.unset_read(\"`out` is read before it holds a value\"))"
        )
    }

    /// `part_value` of a port read inside `depth` comprehensions.
    fn part_value_at(&mut self, ty: &Type, port: &str, depth: usize, at: Location) -> String {
        let read = self.port_value_at(ty, port, depth, at);
        // A list of elements that publish whole is read whole from its
        // `TSL`, which holds it to its own `valid` and fills or fails at
        // a position that holds no value (`list_value`). One of
        // composites reads each element as a part, whose own optional
        // parts may be invalid.
        if matches!(ty, Type::List { element, .. } if publishes_whole(element)) {
            return read;
        }
        let ready = self.valid_test(ty, port);
        if publishes_whole_null(ty) {
            return format!("({read} if {ready} else None)");
        }
        self.natives = true;
        format!("({read} if {ready} else _core.unset_read())")
    }

    /// The name of the field of the tuple position that `index` names
    /// (`tuple_annotation`), as Python text: `"_0"` for a literal, and
    /// for another constant, `pair[1 - 1]` or a constant of a module, the
    /// provider's `_core.position(index)`, of the index as it is
    /// evaluated. EXPR-7: "A tuple's index is a constant within its
    /// length", which the analyser has held it to.
    pub(super) fn position_key(&mut self, function: &Function, index: &Expr) -> String {
        match constant_position(index) {
            Some(at) => string(&position(at)),
            None => {
                self.natives = true;
                let index = self.expression(function, index);
                format!("_core.position({index})")
            }
        }
    }

    /// `value`, a tuple of `entries` written in a graph where one entry
    /// is a connection: the tuple's own connection, hgraph's bundle
    /// (`tuple_annotation`) made of each entry's, with no node of its
    /// own (TYPE-9: tuples map "to child fields"). An entry that is a
    /// value, known where the graph is built, is a constant connection
    /// of its position's type, which hgraph makes of a value that
    /// publishes whole and is not null. A connection of a struct whose
    /// position is an abstract struct it inherits from is narrowed to
    /// it (`narrowed`).
    fn connected_tuple(&mut self, function: &Function, value: &Expr, entries: &[Expr]) -> String {
        let Some(bundle) = self.annotation(&value.ty, true, value.at) else {
            return "None".to_string();
        };
        let mut given = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            if !self.connects(function, entry) && !whole_scalar(&entry.ty) {
                self.unsupported(entry.at, "a composite value in a tuple of connections");
                return "None".to_string();
            }
            let written = self.expression(function, entry);
            // A connection of a struct where its position is an abstract
            // struct it inherits from is that struct's bundle (TYPE-3).
            let written = match &value.ty {
                Type::Tuple(positions) if index < positions.len() => {
                    self.narrowed(function, &positions[index], entry, written)
                }
                _ => written,
            };
            given.push(format!("{}={written}", position(index)));
        }
        format!("{bundle}.from_ts({})", given.join(", "))
    }

    /// True when `value`, written in a graph, is a field of a
    /// connection of a struct or of an atomic struct (TYPE-9: tuples
    /// and structs map "to child fields"). hgraph gives the field's
    /// port: a bundle's own child, and for a `TS` of a whole struct a
    /// connection of that field of each value.
    fn connected_field(&self, function: &Function, value: &Expr) -> bool {
        let ExprKind::Field { base, .. } = &value.kind else {
            return false;
        };
        let whole = payload_of(&base.ty);
        matches!(
            whole,
            Type::Named {
                kind: NamedKind::Struct,
                ..
            }
        ) && self.connects(function, base)
    }

    /// True when `value`, written in a graph, is an element of a
    /// connection of a list or of a dictionary (TYPE-9: "lists to child
    /// positions, dictionaries to keyed children"): hgraph gives the
    /// child's port, and an unsized list's is reported where it is read.
    fn connected_element(&self, function: &Function, value: &Expr) -> bool {
        let ExprKind::Index { base, .. } = &value.kind else {
            return false;
        };
        matches!(
            base.ty,
            Type::List { .. } | Type::Dict { .. } | Type::Tuple(_)
        ) && self.connects(function, base)
    }

    /// True when `value`, written in a graph, is a connection: a
    /// temporal parameter or a local given one, what a temporal
    /// function gives, or what a lifted call gives (FN-6).
    pub(super) fn connects(&self, function: &Function, value: &Expr) -> bool {
        match &value.kind {
            ExprKind::Local(local) => function.locals[*local].category == ValueCategory::Connection,
            ExprKind::Call {
                callee, arguments, ..
            } => callee.declaration().is_some_and(|(module, declaration)| {
                let declared = &self.program.modules[module].declarations[declaration];
                declared.kind.role() == Some(Role::Temporal)
                    || arguments
                        .iter()
                        .any(|argument| self.connects(function, &argument.value))
            }),
            ExprKind::Widen(inner) => self.connects(function, inner),
            // A tuple is a connection where one of its entries is.
            ExprKind::Tuple(entries) => entries.iter().any(|entry| self.connects(function, entry)),
            // ERR-7: a capture is "read through two connections".
            ExprKind::Capture(_) | ExprKind::Captured { .. } => true,
            // ERR-3: "In composition a test of a connection is an
            // ordinary lifted call (FN-6): it gives a connection of
            // `bool`".
            ExprKind::TagTest { value, .. } => self.connects(function, value),
            // A field of a connection is the field's own connection,
            // and so is an element of one.
            ExprKind::Field { base, .. } | ExprKind::Index { base, .. } => {
                self.connects(function, base)
            }
            // CTRL-13: an `if` "is a connection when its condition is
            // one or either branch's value is", and a block is what
            // its value is.
            ExprKind::If {
                condition,
                then,
                otherwise,
            } => {
                self.connects(function, condition)
                    || self.connects(function, then)
                    || otherwise
                        .as_ref()
                        .is_some_and(|otherwise| self.connects(function, otherwise))
            }
            ExprKind::Block {
                value: Some(value), ..
            } => self.connects(function, value),
            _ => false,
        }
    }

    /// TYPE-16: `delta<set<i64>>(added: [1, 2], removed: [3])`, a set's
    /// delta made from its properties, as hgraph's own set delta. A
    /// property left out is empty. And a nullable's, `delta<i64?>()`.
    fn delta(&mut self, function: &Function, value: &Expr, arguments: &[Argument]) -> String {
        // TYPE-16: "A nullable's delta is an optional: it holds what
        // was published, a null among them ... `delta<i64?>()` is a
        // published null", and `delta<i64?>(value: 7)` a published 7.
        // It is hgraph's own delta of such a value: `Null`, or the
        // value.
        let delta = DeltaOf::of(&value.ty);
        if let Some(DeltaOf::Nullable) = delta {
            return match arguments.first() {
                Some(argument) => {
                    let value = self.expression(function, &argument.value);
                    self.published(value)
                }
                None => {
                    self.hgraph = true;
                    "_hg.Null".to_string()
                }
            };
        }
        // TYPE-16: a struct's delta has "a property for each field
        // `name: F`, of the same name and of type `delta<F>?`: null
        // where the field did not change", and "A property left out is
        // ... null for a struct's field". It is hgraph's delta of a
        // bundle: a `dict` of the fields that changed, which the
        // provider makes from the properties given, in the order
        // written, leaving out each that is null.
        if let Some(DeltaOf::Struct(of)) = delta {
            let program = self.program;
            let mut written = Vec::new();
            for argument in arguments {
                let given = self.expression(function, &argument.value);
                if let Some(field) = argument
                    .parameter
                    .and_then(|index| field_of(program, of, index))
                {
                    written.push(format!("{}={given}", name(&field.name)));
                }
            }
            self.natives = true;
            return format!("_core.struct_delta({})", written.join(", "));
        }
        // TYPE-16: a dictionary's delta has "`updated`, a `dict<K,
        // delta<V>>` of each key added or changed with its delta, and
        // `removed`, a `set<K>`". It is hgraph's delta of a `TSD`: one
        // `dict` of each key updated with its delta, and of each
        // removed with hgraph's `REMOVE`, which the provider makes from
        // the properties, given by name in the order written (EXPR-13).
        if let Some(DeltaOf::Dict) = delta {
            const PROPERTIES: [&str; 2] = ["updated", "removed"];
            let mut written = Vec::new();
            for argument in arguments {
                self.nested_list_delta(argument);
                let given = self.expression(function, &argument.value);
                if let Some(property) = argument.parameter.and_then(|index| PROPERTIES.get(index)) {
                    written.push(format!("{property}={given}"));
                }
            }
            self.natives = true;
            return format!("_core.dict_delta({})", written.join(", "));
        }
        // TYPE-16: a list's delta has "`updated`, each changed position
        // with its delta; `removed`, the positions that lost their
        // value ... and `size_modified`". The provider makes it as
        // hgraph's delta of a `TSL`, its `TSLDelta`, a `dict` of each
        // position updated and removed, which holds the size too (see
        // `list_delta`).
        if let Some(DeltaOf::List) = delta {
            const PROPERTIES: [&str; 3] = ["updated", "removed", "size_modified"];
            let mut written = Vec::new();
            for argument in arguments {
                self.nested_list_delta(argument);
                let given = self.expression(function, &argument.value);
                let Some(index) = argument.parameter.filter(|index| *index < PROPERTIES.len())
                else {
                    continue;
                };
                written.push(format!("{}={given}", PROPERTIES[index]));
            }
            self.natives = true;
            return format!("_core.list_delta({})", written.join(", "));
        }
        let Some(DeltaOf::Set(element)) = delta else {
            self.unsupported(
                value.at,
                "a delta that is not a set's, a list's, a nullable's, a dictionary's or a struct's",
            );
            return "None".to_string();
        };
        let Some(element) = self.annotation(element, false, value.at) else {
            return "None".to_string();
        };
        // EXPR-13: the properties are evaluated in the order they are
        // written, as a call's arguments are. Python evaluates keyword
        // arguments as written, so each is given by its name in that
        // order, and one left out is empty.
        const PROPERTIES: [&str; 2] = ["added", "removed"];
        let mut given = [false; 2];
        let mut written = Vec::new();
        for argument in arguments {
            let value = self.expression(function, &argument.value);
            if let Some(index) = argument.parameter.filter(|index| *index < PROPERTIES.len()) {
                given[index] = true;
                written.push(format!("{}={value}", PROPERTIES[index]));
            }
        }
        for (property, given) in PROPERTIES.iter().zip(given) {
            if !given {
                written.push(format!("{property}=_py.frozenset()"));
            }
        }
        self.hgraph = true;
        format!("_hg.set_delta({}, tp={element})", written.join(", "))
    }

    /// A dense trace written out: a list with `None` where it has `_`
    /// (TST-6: "Dense position i denotes the simulation start plus i
    /// minimum engine steps").
    fn trace(&mut self, function: &Function, cells: &[Cell], ty: &Type) -> String {
        // A trace of a nullable: a cell may be a published null. A
        // cell is the delta of what is published (TST-5).
        let nullable = matches!(
            ty,
            Type::Trace { cell, .. }
                if is_nullable(cell) || matches!(&**cell, Type::Delta(of) if is_nullable(of))
        );
        let written: Vec<String> = cells
            .iter()
            .map(|cell| match &cell.value {
                // `_`: nothing is published.
                None => "None".to_string(),
                // A published null is hgraph's `Null`, the tick of a
                // value that is unset: `None` in a trace is silence.
                Some(Expr {
                    kind: ExprKind::Literal(Value::Null),
                    ..
                }) => {
                    self.hgraph = true;
                    "_hg.Null".to_string()
                }
                // A cell that may be null where the test runs: a
                // value of the nullable type, and not a literal of
                // its payload's or the delta it is published as.
                Some(value) if nullable && is_nullable(&value.ty) => {
                    let value = self.expression(function, value);
                    self.published(value)
                }
                // TST-5: a cell "written as a `return` is ... a T,
                // published whole, or a `delta<T>`". A whole value where
                // the delta is a type of its own is marked so: an input
                // publishes it whole, replacing what it held (TST-9), and
                // an expectation's is compared with the value the result
                // holds once its cycle has published.
                // A whole value of a struct below the cell's struct, a
                // `Circle` where the cell is a `Shape`'s (TYPE-3), would be
                // published as the struct below, which the cell's bundle
                // does not take: it is not written yet.
                Some(value) if descendant_cell(ty, &value.ty) => {
                    self.unsupported(value.at, "a whole trace cell of a struct below its type");
                    "None".to_string()
                }
                Some(value) if whole_cell(ty, &value.ty) => {
                    // A whole value of composites, a struct with a struct
                    // field or a dictionary of dictionaries, would replace
                    // each part it holds in turn, which the harness does
                    // not follow yet; so would a list of nullables, whose
                    // whole tuple holds no null (`parts_publish_whole`).
                    // One this target does not read whole has no reader
                    // (`whole_readable`).
                    let unwritten = if !self.parts_publish_whole(&value.ty) {
                        Some(match &value.ty {
                            Type::List { element, .. } if publishes_whole(element) => {
                                "a whole trace cell of a list of nullables"
                            }
                            _ => "a whole trace cell of composites",
                        })
                    } else if !self.whole_readable(&value.ty) {
                        Some(
                            "a whole trace cell of an abstract struct, or of a generic one \
                             whose arguments are not known",
                        )
                    } else {
                        None
                    };
                    if let Some(unwritten) = unwritten {
                        self.unsupported(value.at, unwritten);
                        return "None".to_string();
                    }
                    let fixed = matches!(value.ty, Type::List { size: Some(_), .. });
                    // A tuple's is published as hgraph's bundle takes a
                    // whole one, position by position (`bundled`), a null
                    // position as a published null: its shape says how.
                    let shape = match &value.ty {
                        Type::Tuple(_) => match self.whole_shape(&value.ty, value.at) {
                            Some(shape) => Some(shape.text),
                            None => return "None".to_string(),
                        },
                        _ => None,
                    };
                    let value = self.expression(function, value);
                    self.harness = true;
                    match (fixed, shape) {
                        (true, _) => format!("_harness.whole({value}, fixed=True)"),
                        (false, Some(shape)) => format!("_harness.whole({value}, shape={shape})"),
                        (false, None) => format!("_harness.whole({value})"),
                    }
                }
                Some(value) => self.expression(function, value),
            })
            .collect();
        // TST-7: "Timed entries use duration offsets from run start or
        // absolute datetime keys ... no `_` occurs in timed traces."
        // The harness is given each entry as its key and its
        // publication, in the order written.
        if cells.iter().any(|cell| cell.time.is_some()) {
            self.harness = true;
            let entries: Vec<String> = cells
                .iter()
                .zip(written)
                .filter_map(|(cell, value)| {
                    let time = self.expression(function, cell.time.as_ref()?);
                    Some(format!("({time}, {value})"))
                })
                .collect();
            return format!("_harness.timed([{}])", entries.join(", "));
        }
        format!("[{}]", written.join(", "))
    }
}

/// What a `return` to a `ref` result, or an `out =` of one, designates,
/// as the provider's `Designation` is told it (FN-16).
pub(super) enum Designating {
    /// An ordinary input or a child path: the name of the hidden `REF`
    /// input the node is given for the input (`Designated`), and the
    /// index of a fixed list's position where it names one. "Readers
    /// follow a later rebinding of that endpoint without waking the node
    /// for it."
    Followed(String),
    /// A whole `ref` parameter, its own `REF` input: "Returning a whole
    /// `ref` parameter samples its current designation when the `return`
    /// executes".
    Sampled(String),
}

impl Designating {
    /// The provider's call that writes it: `returning` where it is a
    /// `return`, which gives what the node then returns too.
    pub(super) fn call(&self, returning: bool) -> String {
        match (self, returning) {
            (Self::Followed(key), false) => format!("_designation.designate({key})"),
            (Self::Followed(key), true) => format!("_designation.returned({key})"),
            (Self::Sampled(port), false) => format!("_designation.sample({port})"),
            (Self::Sampled(port), true) => format!("_designation.sampled({port})"),
        }
    }
}

/// A number with a fraction as a Python literal.
pub(super) fn float(value: f64) -> String {
    if value.is_nan() {
        "_py.float(\"nan\")".to_string()
    } else if value.is_infinite() {
        let sign = if value < 0.0 { "-" } else { "" };
        format!("_py.float(\"{sign}inf\")")
    } else {
        // `{:?}` writes the shortest text that reads back as the same
        // number, and always with a `.` or an exponent.
        format!("{value:?}")
    }
}

/// A date as Python's `date` and `datetime` take one: year, month, day.
fn date_fields(date: CalendarDate) -> String {
    format!("{}, {}, {}", date.year, date.month, date.day)
}

/// A time of day as Python's `time` and `datetime` take one: hour,
/// minute, second, microsecond.
fn time_fields(time: ClockTime) -> String {
    format!(
        "{}, {}, {}, {}",
        time.hour, time.minute, time.second, time.microsecond
    )
}

/// Whether the instant of `date` and `time` written `minutes` east of
/// UTC is in the years 1 to 9999 in UTC, which a Python `datetime` holds.
/// An offset is less than a day (LEX-19), so only a time on the first
/// day written east of UTC, or on the last day written west of it, can
/// leave them.
fn in_calendar(date: CalendarDate, time: ClockTime, minutes: i16) -> bool {
    let first = (date.year, date.month, date.day) == (1, 1, 1);
    let last = (date.year, date.month, date.day) == (9999, 12, 31);
    // The minutes of the day the time is at, its seconds aside: an
    // offset is whole minutes, so they do not change which day it is.
    let into_day = i32::from(time.hour) * 60 + i32::from(time.minute);
    let minutes = i32::from(minutes);
    !((first && into_day < minutes) || (last && into_day - minutes >= 24 * 60))
}

/// What the target reports where a generic declaration's type parameter
/// stands for a window whose value its body reads (`window_read`).
const GENERIC_WINDOW_READ: &str =
    "the value of a window that a generic type parameter stands for (GEN-7, TYPE-11)";

/// True where `value` is a child, a field, a position or an entry, whose
/// declared type is a window (TYPE-11), whatever its read is typed as.
pub(super) fn window_child(program: &Program, value: &Expr) -> bool {
    let window = |ty: &Type| matches!(ty, Type::RollingWindow { .. });
    match &value.kind {
        ExprKind::Field { base, member } => {
            field_of(program, &base.ty, *member).is_some_and(|field| window(&field.ty))
        }
        ExprKind::Index { base, index } => match &base.ty {
            Type::List { element: child, .. } | Type::Dict { value: child, .. } => window(child),
            Type::Tuple(elements) => constant_position(index)
                .and_then(|position| elements.get(position))
                .is_some_and(window),
            _ => false,
        },
        _ => false,
    }
}

/// The position a constant index names, `pair[1]`: EXPR-3 has a
/// tuple's position "under a constant index". `None` for an index that
/// is no literal integer.
pub(super) fn constant_position(index: &Expr) -> Option<usize> {
    match index.kind {
        ExprKind::Literal(Value::Int(at)) => usize::try_from(at).ok(),
        _ => None,
    }
}

/// True for the literal `null`.
fn is_null(value: &Expr) -> bool {
    matches!(value.kind, ExprKind::Literal(Value::Null))
}
