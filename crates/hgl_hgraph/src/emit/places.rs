//! Places as the hgraph target writes them: a local read as what holds
//! it, a `var`, `state` or `cache` assigned, a part of one changed where
//! it is through the provider (`_core.set_at`, `_core.given_at`) or
//! given a new value on the way in, a part of `out` assigned through its
//! port, and the places a call's `var` parameters are given (FN-15).
//! Part of the transpiler, the last stage: `emit` writes a module with
//! them.

use super::*;

impl Emitter<'_> {
    /// The `var` parameters of what `value` calls, by their positions,
    /// where it is a call of a function that declares any (FN-15).
    fn places_of(&self, value: &Expr) -> Vec<usize> {
        let Some((module, declaration, _)) = called(value) else {
            return Vec::new();
        };
        match self.program.modules[module].details.get(declaration) {
            Some(Detail::Callable(signature)) => signature
                .parameters
                .iter()
                .enumerate()
                .filter(|(_, parameter)| parameter.var)
                .map(|(index, _)| index)
                .collect(),
            _ => Vec::new(),
        }
    }

    /// A call that assigns places, which is the whole of a statement
    /// (FN-15): the call, and then each place given what its `var`
    /// parameter held when the function returned, "in the order of the
    /// parameters". Returns what the call's own result is read as, or
    /// `None` where `value` is no such call.
    ///
    /// A function with `var` parameters returns its result and then
    /// what each holds, so the call is bound to `_given`: `_given[0]`
    /// is the result, and `_given[1]` the first place's new value. A
    /// function that fails raises, and the target leaves each place as
    /// it was (see where the call is written).
    pub(super) fn place_call(
        &mut self,
        function: &Function,
        value: &Expr,
        depth: usize,
    ) -> Option<String> {
        let places = self.places_of(value);
        if places.is_empty() {
            return None;
        }
        // A function of the program, or the candidate of an operator
        // the call resolved to: the candidate says which of its
        // parameters is `var` (FN-15, GEN-6).
        let (module, declaration, arguments) = called(value)?;
        let (module, declaration) = (&module, &declaration);
        let Some(Detail::Callable(signature)) =
            self.program.modules[*module].details.get(*declaration)
        else {
            return None;
        };
        if let Some(result) = self.native_at(
            function,
            (*module, *declaration),
            signature,
            arguments,
            depth,
        ) {
            return Some(result);
        }
        // FN-15: the place is the one the call's arguments name when
        // they are evaluated. An index or a key of it is evaluated
        // once, there, and held: an earlier place's assignment may
        // change what it would come to, `change(i, items[i])`.
        //
        // EXPR-2: "Evaluate ordinary call/constructor arguments once in
        // written order". Holding an index evaluates it before the
        // call, so where one is held every argument is: each in the
        // order written, an index just before the argument it is of.
        // What an index is of is read before the index is evaluated,
        // as it is in any read: in `rows[key][index]` a `rows` with no
        // such key fails before `index` is evaluated.

        let is_place = |argument: &Argument| {
            argument
                .parameter
                .is_some_and(|parameter| places.contains(&parameter))
        };
        let holds = arguments
            .iter()
            .any(|argument| is_place(argument) && !indices_of(&argument.value).is_empty());
        if holds {
            let (mut indices, mut reads) = (0, 0);
            for (position, argument) in arguments.iter().enumerate() {
                // What this argument's indices are of, by where each is
                // written.
                let mut read_here = Vec::new();
                if is_place(argument) {
                    for (of, index) in indices_of(&argument.value) {
                        if let Some(of) = of {
                            reads += 1;
                            let held = format!("_in_{reads}");
                            // Read only to be read through: nothing of
                            // it is kept.
                            let written = self.expression(function, of);
                            self.line(depth, &format!("{held} = {written}"));
                            self.held.push((of.at, held));
                            read_here.push(of.at);
                        }
                        indices += 1;
                        let held = format!("_at_{indices}");
                        // A key that is a struct changed where it is is
                        // held as a copy: the call may change its source,
                        // and the place is the one named before it.
                        let written = self.expression(function, index);
                        let written = self.copied(function, index, written);
                        self.line(depth, &format!("{held} = {written}"));
                        self.held.push((index.at, held));
                    }
                }
                let held = format!("_arg_{}", position + 1);
                let written = if is_place(argument) {
                    self.given_place(function, &argument.value)
                } else {
                    self.expression(function, &argument.value)
                };
                self.line(depth, &format!("{held} = {written}"));
                // An index is held until the place is assigned. What it
                // is of is not: an earlier place's assignment may have
                // given the root another value, and the assignment
                // reads what is there then.
                self.held.retain(|(at, _)| !read_here.contains(at));
                self.held.push((argument.value.at, held));
            }
        }
        // FN-15: "A function that fails leaves the place holding what
        // its parameter held when it failed: no change it had made is
        // undone." A function of the program gives back what fails in
        // its result's place, beside what each `var` parameter held
        // then (see `value_function`): the places are assigned, and it
        // is raised again. A native function of core changes a list
        // where it is, and raises where it fails.
        let failure_given_back = self.program.modules[*module]
            .bodies
            .get(*declaration)
            .is_some_and(Option::is_some);
        self.place_call = true;
        let called = self.expression(function, value);
        self.place_call = false;
        self.line(depth, &format!("_given = {called}"));
        for (position, parameter) in places.iter().enumerate() {
            let Some(argument) = arguments
                .iter()
                .find(|argument| argument.parameter == Some(*parameter))
            else {
                continue;
            };
            // What the function gives back is its own: it has copied
            // what it kept (see `kept`).
            let given = format!("_given[{}]", position + 1);
            if let Some((target, assigned)) = self.assigned(function, &argument.value, given) {
                self.hoist(depth);
                self.line(depth, &format!("{target} = {assigned}"));
            }
        }
        if failure_given_back {
            self.natives = true;
            self.line(depth, "_core.raise_if_failed(_given[0])");
        }
        self.held.clear();
        Some("_given[0]".to_string())
    }

    /// A call of a native function of core that is given a place inside
    /// a value as its one `var` parameter, `append(book.orders, order)`
    /// (FN-15, LIST-1): the provider goes in to the place, and the
    /// function changes what is there where it is (`_core.given_at`).
    /// Returns what the call's result is read as, or `None` for any
    /// other call, which is given a value and assigns the place after.
    ///
    /// The place is the first argument written, so that the rest are
    /// evaluated after its way in has been read, as they are written
    /// (EXPR-2): they are given as they are asked for.
    fn native_at(
        &mut self,
        function: &Function,
        (module, declaration): (usize, usize),
        signature: &Signature,
        arguments: &[Argument],
        depth: usize,
    ) -> Option<String> {
        let program = self.program;
        let declared = &program.modules[module].declarations[declaration];
        if declared.kind != DeclarationKind::NativeFunction(Role::Value) {
            return None;
        }
        let native = self.native_function(module, format!("fn_{}", declared.name))?;
        let mut places = signature
            .parameters
            .iter()
            .enumerate()
            .filter(|(_, parameter)| parameter.var);
        let (place, None) = (places.next()?, places.next()) else {
            return None;
        };
        let (first, rest) = arguments.split_first()?;
        // Every parameter is given: a native function of core's that
        // takes a place has no default.
        if first.parameter != Some(place.0) || arguments.len() != signature.parameters.len() {
            return None;
        }
        let root = self.way_in(function, &first.value, 1)?;
        self.natives = true;
        let target = self.place(function, root);
        let steps = self.steps(function, &first.value, false);
        let mut given = Vec::new();
        for argument in rest {
            let parameter = signature.parameters.get(argument.parameter?)?;
            let value = self.expression(function, &argument.value);
            // The function puts what it is given into the place: a
            // copy, where something else holds it and the place may
            // hold it.
            let value = if may_hold(self.program, &first.value.ty, &argument.value.ty) {
                self.copied(function, &argument.value, value)
            } else {
                value
            };
            given.push(format!("{}={value}", name(&parameter.name)));
        }
        self.hoist(depth);
        self.line(
            depth,
            &format!(
                "_given = _core.given_at({target}, ({steps}), _core.{native}, {}, lambda: _py.dict({}))",
                string(&name(&place.1.name)),
                given.join(", ")
            ),
        );
        self.line(depth, &format!("{target} = _given[1]"));
        Some("_given[0]".to_string())
    }

    /// True where `base` is a list, a dictionary or a struct that is no
    /// value ([`by_value`]) that a `var`, a `state` or a `cache` of this
    /// function holds itself ([`owns`]): an element, an entry or a field
    /// of it is assigned where it is, and nothing of it is copied for
    /// the change. A `var` parameter is such a place: what it is given
    /// is the caller's own, which the function changes where it is
    /// (FN-15).
    fn changed_where_it_is(&self, function: &Function, base: &Expr) -> bool {
        let ExprKind::Local(local) = &base.kind else {
            return false;
        };
        let declared = &function.locals[*local];
        // In a graph an assignment into a `var` makes a new connection
        // (EXPR-15), and is written as one.
        function.mode != Mode::Graph
            && owns(self.program, function, *local)
            && (matches!(declared.ty, Type::List { .. } | Type::Dict { .. })
                || struct_in_place(self.program, &declared.ty))
    }

    /// What a `var` parameter is given for the place `value` (FN-15):
    /// what the place holds, as it is. The function changes a list, a
    /// dictionary or a struct that is no value where it is, and gives
    /// back what the place is to hold.
    pub(super) fn given_place(&mut self, function: &Function, value: &Expr) -> String {
        if let Some((_, held)) = self.held.iter().find(|(at, _)| *at == value.at) {
            return held.clone();
        }
        self.expression(function, value)
    }

    /// What a local is written as where it is assigned or read as it
    /// is: a field of the node's state for a `state`, its name
    /// otherwise.
    pub(super) fn place(&mut self, function: &Function, local: usize) -> String {
        let declared = &function.locals[local];
        match declared.kind {
            LocalKind::State | LocalKind::Cache => format!("_state.{}", name(&declared.name)),
            // TIME-7: "`inject out` exposes that same result for reads,
            // assignment".
            LocalKind::Inject if declared.name == "out" => {
                // "`start` neither reads nor writes it": the analyser
                // rejects that (`out.lifecycle`), and `start` is given
                // no output.
                if function.mode == Mode::Lifecycle {
                    self.unsupported(declared.at, "`out` in `start`");
                }
                "_output.value".to_string()
            }
            // TIME-8: a capability is the parameter hgraph gives it
            // as.
            LocalKind::Inject => match capability(&declared.name) {
                // A source's statements are resumed in one evaluation
                // after another, each with capabilities of its own:
                // the node puts them in its state for the statements
                // to read.
                Some((parameter, _)) if function.mode == Mode::Source => {
                    format!("_state.{parameter}")
                }
                Some((parameter, _)) => parameter.to_string(),
                None => {
                    self.unsupported(declared.at, "this injected capability");
                    "None".to_string()
                }
            },
            _ => name(&declared.name),
        }
    }

    /// What the assignment of `value`, written `written`, to `place`, a
    /// part of the node's `out`, writes: the port it assigns, and what it
    /// assigns the port.
    ///
    /// TIME-7: "`inject out` exposes that same result for reads,
    /// assignment, and admitted piecewise mutation." EXPR-3: "The
    /// place's root is ... in a `when`, `out`". TYPE-9 maps a struct's
    /// fields, a tuple's positions and a fixed list's positions to
    /// children of their own, each a port of hgraph's: `out.ask = 1`
    /// assigns the port of `ask`, `_output["ask"].value = 1`, which
    /// publishes that field alone, and `out` read after it holds it. A
    /// dictionary's entry is published as the change of that key,
    /// `{key: value}`. A part is assigned a whole value of its type
    /// (EXPR-3), which publishes whole, a struct's fields each. A place
    /// inside a part that
    /// publishes whole, a field of an `atomic<Quote>` or an element of
    /// an unsized list, gives that part a new value with the place
    /// changed, published whole.
    pub(super) fn assigned_to_out(
        &mut self,
        function: &Function,
        place: &Expr,
        value: &Expr,
        written: String,
    ) -> Option<(String, String)> {
        // The steps from `out` in to the place, the outermost first.
        let mut steps: Vec<&Expr> = place_steps(place).map(|(step, _)| step).collect();
        steps.reverse();
        let mut port = "_output".to_string();
        // The part the port holds.
        let mut part = place.place_root();
        for (taken, step) in steps.iter().enumerate() {
            let child = match &step.kind {
                ExprKind::Field { base, member } => field_of(self.program, &base.ty, *member)
                    .map(|field| string(&name(&field.name))),
                ExprKind::Index { base, index } => match &base.ty {
                    Type::Tuple(_) => Some(self.position_key(function, index)),
                    // A fixed list's position, by a constant inside it,
                    // or by an index the provider holds to it: "An index
                    // outside a list fails where it is evaluated".
                    Type::List {
                        size: Some(size), ..
                    } => {
                        let inside = constant_position(index).is_some_and(|at| {
                            matches!(size.value, ConstantValue::Int(count)
                                if i64::try_from(at).is_ok_and(|at| at < count))
                        });
                        let written = self.expression(function, index);
                        if inside {
                            Some(written)
                        } else {
                            self.natives = true;
                            port = format!("_core.child({port}, {written})");
                            part = step;
                            continue;
                        }
                    }
                    // A key of a dictionary: the change of that key.
                    Type::Dict { .. } if taken + 1 == steps.len() => {
                        let key = self.expression(function, index);
                        let published = self.published_to(&step.ty, value.at, written);
                        return Some((format!("{port}.value"), format!("{{{key}: {published}}}")));
                    }
                    Type::Dict { .. } => {
                        self.unsupported(place.at, "an assignment inside an entry of `out`");
                        return None;
                    }
                    _ => None,
                },
                _ => None,
            };
            // A field of an atomic struct is in a part that publishes
            // whole.
            let child = child.filter(|_| !matches!(part.ty, Type::Atomic(_)));
            let Some(child) = child else {
                break;
            };
            port = format!("{port}[{child}]");
            part = step;
        }
        if std::ptr::eq(part, place) {
            let published = self.published_to(&place.ty, value.at, written);
            return Some((format!("{port}.value"), published));
        }
        let changed = self.changed_within(function, place, part, written)?;
        let published = self.published_to(&part.ty, part.at, changed);
        Some((format!("{port}.value"), published))
    }

    /// The value `stop`, a part of `out` that publishes whole, holds once
    /// `place`, inside it, is given `value`: each value on the way given
    /// a new one, as `assigned_by_copy` gives a `var`'s, read from the
    /// port of `out` it is held in.
    fn changed_within(
        &mut self,
        function: &Function,
        place: &Expr,
        stop: &Expr,
        value: String,
    ) -> Option<String> {
        if std::ptr::eq(place, stop) {
            return Some(value);
        }
        let (base, changed) = match &place.kind {
            ExprKind::Field { base, member } => {
                // TYPE-9: an `atomic<V>` of a struct holds the whole
                // struct, whose fields are the struct's.
                let whole = payload_of(&base.ty);
                let Some(field) = field_of(self.program, whole, *member) else {
                    self.unsupported(place.at, "an assignment to this member");
                    return None;
                };
                let held = self.expression(function, base);
                (
                    base,
                    format!("_core.with_field({held}, {}={value})", name(&field.name)),
                )
            }
            ExprKind::Index { base, index } => {
                let with = match element_read(&base.ty) {
                    Some("item") => "with_item",
                    Some(_) => "with_entry",
                    None => {
                        self.unsupported(place.at, "an assignment to this element");
                        return None;
                    }
                };
                let held = self.expression(function, base);
                let index = self.expression(function, index);
                (base, format!("_core.{with}({held}, {index}, {value})"))
            }
            _ => {
                self.unsupported(place.at, "an assignment to this place");
                return None;
            }
        };
        self.natives = true;
        self.changed_within(function, base, stop, changed)
    }

    /// What the assignment of `value` to `place` gives the place's
    /// root: the root as it is assigned, and its new value.
    ///
    /// EXPR-3: "An assignment may name a place inside a value, at any
    /// depth: a field, `point.x = 1` ... The place's root is a `var`, a
    /// `state`, a `cache`, or, in a `when`, `out`." A value that took
    /// the old one is as it was (TYPE-5: "later mutation of a source
    /// cannot change an earlier retained value"). The list, the
    /// dictionary and the struct that is no value ([`by_value`]) that a
    /// place holds are the place's own, and are changed where they are:
    /// what took one of them took a copy (`kept`). Any other value on
    /// the way is given a new one: `point.x = 1` makes `point` the
    /// `Point` whose `x` is 1, and `pair[0] = 1` a new tuple.
    pub(super) fn assigned(
        &mut self,
        function: &Function,
        place: &Expr,
        value: String,
    ) -> Option<(String, String)> {
        // A place more than one step inside the value a `var`, a
        // `state` or a `cache` holds, with a value changed where it is
        // on the way: the provider goes in, and changes each such value
        // where it is. The value is given as it is asked for, after the
        // way in has been read, as it is evaluated in any assignment.
        if let Some(root) = self
            .way_in(function, place, 2)
            .filter(|_| changed_on_the_way(self.program, place))
        {
            self.natives = true;
            let target = self.place(function, root);
            let steps = self.steps(function, place, true);
            return Some((
                target.clone(),
                format!("_core.set_at({target}, ({steps}), lambda: {value})"),
            ));
        }
        self.assigned_by_copy(function, place, value)
    }

    /// The local a place is rooted in, where the provider may go in to
    /// the place and change what is there where it is (`_core.set_at`,
    /// `_core.given_at`); `None` for any other place.
    ///
    /// The local is a `var`, a `var` parameter, a `state` or a `cache`
    /// of this function that holds a value changed where it is
    /// ([`owns`]). The way in has at least `least` steps, and a list, a
    /// dictionary or a struct that is no value ([`by_value`]) on it or
    /// at its end: a way through values alone to a scalar changes
    /// nothing where it is. The provider is given every index
    /// before it reads the first step, so an index that is an
    /// expression is taken only where nothing that may fail is read
    /// before it, the element of a list or the entry of a dictionary:
    /// `groups[i % 2]`, and not `rows[key][f()]`, whose `f()` is
    /// evaluated only once `rows[key]` has been read (EXPR-7).
    fn way_in(&self, function: &Function, place: &Expr, least: usize) -> Option<usize> {
        // A place that is itself a list or a dictionary is one a
        // function may change where it is; so is a value on the way
        // that is changed where it is.
        let mut changed = matches!(place.ty, Type::List { .. } | Type::Dict { .. });
        let mut steps = 0;
        for (step, base) in place_steps(place) {
            match &step.kind {
                ExprKind::Field { member, .. } => {
                    field_of(self.program, &base.ty, *member)?;
                    changed |= struct_in_place(self.program, &base.ty);
                }
                ExprKind::Index { index, .. } => {
                    if !self.simple_index(index) && goes_through_a_collection(base) {
                        return None;
                    }
                    element_read(&base.ty)?;
                    changed |= matches!(base.ty, Type::List { .. } | Type::Dict { .. });
                }
                _ => unreachable!("a place's steps are fields and indices"),
            }
            steps += 1;
        }
        let ExprKind::Local(local) = place.place_root().kind else {
            return None;
        };
        let root = function.mode != Mode::Graph && owns(self.program, function, local);
        (root && changed && steps >= least).then_some(local)
    }

    /// The key `index`, written `written`, as a dictionary keeps it where
    /// it has none: a copy, where something else holds it and may change
    /// it, a `Key` struct of a `var` say. One held before the statement
    /// is a copy already (see `place_call`).
    fn kept_key(&mut self, function: &Function, index: &Expr, written: String) -> String {
        if self.held.iter().any(|(held, _)| *held == index.at) {
            written
        } else {
            self.copied(function, index, written)
        }
    }

    /// True for an index that is a literal, a name or a member of an
    /// enum, or one a call has evaluated already and holds: reading it
    /// cannot fail.
    pub(super) fn simple_index(&self, index: &Expr) -> bool {
        matches!(
            index.kind,
            ExprKind::Literal(_) | ExprKind::Local(_) | ExprKind::EnumMember { .. }
        ) || self.held.iter().any(|(held, _)| *held == index.at)
    }

    /// Writes the lines a place's way in asks for before its statement
    /// (`steps`).
    pub(super) fn hoist(&mut self, depth: usize) {
        for line in std::mem::take(&mut self.hoisted) {
            self.line(depth, &line);
        }
    }

    /// The way in to `place` from the local it is rooted in, as the
    /// provider is given it: each step its kind and what it names, a
    /// struct's field `("f", "bids")`, a dictionary's key `("k", price)`
    /// and a list's index or a tuple's position `("i", 0)`.
    ///
    /// Where `assigned`, the place is assigned, and a dictionary whose
    /// entry it is keeps its key where it has none: a copy, where
    /// something else holds the key and may change it (`copied`).
    fn steps(&mut self, function: &Function, place: &Expr, assigned: bool) -> String {
        let mut written = Vec::new();
        for (at, base) in place_steps(place) {
            let step = match &at.kind {
                ExprKind::Field { member, .. } => {
                    let field = field_of(self.program, &base.ty, *member)
                        .map_or_else(String::new, |field| name(&field.name));
                    format!("(\"f\", {})", string(&field))
                }
                ExprKind::Index { index, .. } => {
                    let kind = if matches!(base.ty, Type::Dict { .. }) {
                        "k"
                    } else {
                        "i"
                    };
                    let mut key = self.expression(function, index);
                    if !self.simple_index(index) {
                        // Evaluated once, before the statement.
                        let held = format!("_key_{}", self.hoisted.len() + 1);
                        self.hoisted.push(format!("{held} = {key}"));
                        key = held;
                    }
                    if assigned && kind == "k" && written.is_empty() {
                        key = self.kept_key(function, index, key);
                    }
                    format!("(\"{kind}\", {key})")
                }
                _ => unreachable!("the loop is over fields and indices"),
            };
            written.push(step);
        }
        // From the root inward, with the comma a tuple of one needs.
        written.reverse();
        written
            .iter()
            .map(|step| format!("{step},"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The copying form of an assignment to a place (`assigned`): each
    /// value on the way is given a new one, but for the list, the
    /// dictionary and the struct that is no value that a `var` of this
    /// function holds itself, which is changed where it is.
    fn assigned_by_copy(
        &mut self,
        function: &Function,
        place: &Expr,
        value: String,
    ) -> Option<(String, String)> {
        match &place.kind {
            ExprKind::Local(local) => Some((self.place(function, *local), value)),
            ExprKind::Field { base, member } => {
                let Some(field) = field_of(self.program, &base.ty, *member) else {
                    self.unsupported(place.at, "an assignment to this member");
                    return None;
                };
                self.natives = true;
                // The struct is read to make the new one of: nothing
                // of it is kept that it does not hold already. A struct
                // that is no value, which a `var`, a `state` or a
                // `cache` holds itself, is changed where it is.
                let held = self.expression(function, base);
                let field = name(&field.name);
                let value = if self.changed_where_it_is(function, base) {
                    format!("_core.set_field({held}, {}, {value})", string(&field))
                } else {
                    format!("_core.with_field({held}, {field}={value})")
                };
                self.assigned_by_copy(function, base, value)
            }
            // "of a list's element type, under an `i64` index; of a
            // dictionary's value type, under a key of its key type; or
            // of a tuple's position, under a constant index. A key the
            // dictionary has not is added. An index outside a list
            // fails where it is evaluated."
            ExprKind::Index { base, index } => {
                let with = match element_read(&base.ty) {
                    Some("item") => "with_item",
                    Some(_) => "with_entry",
                    None => {
                        self.unsupported(place.at, "an assignment to this element");
                        return None;
                    }
                };
                self.natives = true;
                let held = self.expression(function, base);
                let written = self.expression(function, index);
                let index = if matches!(base.ty, Type::Dict { .. }) {
                    self.kept_key(function, index, written)
                } else {
                    written
                };
                // The collection a `var`, a `state` or a `cache` holds
                // itself is changed where it is; one inside another
                // value is given a new one.
                let with = match with {
                    _ if !self.changed_where_it_is(function, base) => with,
                    "with_item" => "set_item",
                    _ => "set_entry",
                };
                let value = format!("_core.{with}({held}, {index}, {value})");
                self.assigned_by_copy(function, base, value)
            }
            _ => {
                self.unsupported(place.at, "an assignment to this place");
                None
            }
        }
    }
}

/// Each step of the way in to `place`, from the outside in: the step,
/// a field `base.member` or an index `base[index]`, with its `base`.
/// `rows[0].orders` gives `rows[0].orders` with `rows[0]`, and then
/// `rows[0]` with `rows`. A place that is a name has no step.
fn place_steps(place: &Expr) -> impl Iterator<Item = (&Expr, &Expr)> {
    // `successors` gives the first step, and then each next one from
    // the one before it, until there is none.
    std::iter::successors(step_of(place), |&(_, base)| step_of(base))
}

/// `at` with what it is a step of, where it is a field or an index.
fn step_of(at: &Expr) -> Option<(&Expr, &Expr)> {
    match &at.kind {
        ExprKind::Field { base, .. } | ExprKind::Index { base, .. } => Some((at, base)),
        _ => None,
    }
}

/// Whether reading `place` reads an element of a list or an entry of
/// a dictionary, which may fail.
fn reads_an_element(place: &Expr) -> bool {
    place_steps(place).any(|(step, _)| matches!(step.kind, ExprKind::Index { .. }))
}

/// The indices of a place that are evaluated, from its root outward as
/// they are written. Each is with what it is an index of, where reading
/// that may fail.
pub(super) fn indices_of(place: &Expr) -> Vec<(Option<&Expr>, &Expr)> {
    let mut indices = Vec::new();
    for (step, base) in place_steps(place) {
        if let ExprKind::Index { index, .. } = &step.kind
            && !matches!(index.kind, ExprKind::Literal(_))
        {
            let of = reads_an_element(base).then_some(base);
            indices.push((of, &**index));
        }
    }
    indices.reverse();
    indices
}

/// True where the way to `place` from its root goes through a value
/// that is changed where it is: an element of a list or an entry of a
/// dictionary is read, `rows[key][0]`, `book.bids[price]`, or a field of
/// a struct that is no value ([`by_value`]), `line.end.x` where `line`
/// is a `Line` of three fields. Not `point.x`, nor `pair[0].x` of a
/// tuple of `Point`s, two values.
fn changed_on_the_way(program: &Program, place: &Expr) -> bool {
    place_steps(place).any(|(step, base)| match &step.kind {
        ExprKind::Index { .. } => matches!(base.ty, Type::List { .. } | Type::Dict { .. }),
        _ => struct_in_place(program, &base.ty),
    })
}

/// True where the way to `place` from its root reads an element of a
/// list or an entry of a dictionary: `rows[key][0]`, `book.bids[price]`,
/// and not `line.end.x`.
fn goes_through_a_collection(place: &Expr) -> bool {
    place_steps(place).any(|(step, base)| {
        matches!(step.kind, ExprKind::Index { .. })
            && matches!(base.ty, Type::List { .. } | Type::Dict { .. })
    })
}

/// True for a local that holds a value of its own that is changed where
/// it is: a `var`, a `var` parameter, a `state` or a `cache` of such a
/// type ([`changed_in_place`]), which is no connection.
///
/// LIST-3: "a value taken from the place before the call is as it
/// was". Python's list, dictionary and struct are each one object that
/// every name of it sees change. So what such a local holds is changed
/// where it is, and is the local's alone: what is kept of it elsewhere,
/// and a name that outlives a change of it, is a copy
/// (`Emitter::kept`).
fn owns(program: &Program, function: &Function, local: usize) -> bool {
    let declared = &function.locals[local];
    matches!(
        declared.kind,
        LocalKind::Var | LocalKind::State | LocalKind::Cache
    ) && changed_in_place(program, &declared.ty, 0)
        && declared.category != ValueCategory::Connection
}
