//! Calls as the hgraph target writes them: a function's or a
//! candidate's call with its arguments and defaults, a native function
//! of the provider, a value function lifted over connections (FN-6), and
//! an eval or a run of the harness. Part of the transpiler, the last
//! stage: `emit` writes a module with them.

use super::*;

impl<'a> Emitter<'a> {
    /// `python`, the name of a function of the provider of core's
    /// natives, where what it is for is declared in core, the module
    /// `module`, and the provider defines it (NATIVE-7). `None` for
    /// any other.
    pub(super) fn native_function(&self, module: usize, python: String) -> Option<String> {
        (Some(module) == self.program.core() && provides(&python)).then_some(python)
    }

    /// The parameter, by its position, of a wake's duration that is
    /// checked as the program runs: a call of core's `schedule` (CAP-4,
    /// CAP-5) given a duration. TIME-8: "A wake asked for with a
    /// `duration` takes one more than zero, but in `start`, which takes
    /// zero too ... A duration that is not admitted, a negative one in
    /// `start` included, rejects where the compiler sees its value, a
    /// literal or a constant (UNIT-13), and elsewhere fails the call as
    /// the program runs."
    ///
    /// In a hook a literal that is admitted there is given as it is; any
    /// other duration is given through the provider's `wake_delay`, which
    /// fails the call where it is not admitted. A literal that is not
    /// admitted is one an instance gives a generic value parameter, which
    /// the analyser did not see. A value function is given the
    /// capability by a parameter, and whether a zero there is "in
    /// `start`" the rule does not say: a duration there that may be zero,
    /// anything but a literal more than zero or a constant that is one,
    /// is reported.
    fn wake_duration(
        &mut self,
        function: &Function,
        callee: (usize, usize),
        signature: &Signature,
        arguments: &[Argument],
    ) -> Option<usize> {
        let program = self.program;
        if Some(callee.0) != program.core()
            || program.modules[callee.0].declarations[callee.1].name != "schedule"
        {
            return None;
        }
        let (index, given) = arguments.iter().find_map(|argument| {
            let index = argument.parameter?;
            let parameter = signature.parameters.get(index)?;
            (parameter.ty == Type::Scalar(Scalar::Duration)).then_some((index, &argument.value))
        })?;
        // A literal, or a constant followed to what it is written as: the
        // analyser has rejected a constant that depends on itself
        // (`constant.cycle`), so the chain ends.
        let mut value = given;
        while let ExprKind::Constant {
            module,
            declaration,
        } = value.kind
            && let Some(Some(constant)) = program.modules[module].constants.get(declaration)
        {
            value = constant;
        }
        let literal = match value.kind {
            ExprKind::Literal(Value::Duration(micros)) => Some(micros),
            _ => None,
        };
        match function.mode {
            Mode::Node | Mode::Stop | Mode::Lifecycle => {
                let starting = function.mode == Mode::Lifecycle;
                let admitted = matches!(given.kind, ExprKind::Literal(Value::Duration(micros))
                    if micros > 0 || (micros == 0 && starting));
                (!admitted).then_some(index)
            }
            _ => {
                if !literal.is_some_and(|micros| micros > 0) {
                    self.unsupported(
                        given.at,
                        "a wake asked for with a duration that may be zero in a value function \
                         (TIME-8)",
                    );
                }
                None
            }
        }
    }

    /// The arguments of a call of `callee`, a module and a declaration
    /// of it: each by its parameter's name, so that the order they are
    /// written in does not matter to Python.
    ///
    /// UNIT-8: a parameter with a default may be left out. The call is
    /// then written with the default in its place, read from the
    /// analysed program: a default names no parameter and no local
    /// (UNIT-7), so it means the same wherever it is written, and a
    /// function it calls is named through its own module.
    pub(super) fn arguments(
        &mut self,
        function: &Function,
        callee: (usize, usize),
        generics: &[GenericArgument],
        signature: &Signature,
        arguments: &[Argument],
    ) -> String {
        let checked_delay = self.wake_duration(function, callee, signature, arguments);
        let kept = self.kept_configuration(callee, generics, signature);
        // Whether the callee is a node: a node keeps what a graph
        // gives it that is no connection, so such an argument is
        // copied below.
        let node = self.is_node(callee);
        // The owner's rule: an argument is a reference to what it
        // names, and is copied in two cases alone.
        //
        // "the value is passed to a function and the function also
        // recieves a var reference to the item or items it contains":
        // an argument read from a place that the same call gives to a
        // `var` parameter, itself or a part of it or what holds it.
        //
        // And one a native function keeps: a native function with a
        // `var` parameter puts what it is given into that place, as
        // `append` does its item, and what is kept is copied where
        // something else holds it. A function of the program copies
        // what it keeps itself, where it keeps it.
        let places: Vec<usize> = arguments
            .iter()
            .filter(|argument| {
                argument
                    .parameter
                    .and_then(|index| signature.parameters.get(index))
                    .is_some_and(|parameter| parameter.var)
            })
            .filter_map(|argument| copies::place_of(function.locals, &argument.value))
            .collect();
        let native = self.program.modules[callee.0]
            .bodies
            .get(callee.1)
            .is_none_or(Option::is_none);
        // The types of the places such a function is given: what it
        // keeps of another argument, it keeps in one of them.
        let holders: Vec<&Type> = arguments
            .iter()
            .filter(|argument| {
                argument
                    .parameter
                    .and_then(|index| signature.parameters.get(index))
                    .is_some_and(|parameter| native && parameter.var)
            })
            .map(|argument| &argument.value.ty)
            .collect();
        let mut given_places: Vec<usize> = Vec::new();
        let mut written: Vec<String> = arguments
            .iter()
            .filter_map(|argument| {
                let index = argument.parameter?;
                let parameter = signature.parameters.get(index)?;
                let changeable = !self.connection(function, &argument.value)
                    && changed_in_place(self.program, &argument.value.ty, 0);
                let value = if parameter.var {
                    // FN-15: the place itself. A second `var`
                    // parameter given the same place takes a copy:
                    // each has a value of its own, and the place is
                    // given them "in the order of the parameters".
                    let written = self.given_place(function, &argument.value);
                    let place = copies::place_of(function.locals, &argument.value);
                    let again = place.is_some_and(|place| given_places.contains(&place));
                    given_places.extend(place);
                    if again && changeable {
                        self.natives = true;
                        format!("_core.copied({written})")
                    } else {
                        written
                    }
                } else {
                    let written = self.expression(function, &argument.value);
                    let written = self.narrowed(function, &parameter.ty, &argument.value, written);
                    let read_from =
                        function
                            .copies
                            .places(self.program, function.locals, &argument.value);
                    let shared = read_from.iter().any(|place| places.contains(place));
                    if changeable && shared {
                        self.natives = true;
                        format!("_core.copied({written})")
                    } else if changeable
                        && ((node && function.mode == Mode::Graph)
                            || holders
                                .iter()
                                .any(|holder| may_hold(self.program, holder, &argument.value.ty)))
                    {
                        // A node keeps what a graph gives it that is no
                        // connection, its configuration or a constant
                        // hgraph wires for an input: hgraph holds it
                        // while the graph goes on changing what it was
                        // read from.
                        self.copied(function, &argument.value, written)
                    } else {
                        written
                    }
                };
                // TIME-8: a wake's duration known only as the program
                // runs, checked then (`wake_duration`).
                let value = if checked_delay == Some(index) {
                    self.natives = true;
                    let starting = if function.mode == Mode::Lifecycle {
                        "True"
                    } else {
                        "False"
                    };
                    format!("_core.wake_delay({value}, starting={starting})")
                } else {
                    value
                };
                let value = self.held_value(kept[index], value);
                Some(format!("{}={value}", name(&parameter.name)))
            })
            .collect();
        for (index, parameter) in signature.parameters.iter().enumerate() {
            let given = arguments
                .iter()
                .any(|argument| argument.parameter == Some(index));
            if given || parameter.pack.is_some() {
                continue;
            }
            if let Some(value) = self.left_out_of_call(callee, generics, index, parameter) {
                let value = self.held_value(kept[index], value);
                written.push(format!("{}={value}", name(&parameter.name)));
            }
        }
        written.join(", ")
    }

    /// What a call of the function `callee`, with `generics`, binds
    /// parameter `index` to when it leaves it out: every call of a
    /// function by name, captured or not, asks this. UNIT-8: a temporal
    /// input written `T?` with no default may be left out, "an input
    /// that is then never valid" (`never_valid`); any other parameter
    /// takes what `left_out` gives it.
    pub(super) fn left_out_of_call(
        &mut self,
        callee: (usize, usize),
        generics: &[GenericArgument],
        index: usize,
        parameter: &Parameter,
    ) -> Option<String> {
        let temporal = self.program.modules[callee.0].declarations[callee.1]
            .kind
            .role()
            == Some(Role::Temporal);
        if !temporal
            || parameter.constant
            || parameter.has_default
            || !matches!(parameter.ty, Type::Nullable(_))
        {
            return self.left_out(callee, index, parameter);
        }
        let ty = self
            .instance_signature(callee, generics)
            .and_then(|signature| signature.parameters.get(index))
            .map_or(&parameter.ty, |parameter| &parameter.ty);
        Some(self.never_valid(ty, parameter.at))
    }

    /// What a temporal input of `ty` left out is bound to (UNIT-8: "an
    /// omitted temporal input is never valid"): hgraph's `nothing`, an
    /// output of the input's type that never ticks. A `signal` is bound
    /// to one of a `bool`, as it may be bound to any output.
    fn never_valid(&mut self, ty: &Type, at: Location) -> String {
        let annotation = match ty.omission_removed() {
            Type::Signal => Some("_hg.TS[_py.bool]".to_string()),
            _ => self.annotation(ty, true, at),
        };
        let Some(annotation) = annotation else {
            return "None".to_string();
        };
        self.hgraph = true;
        format!("_hg.nothing(tp={annotation})")
    }

    /// The signature of the instance a call of `callee`, a module and a
    /// declaration of it, names with the generic arguments `generics`,
    /// where `callee` is written by instance (GEN-2). `None` where it is
    /// not, or where no instance is found for them.
    fn instance_signature(
        &self,
        callee: (usize, usize),
        generics: &[GenericArgument],
    ) -> Option<&'a Signature> {
        let instances = self.instances;
        instances
            .instanced(self.program, callee.0, callee.1)
            .then(|| instances.find(self.program, callee, generics))
            .flatten()
            .map(|instance| &instance.signature)
    }

    /// `written`, configuration a node is given, inside `_core.Held(...)`
    /// where `kept`: an object hgraph keeps as it is (`held`,
    /// `kept_configuration`). Otherwise `written` as it is.
    pub(super) fn held_value(&mut self, kept: bool, written: String) -> String {
        if !kept {
            return written;
        }
        self.natives = true;
        format!("_core.Held({written})")
    }

    /// For each parameter of `callee`, a module and a declaration of it,
    /// whether a call gives it inside `_core.Held(...)`.
    ///
    /// A node keeps its configuration: a list, a tuple or a dictionary
    /// is given to it inside an object hgraph keeps as it is. What a
    /// parameter is of is read from the instance the call names, where
    /// the node is written by instance: a parameter declared a `T` is a
    /// list where `T` is one (GEN-2), and the node written for it takes
    /// the list out.
    pub(super) fn kept_configuration(
        &mut self,
        callee: (usize, usize),
        generics: &[GenericArgument],
        signature: &Signature,
    ) -> Vec<bool> {
        let node = self.is_node(callee);
        let taken = self
            .instance_signature(callee, generics)
            .unwrap_or(signature);
        let kept: Vec<bool> = signature
            .parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| {
                let ty = taken
                    .parameters
                    .get(index)
                    .map_or(&parameter.ty, |parameter| &parameter.ty);
                node && parameter.constant && held(ty)
            })
            .collect();
        if kept.contains(&true) {
            self.natives = true;
        }
        kept
    }

    /// True when `callee`, a module and a declaration of it, is written
    /// as an hgraph node that a call names: a temporal function with a
    /// runtime body that is no source. A source's name is the graph
    /// that wires its node.
    pub(super) fn is_node(&self, (module, declaration): (usize, usize)) -> bool {
        let declared_in = &self.program.modules[module];
        declared_in.declarations[declaration].kind.role() == Some(Role::Temporal)
            && declared_in.phases.get(declaration).copied().flatten() == Some(Phase::Node)
            && declared_in
                .bodies
                .get(declaration)
                .and_then(Option::as_ref)
                .is_some_and(|body| !yields(&body.statements))
    }

    /// What parameter `index` of the function `callee` is given when a
    /// call leaves it out: its default, or `None` for one written `T?`
    /// with none. Nothing for a parameter that cannot be left out.
    fn left_out(
        &mut self,
        callee: (usize, usize),
        index: usize,
        parameter: &Parameter,
    ) -> Option<String> {
        let program = self.program;
        let defaults = program.modules[callee.0]
            .defaults
            .get(callee.1)
            .map_or(&[][..], Vec::as_slice);
        if let Some(Some(default)) = defaults.get(index) {
            // A default that calls its own function with that same
            // argument left out would be written inside itself
            // without end.
            let key = (callee.0, callee.1, index);
            if self.expanding.contains(&key) {
                self.unsupported(default.at, "a default that leaves itself out");
                return Some("None".to_string());
            }
            // A default is read with no local in scope.
            let cold = Function::cold(Mode::Value);
            self.expanding.push(key);
            let value = self.expression(&cold, default);
            self.expanding.pop();
            Some(value)
        } else if matches!(parameter.ty, Type::Nullable(_)) {
            // UNIT-8: "A parameter whose type is written `T?` needs
            // no default to be omitted: it is taken to have a
            // default of `null`".
            Some("None".to_string())
        } else {
            None
        }
    }

    /// What the call `called` of a native candidate gives, as a value
    /// of the program's. `result` is the candidate's result type.
    ///
    /// A candidate that gives an enum, as core's `compare_` gives a
    /// `CmpResult` (OPS-3), gives its member's name: the provider
    /// declares no enum of the program's. The member is read from the
    /// enum by that name, as `Side("buy")` is (TYPE-7).
    pub(super) fn native_result(&mut self, result: Option<&Type>, called: String) -> String {
        match result {
            Some(Type::Named {
                module,
                declaration,
                kind: NamedKind::Enum,
                ..
            }) => {
                let of = self.type_name(*module, *declaration);
                format!("_core.member({of}, {called})")
            }
            _ => called,
        }
    }

    /// The generic values a call gives a native candidate, each as
    /// `name=value` for the provider's function.
    ///
    /// NUM-9: "`zero` is a generic value parameter of the candidates
    /// that declare it, and a call gives it a compile-time constant."
    /// A value of an enum is passed as the name of its member,
    /// `"ieee"`, and an integer as itself. A generic type is not
    /// passed: a Python function is not typed.
    fn options(
        &mut self,
        signature: &Signature,
        generics: &[GenericArgument],
        at: Location,
    ) -> Vec<String> {
        let mut written = Vec::new();
        for (parameter, argument) in signature.generics.iter().zip(generics) {
            let GenericArgument::Constant(constant) = argument else {
                continue;
            };
            // The analyser says what the constant comes to: a member
            // of an enum is one however it was written (TYPE-7), and
            // none of its spelling is read here.
            let value = match &constant.value {
                ConstantValue::Member(member) => Some(string(member)),
                ConstantValue::Int(number) => Some(number.to_string()),
                _ => None,
            };
            match value {
                Some(value) => written.push(format!("{}={value}", name(&parameter.name))),
                // A value that is not a constant here, such as a
                // generic value of the function the call is in.
                None => self.unsupported(at, &format!("the option `{}`", constant.text)),
            }
        }
        written
    }

    /// Gives a lifted node a name and writes it before the piece being
    /// written. `before` is the node's text up to its name and `after`
    /// the text from there on; `of` is what it lifts, for the name. A
    /// node lifted the same way twice in a piece is written once.
    ///
    /// The text is in two parts, and the name is not put in for a mark
    /// in it: a mark could as well be in a string a program wrote.
    pub(super) fn lifted_node(&mut self, of: &str, before: String, after: String) -> String {
        let text = (before, after);
        if let Some((_, known)) = self.lifts.iter().find(|(written, _)| *written == text) {
            return known.clone();
        }
        // The name starts with `_`, as no name of a program's does.
        let python = format!("_lift_{}_{of}", self.lifted);
        self.lifted += 1;
        self.hgraph = true;
        self.prelude.push_str("\n\n");
        self.prelude.push_str(&text.0);
        self.prelude.push_str(&python);
        self.prelude.push_str(&text.1);
        self.lifts.push((text, python.clone()));
        python
    }

    /// FN-6: "A selected value helper, a `const fn` or a value candidate
    /// of an operator, with temporal arguments in composition is
    /// lifted: temporal arguments become inputs; scalar arguments and
    /// omitted defaults bind as configuration. ... Any input
    /// notification activates the node ... Each admitted evaluation
    /// publishes its result, including an equal result. An outputless
    /// helper lifts to a sink."
    ///
    /// Writes the node that calls the function `target`, or the
    /// provider's function when `lifts` says it is a native candidate
    /// of core, and gives the node's name. `bound` says how each
    /// parameter of `signature` is given, with `None` for one left
    /// out; `result` is what the function gives.
    ///
    /// FN-11: "A lifted argument with a declared default does not
    /// require a valid input: bind the default when that input is
    /// invalid. Otherwise, an argument whose value contract admits null
    /// does not require a valid input: bind null when it is invalid.
    /// Every other input must satisfy ordinary `valid(input)`."
    pub(super) fn lifted(
        &mut self,
        target: (usize, usize),
        lifts: Lifts,
        signature: &Signature,
        bound: &[Option<Bound>],
        result: Option<&Type>,
        at: Location,
    ) -> String {
        // The node's parameters, the inputs that must be valid, how
        // many inputs there are, and the arguments of the call it
        // makes.
        let mut parameters = Vec::new();
        let mut required = Vec::new();
        let mut inputs = 0;
        let mut passed = Vec::new();
        // What the node tests of a required input that holds a list,
        // whose `valid` is not hgraph's (`valid_test`).
        let mut admitted = Vec::new();
        for (index, parameter) in signature.parameters.iter().enumerate() {
            let python = name(&parameter.name);
            match bound.get(index).and_then(Option::as_ref) {
                Some(Bound::Input(ty)) => {
                    inputs += 1;
                    let annotation = self.annotation_or_object(ty, true, at);
                    parameters.push(format!("{python}: {annotation}"));
                    // What the input holds, a struct's value made
                    // from its bundle's fields (TYPE-9).
                    let read = self.port_value(ty, &python, at);
                    // FN-12: "An unsized `list<T>` needs
                    // `valid(list_value)` ... A fixed-size list is valid
                    // as any list is (TIME-9)": `valid` as the language
                    // has it (`valid_test`).
                    let precise = self.precise_valid(ty, &python);
                    match self.left_out(target, index, parameter) {
                        Some(fallback) => {
                            let valid = precise.unwrap_or_else(|| format!("{python}.valid"));
                            passed.push(format!("{python}=({read} if {valid} else {fallback})"));
                        }
                        None => {
                            passed.push(format!("{python}={read}"));
                            admitted.extend(precise);
                            required.push(python);
                        }
                    }
                }
                Some(Bound::Configuration(ty)) => {
                    let annotation = self.annotation_or_object(ty, false, at);
                    parameters.push(format!("{python}: {annotation}"));
                    // Configuration the node keeps comes inside an
                    // object hgraph keeps as it is.
                    if held(ty) {
                        passed.push(format!("{python}={python}.value"));
                    } else {
                        passed.push(format!("{python}={python}"));
                    }
                }
                None => {
                    if let Some(value) = self.left_out(target, index, parameter) {
                        passed.push(format!("{python}={value}"));
                    }
                }
            }
        }
        let (called, of) = match lifts {
            Lifts::Native(native, configured) => {
                self.natives = true;
                passed.extend(configured.iter().cloned());
                (format!("_core.{native}"), native.to_string())
            }
            Lifts::Function(generics) => {
                let declared = &self.program.modules[target.0].declarations[target.1];
                (self.called_name(target, generics, at), name(&declared.name))
            }
        };
        let returns = result.and_then(|result| self.annotation(result, true, at));
        // hgraph requires every input to be valid unless it is told
        // which ones.
        let mut options = Vec::new();
        if required.len() != inputs {
            options.push(format!("valid={}", tuple(&required)));
        }
        let options = if options.is_empty() {
            String::new()
        } else {
            format!("({})", options.join(", "))
        };
        let call = format!("{called}({})", passed.join(", "));
        let call = match lifts {
            Lifts::Native(..) => self.native_result(signature.result.as_ref(), call),
            Lifts::Function(_) => call,
        };
        // An unsized list in the result ticks through the node's own
        // port of its output where it is published empty (`bundle_of`).
        let output = match (result, &returns) {
            (Some(result), Some(returns)) if holds_growing_list(self.program, result) => {
                parameters.push(format!("_output: {} = None", output_annotation(returns)));
                Some("_output")
            }
            _ => None,
        };
        let call = match result {
            Some(result) => self.published_to(result, at, call, output),
            None => call,
        };
        // A required input that holds a list and is not valid as the
        // language has it publishes nothing.
        let admission = if admitted.is_empty() {
            String::new()
        } else {
            format!("    if not ({}):\n        return\n", admitted.join(" and "))
        };
        let (decorator, after) = match (result, returns) {
            (Some(_), returns) => (
                "compute_node",
                format!(
                    "({}) -> {}:\n{admission}    return {call}\n",
                    parameters.join(", "),
                    returns.unwrap_or_else(|| "_py.object".to_string()),
                ),
            ),
            (None, _) => (
                "sink_node",
                format!("({}):\n{admission}    {call}\n", parameters.join(", ")),
            ),
        };
        self.lifted_node(&of, format!("@_hg.{decorator}{options}\ndef "), after)
    }

    /// The arguments a call gives, each by its parameter's name, and
    /// how each parameter of `signature` is then bound in a lifted
    /// node: as an input where `input` gives the argument a type, as
    /// configuration otherwise.
    pub(super) fn bound(
        &mut self,
        function: &Function,
        signature: &Signature,
        arguments: &[Argument],
        input: impl Fn(&Self, &Expr) -> Option<Type>,
    ) -> (Vec<String>, Vec<Option<Bound>>) {
        let mut written = Vec::new();
        let mut bound: Vec<Option<Bound>> = vec![None; signature.parameters.len()];
        for argument in arguments {
            let parameter = argument
                .parameter
                .and_then(|index| Some((index, signature.parameters.get(index)?)));
            let Some((index, parameter)) = parameter.filter(|(_, p)| p.pack.is_none()) else {
                // FN-8: a pack takes the arguments the fixed parameters
                // leave, which are not written yet.
                self.unsupported(argument.value.at, "an argument of a pack in a lifted call");
                continue;
            };
            // FN-12: "A window is passed only where it is `all_valid`",
            // as the `list<V>` it holds (TYPE-11): not written yet.
            if input(self, &argument.value)
                .is_some_and(|ty| matches!(ty.omission_removed(), Type::RollingWindow { .. }))
            {
                self.unsupported(
                    argument.value.at,
                    "a window given to a lifted helper (FN-12)",
                );
                continue;
            }
            let taken = match input(self, &argument.value) {
                // TIME-10: "Ordinary followers observe the selected
                // target": a lifted function given a `ref` result takes
                // an ordinary input of its type (FN-16), which hgraph
                // binds to what it designates. A `?` that marks omission
                // is no part of it, as the analyser admitted it
                // (`Type::omission_removed`).
                Some(ty) => Bound::Input(ty.omission_removed().clone()),
                None => Bound::Configuration(argument.value.ty.clone()),
            };
            let value = self.expression(function, &argument.value);
            // A lifted node keeps its configuration, as any node does:
            // a copy, where the graph that wires it holds it too. An
            // eval of a test runs before the test goes on.
            let value = match &taken {
                Bound::Configuration(_) if function.mode == Mode::Graph => {
                    self.copied(function, &argument.value, value)
                }
                _ => value,
            };
            let value = match &taken {
                Bound::Configuration(ty) => self.held_value(held(ty), value),
                _ => value,
            };
            bound[index] = Some(taken);
            written.push(format!("{}={value}", name(&parameter.name)));
        }
        (written, bound)
    }

    /// A call: of a function of the program, or of a native candidate
    /// of core, which the target's own provider has a function for.
    pub(super) fn call(
        &mut self,
        function: &Function,
        callee: &Callee,
        arguments: &[Argument],
        whole: &Expr,
    ) -> String {
        let at = whole.at;
        let program = self.program;
        // True for the call that is the whole of a statement, and not
        // for what its arguments call.
        let whole_statement = std::mem::take(&mut self.place_call);
        let (module, declaration, generics, candidate) = match callee {
            Callee::Function {
                module,
                declaration,
                generics,
                ..
            } => (*module, *declaration, generics, false),
            Callee::Candidate {
                module,
                declaration,
                generics,
                ..
            } => (*module, *declaration, generics, true),
            // GEN-16: in an instance this is the call of the candidate
            // that answers the requirement. It is left where nothing
            // answers, and in a value function with generic parameters,
            // which is written once and not by instance.
            Callee::Required { .. } => {
                self.unsupported(
                    at,
                    "an operation a generic declaration requires, with no candidate selected for it",
                );
                return "None".to_string();
            }
            Callee::Unread => {
                self.unsupported(at, "this call");
                return "None".to_string();
            }
        };
        let Some(Detail::Callable(signature)) = program.modules[module].details.get(declaration)
        else {
            self.unsupported(at, "this call");
            return "None".to_string();
        };
        let declared = &program.modules[module].declarations[declaration];
        // FN-15: a call that assigns places is written with the
        // assignments after it, which only a statement has room for.
        if !whole_statement && signature.parameters.iter().any(|parameter| parameter.var) {
            self.unsupported(at, "a call that assigns a place, inside an expression");
            return "None".to_string();
        }
        // A test is a composition block (TST-3), and a temporal function
        // called in one makes a connection, which Python has no graph
        // to make in: a target is run through `eval`.
        if function.mode == Mode::Test && declared.kind.role() == Some(Role::Temporal) {
            self.unsupported(
                at,
                "a call of a temporal function in a test, outside an eval",
            );
            return "None".to_string();
        }
        // A candidate with a body of its own is a function of its
        // module, a value one, or a node or a graph, a temporal one.
        // One with none is native, and a native candidate of core is a
        // function of the target's provider.
        let bodied = matches!(declared.kind, DeclarationKind::Implementation(..));
        let native = if candidate && !bodied {
            let types: Vec<String> = signature
                .parameters
                .iter()
                .map(|parameter| mangled(&parameter.ty))
                .collect();
            let mut native = format!("{}_{}", declared.name, types.join("_"));
            let mut shown = format!("{}({})", declared.name, types.join(", "));
            // OPS-4: "`convert<T>(value)` converts a value to the type
            // T": core's candidates of `convert` take the same type and
            // give different ones, so the provider's function is named
            // with what it gives too, `convert_i64_to_f64`.
            if let DeclarationKind::NativeCandidate(_, Some(operator)) = declared.kind
                && told_apart_by_result(program, operator)
                && let Some(given) = given(signature)
            {
                native = format!("{native}_to_{}", mangled(given));
                shown = format!("{shown} -> {given}");
            }
            let Some(native) = self.native_function(module, native) else {
                self.unsupported(at, &format!("the candidate `{shown}`"));
                return "None".to_string();
            };
            Some(native)
        } else {
            None
        };
        // NATIVE-7: a native function of core is the provider's too,
        // `_core.fn_log`. It binds its own name (NATIVE-3), which the
        // provider's own helpers are kept apart from by the `fn_`.
        if let DeclarationKind::NativeFunction(Role::Value) = declared.kind {
            let native = self.native_function(module, format!("fn_{}", declared.name));
            let lifted = function.mode == Mode::Graph
                && arguments
                    .iter()
                    .any(|argument| self.connects(function, &argument.value));
            let (Some(native), false) = (native, lifted) else {
                self.unsupported(at, &format!("the native function `{}`", declared.name));
                return "None".to_string();
            };
            self.natives = true;
            let arguments = self.arguments(
                function,
                (module, declaration),
                generics,
                signature,
                arguments,
            );
            let called = format!("_core.{native}({arguments})");
            return self.native_result(signature.result.as_ref(), called);
        }
        // What a native candidate is configured with: its provider's
        // function takes each generic value by its name.
        let options = match native {
            Some(_) => self.options(signature, generics, at),
            None => Vec::new(),
        };
        // FN-6: in a graph a symbol or a value function given a
        // connection is lifted over it. "All-scalar calls execute
        // directly and invent no source timing": one given none is
        // called as it is, where the graph is built.
        // GEN-5: "a value candidate it selects for temporal arguments
        // is lifted (FN-6)". A temporal candidate is the node or the
        // graph itself.
        let lifted = function.mode == Mode::Graph
            && declared.kind.role() != Some(Role::Temporal)
            && (candidate || declared.kind.role() == Some(Role::Value))
            && arguments
                .iter()
                .any(|argument| self.connects(function, &argument.value));
        if lifted {
            let (written, bound) = self.bound(function, signature, arguments, |emitter, value| {
                emitter.connects(function, value).then(|| value.ty.clone())
            });
            let result = signature.result.as_ref().map(|_| &whole.ty);
            let lifts = match &native {
                Some(native) => Lifts::Native(native, &options),
                None => Lifts::Function(generics),
            };
            let node = self.lifted((module, declaration), lifts, signature, &bound, result, at);
            return format!("{node}({})", written.join(", "));
        }
        if let Some(native) = native {
            // NUM-2: an `i64` result outside the signed 64-bit range is
            // an error, which Python's own `+` would not make it: the
            // provider's function does.
            self.natives = true;
            let arguments = self.arguments(
                function,
                (module, declaration),
                generics,
                signature,
                arguments,
            );
            let arguments: Vec<String> = [arguments]
                .into_iter()
                .filter(|written| !written.is_empty())
                .chain(options)
                .collect();
            let called = format!("_core.{native}({})", arguments.join(", "));
            return self.native_result(signature.result.as_ref(), called);
        }
        if !bodied && !matches!(declared.kind, DeclarationKind::Function(_)) {
            self.unsupported(at, &format!("a call of `{}`", declared.name));
            return "None".to_string();
        }
        let python = self.called_name((module, declaration), generics, at);
        let arguments = self.arguments(
            function,
            (module, declaration),
            generics,
            signature,
            arguments,
        );
        format!("{python}({arguments})")
    }

    /// `eval(target, ...)`: the harness's `evaluate`, given the target,
    /// what its run is wired with ([`Emitter::run_wired`]), and each
    /// argument by its parameter's name.
    ///
    /// The harness does the rest, as core has an eval: each dense trace
    /// becomes a replay list (CEVAL-2), which a replay publishes
    /// (CEVAL-3); a record keeps each publication of the result
    /// (CEVAL-4) and puts them in the run's store (CRUN-7) under the
    /// run's key (CRUN-8); and the eval's value is that recording
    /// written dense, of a run of its own (CEVAL-5).
    ///
    /// RUN-1, core's CRUN-2: a run's configuration is given last, as
    /// `_config`, a name no parameter has, so that it is evaluated
    /// after the call's arguments, as EXPR-13 has them. The harness
    /// reads its `start`, `end` and `length`.
    ///
    /// TST-17: "An integration test ... beside `eval` it writes
    /// `run(call, config)` (RUN-3), whose value is the run's output as
    /// an eval's is." `real_time` is true for that form, and the
    /// harness is told so with `_real_time=True`.
    pub(super) fn eval(
        &mut self,
        function: &Function,
        call: &Expr,
        ty: &Type,
        config: Option<&Expr>,
        real_time: bool,
    ) -> String {
        let ExprKind::Call {
            callee:
                Callee::Function {
                    module,
                    declaration,
                    generics,
                    ..
                },
            arguments,
            ..
        } = &call.kind
        else {
            self.unsupported(call.at, "this eval");
            return "None".to_string();
        };
        let Some(Detail::Callable(signature)) =
            self.program.modules[*module].details.get(*declaration)
        else {
            self.unsupported(call.at, "this eval");
            return "None".to_string();
        };
        self.harness = true;
        // FN-4: where a name has no temporal definition, eval takes the
        // value one, and TST-8: "A selected value helper uses ordinary
        // lifting". A trace is an input of the lifted node, and any
        // other argument its configuration.
        let declared = &self.program.modules[*module].declarations[*declaration];
        if declared.kind.role() == Some(Role::Value) {
            // GEN-2: a function written by instance is lifted as its
            // instance for the eval's generic arguments, whose result
            // names no generic parameter.
            let signature = self
                .instance_signature((*module, *declaration), generics)
                .unwrap_or(signature);
            // An input is of its parameter's type where the function has
            // no generic parameter, as the run's replay is
            // (`run_wiring`), and of the type its trace is of otherwise.
            let own = signature.generics.is_empty();
            let (written, bound) = self.bound(function, signature, arguments, |_, value| {
                let payload = crate::instances::trace_payload(&value.ty)?;
                let parameter = arguments
                    .iter()
                    .find(|argument| std::ptr::eq(&argument.value, value))
                    .and_then(|argument| signature.parameters.get(argument.parameter?));
                Some(match parameter {
                    Some(parameter) if own => parameter.ty.clone(),
                    _ => payload,
                })
            });
            let node = self.lifted(
                (*module, *declaration),
                Lifts::Function(generics),
                signature,
                &bound,
                signature.result.as_ref(),
                call.at,
            );
            let run = self.run_wired(call, ty);
            let mut given = vec![node, run];
            given.extend(written);
            return self.evaluated(function, given, call, config, real_time);
        }
        let run = self.run_wired(call, ty);
        let target = self.called_name((*module, *declaration), generics, call.at);
        let arguments = self.arguments(
            function,
            (*module, *declaration),
            generics,
            signature,
            arguments,
        );
        let mut given = vec![target, run];
        if !arguments.is_empty() {
            given.push(arguments);
        }
        self.evaluated(function, given, call, config, real_time)
    }

    /// The harness's run of an eval of `call`, given `given`: what it
    /// runs, the run's wiring and the arguments. After them come the
    /// configuration, where the eval has one, `_real_time=True` for a
    /// `run`, and how the harness reads what the run gives (`eval`).
    fn evaluated(
        &mut self,
        function: &Function,
        mut given: Vec<String>,
        call: &Expr,
        config: Option<&Expr>,
        real_time: bool,
    ) -> String {
        if let Some(config) = config {
            given.push(format!("_config={}", self.expression(function, config)));
        }
        if real_time {
            given.push("_real_time=True".to_string());
        }
        given.extend(self.held_reader(&call.ty, call.at));
        format!("_harness.evaluate({})", given.join(", "))
    }

    /// True where this target reads a temporal value of `ty` whole from
    /// its port (`port_value`): one that holds no window, which no core
    /// operation reads (TYPE-12), and no abstract struct, whose value is
    /// of a struct below it, nor a generic one whose arguments have no
    /// text of their own.
    pub(super) fn whole_readable(&self, ty: &Type) -> bool {
        match ty {
            Type::RollingWindow { .. } => false,
            Type::Nullable(inner) => self.whole_readable(inner),
            Type::List { element, .. } => self.whole_readable(element),
            Type::Dict { value, .. } => self.whole_readable(value),
            Type::Tuple(elements) => elements.iter().all(|element| self.whole_readable(element)),
            Type::Named {
                module,
                declaration,
                arguments,
                kind: NamedKind::Struct,
                ..
            } => {
                !inherited(self.program, *module, *declaration)
                    && arguments
                        .iter()
                        .all(|argument| canonical_argument(self.program, argument).is_some())
                    && self
                        .program
                        .field_types(ty)
                        .unwrap_or_default()
                        .iter()
                        .all(|field| self.whole_readable(field))
            }
            _ => true,
        }
    }

    /// True where each part of a whole value of `ty`, an element, a
    /// field, an entry or a position, publishes whole: a scalar, an enum
    /// or a nullable one, and for a tuple's position an atomic one too,
    /// which hgraph holds as one value (TYPE-9: "`atomic<V>` stops
    /// expansion at a complete ordinary V"). A list's element is not
    /// nullable: hgraph's whole tuple of a list holds no null (see
    /// `bundle_of`).
    pub(super) fn parts_publish_whole(&self, ty: &Type) -> bool {
        match ty {
            Type::List { element, .. } => whole_scalar(element),
            Type::Set(_) => true,
            Type::Dict { value, .. } => publishes_whole(value),
            Type::Tuple(positions) => positions
                .iter()
                .all(|position| publishes_whole(position) || matches!(position, Type::Atomic(_))),
            Type::Named {
                module,
                declaration,
                kind: NamedKind::Struct,
                ..
            } => match self.program.modules[*module].details.get(*declaration) {
                Some(Detail::Struct(read)) => read
                    .members
                    .iter()
                    .all(|member| publishes_whole(&member.ty)),
                _ => false,
            },
            _ => false,
        }
    }

    /// What the harness reads, for a result of `ty`, as the value the
    /// result holds once a cycle has published: TST-5 has "An
    /// expectation's cell written as a `delta<T>` ... compared with the
    /// delta its cycle published, and one written as a T with the value
    /// the result holds once its cycle has published". `_hold=` a
    /// function of the result's port, for a type whose delta is another
    /// type: a list, a set, a dictionary, a struct and a tuple. `None`
    /// for one whose delta is the value itself.
    fn held_reader(&mut self, ty: &Type, at: Location) -> Option<String> {
        // A value this target does not read whole, an abstract struct's
        // or one that holds a window, has no reader: an eval of it that
        // compares deltas alone is written, and a cell written whole of
        // it is reported where it is written (`trace`).
        if !self.whole_readable(ty) {
            return None;
        }
        if !matches!(
            ty,
            Type::List { .. }
                | Type::Set(_)
                | Type::Dict { .. }
                | Type::Tuple(_)
                | Type::Named {
                    kind: NamedKind::Struct,
                    ..
                }
        ) {
            return None;
        }
        // A struct, a tuple or a fixed list is read whole as a part is
        // (`part_value`): each required part where it is valid, an
        // optional one null where it is not, and no whole value, the
        // failure `value.unset_read`, where a required part is not valid.
        let read = self.part_value(ty, "_port", at);
        Some(format!("_hold=lambda _port: {read}"))
    }

    /// What the harness wires the run of the eval that takes `call`
    /// with; `ty` is the eval's own type. Core's CEVAL-1: "core's
    /// `replay` wired to each temporal input of the target, and the
    /// target's result as the entry's", which the run records (CRUN-8).
    ///
    /// Core has `replay` and `record` as HGL, each for any type, so
    /// each is written for the type it is of here (GEN-2): `replay`
    /// for each input the eval gives a trace, by the input's name,
    /// `{"value": _inst_replay__i64}`, and then `record` for the
    /// target's result, or `None` where it has none.
    fn run_wired(&mut self, call: &Expr, ty: &Type) -> String {
        let at = call.at;
        let program = self.program;
        let Some(wiring) = crate::instances::run_wiring(program, call, ty) else {
            self.unsupported(at, "this eval");
            return "{}, None".to_string();
        };
        let instance =
            |emitter: &mut Self, function: &str, of: &Type| match crate::instances::core_function(
                program, function,
            ) {
                Some(found) => emitter.called_name(found, &[GenericArgument::Type(of.clone())], at),
                None => {
                    emitter.unsupported(at, &format!("an eval, with no `{function}` in core"));
                    "None".to_string()
                }
            };
        let inputs: Vec<String> = wiring
            .inputs
            .iter()
            .map(|(input, of)| {
                let replay = instance(self, "replay", of);
                format!("{}: {replay}", string(&name(input)))
            })
            .collect();
        let result = match &wiring.result {
            Some(of) => instance(self, "record", of),
            None => "None".to_string(),
        };
        format!("{{{}}}, {result}", inputs.join(", "))
    }
}

/// How a parameter of a lifted function is given (FN-6: "temporal
/// arguments become inputs; scalar arguments and omitted defaults bind
/// as configuration").
#[derive(Clone)]
pub(super) enum Bound {
    /// By a connection: an input of the lifted node, of this type.
    Input(Type),
    /// By a value known where the graph is built, of this type.
    Configuration(Type),
}

/// What a lifted node calls.
#[derive(Clone, Copy)]
pub(super) enum Lifts<'a> {
    /// A function of the program, or a candidate with a body, with
    /// what the call gives its generic parameters: one written by
    /// instance is called by its instance for those.
    Function(&'a [GenericArgument]),
    /// A native candidate of core: the provider's function for it, and
    /// what the call configures it with, each as `name=value`.
    Native(&'a str, &'a [String]),
}

/// What a candidate gives, for the name of its provider's function: its
/// result, or the T of a `result<T, error>`. A candidate that can fail
/// is declared once for each value of its policy (NUM-9, OPS-8), and
/// the provider has one function for the two, which takes the policy.
pub(super) fn given(signature: &Signature) -> Option<&Type> {
    match signature.result.as_ref()? {
        Type::Result { value, .. } => Some(value),
        other => Some(other),
    }
}

/// True when core's native candidates of `operator` are told apart by
/// what they give: two of them take the same types and give different
/// ones, as `convert(value: i64) -> f64` and `convert(value: i64) ->
/// bool` do (OPS-4). The provider's function of each such candidate is
/// named with what it gives.
fn told_apart_by_result(program: &Program, operator: OperatorRef) -> bool {
    let Some(core) = program.core() else {
        return false;
    };
    let module = &program.modules[core];
    // What each candidate read so far takes and gives.
    let mut read: Vec<(Vec<&Type>, Option<&Type>)> = Vec::new();
    for (position, declared) in module.declarations.iter().enumerate() {
        if declared.kind != DeclarationKind::NativeCandidate(Role::Value, Some(operator))
            && declared.kind != DeclarationKind::NativeCandidate(Role::Temporal, Some(operator))
        {
            continue;
        }
        let Some(Detail::Callable(signature)) = module.details.get(position) else {
            continue;
        };
        let takes: Vec<&Type> = signature
            .parameters
            .iter()
            .map(|parameter| &parameter.ty)
            .collect();
        let gives = given(signature);
        if read
            .iter()
            .any(|(other, result)| *other == takes && *result != gives)
        {
            return true;
        }
        read.push((takes, gives));
    }
    false
}

/// What a call calls, and its arguments: a function of the program, or
/// the candidate of an operator the call resolved to. `None` for any
/// other expression, and for a call of what is not yet known.
pub(super) fn called(value: &Expr) -> Option<(usize, usize, &[Argument])> {
    let ExprKind::Call {
        callee, arguments, ..
    } = &value.kind
    else {
        return None;
    };
    let (module, declaration) = callee.declaration()?;
    Some((module, declaration, arguments))
}
