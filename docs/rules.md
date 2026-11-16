# Rules and stages

Each rule of the pinned specification and core, the stage that implements
it, and whether that stage does so yet. A rule shared between stages has one
row for each. `crates/hgl_cli/tests/rule_table.rs` checks this file against
the pinned documents: every rule has a row, no row names an unknown rule,
and a `done` row's rule is cited in its stage's code.

| State | Meaning |
| --- | --- |
| `done` | The stage implements its part of the rule and a test cites the rule |
| `partial` | The stage implements some of its part, and a test cites the rule; the row says what is left |
| `planned` | The rule is expected to land in this stage; the row moves if building the stage shows otherwise |
| `unscheduled` | No stage in the [planned order](architecture.md#planned-order) takes it yet |

`done` records what this compiler implements and tests. It is not a
conformance claim. A proposed rule is listed like an agreed one; the
specification says which is which.

## Stages

| Stage | Crates | Its part |
| --- | --- | --- |
| lexer | `hgl_source`, `hgl_lexer` | Source text to tokens and literal values |
| parser | `hgl_parser` | Tokens to a syntax tree |
| formatter | planned | Syntax tree to source text |
| analyser | `hgl_analyser` | Names, types, phases and constants: every source rejection the parser cannot make |
| optimiser | planned | Rewrites that keep what a program is observed to do |
| transpiler | planned | The generated program and the runtime it runs on. A rule here may need a change to the runtime, not to generated code |
| runner | `hgl_runner` | `hgl test`: selects cases, matches `# expect-error`, runs eval |
| driver | `hgl_cli` | The `hgl` command: source roots, target, entry and run configuration |
| diagnostics | `hgl_diagnostics` | The diagnostic model every stage reports through |
| docs | none | Documentation extraction |

Not listed: `TEST-1` to `TEST-4` and `WRITE-1` to `WRITE-12` of the
specification. They say who owns what and how the documents are written;
no compiler stage implements them.

## Grammar

Productions carry no rule ID. `crates/hgl_parser/tests/grammar.rs` compares
the pinned chapters' productions with the copy the parser was written
against, so raising the pin shows which ones moved.

| Productions | Stage | State |
| --- | --- | --- |
| Tokens: `source-text.md`, less `literal`, `NL` and `EOF` | lexer | done |
| `expect_error`, `gap`, `error_category`: a comment's form | runner | done |
| All others | parser | done |

The parser reads `value_type` and its variants as `type`. Which types are
value types is the analyser's to check (TYPE-12, TYPE-14).

## Source text

`source-text.md`

| Rule | Stage | State | What the stage does |
| --- | --- | --- | --- |
| LEX-1 | lexer | done | Identifier form; reserved words and the lone `_` are their own token kinds |
| LEX-2 | lexer | done | String tokens; unknown escape, unclosed string and unclosed comment rejected |
| LEX-3 | lexer | done | Line and block comments are tokens; `//` is a symbol |
| LEX-4 | parser | done | Braces delimit blocks; indentation ignored; `;` only in native hook markers |
| LEX-5 | parser | done | A line break is ignored in a list, in an incomplete statement, and before a token that cannot begin one; two or more always separate |
| LEX-6 | parser | done | Two statements on one line rejected; an inject list continues after a comma |
| LEX-7 | lexer | done | Minus is its own token; integer and float literals checked against i64 and f64 |
| LEX-7 | analyser | done | 2^63 is an integer only directly under a minus; an enum's numbers are held as `i64`, the least included |
| LEX-8 | lexer | done | `>` never joined to `>`; digits then letters are one duration candidate; exponent forms |
| LEX-9 | parser | done | Any word of identifier form, reserved words included, as member name and argument label; `const` as modifier or name by position |
| LEX-10 | parser | done | Contextual names take their meaning by position; `str(value)` is a conversion and `error(...)` a constructor |
| LEX-11 | lexer | done | Date, time and offset fields; civil and zoned forms |
| LEX-12 | lexer | done | Zone name source form |
| LEX-12 | transpiler | planned | Run provider checks zone catalogue membership and offset consistency |
| LEX-13 | lexer | done | Duration units, their order, the final fraction, whole microseconds |
| LEX-14 | lexer | done | `<=>` is one token |
| LEX-14 | parser | done | `<=>` continues a line as the other binary operators do |
| LEX-15 | lexer | done | LF, CRLF and CR are each one line break, in the line index and in every token |
| LEX-16 | lexer | done | UTF-8 check; leading U+FEFF dropped; whitespace; a character that begins no token rejected |
| LEX-17 | lexer | done | The longest symbol is taken; `>=` is one token even after generic arguments |
| LEX-17 | parser | done | `>=` is not split to close generic arguments |
| LEX-18 | lexer | done | A malformed numeric, duration or temporal run is one rejected token |
| LEX-19 | lexer | done | Offset hours 00 to 23, minutes 00 to 59 |
| LEX-20 | lexer | done | Float value nearest, ties to even; infinity rejected; underflow accepted |
| LEX-21 | lexer | done | Block and documentation comments are tokens; `/**/` is an empty block comment |
| LEX-21 | parser | done | A comment containing a line break counts as one line break |
| LEX-22 | lexer | done | One `_` between two digits |
| LEX-23 | lexer | done | Hexadecimal and binary integers |
| LEX-24 | lexer | done | `\u{...}` escapes |
| LEX-25 | lexer | done | Text blocks |
| LEX-26 | lexer | done | `\` and the line break after it are one continuation token; any other `\` is rejected |
| LEX-26 | parser | done | A continued line break is no line break |
| LEX-27 | parser | done | The opening bracket of a call, parameter list, index, generic list or cardinality directly follows what it applies to |

## Units and names

`units-and-names.md`

| Rule | Stage | State | What the stage does |
| --- | --- | --- | --- |
| UNIT-1 | parser | done | One `module` line, with an optional `part`, opens a file |
| UNIT-1 | analyser | done | Assembles a module's files: one scope, one unnamed file, unique part names |
| UNIT-2 | analyser | done | Imports precede declarations and bind in their file; they and `alias::name` reach public declarations only |
| UNIT-3 | analyser | partial | Private unless exported, a native function and a native type among them; operators public; `impl fn` reached through its operator; import cycles rejected. Left: export closure |
| UNIT-4 | analyser | done | Module-qualified nominal identity; an alias adds none |
| UNIT-5 | analyser | done | Duplicate functions of one role, parameters, parts and locals rejected |
| UNIT-6 | analyser | done | Forward reference to declarations; locals visible after declaration; test-only declarations kept out of production |
| UNIT-7 | analyser | partial | A default reads no parameter of its declaration and no field of its struct. Left: the closure an imported default keeps |
| UNIT-8 | analyser | partial | Any parameter but a pack may have a default; a pack's is rejected under FN-8. A parameter that may be omitted follows every one that may not, and a value function's call may omit one written `T?`. Left: defaults as required constants of the expected type; a temporal default as a constant of its type |
| UNIT-9 | analyser | planned | Inherited defaults compared; a conflict needs an override |
| UNIT-10 | analyser | partial | Every name in a signature, body, type and constraint resolved to its one binding. Left: capability receivers; activation forms; the prelude |
| UNIT-11 | analyser | partial | When the program has the module `core`, a name nothing nearer binds is its public declaration of that name. `hgl` supplies the pinned core's module to a program whose source roots hold none. Left: a name core does not declare yet is listed, not reported |
| UNIT-12 | analyser | partial | No declaration, import, local, parameter or generic parameter takes a name already in scope, a prelude name included for a declaration, an import alias and a local. Left: a prelude name core does not declare yet |

## Types and values

`types-and-values.md`

| Rule | Stage | State | What the stage does |
| --- | --- | --- | --- |
| TYPE-1 | analyser | partial | Scalar type identities. Left: capabilities |
| TYPE-1 | transpiler | planned | Scalar representations; bytes as unsigned octets |
| TYPE-2 | lexer | done | Literal dates keep years 1 to 9999; literal durations fit signed 64-bit microseconds |
| TYPE-2 | analyser | partial | Each temporal scalar a type of its own. Left: the domains of computed values |
| TYPE-2 | transpiler | planned | Temporal scalar representations; zone names kept distinct |
| TYPE-3 | analyser | partial | Nominal identity by declaration and complete generic arguments; a parent is an abstract struct, listed once. an abstract struct is not constructed in a value function. Left: constructions elsewhere |
| TYPE-4 | analyser | partial | A field's type, and so its optionality, agrees between a struct and its parents, and between parents; `name = value` overrides a parent's field; one member to a name; a default of null only on an optional field. Left: a default's value against its field's type |
| TYPE-5 | analyser | partial | A construction in a value function names each field it gives, in any order, and gives every field that has no default and is not optional; a positional argument is rejected. Left: constructions elsewhere |
| TYPE-5 | transpiler | planned | Arguments evaluated once in order; defaults only when omitted; retained results own their data |
| TYPE-6 | analyser | partial | No struct inherits from itself; a struct holds itself only through a field that is optional, `Node?`, `atomic<Node?>` or `atomic<Node>?`, or that holds it only as elements of a collection that may be empty; the field's default is `null` or `[]`, and the structs below keep a null one; the field passes each generic parameter on whole; an abstract-typed field holds each descendant; a collection is admitted inside an `atomic<...>` only. Left: a constant argument built from a parameter |
| TYPE-7 | parser | done | `.member` read as an expression where an operand is due |
| TYPE-7 | analyser | partial | Numbering from zero or the number written; numbers are integers no two members share; overflow rejected. in a value function a member is of its enum, and `.member` is given its enum by the type its place expects. Left: a `switch` label by its selector; temporal bodies |
| TYPE-7 | transpiler | planned | Enum string and numeric conversion |
| TYPE-8 | analyser | planned | Typing of `any` |
| TYPE-8 | transpiler | planned | A boxed value keeps its exact type; equality, capability failure, independent copies |
| TYPE-9 | analyser | partial | An atomic payload is an ordinary value type. Left: temporalization of composite types; `atomic` normalisation |
| TYPE-10 | analyser | partial | A size is a count that is not negative; `unbounded`; size in identity; tuples distinct from lists. Left: sizes computed from other constants; an unsized pattern accepting a fixed size |
| TYPE-11 | analyser | partial | Size kind and bounds of literal sizes and generic value parameters (`rolling.size_kind`, `rolling.size_bounds`); an omitted minimum is the maximum in identity. Left: sizes computed from other constants |
| TYPE-11 | transpiler | planned | Window publications under core's contract |
| TYPE-12 | parser | done | `endpoint<T>` is a type of one argument, written as `ref<T>` is |
| TYPE-12 | analyser | partial | `ref` and `signal` kept out of keys, payloads, window values, constant configuration and value functions; `signal` only as a temporal function's input; `endpoint<T>` only as the whole type of a native value function's parameter (`endpoint.position`). Left: what a body admits; a `signal` input's default other than `null` |
| TYPE-13 | analyser | partial | A result's payloads are ordinary value types. Left: typing of the envelopes |
| TYPE-14 | parser | done | `?` after a type |
| TYPE-14 | analyser | partial | What `?` is applied to is an ordinary value type throughout, but on the whole type of a temporal input or a struct field; a set's element and a dictionary's key take none. a literal `null` is no value of a type with no `?`, and is rejected where nothing gives it a type. Left: null from other expressions |
| TYPE-15 | parser | done | The suffix covers the whole preceding type; `?` in a type and `?` in an expression kept apart |
| TYPE-15 | analyser | partial | `list<f64?>` and `list<f64>?` are two types. Left: a present null is data, distinct from an invalid input |

## Expressions and bindings

`expressions-and-bindings.md`

| Rule | Stage | State | What the stage does |
| --- | --- | --- | --- |
| EXPR-1 | parser | done | Precedence, associativity, grouping; a tuple has a comma |
| EXPR-1 | analyser | planned | A chained comparison is typed as written, not as a conjunction |
| EXPR-2 | analyser | partial | In a value function, the arguments of a value function's call, positional before named, and of a struct's construction, all named; duplicate, unknown, missing, surplus and mistyped arguments rejected. A named operator call, and a symbol's configuration, may name an option its candidates declare. Left: what a pack holds; a generic function's parameter types; temporal bodies |
| EXPR-2 | transpiler | planned | Arguments evaluated once in written order |
| EXPR-3 | analyser | partial | A local has the type written or its first value's; only a `var` is assigned, with a value of its type; a `var` is read only once assigned, through `if`, `switch` and loops (CTRL-11). In a graph a local is a connection or an ordinary value by its first value. A `var` of a graph declared with no value takes the category of its first assignment. Left: an annotation checked against the category of its value |
| EXPR-4 | analyser | partial | In a graph each value is a connection or an ordinary value: a temporal parameter and the result of a temporal or lifted call are connections. A reassignment across the two is rejected (`binding.category`, with CTRL-14). Left: reassigning a graph `var` rewires |
| EXPR-5 | analyser | partial | A value fits the type its place expects; an integer literal is an `f64` where one is expected, and a value that has a type is not converted; `null`, a tuple and a sequence take the expected type. Left: const expression restrictions; temporal bodies |
| EXPR-6 | analyser | partial | The Boolean connectives and `!` take bools, in value functions. Left: a guard precedes the payload read it protects |
| EXPR-6 | transpiler | planned | Short-circuit evaluation; composition reads no payload |
| EXPR-7 | analyser | partial | A field of a struct value has its type, the value's generic arguments in place; a list's element by an `i64`, a tuple's by a constant within its length, where the constant is integer literals under a minus or joined by `+`, `-` and `*`. Left: a tuple's index that is any other constant; temporal projection; `ref` access; the payload of a nullable or a result |
| EXPR-8 | analyser | planned | `_` and timed cells only in test traces |
| EXPR-9 | parser | done | Brackets delimit a sequence whose entries are all values or all pairs |
| EXPR-9 | analyser | partial | A sequence of values is a list or a set by the type expected, and with none a list of its values' one type, nullable when a `null` is among them; entries of two types, and entries that give no type, rejected. Left: pairs; `_`; ordering and uniqueness |
| EXPR-10 | parser | done | A named-entry brace directly after an operator is its configuration; an empty one rejected |
| EXPR-10 | analyser | partial | In a value function, a symbol's configuration is bound as the named arguments of the call the symbol stands for: to the options its operator's candidates declare, a generic value parameter before a parameter; an unknown or a repeated entry, and one no candidate for the operands declares, rejected. Left: the Boolean connectives and `!`; temporal bodies |
| EXPR-11 | parser | done | `<=>` at the comparison tier |
| EXPR-11 | analyser | partial | In a value function `<=>` is core's `CmpResult`, through the candidates of `compare_`, and each other comparison a bool; an outcome where a bool is expected is rejected. Left: temporal bodies |
| EXPR-12 | analyser | partial | In a value function, an option of an operator's call, by name or as a symbol's configuration, that a candidate's `requires` reads selects the candidate, and so the result type; one that is a generic value parameter is given a compile-time constant. Left: temporal bodies |
| EXPR-13 | transpiler | planned | Evaluation order of a configured operator |
| EXPR-14 | analyser | planned | Configuration introduces no temporal input |
| EXPR-14 | transpiler | planned | Configuration bound at graph construction in composition, per invocation elsewhere |

## Functions and phases

`functions-and-phases.md`

| Rule | Stage | State | What the stage does |
| --- | --- | --- | --- |
| FN-1 | analyser | partial | A value function's parameters and result, and a `const` parameter, are ordinary value types; each signature recorded; a value function holds no `state`, `cache`, `inject`, `start`, `when` or `stop`. A parameter of a capability's type, and `endpoint<T>` in a native value function, are admitted (TIME-8, TYPE-12). In a hook, a source and a value function a `const` parameter is given a constant expression (`argument.constant`); in a graph, no connection (`connection.as_constant`). Left: a `const` parameter that only an operator marks; roles of `const fn` and a temporal `fn` in bodies |
| FN-2 | analyser | done | Each temporal function and temporal candidate is a graph or a node by the constructs its body holds, wherever it holds them. A node's body is its declarations and then its hooks: any other statement is rejected, but in a source, which has its own shape (SOURCE-1) |
| FN-3 | analyser | partial | A value function calls only value functions (with FN-4), and an operator through a value candidate (GEN-5). A node's hook is read as value code: a temporal parameter is a value of its declared type, and a temporal function called there is rejected (`call.role`). A graph's body calls any function: a temporal parameter and the result of a temporal or lifted call are connections, and a connection given to a `const` parameter is rejected (`connection.as_constant`). Left: what a composition body admits of a connection; what it calls; iteration and conditionals in composition |
| FN-4 | analyser | partial | A temporal and a value definition may share a name. a call in a value function takes the value definition, and a temporal one alone is rejected. A call in a graph takes the temporal definition where there is one. Left: selection in eval |
| FN-5 | parser | done | `const` is a modifier only where one is admitted |
| FN-5 | analyser | planned | `const(argument)` resolves as an ordinary call |
| FN-6 | analyser | partial | In a graph a value function or a value candidate called with a connection among its arguments gives a connection, and one called with ordinary values alone an ordinary value. Left: which arguments are inputs and which configuration, recorded for the lifted node; an outputless helper as a sink |
| FN-6 | transpiler | planned | Activation and publication of a lifted node |
| FN-7 | analyser | partial | A value function's `return`, arrow body and final expression are of its result type; a `return` with no value is rejected where a result is declared, and one with a value where none is; a function that declares a result returns on every path (`return.missing`). In a hook a `return` with no value is accepted where a result is declared, and one with a value is of the result's type. Left: a composition return; an anonymous function's context |
| FN-7 | transpiler | planned | A runtime return publishes and exits the current function evaluation, skipping later handlers; no write, no tick |
| FN-8 | parser | done | Pack parameter forms |
| FN-8 | analyser | done | Packs follow fixed parameters, have no defaults and are not const; a named pack names a type pack of its declaration |
| FN-9 | analyser | partial | An `inject` in a `const fn`, a value candidate, the contract of a native value function, or an anonymous function written in a node is rejected (`node.in_value_function`); a value function is given a capability by a parameter of its type (TIME-8). Left: an anonymous function written in a graph; a capability used outside a hook or a source's body |
| FN-10 | parser | done | Where an arrow body ends |
| FN-11 | analyser | planned | Which lifted arguments need a valid input |
| FN-11 | transpiler | planned | Default or null bound while the input is invalid |
| FN-12 | transpiler | planned | Aggregate lifting: one invocation with the whole value |
| FN-13 | transpiler | planned | Stable argument values; retained results independent of later changes |
| FN-14 | analyser | done | What an anonymous function may use from around it |

## Generics and operators

`generics-and-operators.md`

| Rule | Stage | State | What the stage does |
| --- | --- | --- | --- |
| GEN-1 | analyser | partial | Generic scope covers the whole declaration; a repeated type parameter is one type at a call of a value function. Left: equal constants; occurrence requirements; temporal bodies |
| GEN-2 | parser | done | A default on a generic value parameter |
| GEN-2 | analyser | partial | A generic type in a type position or constructor is given all its arguments; a type pack is no single type; a value function's generics resolved at a call from what the call writes, then the type its place expects, then its arguments in declared parameter order; an abstract struct a later argument has widens what an earlier argument decided. A generic value parameter takes its default where nothing else says what it is: a function's and a candidate's at a call, a struct's where it is named bare or constructed, an operator's in the output requested. Left: packs; solvable equalities; temporal bodies |
| GEN-3 | analyser | partial | A parameter compared in a constraint is a const option or is rejected. At a named operator call in a value function, a candidate's `T in {types}` and `T == type` are checked against what `T` stands for, and a named operator requirement is tested with the call's types (GEN-16). Left: every other form `requires` admits; binding equalities solved to a fixed point |
| GEN-4 | analyser | partial | An `impl fn` is a temporal candidate and an `impl const fn` a value candidate; each binds an operator its module declares, its file imports, or the prelude binds, and a value candidate's body is read as a value function's. An operator's requirement `O in {...}` on its bare result parameter limits its candidates' results. Left: operator identity in selection; every other requirement of an operator |
| GEN-5 | analyser | partial | In a value function, a call of an operator, by name or by the symbol that stands for it, selects among its value candidates: the requested output first, then the inputs; an integer literal selects as an `i64` before it is widened, which the candidates' types settle before their conditions, and candidates that tie in which literals they widen give way to the one that widens every one; no candidate and a tie rejected. A generic candidate is admitted with its parameters resolved from the call, and the most specific admitted one is selected (GEN-12). In a graph a call selects among temporal and value candidates, and a temporal candidate outranks a value one of the same parameter types. Left: a candidate with a pack; an operator with a pack, and so the Boolean connectives and `!`; an argument that is itself a call only its requested output selects; composition, where a temporal candidate outranks a value one of the same parameter types, and runtime bodies |
| GEN-6 | analyser | partial | In a value function a named call is matched against each value candidate's own signature. An option that selects is a generic value parameter, given by name or by its default, and the candidate's `requires` is checked against its value; one that does not select is a parameter, by name or position. An option's name is one kind across an operator's candidates. Left: a `requires` on a generic value parameter of the enclosing function; a candidate's shape against its operator, the names and types of the operator's parameters and its generic relationships; candidates that stay resolvable together; temporal candidates |
| GEN-7 | analyser | partial | In a body with generic parameters, a call of an operator, by name or by symbol, with an argument whose type names one of them is admitted where a requirement of the declaration names that operator with those types, each given by its position or by the operator's parameter name, and is of the type its arrow states; with none it selects among the candidates as any call does, so one that takes any type is still taken. Left: a requirement met by widening an argument; a type or size parameter used as a runtime value |
| GEN-8 | parser | done | `_` as an `instantiate` argument |
| GEN-8 | analyser | partial | A call's generic arguments, all or the first of them, for a value function and for the output requested of an operator's candidates; a call that leaves a parameter unresolved rejected; a function's or a candidate's generic value parameter given by name, held to the parameter's type, and one given no compile-time constant rejected. An operator's generics are resolved from what the call writes, then its place, then its arguments, then a default, and the output so requested is matched as far as it is known. Left: an operator's own generic value parameter given by name; `instantiate`; packs; what an operator's generic arguments say of its operands; temporal bodies |
| GEN-9 | analyser | partial | A property domain is an ordinary value type. Left: clauses applied to explicit concrete domains |
| GEN-9 | optimiser | planned | A recorded law licenses no reassociation across overflow or float effects |
| GEN-10 | parser | done | A cardinality after a pack parameter or a generic pack parameter |
| GEN-10 | analyser | partial | A cardinality bounds the arguments a generic type's pack takes, and has its upper bound at least its lower. Left: pack views; `each`; the arity of a call |
| GEN-10 | transpiler | planned | `schemas(pack)` as a runtime-only borrowed view |
| GEN-11 | analyser | planned | Reference removal in inference |
| GEN-12 | analyser | partial | Among the candidates a named call admits in a value function, each of the operator's parameters is compared with the other candidate's part for part: a concrete type or a structure over a bare variable in the same place; a candidate as specific everywhere and more in one place outranks. A candidate's generics are bound in declared parameter order. Left: a repeated variable counting once; reference qualification; temporal bodies |
| GEN-13 | analyser | partial | A plain parameter is a type or a value by its uses in the signature; uses that disagree are rejected, and a struct's parameter nothing uses; a value parameter or a pack is not named as a type; a generic value parameter's default, a callable's or a struct's, is of its declared type, as far as how the default is written says what it is, and of the type a call resolves where that names a type parameter. Left: a default that names something; the kind a constraint gives; `type(N)`; a call that supplies no type for a function's parameter its signature does not settle |
| GEN-14 | parser | done | `<` after a name read as generic arguments or comparison; marked undecided when both parse |
| GEN-14 | analyser | partial | Decides an undecided constructor from what its name resolves to, a generic type, function or operator; a generic type before a comparison's `<` rejected; each argument against its parameter's kind. Left: a generic function or operator before a comparison's `<` |
| GEN-15 | parser | done | Constraint precedence: negation, then conjunction, then disjunction |
| GEN-16 | parser | done | One form for constraint calls, with an optional result arrow |
| GEN-16 | analyser | partial | A constraint call that names an operator is a requirement, and names a value operation. Where a call selects a candidate it is tested with the candidate's generic parameters in place, as the call it names would select: one most-specific value candidate admits the types supplied and gives the arrow's type where one is stated, the operator's defaults taking part (GEN-8). Left: a predicate, and the arrow rejected on one; a requirement supplied with a value or a set; a requirement checked where it is declared |

## Temporal model

`temporal-model.md`

| Rule | Stage | State | What the stage does |
| --- | --- | --- | --- |
| TIME-1 | analyser | planned | A payload read needs admission |
| TIME-1 | transpiler | planned | Endpoint observations: validity, modification, retained data, delta |
| TIME-2 | analyser | planned | Composition reads no live payload |
| TIME-2 | transpiler | planned | Evaluation in dependency order, once per node per cycle |
| TIME-3 | analyser | partial | A `when` inside another block is rejected. A `when` whose header has no `valid` selector alone or as a top-level conjunct runs with every temporal input valid; one that has admits those its selectors name: `valid(a, b)` each input it names, `valid([])` none. `modified(a, b)` names the inputs it subscribes to, each by its name, and `modified([])` none: where the node injects neither `scheduler` nor `alarm`, or its `start` does not use the one it injects, the selector is warned of (`node.no_activation`) and the source accepted; any use in `start` counts, and `@suppress_warning("node.no_activation")` before the function acknowledges it. A sequence that names inputs is no selector's form (`endpoint.argument`). Left: subscription, which `modified` sets; a selector that takes no part in its header's condition |
| TIME-3 | transpiler | planned | Conditions checked in source order; a satisfied body runs, then checking continues unless it returns |
| TIME-4 | analyser | partial | In a `when`, a payload read of an input its header does not admit needs a `valid(input)` that holds where it stands: in the branch it holds for, in the right operand of the `&&` it guards, after a branch that returns (`payload.unguarded`). Left: aggregate subscriptions; node-wide validity; `all_valid` |
| TIME-5 | analyser | partial | A node's declarations precede its hooks and stand in its body itself; one `start`, one `stop`; a source is held to the same. A state's initialiser reads no temporal input (`state.input_read`). Left: a cache's initialiser |
| TIME-5 | transpiler | planned | Start follows initialisation; stop finalises |
| TIME-6 | transpiler | planned | State is recordable history; cache is rebuilt on every start |
| TIME-7 | analyser | partial | `inject out` needs a result, and is of the result's type. `out` is read in a `when` and in `stop` and written only in a `when`, and a `return` with a value stands in neither `start` nor `stop` (`out.lifecycle`). Left: piecewise mutation of `out` |
| TIME-7 | transpiler | planned | Return and `out` share one result; no write, no tick |
| TIME-8 | analyser | partial | `inject` names one of the listed capabilities and binds each but `out` to a local of the type core declares for it; `alarm` and `scheduler` in one node are rejected (`inject.alarm_and_scheduler`). A capability type written other than as the whole type of a parameter of a `const fn` or a native value function, and a capability that is the value of a `state` or `cache`, an entry of a tuple, list, set, dictionary or struct, returned, or published, is rejected (`capability.as_value`); a `let` and an argument of a parameter of its type are uses, one given to a parameter written as a generic parameter or `any` is rejected (hhenson/hgl_spec#73), and a property is read with its type (NATIVE-6). Left: one in a dictionary, whose literal is not read yet. A capture by an anonymous function is reported: of the injected name by the resolver (`name.captured`, FN-14), and of a `let` it is bound to or a parameter of its type here |
| TIME-9 | analyser | partial | In a node's hooks the endpoint operations are read as the forms they are, whether core declares them or nothing does yet: `valid`, `modified` and `scheduled` are bools, `delta_value` the input's type, `last_modified` a `datetime?`; each takes a temporal input of the node by its name, or `out` (`endpoint.argument`); a selector may stand with no argument or with several inputs, and `valid` and `modified` with `[]`. An `endpoint<T>` parameter of a native value function is given an input by its name, or `out`, in a hook, and its payload is not read; anything else, and any argument in a graph or a value function, is `endpoint.argument`. Core declares the operations but `delta_value` (hhenson/hgl_core#30), so each of those names is a prelude name, taken by no declaration (UNIT-12). Left: `delta_value` and `scheduled`, which nothing declares; `activate` and `passivate` on direct inputs; which place a selector stands in; an operator's candidate with an `endpoint<T>` parameter; that an endpoint is not retained |
| TIME-10 | transpiler | planned | A reference ticks on a changed designation |

## Control flow and sources

`control-flow-and-sources.md`

| Rule | Stage | State | What the stage does |
| --- | --- | --- | --- |
| CTRL-1 | analyser | done | In a graph an `if` is told apart by its condition: an ordinary `bool` composes one branch, a connection selects a child graph at runtime; in a value function and a hook it is ordinary control flow. |
| CTRL-2 | transpiler | planned | Only the selected child runs; a changed selector restarts it |
| CTRL-3 | analyser | done | A temporal `if` with no `else` whose value is used is a connection of `T`, not a `T?` (with CTRL-13) |
| CTRL-3 | transpiler | planned | The never-ticking branch |
| CTRL-4 | analyser | partial | A branch's locals do not escape. A `var` assigned under a temporal condition has a binding before the `if`, or every branch assigns it (with CTRL-14). Left: several results kept as separate connections |
| CTRL-5 | analyser | partial | In a value function, a path that has returned takes no part in what is assigned or in the type of an `if`. In a graph a `return` with a value in a branch selected at runtime needs the continuation to return (CTRL-14). Left: runtime bodies |
| CTRL-6 | analyser | partial | In a value function, a label is of the selector's type; a repeated literal or enum member is rejected (`switch.duplicate_label`); an enum's coverage stands for a `default`. Left: a label that is no constant; temporal bodies |
| CTRL-6 | transpiler | planned | No fallthrough; an unmatched selector uses default or fails |
| CTRL-7 | analyser | done | What a `for` in a graph receives: ordinary values over ordinary data, a connection of `T` over a connection of `list<T, N>`, and over a connection of `list<T>`, `set<T>` or `dict<K, V>` one child for each member, with the set's member and the dictionary's key ordinary values (with CTRL-15) |
| CTRL-7 | transpiler | planned | One child per member; update keeps it, removal stops it |
| CTRL-8 | analyser | partial | In a loop that makes one child for each member, an assignment to a `var` declared outside it and a `return` are rejected (`loop.carried`). Left: the traversal forms `elements`, `keys`, `values` and `items`; no implicit crossing of a reference |
| CTRL-8 | transpiler | planned | Traversal over current values, lists in index order |
| CTRL-9 | parser | done | The first `{` outside parentheses, brackets and configuration opens the body of an `if`, `for`, `switch`, `when` or `while` |
| CTRL-10 | analyser | partial | In a value function: a block's value; a `bool` condition; the type of an `if` whose value is used, with an `else` and without. Left: temporal bodies |
| CTRL-10 | transpiler | planned | A block and an `if` as values |
| CTRL-11 | analyser | partial | In a value function: what is assigned after an `if`, a `switch` and a loop; a `var` written `T?` is null from its declaration. Left: temporal bodies |
| CTRL-12 | analyser | partial | In a value function: `for` over a list, a set or a dictionary; `while` with its condition (`while.condition`); a `switch`'s selector. Left: a label that is no constant |
| CTRL-12 | transpiler | planned | The loops and the `switch` of a value function |
| CTRL-13 | analyser | done | In a graph the condition of an `if` is a `bool`, a value or a connection; the `if` has its branches' one type, and is a connection when its condition or a branch is one; with no `else` it is a `T` connection under a temporal condition, and a `T?` or a `T` connection under a wiring-time one by what its branch is |
| CTRL-13 | transpiler | planned | A child graph for each branch of a temporal `if`; an ordinary value in a branch wired as a constant |
| CTRL-14 | analyser | done | A `var` of a graph keeps the category of its first value, and one that holds an ordinary value is not assigned in a branch selected at runtime (`binding.category`); a `return` with a value in such a branch needs every path to return (`return.missing`). The same holds in the cases of a `switch` selected at runtime |
| CTRL-14 | transpiler | planned | A `var` after a temporal `if` as one connection; the result of a graph that returns in a branch |
| CTRL-15 | analyser | done | In a graph: a `switch` over an ordinary value or over a connection, its labels constants (`connection.as_constant`); what a `for` binds by what it loops over, and a loop that makes one child for each member carrying nothing (`loop.carried`); a `while` with an ordinary condition (`connection.as_constant`, `while.condition`) |
| CTRL-15 | transpiler | planned | One case, or one child for each member; a wiring-time loop unrolled |
| SOURCE-1 | analyser | done | A source's shape: no temporal parameter (`source.input`), no `when` (`source.when`), no `var`, and no `inject` of `out`, `scheduler` or `alarm` (`source.statement`). One with a declaration or a hook has its declarations, `start`, one block that holds every `yield` (`{ }`, `while` or `for`), and `stop`, in that order and nothing else: a statement outside the block, a second block, a hook out of order and a `yield` in a hook are `source.statement`. Its declarations and hooks stand in its body itself and each hook once, as a node's do (TIME-5). Its body is read: its declarations and hooks as a node's, and its other statements as value code, with each `yield`'s value held to the result's type |
| SOURCE-1 | transpiler | planned | A `cache` rebuilt on every start and a `state` recorded; `stop` runs when the node ends, whatever state the generator is in |
| SOURCE-2 | transpiler | planned | Yield evaluates its time, then its payload; target validation |
| SOURCE-3 | transpiler | planned | Targets strictly increase; skip, publish or suspend |
| SOURCE-4 | analyser | planned | A valued return and a `yield` inside a value expression rejected |
| SOURCE-4 | transpiler | planned | Completion, suspension and restore of a source |

## Native boundaries

`native-boundaries.md`

| Rule | Stage | State | What the stage does |
| --- | --- | --- | --- |
| NATIVE-1 | parser | done | `export` before a native function's declaration; with `impl`, or before a contract body, it is rejected |
| NATIVE-1 | analyser | partial | A `native impl fn` or `native impl const fn` is a candidate of the operator it names, with its role. A native function whose declaration is exported is public. Left: a native declaration checked as an HGL callable |
| NATIVE-2 | analyser | partial | An implementation completes one declaration of its name and role; duplicates rejected. A native candidate's implementation needs a candidate of its name and role. Left: the exact match of signatures, which says which candidate an implementation completes; selection |
| NATIVE-3 | parser | done | Contract body forms: none, `{}`, hook markers |
| NATIVE-3 | analyser | partial | `start` and `stop` need `when` and are unique; a const implementation has no hook. Left: graph or node by body, where a later pass reads it |
| NATIVE-4 | driver | planned | Target selection is explicit |
| NATIVE-4 | transpiler | planned | The provider meets the declared contract |
| NATIVE-5 | analyser | partial | A native value function's contract injects nothing (`node.in_value_function`, with FN-9). Left: borrowed access stated in the contract; a scalar parameter gains no endpoint privilege |
| NATIVE-6 | parser | done | A native type's body of properties, one to a line, each a name and a type |
| NATIVE-6 | analyser | partial | A native type's properties are read: each an ordinary value type, no two of one name (`field.duplicate`); `value.property` has the property's type, and a name the type does not declare is `member.unknown`. `name.property = value` on a local of a native type, a capability among them, is `binding.immutable`, and a call of a native type's name `call.not_callable`. Left: a property assigned through a longer place, which is not read yet; identity and capabilities as a payload or key |
| NATIVE-7 | analyser | planned | An interface is checked without loading a provider |
| NATIVE-7 | transpiler | planned | Execution needs a compatible provider; an absent binding fails |

## Errors and diagnostics

`errors-and-diagnostics.md`

| Rule | Stage | State | What the stage does |
| --- | --- | --- | --- |
| ERR-1 | diagnostics | done | Every diagnostic has a category and a code; a warning has its code alone, is listed apart from the source codes, and does not reject |
| ERR-2 | analyser | partial | `ok(value)` and `err(value)` take one value by position and no generic argument (`generic.argument_count`), and both types from the complete `result<T, E>` the place expects, a nullable one included; with none it is `result.context_type`. The payload is read as its side's type. Left: a result expected through a generic parameter still being inferred; equality |
| ERR-2 | transpiler | planned | Result equality; atomic publication |
| ERR-3 | analyser | partial | `is_ok` and `is_err` take one result by position and give a `bool`. A direct guard on an immutable result binding proves its tag: in the branch it holds for, across `!` and the Boolean connectives, and after a branch that returns. `binding.value` and `binding.error` need the matching fact (`result.required_variant`), and have the T and the E. In a graph a test of a connection gives no fact. Left: a fact carried through a `switch` or a loop |
| ERR-4 | analyser | partial | `error(message: text, code: optional_code)`: each by its name, `message` a `str` and required, `code` a `str?`; `.message` is a `str` and `.code` a `str?`. Left: `.context` and its record |
| ERR-4 | transpiler | planned | Provenance attached on first graph failure and kept |
| ERR-5 | analyser | partial | `value?` has the T of a `result<T, E>`. In a value function the enclosing result is a `result<U, E>` of the same E, and in a `when` the E is `error` (`result.propagation_type`, which what is no result, a nullable included, is too); in a graph, `start`, `stop`, a node's declarations and a source it is `result.propagation_phase`. Left: an anonymous function |
| ERR-5 | transpiler | planned | `?` evaluates once; failure returns or fails the node |
| ERR-6 | analyser | partial | A list is indexed by an `i64` and a dictionary by its key, for the element or the value. Left: `at`, `get` and their fallbacks (`lookup.default_type`, `nullable.required_presence`) |
| ERR-6 | transpiler | planned | A strict read fails on absence (`value.missing_key`); `get` fallback |
| ERR-7 | analyser | partial | `capture(callable, args...)` in a graph reads the call it names, from the same arguments less the name, as that call is read; in a value function or a hook, of what names no callable, and of a call that composes nothing it is `capture.phase`. Bound with `let`, `.value` is the call's output and `.error` an `error`, both connections; `.value` of an outputless call is `capture.output_field`. Left: a capture used other than through a `let` and its two connections is not judged |
| ERR-7 | transpiler | planned | Capture as a temporal error boundary |
| ERR-8 | transpiler | planned | Capture ends the failed pass without rollback |
| ERR-9 | analyser | partial | A type that is not known is reported once, and nothing is reported of its uses. Left: required constant failures |
| ERR-9 | optimiser | planned | Folding keeps the failure phase of an executed operation |
| ERR-10 | parser | done | `@suppress_warning("...")` before a named function |
| ERR-10 | analyser | partial | `@suppress_warning("node.no_activation")` before a function acknowledges its dormant-node warning. Left: a string that is no warning's code; the warning `statement.no_effect` |
| ERR-11 | transpiler | planned | NaN, infinity and an unordered outcome are data, not errors |
| ERR-12 | analyser | partial | In a value function a direct test against null refines an immutable nullable parameter, `let` or loop binding to its payload's type: in the branch the test holds for, across `!` and the Boolean connectives, and after a branch that returns. A nullable where its payload is required, or as an operand no candidate takes, is `nullable.required_presence`. Left: temporal bodies |
| ERR-13 | parser | done | The result forms are calls of a name, and `?` a postfix suffix: `ok(value)`, `is_err(value)`, `capture(callable, args)`, `value?` |
| ERR-13 | analyser | done | `ok`, `err`, `is_ok`, `is_err` and `capture` as the callee of a call are the forms, looked up nowhere; no declaration, import alias, parameter or local takes one of the five names (`name.duplicate`) |

## Tests and eval

`tests-and-eval.md`

| Rule | Stage | State | What the stage does |
| --- | --- | --- | --- |
| TST-1 | parser | done | An unnamed test context holds functions, structs, enums and named tests only |
| TST-1 | analyser | done | One test scope across parts; exports and operator implementations rejected there |
| TST-2 | analyser | done | Production names no test-only declaration; a test-only declaration takes no name of its module; duplicate test names rejected |
| TST-2 | transpiler | planned | Artifacts omit test-only declarations |
| TST-3 | analyser | partial | A test declares no `state` or `cache`, injects nothing, and has no hook, `yield` or `return`, in its block or a block inside it (`test.statement_phase`). Its block is read as a graph's body is, with `assert` among its statements. Left: eval traces stay test-local |
| TST-4 | analyser | partial | An `assert` takes a `bool` (`type.mismatch`) that is no connection (`connection.as_constant`). Left: a comparison with an `eval`, which is the harness's and is not read |
| TST-4 | runner | planned | Harness comparison and the mismatch report |
| TST-5 | analyser | planned | Eval target selection and argument binding |
| TST-6 | runner | planned | Dense traces: positions, `_` placeholders, a result ending at its last publication |
| TST-7 | analyser | planned | Timed traces: increasing keys, no `_`, one style in an eval |
| TST-7 | runner | planned | Sparse expectations match the actual ticks and times |
| TST-8 | runner | planned | Eval builds core replay, the target and core record; fresh state for each invocation |
| TST-9 | runner | planned | Arguments and profile admission validated before the graph starts |
| TST-10 | analyser | done | `assert raises` takes one string literal whose value is an execution code of the catalogue (`test.raises_code`) |
| TST-10 | runner | planned | The block runs once; a matching escaping error passes after cleanup |
| TST-11 | runner | planned | What cannot satisfy `raises` |
| TST-12 | runner | done | `# expect-error` annotations read from line comments; the owner of each case |
| TST-12 | diagnostics | done | The catalogue's source categories and codes, which an annotation may name |
| TST-13 | parser | done | An error at the end of the file points at the end of the file |
| TST-13 | runner | done | Rejection owners excluded; diagnostics and annotations matched one to one |
| TST-14 | runner | partial | Invalid metadata and surviving-source errors stop the run; selectors; rejection counts. Left: counts of executed cases, with the first target |
| TST-15 | runner | partial | When a run succeeds; identity, kind and outcome of each case. Left: executable cases, which are listed and not run |

## Program execution

`program-execution.md`

| Rule | Stage | State | What the stage does |
| --- | --- | --- | --- |
| RUN-1 | analyser | planned | An entry is an exported temporal function with no temporal parameters |
| RUN-2 | driver | planned | Entry selection; arguments, interfaces and providers checked before start |
| RUN-3 | transpiler | planned | Mode, clock, start and end come from run configuration |
| RUN-4 | transpiler | planned | Independent instantiations; lifecycle order; teardown after failure |
| RUN-5 | optimiser | planned | No pass changes an observable trace, effect, failure or cleanup |
| RUN-5 | transpiler | planned | The generated program gives the same observations |

## Source documentation

`source-documentation.md`

| Rule | Stage | State | What the stage does |
| --- | --- | --- | --- |
| SDOC-1 | parser | done | A documentation comment attaches to the declaration after it; repeated or unattached ones rejected |
| SDOC-2 | docs | unscheduled | Documentation text: delimiters, indentation, line breaks |
| SDOC-3 | analyser | planned | Structured keys name real parameters, generics, requirements or domains |
| SDOC-3 | docs | unscheduled | Section headings and their contents |
| SDOC-4 | docs | unscheduled | Interface text kept with its identity, signature and part |
| SDOC-5 | formatter | planned | Formatting keeps meaning, comments and line-break form, and is idempotent |
| SDOC-6 | analyser | planned | Documentation is no input to checking, selection or lowering |

## Core

`spec/hgl_core/spec`. Required constants are evaluated by the analyser under
the same numeric rules the transpiler rows give.

| Rule | Stage | State | What the stage does |
| --- | --- | --- | --- |
| NUM-1 | analyser | partial | Result types of arithmetic on `i64` and `f64`, and of unary minus, in value functions, through core's candidates. Left: temporal bodies |
| NUM-1 | transpiler | planned | Floor division and modulo rounding |
| NUM-2 | transpiler | planned | i64 overflow is an error; IEEE float arithmetic; exact mixed comparison |
| NUM-3 | transpiler | planned | Division by zero under `ieee` and `nan` |
| NUM-4 | transpiler | planned | Exact and tolerant equality; invalid tolerance is an error |
| NUM-5 | transpiler | planned | NaN, infinity and signed zero in equality and closeness |
| NUM-6 | analyser | partial | In a value function a comparison outcome is a `CmpResult`, and is rejected where a bool or a number is expected. Left: temporal bodies |
| NUM-6 | transpiler | planned | `compare_` outcomes; Boolean comparisons |
| NUM-7 | analyser | planned | Tolerance does not configure `<=>`; ordering an unordered domain needs a stated policy |
| NUM-7 | optimiser | planned | A tolerance grants no ordering law or rewrite |
| NUM-8 | analyser | partial | In a value function, configuration selects among the candidates of the operator its symbol stands for, and an option left out takes the default its candidate declares. Left: temporal bodies |
| NUM-8 | optimiser | planned | Folding alters no default or outcome |
| NUM-8 | transpiler | planned | Value and temporal execution give the same numbers |
| NUM-9 | analyser | partial | A division, by name or by symbol, through core's candidates in a value function: its own type, or a `result` of it under `zero: .error`; `zero` is a generic value parameter, so one given no compile-time constant is rejected. Left: temporal bodies |
| NUM-9 | transpiler | planned | `zero: error` gives `err` with `numeric.division_by_zero` for a zero divisor |
| NUM-10 | transpiler | planned | A checked-division `Err` is data; `?` propagation |
| NUM-11 | analyser | partial | `eq_(a, b, abs: ..., rel: ...)` by name through core's candidates in a value function: the tolerances are options of the candidates for numbers, and of no other. The same as the symbol's configuration, `a =={abs: ...} b`. Left: temporal bodies |
| NUM-11 | transpiler | planned | Tolerances bound at graph construction in composition |
| NUM-12 | analyser | partial | Every division, by name or by symbol, through core's candidates in a value function: a division whose result is an `f64` admits each policy, one whose result is an `i64` or a `duration` only `.error` and `.fail`, and `.error` gives a `result`. Left: temporal bodies |
| NUM-12 | transpiler | planned | A zero divisor under each policy a division admits |
| OPS-1 | analyser | partial | In a value function a symbol is the call of the core operator it stands for, and admits the operand types that operator's candidates declare, core's and a module's own; any other pairing rejected (`operator.no_candidate`). Its operands are read as the call's arguments are, and the type its place expects is requested of the candidates. Left: temporal bodies |
| OPS-1 | transpiler | planned | The operators on strings, durations and instants |
| OPS-2 | analyser | partial | In a value function, ordering between two of one ordered type and equality between two values of one type, by symbol and by name through core's candidates. An operand that is a generic parameter is compared as its declaration's requirements state (GEN-7). Left: a comparison with a nullable operand; temporal bodies |
| OPS-3 | analyser | partial | `a <=> b` and `compare_(a, b)` through core's candidates in a value function are a `CmpResult`. Left: temporal bodies |
| OPS-4 | analyser | partial | `convert<T>(value)` through core's candidates in a value function: the conversions core declares, and no other. Left: a list of structs to a list of their parent; temporal bodies |
| OPS-4 | transpiler | planned | Each conversion |
| OPS-5 | analyser | partial | The Boolean connectives and `!` take bools, in a value function. Left: temporal bodies |
| OPS-6 | analyser | planned | The type each conversion yields; `convert<list<B>>(values)` for a list of structs that inherit from B |
| OPS-6 | transpiler | planned | What each conversion yields |
| OPS-7 | analyser | planned | A conversion's options, and the `result` its checked error policy returns |
| OPS-7 | transpiler | planned | Rounding, what a NaN becomes, and a failure that is checked or escapes |
| OPS-8 | analyser | partial | The options of `convert` through core's candidates in a value function: `invalid` selects `result<T, error>` or T and is given a compile-time constant, `rounding` and `nan` are parameters, and an option is admitted only for the conversions that declare it. Left: temporal bodies |
| OPS-8 | transpiler | planned | Each rounding member, the NaN value, and the two failure policies |
| OPS-9 | analyser | done | In a graph `invalid: .identity`, by symbol or by name, selects core's temporal candidate of `add_` or `sub_` for two operands of one type, where the value operations it requires exist for that type (GEN-16); without the option the value candidate is selected and lifted. Core's two candidates are nodes, and are read as any node is |
| OPS-9 | transpiler | planned | One valid operand is forwarded (right-only subtraction negates it); both invalid invalidate the output; ordered handlers return after the first publication |
| END-1 | transpiler | planned | A publication sets validity and the retained value |
| END-2 | transpiler | planned | A publication marks modified, an equal one included |
| END-3 | analyser | planned | `delta_value(x)` needs proof that `x` is valid and modified |
| END-3 | transpiler | planned | `delta_value` returns the value published this cycle |
| END-4 | transpiler | planned | Invalidation |
| END-5 | transpiler | planned | Active and passive affect scheduling only; repeated writes in one evaluation |
| END-6 | analyser | planned | `last_modified` has type `datetime?` |
| END-6 | transpiler | planned | `last_modified` value while valid and while invalid |
| CAP-1 | analyser | done | `inject` binds each capability but `out` to a local of the native type core declares for it; a property is read with its type and not assigned, and no call constructs the type (with TIME-8 and NATIVE-6) |
| CAP-2 | analyser | done | `log` and `log_debug`, `log_info`, `log_warning`, `log_error` are core's declarations, typed as any call; `logger.level` is a `LogLevel` |
| CAP-2 | transpiler | planned | One record at its level |
| CAP-3 | analyser | done | A clock's four properties, each with its type |
| CAP-3 | transpiler | planned | What each clock property observes |
| CAP-4 | analyser | done | A scheduler's two properties; `un_schedule`, with a tag or without, and `reset` |
| CAP-4 | transpiler | planned | Cancelling a pending wake, and every one |
| CAP-5 | analyser | partial | An alarm has no property, and a node injects it or `scheduler` (TIME-8). Left: asking for a wake on one, which core does not declare yet |
| CAP-5 | transpiler | planned | A best-effort wake |
| CAP-6 | transpiler | planned | Reading or writing the global store wakes no node |
| CONF-1 | runner | planned | Runs core's cases as written and adds no expectation |
| CONF-2 | runner | planned | Conformance runs with core and its native providers only |
| CONF-3 | runner | planned | `conformance` and `conformance.rejections` assembled from parts; never shipped |
| CONF-4 | runner | planned | Conformance results reported apart from this repository's own tests |
| CEVAL-1 | runner | planned | Eval uses core's designated replay and record |
| CEVAL-2 | runner | planned | Typed replay entries for present publications; a dense placeholder adds no entry and no run bound |
| CEVAL-3 | transpiler | planned | Replay publishes present entries; silence publishes nothing |
| CEVAL-4 | transpiler | planned | Record starts with an empty recording and retains each publication and its time |
| CEVAL-5 | runner | planned | A fresh run, with its own node state and recording, for each eval; dense results built by TST-6 |
| CEVAL-6 | runner | planned | A failed run or a missing required recording fails the eval; neither is an empty result |
