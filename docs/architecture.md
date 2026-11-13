# Architecture

Status: stage 1 (lexing) is implemented. Everything marked *planned* is a
plan, not a contract; it changes as stages are built.

## Pipeline

```text
source text
    |  hgl_lexer
tokens
    |  parser (planned)
syntax tree ---------> formatter (planned) ---------> source text
    |  analyser (planned)
analysed program ----> error reporting (`check`)
    |  optimiser (planned)
analysed program
    |  one transpiler per target (planned)
target output
```

Rules for every stage:

- One input, one output, and the diagnostics sink. No stage reads an earlier
  stage's input.
- Total: bad input is reported and processing continues. A stage does not
  panic or stop at the first error.
- Everything carries a span, so any later stage can report against source.
- No target knowledge before a transpiler.

## Crates

| Crate | Role | State |
| --- | --- | --- |
| `hgl_source` | Source files, spans, line and column lookup | implemented |
| `hgl_diagnostics` | Diagnostic model, sink, terminal rendering | implemented |
| `hgl_lexer` | Source text to tokens; literal text to values | implemented |
| `hgl_cli` | The `hgl` command | `hgl tokens` only |
| parser | Tokens to a lossless syntax tree | planned |
| formatter | Syntax tree to source text | planned |
| analyser | Names, types, phases, const evaluation; `# expect-error` matching | planned |
| optimiser | Analysed program to analysed program | planned |
| transpilers | Analysed program to one target each | planned |

A crate is created when its stage is built, not before.

## Lexer

Context-free, lossless and total: the same text always gives the same
tokens; the tokens cover the file exactly, comments and whitespace included;
lexing never stops early. Four lexical rules need context and are left to
the stage that has it:

| Rule | Left to | What the lexer keeps |
| --- | --- | --- |
| Significant line breaks (LEX-5, LEX-6) | parser | every line break as one `Newline` token |
| Contextual names (LEX-9, LEX-10) | parser | `Ident` tokens with their text |
| Literal range under a minus (LEX-7) | analyser | `Minus` and the unsigned literal as separate tokens |
| Documentation attachment (SDOC-1) | parser | `DocComment` tokens |

The lexer settles a literal's shape. `hgl_lexer::literal` settles its value
and the rules that need arithmetic (LEX-11 to LEX-13).

### Line breaks

A line break is LF, CRLF or CR. Each is one line break, they mean the same
everywhere, and a file may mix them (owner decision, 2026-11-13; the pinned
specification does not state it yet, so no rule is cited).
`hgl_source::line_break_length` is the single definition. The line index,
the lexer and the diagnostic renderer all use it, and tests lex the whole
specification corpus under each form. CI runs the tests on Linux, macOS and
Windows.

## Diagnostics

A diagnostic has a category from the specification's list, a code, a primary
location, and the rule it enforces. A code is *catalogued* (it appears in the
specification's catalogue) or *local* (the specification rejects the
condition but names no code). Only a catalogued code may satisfy
`# expect-error` (TST-12). Every lexical code is local today.

## Adding a transpiler (planned)

A transpiler is a crate that depends on the analysed-program crate and the
diagnostics crate, and on no other transpiler. `hgl_cli` selects one by
target name. The front end does not change.

First target (proposed, awaiting the owner's confirmation): Python wiring
for [hgraph](https://github.com/hhenson/hgraph) running on its C++ runtime.
Behaviour that Python wiring cannot express faithfully is added to hgraph on
its `0.9.x` branch, not approximated in generated code.

## Planned order

1. Lexer (this stage).
2. Parser for every grammar in the specification; formatter.
3. Analyser for the scalar slice; `hgl check`; `# expect-error` matching.
4. Transpiler for hgraph; `hgl test` running the core conformance cases.
5. Wider language coverage; optimiser passes; a second transpiler.

## Specification pin

`spec/hgl_spec` and `spec/hgl_core` are submodules. Their commits are the
specification revision this compiler implements. Tests read HGL from them;
no test depends on a file outside this repository.

## Dependencies

| Crate | Used by | For |
| --- | --- | --- |
| `clap` | `hgl_cli` | command-line parsing and help text |
| `codespan-reporting` | `hgl_diagnostics` | source excerpts and underlines in rendered diagnostics |

The lexer is hand-written: durations, temporal literals and precise error
spans do not fit a generated scanner's regular-expression table.
