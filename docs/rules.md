# Rules and stages

Each rule of the pinned specification and core, the stage that implements
it, and whether that stage does so yet, in one file for each chapter: see
[Chapters](#chapters). A rule shared between stages has one entry for each.
`crates/hgl_cli/tests/rule_table.rs` checks the chapters against the pinned
documents: every rule has an entry, no entry names an unknown rule, and a
`done` entry's rule is cited in its stage's code.

| State | Meaning |
| --- | --- |
| `done` | The stage implements its part of the rule and a test cites the rule |
| `partial` | The stage implements some of its part, and a test cites the rule; the entry says what is left |
| `planned` | The rule is expected to land in this stage; the entry moves if building the stage shows otherwise |
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

The `transpiler` entries are the hgraph target's. Nothing is marked for
the cpp target (`hgl_cpp`) yet. Its progress is counted in cases, as
`docs/cpp-target.md` has it: each of core's runtime cases it does not
run is listed with its reason in `crates/hgl_cpp/ci/unsupported_cases.toml`,
and `crates/hgl_hgraph/ci/check_core.py --target cpp` fails where a
listed case runs or a case not listed is not run
([architecture](architecture.md#cpp)).

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

## Chapters

Each chapter's rules are in a file of their own. A rule is a `## RULE`
heading; under it, each stage's entry is a line `stage: state`, then a
line `- ...` for each fact of what the stage does. Two changes to one
rule, made apart, then merge as long as they change different lines.

- [Source text](rules/source-text.md)
- [Units and names](rules/units-and-names.md)
- [Types and values](rules/types-and-values.md)
- [Expressions and bindings](rules/expressions-and-bindings.md)
- [Functions and phases](rules/functions-and-phases.md)
- [Generics and operators](rules/generics-and-operators.md)
- [Temporal model](rules/temporal-model.md)
- [Control flow and sources](rules/control-flow-and-sources.md)
- [Native boundaries](rules/native-boundaries.md)
- [Errors and diagnostics](rules/errors-and-diagnostics.md)
- [Tests and eval](rules/tests-and-eval.md)
- [Program execution](rules/program-execution.md)
- [Source documentation](rules/source-documentation.md)
- [Core](rules/core.md)

## The C++ target's emitter

What `hgl_cpp` writes of value code, value functions and the tests of
them, and the rules it follows doing so; core's cases it runs are listed
in `crates/hgl_cpp/ci/unsupported_cases.toml` ([architecture](architecture.md#cpp)).

| What it writes | Rules |
| --- | --- |
| A `const fn`, an HGL candidate of an operator, and each instance of a generic one | FN-1, GEN-2, GEN-4 |
| A parameter, a `var` parameter, and a call that gives it a place; a copy where the copy rule says | FN-13, FN-15 |
| A module constant, one value wherever it is read | UNIT-13 |
| An enum and its members; `switch` on one | TYPE-7, CTRL-6 |
| The scalars, `tenor` among them, `T?` and `null`, tuples, lists and fixed lists, and their literals | TYPE-1, TYPE-2, TYPE-4, TYPE-10, TYPE-14, TYPE-17, LEX-11 |
| `let`, `var`, assignment, compound assignment, a place of a list or a tuple | EXPR-3 |
| `if` and `else`, as statements and values; blocks; `while`; `for` over a list; `return` | CTRL-5, CTRL-10, CTRL-12, FN-7 |
| A test against `null`, and what it proves | ERR-12 |
| A default, and a `T?` left out | UNIT-7, UNIT-8 |
| Arguments evaluated in the order written | EXPR-2, EXPR-13 |
| `ok`, `err`, `is_ok`, `is_err`, `.value`, `.error`, `error(...)`, `?` in a value function | ERR-2, ERR-3, ERR-4, ERR-5 |
| A failure with its code, reported at the statement of the test that ran | ERR-1, EXPR-3 |
| `assert`; `assert raises` | TST-4, TST-10, TST-11 |
| A call of core's native candidates and functions, the library's | NATIVE-7, OPS-4 |
| A struct, a generic one and an abstract one, with `==` of each field; its construction by named fields, defaults and nulls, in the order written; its fields read and assigned; structs in lists, tuples, optionals, sets and dictionaries; a candidate of an operator over one | TYPE-3, TYPE-4, TYPE-5, OPS-2, GEN-4 |
| A set and a dictionary, in no order, equal whatever the order; their literals; a strict read that fails with `value.missing_key`, an assignment that adds a key, core's `get`; `for` over each | TYPE-14, EXPR-3, EXPR-9, ERR-6, CTRL-12, OPS-2 |
| The delta of a nullable, a list, a set, a dictionary and a struct, built by its properties and read through them; a tuple's as a tuple | TYPE-16 |
| An enum's member made from its number or its name | TYPE-7 |
| `timezone` and `zoned_time`, and their literals; a `T?` compared with a `T` | TYPE-2, TYPE-14, OPS-2 |
| A `var` place beside an argument that runs statements, taken where the call is made | FN-15, EXPR-2 |
| A node of scalars: its inputs' activity and validity, handlers in source order, `state` and `cache`, `start` and `stop` | FN-2, TIME-3, TIME-4, TIME-5, TIME-6 |
| `return value`, `out = value`, `invalidate(out)` in a handler | TIME-7, FN-7, END-4 |
| A payload read; `valid`, `modified`, `delta_value` plain and into a `result`, `last_modified` of an input | TIME-1, END-1, END-2, END-3, END-6, ERR-14 |
| `is_active`, `activate` and `passivate` of a direct input in a handler, `is_active` in `stop` too | TIME-9, END-5 |
| A temporal value of an enum: read, published, replayed, recorded, wired as a constant | TYPE-7, TYPE-16 |
| `inject clock`, `logger`, `global_state`, and a property of each | TIME-8, CAP-1, CAP-2, CAP-3 |
| A source, and its `state`, `cache`, `inject`, `start` and `stop` | SOURCE-1, SOURCE-2, SOURCE-3, SOURCE-4 |
| `inject scheduler` and `inject alarm` in a node; `schedule`, `un_schedule`, `reset`; what woke the node, which decides the handlers it runs; `scheduled()` and `modified([])` read as a `bool` | TIME-3, TIME-8, TIME-9, CAP-4, CAP-5 |
| A graph; a value where a connection is expected; a lifted call, an input that binds its default or null where it is not valid, a parameter left out bound as configuration, a sink | FN-2, FN-3, FN-6, FN-11 |
| `eval`, dense and timed traces, their comparison, a `let` of one, a trace given to a `const fn`; an eval of a value function | TST-4 to TST-8, TST-16 |
| `capture`, its `.value` and `.error`; an `error` and a `result` as temporal values and in a trace; `?` in a `when`; an error's `.context` | ERR-2, ERR-4, ERR-5, ERR-7, ERR-8, TST-5 |
| A connection of T where one of `T?` is taken | TYPE-14, FN-11 |
| A temporal candidate of an operator | GEN-4, OPS-9 |
| A run's bounds from its `RunConfig` | RUN-1, CRUN-2 |
| A window of a scalar, of a tick count or of a duration: a publication to one, `all_valid`, its value read in a hook or given to a lifted helper where it is all valid, its delta and `removed_value` in the form their place takes, its trace, record and replay | TYPE-11, TYPE-16, END-3, END-9, ERR-14, FN-12, TIME-7, TST-16 |
| A temporal list, growing and fixed: published whole and by delta, read whole, `valid`, `all_valid` and `delta_value` of one; a child read, at a constant index and one that runs, plain and into a `result`, given to `valid` and the other operations, and wired; a trace cell of one, whole or a delta, an input of one replayed, and a whole cell compared with the value the result holds | TYPE-9, TYPE-10, TYPE-16, TIME-1, TIME-7, TIME-9, END-3, END-7, END-8, ERR-14, TST-5, TST-9 |
| A `ref` parameter and result: hgraph's `REF` input and output; a whole `ref` parameter returned sampled; an input that is no `ref` parameter designated whole, and followed through its re-binding with no handler run for it; `out =` and `invalidate(out)` of a `ref` result | FN-16, TIME-10, END-10 |
| `inject global_state` in a graph, and `get`, `has`, `put` on it where the graph is built, once each, in the order written | FN-9, CAP-6, CRUN-7 |
| An application: its statements setting the run's store before its run statement, in the order written; its run statement, `eval` in simulation and `run` in real time, with the entry's arguments and its `RunConfig` held in the order written; its entry unit and executable | UNIT-14, RUN-1, RUN-3, EXPR-2 |
| `if` and `switch` over a connection in a graph: each branch a child graph of hgraph's `switch_`, taking what it reads whole; its value, the `var` it assigns, or a `return` and what follows; no `else`, no `default` | CTRL-2, CTRL-3, CTRL-5, CTRL-6, CTRL-13, CTRL-14, CTRL-15 |
| A `var` of a connection; an `if` over an ordinary value that gives a connection | CTRL-1, CTRL-13, CTRL-14 |
| `for` over a connection of a dictionary or a list, a child graph of hgraph's `map_` for each member; over a fixed list, the body for each position | CTRL-7, CTRL-15 |
| A temporal struct, tuple and atomic struct: a field and a position a child of its own, read, given to an endpoint operation and written in a `when` (`out.bid = value`), and its connection in a graph; a whole read, `all_valid` one level deep with an optional child aside, a publication whole and by a delta, `delta_value`, trace cells | TYPE-9, TYPE-15, TYPE-16, TIME-1, TIME-7, TIME-9, EXPR-7, TST-5 |
| A temporal nullable scalar or enum: a published null, its delta, a test against null of an input, a trace cell | TYPE-14, TYPE-15, END-1, END-3, END-6, ERR-12 |
| A temporal set and dictionary: published whole and as a delta, read whole, `delta_value`, `all_valid` of a dictionary, an entry of a dictionary read in a hook and given to an endpoint operation, `invalidate(out)`, their trace cells whole and as deltas | TYPE-9, TYPE-16, TIME-1, TIME-7, TIME-9, EXPR-7, END-3, END-4, TST-5, TST-9 |

Not written yet, and reported: a `?` after anything its statement reads
or runs before it (ERR-5), a `switch` case's binding read after the
`switch` (UNIT-12), an element of a `list<bool>` given to a `var`
parameter (a bit, no `bool&`), a struct that holds itself (TYPE-6), a
field's default that runs statements, a field of an abstract struct
assigned, a dictionary written with keys that may be one, a dictionary's
entry given to a `var` parameter, an application of a generic struct its
C++ template is not, a window of any but a scalar, and of temporal code: a
generic or an abstract struct as a temporal value, a struct or a tuple
with a part of a form the library does not read whole, a list with a part
that is no scalar, enum, nullable or list, an atomic struct that holds an
optional field (hgraph issue 1874), a set's element or a dictionary's key
of any but a scalar, a constant connection of a set or a dictionary, an
entry of a dictionary's entry, a key's child as a connection of a graph
(EXPR-7), a nullable put in the run's store (hgl_spec issue 165), a
designation of a child of an input (FN-16), `all_valid` of a set, a
change of an input's activity in `stop` (END-5), a `for` over a connection of a set (CTRL-15), in a branch or a
loop's body the store or an ordinary value that may fail (FN-9: hgraph
builds each child as the graph around it is built), a `return` under a
branch selected at runtime in a loop unrolled where the graph is built,
branches that assign two `var`s (CTRL-14), a `capture` given an `f64`
configuration (ERR-7, hgraph issue 1866), a lifted call that leaves out a
`T?` whose default may fail (FN-6), and a connection of T given to a `ref`
parameter of `T?` (TYPE-14: hgraph binds no `REF` of a `T?` to an output
of T). A call
of a function with a pack, core's `all_` and `any_`, has no callee the
analyser reads.

## The C++ target's runtime library

No rule is `done` or `partial` for the C++ target yet: no program reaches
its runtime library, `libhglrt` (`runtime/cpp`), until the target's emitter
writes one, and the target's progress is counted in cases
([cpp-target.md](cpp-target.md#progress-and-acceptance)). What the library
implements and its unit tests check today, under `runtime/cpp`, is this;
the chapters' entries stay the hgraph target's.

| What | Rules or core declarations | Header | Tests |
| --- | --- | --- | --- |
| An execution failure: hgraph's `ExecutionFailure` carrying the error, its code or none, and the place it was raised at; `?` raising a retained error with its provenance | ERR-1, ERR-4, ERR-5, ERR-8 | `failure.h` | `failure_tests.cpp` |
| Reads checked when they run: `value.unset_read`, or `err` of it where the place takes a result; an index outside an ordinary list, read or assigned, `value.index_out_of_range` | TIME-1, ERR-14, EXPR-3, EXPR-7 | `checked.h` | `checked_tests.cpp` |
| `add_`, `sub_`, `mul_`, `neg_`, `floordiv_`, `mod_` of `i64`, checked, with their zero policies | NUM-1, NUM-2, NUM-3, NUM-9, NUM-12 | `scalar.h` | `scalar_tests.cpp` |
| `add_`, `sub_`, `mul_`, `neg_` of `f64`; `div_` of two `f64` and of two `i64` under each zero policy | NUM-1, NUM-2, NUM-3, NUM-9, NUM-12 | `scalar.h` | `scalar_tests.cpp` |
| `add_`, `sub_`, `mul_` of an `i64` and an `f64`; `div_`, `floordiv_`, `mod_` of an `i64` and an `f64`, and `floordiv_`, `mod_` of two `f64`, under each zero policy, special values included | NUM-1, NUM-2, NUM-3, NUM-9, NUM-12 | `scalar.h` | `scalar_tests.cpp` |
| `eq_`, `ne_` of `i64` and `i64`, `f64` and `f64`, `i64` and `f64`: exact, and within `abs` and `rel` | NUM-2, NUM-4, NUM-5 | `scalar.h` | `scalar_tests.cpp` |
| `eq_<T>`, `ne_<T>`; `lt_<T>`, `le_<T>`, `gt_<T>`, `ge_<T>`, `compare_<T>` of each ordered type; `lt_`, `le_`, `gt_`, `ge_`, `compare_` of an `i64` and an `f64`, exactly | OPS-2, OPS-3, NUM-2, NUM-6 | `scalar.h` | `scalar_tests.cpp`, `text_tests.cpp` |
| `and_`, `or_`, `not_`, `all_`, `any_` | OPS-5 | `scalar.h` | `scalar_tests.cpp` |
| A duration's `neg_`, `add_`, `sub_`, `mul_` by an `i64` or an `f64`, `div_` by a number or a duration under each zero policy it admits, and `floordiv_` of two: rounded to the microsecond, a tie to the even one, checked | OPS-1, OPS-10, NUM-12 | `temporal.h` | `temporal_tests.cpp` |
| An instant moved by a duration, and the duration between two: `datetime`, `civil_datetime`, a `date` as a `civil_datetime`, a `zoned_datetime` in its zone; `date + time`; two zoned datetimes equal by instant and zone, or as UTC | OPS-1, OPS-2, OPS-10, TYPE-2 | `temporal.h` | `temporal_tests.cpp` |
| `str` concatenation and repetition, a count below zero `numeric.out_of_range` | OPS-1 | `text.h` | `text_tests.cpp` |
| Every `convert` of the scalars: those that cannot fail, and those that can under `invalid`, with `rounding` and `nan` | OPS-4, OPS-6, OPS-7, OPS-8 | `conversions.h` | `conversion_tests.cpp` |
| A tenor, hgraph's `Tenor`: its arithmetic, count by count and checked; its order and `compare_`; its text; and the parts core's calendar candidates are written with, `business_days`, `moved`, `weekend`, `same_month` | TYPE-17, TEN-1 to TEN-6, TEN-8, TEN-9 | `tenors.h` | `tenor_tests.cpp` |
| `append`, `extend` | LIST-1, LIST-2, LIST-3 | `lists.h` | `capabilities_tests.cpp` |
| A set and a dictionary, hash containers equal where their members are; a key told apart by `==` of its type, and its hash; a strict read, `value.missing_key`; the place an assignment adds a key at; core's `get`; the deltas of a set and a dictionary, and a list's delta's properties; what a struct names of its fields, a value of an abstract struct, and what a struct inherits from; an enum's member made from a value | TYPE-3, TYPE-7, TYPE-14, TYPE-16, EXPR-3, ERR-6, OPS-2 | `collections.h` | `collections_tests.cpp` |
| The operations on an endpoint of a scalar or a nullable scalar, input or result: `valid`, `modified`, `delta_value` plain and where a `result` is taken, `last_modified`, `invalidate` of the result, `is_active`, `activate`, `passivate`; `all_valid` and a form's own delta wait for their forms | TIME-9, END-1 to END-6, ERR-14 | `endpoints.h` | `endpoint_tests.cpp` |
| A temporal list, growing and fixed: a value published whole or as its delta, and a delta that it cannot take refused; read whole, a gap of a growing list as TIME-1's table has it; `all_valid`; its delta read from hgraph's; a child of one, read, given to the operations on an endpoint, and wired | TYPE-9, TYPE-10, TYPE-16, TIME-1, TIME-7, TIME-9, END-3, END-7, END-8, ERR-14, EXPR-3, TST-5 | `list_endpoints.h` | `list_endpoints_tests.cpp` |
| `Logger` and `log`, `Clock` and its four properties | CAP-1, CAP-2, CAP-3 | `capabilities.h` | `capabilities_tests.cpp` |
| `Scheduler`: its three properties, `schedule` with a duration or a datetime under a tag or none, on the wall clock in real time and unchanged by it in simulation, `un_schedule`, `reset`; `Alarm` and its `schedule`; what may be asked of each | CAP-4, CAP-5, TIME-8 | `capabilities.h` | `scheduler_tests.cpp` |
| `get`, `has`, `put` of the global store, in a node and where a graph is built | CAP-6, FN-9 | `capabilities.h` | `capabilities_tests.cpp` |
| A value wired as a constant connection | FN-3 | `sources.h` | `sources_tests.cpp` |
| A capture, hgraph's `try_except_` of the graph of a captured call with canonical errors: its value, and its error ticked with each failure and where it failed; a failure of `start` escaping it; a `result` published whole | ERR-2, ERR-4, ERR-7, ERR-8 | `capture.h` | `capture_tests.cpp` |
| What a `ref` parameter designates: the reference its `REF` input holds, present from the start where it is given a connection, or none, which an ordinary reader of the result it is published to observes | FN-16, TIME-10, END-10 | `references.h` | `references_tests.cpp` |
| The branch of no `else`, a connection that never ticks; the branch of no `default`, which fails with the selector's value | CTRL-3, CTRL-6, CTRL-13 | `control.h` | `control_tests.cpp` |
| A nullable scalar's temporal form: its payload read checked, a published null; a nullable value held by hgraph as an atomic | TYPE-14, TYPE-15, TIME-1, TIME-7 | `nullables.h` | `structs_tests.cpp` |
| A struct's, a tuple's and an atomic struct's temporal forms: a child's view and connection, a whole read checked, `all_valid`, a publication whole and by a delta, the delta read, a field of an atomic struct, what a whole cell publishes and the text of a struct | TYPE-9, TYPE-16, TIME-1, TIME-7, TIME-9, EXPR-7, TST-9 | `structs.h` | `structs_tests.cpp` |
| A set's and a dictionary's temporal forms, hgraph's `TSS` and `TSD`: published whole and by a delta, an empty delta and a removal of what is not held no tick but where it makes one valid, two deltas in a cycle one change, an invalidation emptying it; read whole checked; an entry of a dictionary an endpoint whether or not it is held; whole trace cells published as their change; their names and text | TYPE-9, TYPE-16, TIME-1, TIME-2, TIME-7, TIME-9, END-4, END-5, ERR-14, TST-9 | `keyed.h` | `keyed_tests.cpp` |
| An enum as a value of the library and of the test host: its HGL name, its members' text, its schema with hgraph; core's `CmpResult`, `ZeroPolicy`, `Rounding`, `InvalidPolicy`, `LogLevel` | TYPE-7, TST-4, TST-5, OPS-3 | `values.h`, `traces.h` | `traces_tests.cpp` |
| The timing of a source's `yield`, and its completion | SOURCE-2, SOURCE-3, SOURCE-4 | `sources.h` | `sources_tests.cpp` |
| Whether an input ticked in an evaluation, published or lost its value, by which a node with a timer tells its wakes apart; a capability lent to a source's body for one resumption | TIME-3, TIME-9, END-4, SOURCE-1, TIME-8 | `timing.h` | `timing_tests.cpp` |
| A source's body as a coroutine, `generator<T>`: run on to each `yield` as it is resumed, in loops and branches, giving what it yields; what it lets out thrown again where it was resumed; its frame and the locals in it destroyed wherever it stopped | SOURCE-1 | `generator.h` | `generator_tests.cpp` |
| Tests registered by name and run as named, from an entry unit that includes no hgraph header; `assert`; `assert raises`; a test held to a time limit, failed once past it and its runs stopped | TST-2, TST-10, TST-11, TST-15, TST-17 | `host.h`, `testing.h` | `testing_tests.cpp` |
| Traces of scalars, nullables, tuples and lists, written dense and timed, each cell whole or a delta; their comparison with an eval's recording and the values its result held, and where two first differ; what one written out replays to an input and gives a `const fn` | TST-4, TST-5, TST-6, TST-7, TST-9, TST-16, CEVAL-2, TYPE-16 | `traces.h`, `testing.h` | `traces_tests.cpp`, `testing_tests.cpp` |
| A run's bounds, the run in simulation and in real time, its store and its recording; an eval and its failure | CRUN-1, CRUN-2, CRUN-3, CRUN-7, CRUN-8, CEVAL-1, CEVAL-4, CEVAL-5, CEVAL-6, RUN-3 | `testing.h` | `testing_tests.cpp`, `sources_tests.cpp` |
| A window of a tick count and of a duration, hgraph's `TSW` and `TSWDuration`: a publication to one; `all_valid`; its value read checked, where hgraph reads the values held below the minimum (`hgraph_window_read.cpp`, hgraph alone); its delta, the value that arrived, none at an exit alone, plain and where a `result` is taken; `removed_value` where a `result` is taken; values leaving by age, and `last_modified` at an exit | TYPE-11, END-3, END-9, ERR-14 | `windows.h` | `window_tests.cpp`, `hgraph_window_read.cpp` |
| An application's run: its store set before its graph is built, its output as text; its host, from an entry unit that includes no hgraph header, writing what became of the run, and stopping a run in real time asked down standard input or by an interrupt, as at its end; a simulation asked to stop, and a second interrupt, ending at once | UNIT-14, RUN-1, RUN-3, RUN-4, CRUN-1, CRUN-6, CRUN-7, CRUN-8 | `application.h`, `application_host.h` | `application_tests.cpp` |
