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

Project rules: [AGENTS.md](AGENTS.md).
