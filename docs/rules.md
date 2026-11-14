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
| LEX-7 | analyser | planned | The literal 2^63 admitted only under a minus; enum numbering range |
| LEX-8 | lexer | done | `>` never joined to `>`; digits then letters are one duration candidate; exponent forms |
| LEX-9 | parser | done | `time` as member name and argument label; `const` as modifier or name by position |
| LEX-10 | parser | done | Contextual names take their meaning by position; `str(value)` is a conversion |
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
| UNIT-3 | analyser | partial | Private unless exported; operators public; `impl fn` reached through its operator; import cycles rejected. Left: export closure |
| UNIT-4 | analyser | planned | Module-qualified nominal identity; an alias adds none |
| UNIT-5 | analyser | done | Duplicate functions of one role, parameters, parts and locals rejected |
| UNIT-6 | analyser | done | Forward reference to declarations; locals visible after declaration; test-only declarations kept out of production |
| UNIT-7 | analyser | partial | A default reads no parameter of its declaration and no field of its struct. Left: the closure an imported default keeps |
| UNIT-8 | analyser | planned | Defaults are required constants; which parameters may have one |
| UNIT-9 | analyser | planned | Inherited defaults compared; a conflict needs an override |
| UNIT-10 | analyser | partial | Every name in a signature, body, type and constraint resolved to its one binding. Left: capability receivers; activation forms; the prelude |
| UNIT-11 | analyser | planned | The prelude is core's mandatory module, bound in every module |
| UNIT-12 | analyser | partial | No declaration, import, local, parameter or generic parameter takes a name already in scope. Left: prelude names |

## Types and values

`types-and-values.md`

| Rule | Stage | State | What the stage does |
| --- | --- | --- | --- |
| TYPE-1 | analyser | planned | Scalar type identities and capabilities |
| TYPE-1 | transpiler | planned | Scalar representations; bytes as unsigned octets |
| TYPE-2 | lexer | done | Literal dates keep years 1 to 9999; literal durations fit signed 64-bit microseconds |
| TYPE-2 | analyser | planned | Temporal scalar types and their domains |
| TYPE-2 | transpiler | planned | Temporal scalar representations; zone names kept distinct |
| TYPE-3 | analyser | planned | Nominal structs; only abstract parents; concrete structs final; invariant generic applications |
| TYPE-4 | analyser | planned | Field type, optionality and defaults; parent conflicts rejected |
| TYPE-5 | analyser | planned | A constructor supplies every required field |
| TYPE-5 | transpiler | planned | Arguments evaluated once in order; defaults only when omitted; retained results own their data |
| TYPE-6 | analyser | planned | Recursive struct edges; inheritance and collection cycles rejected |
| TYPE-7 | analyser | planned | Enum members as typed constants; numbering, range and duplicates; checked construction |
| TYPE-7 | transpiler | planned | Enum string and numeric conversion |
| TYPE-8 | analyser | planned | Typing of `any` |
| TYPE-8 | transpiler | planned | A boxed value keeps its exact type; equality, capability failure, independent copies |
| TYPE-9 | analyser | planned | Temporalization of composite types; `atomic` normalisation |
| TYPE-10 | analyser | planned | List sizes as constants; `unbounded`; size in identity; tuples distinct from lists |
| TYPE-11 | analyser | planned | Rolling size kind and bounds (`rolling.size_kind`, `rolling.size_bounds`) |
| TYPE-11 | transpiler | planned | Window publications under core's contract |
| TYPE-12 | analyser | planned | Where `ref` and `signal` are admitted |
| TYPE-13 | analyser | planned | Result and error envelopes as ordinary atomic values |
| TYPE-14 | parser | done | `?` after a type |
| TYPE-14 | analyser | planned | Where a nullable type is admitted; null is no value of plain T |
| TYPE-15 | parser | done | The suffix covers the whole preceding type; `?` in a type and `?` in an expression kept apart |
| TYPE-15 | analyser | planned | A present null is data, distinct from an invalid input |

## Expressions and bindings

`expressions-and-bindings.md`

| Rule | Stage | State | What the stage does |
| --- | --- | --- | --- |
| EXPR-1 | parser | done | Precedence, associativity, grouping; a tuple has a comma |
| EXPR-1 | analyser | planned | A chained comparison is typed as written, not as a conjunction |
| EXPR-2 | analyser | planned | Positional before named; duplicate, unknown, missing and mistyped arguments rejected |
| EXPR-2 | transpiler | planned | Arguments evaluated once in written order |
| EXPR-3 | analyser | planned | Category and type of `let` and `var`; definite assignment |
| EXPR-4 | analyser | planned | Values and connections kept distinct; reassigning a graph `var` rewires |
| EXPR-5 | analyser | planned | Expected type; numeric widening; const expression restrictions |
| EXPR-6 | analyser | planned | A guard precedes the payload read it protects |
| EXPR-6 | transpiler | planned | Short-circuit evaluation; composition reads no payload |
| EXPR-7 | analyser | planned | Structural and atomic projection; `ref` access kept |
| EXPR-8 | analyser | planned | `_` and timed cells only in test traces |
| EXPR-9 | parser | done | Brackets delimit a sequence whose entries are all values or all pairs |
| EXPR-9 | analyser | planned | The use site and expected type give a sequence its meaning |
| EXPR-10 | parser | done | A named-entry brace directly after an operator is its configuration; an empty one rejected |
| EXPR-10 | analyser | planned | Configuration bound to the operator's public parameters; unknown, duplicate and unsupported options rejected |
| EXPR-11 | parser | done | `<=>` at the comparison tier |
| EXPR-11 | analyser | planned | `<=>` yields a comparison outcome, not a bool |
| EXPR-12 | analyser | planned | Configuration that selects a result type is resolved during checking |
| EXPR-13 | transpiler | planned | Evaluation order of a configured operator |
| EXPR-14 | analyser | planned | Configuration introduces no temporal input |
| EXPR-14 | transpiler | planned | Configuration bound at graph construction in composition, per invocation elsewhere |

## Functions and phases

`functions-and-phases.md`

| Rule | Stage | State | What the stage does |
| --- | --- | --- | --- |
| FN-1 | analyser | planned | Roles of `const fn`, a `const` parameter and a temporal `fn` |
| FN-2 | analyser | planned | A body is classified composition or runtime before phase checking |
| FN-3 | analyser | planned | Phase rules for iteration, conditionals and hooks |
| FN-4 | analyser | partial | A temporal and a value definition may share a name. Left: role selection without fallback |
| FN-5 | parser | done | `const` is a modifier only where one is admitted |
| FN-5 | analyser | planned | `const(argument)` resolves as an ordinary call |
| FN-6 | analyser | planned | Lifting a value helper: which arguments are inputs, which configuration |
| FN-6 | transpiler | planned | Activation and publication of a lifted node |
| FN-7 | analyser | planned | Return by phase; an anonymous function takes its context from the accepting form |
| FN-7 | transpiler | planned | A runtime return publishes; no write, no tick |
| FN-8 | parser | done | Pack parameter forms |
| FN-8 | analyser | planned | Packs follow fixed parameters, have no defaults and are not const |
| FN-9 | analyser | planned | Capabilities propagate through value helpers |
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
| GEN-1 | analyser | partial | Generic scope covers the whole declaration. Left: repeated parameters agree; occurrence requirements |
| GEN-2 | analyser | planned | Every generic resolved; no partial application or generic default |
| GEN-3 | analyser | planned | Forms `requires` admits; binding equalities solved to a fixed point |
| GEN-4 | analyser | partial | An `impl fn` binds an operator its module declares or its file imports. Left: the prelude; operator identity in selection |
| GEN-5 | analyser | planned | Candidate selection: output first, exactly one most specific |
| GEN-6 | analyser | planned | A candidate honours its operator's contract |
| GEN-7 | analyser | planned | A generic body is checked under its stated requirements |
| GEN-8 | parser | done | `_` as an `instantiate` argument |
| GEN-8 | analyser | planned | `instantiate` requests candidates; no explicit type arguments on ordinary calls |
| GEN-9 | analyser | planned | Property clauses apply to explicit concrete domains |
| GEN-9 | optimiser | planned | A recorded law licenses no reassociation across overflow or float effects |
| GEN-10 | parser | done | A cardinality after a pack parameter or a generic pack parameter |
| GEN-10 | analyser | planned | Pack views, `each`, what a cardinality admits |
| GEN-10 | transpiler | planned | `schemas(pack)` as a runtime-only borrowed view |
| GEN-11 | analyser | planned | Reference removal in inference |
| GEN-12 | analyser | planned | Structural specificity |
| GEN-13 | analyser | planned | Kind of a generic parameter |
| GEN-14 | parser | done | `<` after a name read as generic arguments or comparison; marked undecided when both parse |
| GEN-14 | analyser | partial | Decides an undecided constructor from what its name resolves to; a generic type before a comparison's `<` rejected. Left: each argument against its parameter's kind |
| GEN-15 | parser | done | Constraint precedence: negation, then conjunction, then disjunction |
| GEN-16 | parser | done | One form for constraint calls, with an optional result arrow |
| GEN-16 | analyser | planned | Predicate or operator requirement by the referenced declaration; arrow rejected on a predicate |

## Temporal model

`temporal-model.md`

| Rule | Stage | State | What the stage does |
| --- | --- | --- | --- |
| TIME-1 | analyser | planned | A payload read needs admission |
| TIME-1 | transpiler | planned | Endpoint observations: validity, modification, retained data, delta |
| TIME-2 | analyser | planned | Composition reads no live payload |
| TIME-2 | transpiler | planned | Evaluation in dependency order, once per node per cycle |
| TIME-3 | analyser | planned | `when` selectors and their defaults; a nested `when` rejected |
| TIME-3 | transpiler | planned | Handlers evaluated in source order |
| TIME-4 | analyser | planned | Aggregate subscriptions; node-wide validity; guarded reads under manual admission |
| TIME-5 | analyser | planned | Declarations precede hooks; one `start`, one `stop`; state initialisers read no input |
| TIME-5 | transpiler | planned | Start follows initialisation; stop finalises |
| TIME-6 | transpiler | planned | State is recordable history; cache is rebuilt on every start |
| TIME-7 | analyser | planned | `inject out` needs an output and is used during evaluation |
| TIME-7 | transpiler | planned | Return and `out` share one result; no write, no tick |
| TIME-8 | analyser | planned | Injectables are capabilities, used receiver-first, never passed as data |
| TIME-9 | analyser | planned | `scheduled()`, `activate` and `passivate` are reserved; helpers inherit capabilities |
| TIME-10 | transpiler | planned | A reference ticks on a changed designation |

## Control flow and sources

`control-flow-and-sources.md`

| Rule | Stage | State | What the stage does |
| --- | --- | --- | --- |
| CTRL-1 | analyser | planned | Wiring-time, temporal and value conditions told apart |
| CTRL-2 | transpiler | planned | Only the selected child runs; a changed selector restarts it |
| CTRL-3 | analyser | planned | Type of a temporal `if` without `else` |
| CTRL-3 | transpiler | planned | The never-ticking branch |
| CTRL-4 | analyser | planned | Escaping assignments need an enclosing declaration; branch locals stay local |
| CTRL-5 | analyser | planned | Return targets the enclosing function; definite assignment after an early return |
| CTRL-6 | analyser | planned | Switch labels are typed constants; duplicates rejected; enum coverage |
| CTRL-6 | transpiler | planned | No fallthrough; an unmatched selector uses default or fails |
| CTRL-7 | analyser | planned | What a composition loop receives |
| CTRL-7 | transpiler | planned | One child per member; update keeps it, removal stops it |
| CTRL-8 | analyser | planned | Traversal forms; no loop-carried accumulation in a dynamic composition loop |
| CTRL-8 | transpiler | planned | Traversal over current values, lists in index order |
| CTRL-9 | parser | done | The first `{` outside parentheses, brackets and configuration opens a `when` or `while` body |
| SOURCE-1 | analyser | planned | What a body containing `yield` may hold |
| SOURCE-2 | transpiler | planned | Yield evaluates its time, then its payload; target validation |
| SOURCE-3 | transpiler | planned | Targets strictly increase; skip, publish or suspend |
| SOURCE-4 | analyser | planned | A valued return and a `yield` inside a value expression rejected |
| SOURCE-4 | transpiler | planned | Completion, suspension and restore of a source |

## Native boundaries

`native-boundaries.md`

| Rule | Stage | State | What the stage does |
| --- | --- | --- | --- |
| NATIVE-1 | analyser | planned | A native declaration is checked as an HGL callable; roles kept |
| NATIVE-2 | analyser | partial | An implementation completes one declaration of its name and role; duplicates rejected. Left: the exact match of signatures; selection |
| NATIVE-3 | parser | done | Contract body forms: none, `{}`, hook markers |
| NATIVE-3 | analyser | planned | Graph or node by body; hooks need `when` and are unique |
| NATIVE-4 | driver | planned | Target selection is explicit |
| NATIVE-4 | transpiler | planned | The provider meets the declared contract |
| NATIVE-5 | analyser | planned | Capability requirements propagate; a scalar parameter gains no endpoint privilege |
| NATIVE-6 | analyser | planned | Native type identity and capabilities |
| NATIVE-7 | analyser | planned | An interface is checked without loading a provider |
| NATIVE-7 | transpiler | planned | Execution needs a compatible provider; an absent binding fails |

## Errors and diagnostics

`errors-and-diagnostics.md`

| Rule | Stage | State | What the stage does |
| --- | --- | --- | --- |
| ERR-1 | diagnostics | done | Every diagnostic has a category and a code; a warning does not reject |
| ERR-2 | analyser | planned | `ok` and `err` need a complete result type (`result.context_type`) |
| ERR-2 | transpiler | planned | Result equality; atomic publication |
| ERR-3 | analyser | planned | Tag facts from `is_ok` and `is_err` guards (`result.required_variant`) |
| ERR-4 | analyser | planned | Typing of `error(message, code)` |
| ERR-4 | transpiler | planned | Provenance attached on first graph failure and kept |
| ERR-5 | analyser | planned | Where `?` may propagate (`result.propagation_type`, `result.propagation_phase`) |
| ERR-5 | transpiler | planned | `?` evaluates once; failure returns or fails the node |
| ERR-6 | analyser | planned | Lookup typing (`lookup.default_type`, `nullable.required_presence`) |
| ERR-6 | transpiler | planned | A strict read fails on absence (`value.missing_key`); `get` fallback |
| ERR-7 | analyser | planned | `capture` only in composition (`capture.phase`, `capture.output_field`) |
| ERR-7 | transpiler | planned | Capture as a temporal error boundary |
| ERR-8 | transpiler | planned | Capture ends the failed pass without rollback |
| ERR-9 | analyser | planned | Required constant failures reject; no cascaded diagnostics |
| ERR-9 | optimiser | planned | Folding keeps the failure phase of an executed operation |
| ERR-10 | parser | done | `@suppress_warning("...")` before a named function |
| ERR-10 | analyser | planned | The string is checked against the warning catalogue |
| ERR-11 | transpiler | planned | NaN, infinity and an unordered outcome are data, not errors |
| ERR-12 | analyser | planned | A null test refines an immutable nullable binding |

## Tests and eval

`tests-and-eval.md`

| Rule | Stage | State | What the stage does |
| --- | --- | --- | --- |
| TST-1 | parser | done | An unnamed test context holds functions, structs, enums and named tests only |
| TST-1 | analyser | done | One test scope across parts; exports and operator implementations rejected there |
| TST-2 | analyser | done | Production names no test-only declaration; a test-only declaration takes no name of its module; duplicate test names rejected |
| TST-2 | transpiler | planned | Artifacts omit test-only declarations |
| TST-3 | analyser | planned | A test is a composition block (`test.statement_phase`); eval traces stay test-local |
| TST-4 | analyser | planned | `assert` needs a wiring-time bool |
| TST-4 | runner | planned | Harness comparison and the mismatch report |
| TST-5 | analyser | planned | Eval target selection and argument binding |
| TST-6 | runner | planned | Dense traces: positions, horizon, `_` |
| TST-7 | analyser | planned | Timed traces: increasing keys, no `_`, one style in an eval |
| TST-7 | runner | planned | Sparse expectations match the actual ticks and times |
| TST-8 | runner | planned | Eval builds core replay, the target and core record; fresh state for each invocation |
| TST-9 | runner | planned | Arguments and profile admission validated before the graph starts |
| TST-10 | analyser | planned | `assert raises` takes one literal catalogued execution code (`test.raises_code`) |
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
| NUM-1 | analyser | planned | Result types of arithmetic |
| NUM-1 | transpiler | planned | Floor division and modulo rounding |
| NUM-2 | transpiler | planned | i64 overflow is an error; IEEE float arithmetic; exact mixed comparison |
| NUM-3 | transpiler | planned | Division by zero under `ieee` and `nan` |
| NUM-4 | transpiler | planned | Exact and tolerant equality; invalid tolerance is an error |
| NUM-5 | transpiler | planned | NaN, infinity and signed zero in equality and closeness |
| NUM-6 | analyser | planned | A comparison outcome converts to neither bool nor number |
| NUM-6 | transpiler | planned | `compare_` outcomes; Boolean comparisons |
| NUM-7 | analyser | planned | Tolerance does not configure `<=>`; ordering an unordered domain needs a stated policy |
| NUM-7 | optimiser | planned | A tolerance grants no ordering law or rewrite |
| NUM-8 | analyser | planned | Configuration selects the same operator; omitted options take its public defaults |
| NUM-8 | optimiser | planned | Folding alters no default or outcome |
| NUM-8 | transpiler | planned | Value and temporal execution give the same numbers |
| NUM-9 | analyser | planned | The `zero` policy is resolved in checking and selects `f64` or `result<f64, error>` |
| NUM-9 | transpiler | planned | `zero: error` gives `err` with `numeric.division_by_zero` for a zero divisor |
| NUM-10 | transpiler | planned | A checked-division `Err` is data; `?` propagation |
| NUM-11 | analyser | planned | `abs` and `rel` are scalar f64 configuration |
| NUM-11 | transpiler | planned | Tolerances bound at graph construction in composition |
| END-1 | transpiler | planned | A publication sets validity and the retained value |
| END-2 | transpiler | planned | A publication marks modified, an equal one included |
| END-3 | analyser | planned | `delta_value(x)` needs proof that `x` is valid and modified |
| END-3 | transpiler | planned | `delta_value` returns the value published this cycle |
| END-4 | transpiler | planned | Invalidation |
| END-5 | transpiler | planned | Active and passive affect scheduling only; repeated writes in one evaluation |
| END-6 | analyser | planned | `last_modified` has type `datetime?` |
| END-6 | transpiler | planned | `last_modified` value while valid and while invalid |
| CONF-1 | runner | planned | Runs core's cases as written and adds no expectation |
| CONF-2 | runner | planned | Conformance runs with core and its native providers only |
| CONF-3 | runner | planned | `conformance` and `conformance.rejections` assembled from parts; never shipped |
| CONF-4 | runner | planned | Conformance results reported apart from this repository's own tests |
| CEVAL-1 | runner | planned | Eval uses core's designated replay and record |
| CEVAL-2 | runner | planned | Typed input data omits silent cells; the horizon is kept |
| CEVAL-3 | transpiler | planned | Replay publishes present entries; silence publishes nothing |
| CEVAL-4 | transpiler | planned | The recorder retains each publication and its time |
| CEVAL-5 | runner | planned | A fresh run and recording for each eval; dense result construction |
| CEVAL-6 | runner | planned | Failure or a missing recording is an error, never an empty result |
