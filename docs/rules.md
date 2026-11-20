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

Not written yet, and reported: an enum's member made from a value
(TYPE-7), a pack, a struct, a set, a dictionary, a window, a delta,
`timezone`, `zoned_time`, and anything temporal.

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
| Reads checked when they run: `value.unset_read`, or `err` of it where the place takes a result; an index outside a list | TIME-1, ERR-14, EXPR-3 | `checked.h` | `checked_tests.cpp` |
| `add_`, `sub_`, `mul_`, `neg_`, `floordiv_`, `mod_` of `i64`, checked, with their zero policies | NUM-1, NUM-2, NUM-3, NUM-9, NUM-12 | `scalar.h` | `scalar_tests.cpp` |
| `add_`, `sub_`, `mul_`, `neg_` of `f64`; `div_` of two `f64` and of two `i64` under each zero policy | NUM-1, NUM-2, NUM-3, NUM-9, NUM-12 | `scalar.h` | `scalar_tests.cpp` |
| `eq_`, `ne_` of `i64` and `i64`, `f64` and `f64`, `i64` and `f64`: exact, and within `abs` and `rel` | NUM-2, NUM-4, NUM-5 | `scalar.h` | `scalar_tests.cpp` |
| `eq_<T>`, `ne_<T>`; `lt_<T>`, `le_<T>`, `gt_<T>`, `ge_<T>` | OPS-2, NUM-6 | `scalar.h` | `scalar_tests.cpp` |
| `and_`, `or_`, `not_` | OPS-5 | `scalar.h` | `scalar_tests.cpp` |
| `add_(datetime, duration)`, checked | OPS-1, OPS-10 | `scalar.h` | `scalar_tests.cpp` |
| `append`, `extend` | LIST-1, LIST-2, LIST-3 | `lists.h` | `capabilities_tests.cpp` |
| `get`, `has`, `put` of the global store | CAP-6 | `capabilities.h` | `capabilities_tests.cpp` |
| A value wired as a constant connection | FN-3 | `sources.h` | `sources_tests.cpp` |
| The timing of a source's `yield`, and its completion | SOURCE-2, SOURCE-3, SOURCE-4 | `sources.h` | `sources_tests.cpp` |
| Tests registered by name and run as named; `assert`; `assert raises` | TST-2, TST-10, TST-11, TST-15 | `testing.h` | `testing_tests.cpp` |
| Traces written dense and timed, and their comparison; a trace replayed to an input | TST-4, TST-6, TST-7, CEVAL-2 | `testing.h` | `testing_tests.cpp` |
| A run's bounds, the run in simulation, its store and its recording; an eval and its failure | CRUN-1, CRUN-2, CRUN-3, CRUN-7, CRUN-8, CEVAL-1, CEVAL-4, CEVAL-5, CEVAL-6 | `testing.h` | `testing_tests.cpp`, `sources_tests.cpp` |
