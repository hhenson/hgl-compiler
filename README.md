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
| Lexer | implemented: tokens and literal values under `LEX-1` to `LEX-14`; LF, CRLF and CR line breaks |
| Parser, formatter | planned |
| Analyser, error reporting | planned |
| Optimiser | planned |
| Transpiler | planned |

Design, crate layout and planned order: [docs/architecture.md](docs/architecture.md).

## Use

```text
git clone --recurse-submodules https://github.com/hhenson/hgl-compiler
cargo test --workspace
cargo run -- tokens path/to/file.hgl
cargo doc --workspace --no-deps --open
```

`hgl tokens` prints one token per line with its position, kind, text and, for
a literal, the value read from it. Exit status is 0 when the source is
accepted, 1 when it is rejected, 2 when the command could not run.

The specification and core are pinned under `spec/` as submodules; tests read
HGL from them.

Project rules: [AGENTS.md](AGENTS.md).

## License

[MIT](LICENSE), the same as [hgraph](https://github.com/hhenson/hgraph).
