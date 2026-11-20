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
