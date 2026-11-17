# HGL compiler

Status: experimental. No stage is complete and no conformance claim is made.

A compiler for HGL, written from the
[language specification](https://github.com/hhenson/hgl_spec) and checked
against the [core conformance cases](https://github.com/hhenson/hgl_core).
The specification is the authority; this project owns no language rule.

## Pipeline

```text
compile   Lexer -> Parser -> Analyser -> Optimiser -> Transpiler
format    Lexer -> Parser -> Formatter
check     Lexer -> Parser -> Analyser -> Error reporting
```

Each stage is a separate crate with one input and one output. Transpilers
consume only the analysed program, so targets can be added without touching
the front end. The first target is the
[hgraph](https://github.com/hhenson/hgraph) runtime.

## State

| Stage | State |
| --- | --- |
| Lexer | implemented: tokens and literal values under `LEX-1` to `LEX-25`, less the five rules left to later stages; catalogued diagnostic codes |
| Parser | implemented: every production of the pinned grammar to a lossless syntax tree; the parser's part of 31 rules |
| Formatter | planned |
| Analyser, error reporting | first passes: modules, parts, declarations, imports, test scopes and the names used in signatures and bodies (`hgl check`); types, phases and constants planned |
| Test runner | rejection cases: `# expect-error` annotations matched one to one against diagnostics, for every catalogued code the compiler reports (`hgl test`); tests run on hgraph (`hgl test --target hgraph`), where the target writes what they need |
| Optimiser | planned |
| Transpiler | hgraph, first slice: scalar nodes, value functions, operator candidates with a body, enums, structs as ordinary values, results and errors, graphs that call nodes and lift value functions and symbols over connections, and tests with dense evals, as Python wiring (`hgl emit`) |

Design, crate layout and planned order: [docs/architecture.md](docs/architecture.md).
Each rule and the stage that implements it: [docs/rules.md](docs/rules.md).

## Use

```text
git clone --recurse-submodules https://github.com/hhenson/hgl-compiler
cargo test --workspace
cargo run -- tokens path/to/file.hgl
cargo run -- tree path/to/file.hgl
cargo run -- check path/to/sources --modules
cargo run -- test path/to/sources --rejections-only
cargo run -- test path/to/sources --integration
cargo doc --workspace --no-deps --open
```

`hgl tokens` prints one token per line with its position, kind, text and, for
a literal, the value read from it. `hgl tree` prints the syntax tree, one
node or token per line; `--fragment declarations` or `--fragment statements`
parses source that has no `module` line. `hgl check` reads a program, every
`.hgl` file under the source roots given, and reports what is wrong with
its modules, declarations, imports, names and written types; `--modules`
lists what it found. Core's module, which every program has and every
module uses with no `use`, is the one `hgl` is built with unless the
roots hold a `module core` of their own. What an expression is,
phases and constants are not checked yet. `hgl test` checks the
program's rejection cases, each an import, declaration or test marked
`# expect-error(category, "code")`, and lists its tests.
`hgl test --target hgraph` also runs the tests on hgraph, with the
Python interpreter `--python PATH` names, which must have hgraph
installed; a test that needs a form the target does not write yet is
reported `not run`, with the form. An `integration_test` is left out
unless `--integration` asks for them or `--test NAME` names it.
`hgl emit --out DIR` writes an
accepted program as Python wiring for hgraph, and reports what the
target does not write yet. Exit status is 0 when the source is accepted
and every selected case passed, 1 when the source is rejected or a case
failed, 2 when the command could not run or a selected test was not
run.

The compiler reports nesting deeper than 100 levels and a syntax tree deeper
than 500. `--nesting-limit` and `--tree-depth-limit` raise them.

The specification and core are pinned under `spec/` as submodules; tests read
HGL from them.

Project rules: [AGENTS.md](AGENTS.md).

## License

[MIT](LICENSE), the same as [hgraph](https://github.com/hhenson/hgraph).
