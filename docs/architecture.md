# Architecture

Status: stages 1 (lexing) and 2 (parsing) are implemented, the first
passes of stage 3 (analysis): modules, declarations, imports and name
resolution, and the test runner's rejection cases. Everything marked *planned* is a plan, not
a contract; it changes as stages are built.

## Pipeline

```text
source text
    |  hgl_lexer
tokens
    |  hgl_parser
syntax tree ---------> formatter (planned) ---------> source text
    |  hgl_analyser (first pass)
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
| `hgl_source` | Source files (UTF-8 check, byte-order mark), spans, line and column lookup | implemented |
| `hgl_diagnostics` | Diagnostic model, sink, terminal rendering | implemented |
| `hgl_lexer` | Source text to tokens; literal text to values | implemented |
| `hgl_parser` | Tokens to a lossless syntax tree | implemented |
| `hgl_analyser` | Syntax trees of a program to the analysed program | modules, declarations, imports, name resolution; types, phases and constants planned |
| `hgl_runner` | The cases of a program and the outcome of each | rejection cases (`# expect-error`); executable cases listed, run with the first target |
| `hgl_cli` | The `hgl` command | `hgl tokens`, `hgl tree`, `hgl check`, `hgl test` |
| formatter | Syntax tree to source text | planned |
| optimiser | Analysed program to analysed program | planned |
| transpilers | Analysed program to one target each | planned |

A crate is created when its stage is built, not before.
[rules.md](rules.md) lists each rule of the specification against the stage
that implements it.

## Lexer

Context-free, lossless and total: the same text always gives the same
tokens; the tokens cover the file exactly, comments and whitespace included;
lexing never stops early. A token is cut by its form alone: the longest
symbol is taken whatever surrounds it (LEX-17), and a literal that runs into
a letter, digit or `_` is one malformed token, never a literal and a name
(LEX-18). Five lexical rules need context and are left to the stage that has
it:

| Rule | Left to | What the lexer keeps |
| --- | --- | --- |
| Significant line breaks (LEX-5, LEX-6) | parser | every line break as one `Newline` token, and a continued one inside a `LineContinuation` token (LEX-26) |
| A comment over several lines counts as a line break (LEX-21) | parser | the comment as one token with its text |
| Contextual names (LEX-9, LEX-10) | parser | `Ident` tokens with their text |
| Literal range under a minus (LEX-7) | analyser | `Minus` and the unsigned literal as separate tokens |
| Documentation attachment (SDOC-1) | parser | `DocComment` tokens |

The lexer settles a literal's shape. `hgl_lexer::literal` settles its value
and the rules that need arithmetic: integer and float range (LEX-7, LEX-20,
LEX-23), date, time and offset fields (LEX-11, LEX-19), zone names (LEX-12),
durations (LEX-13), and the contents of strings and text blocks (LEX-24,
LEX-25). Both read shapes through one private module, `shape`, so they agree
on where a literal ends.

### Encoding

A source is read as bytes. One that is not well-formed UTF-8 gets one
diagnostic, at the first offending byte; nothing else is reported for that
file. One leading U+FEFF is dropped when the file is loaded, so offsets and
columns count from the character after it (LEX-16).

### Line breaks

A line break is LF, CRLF or CR. Each is one line break, they mean the same
everywhere, and a file may mix them (LEX-15).
`hgl_source::line_break_length` is the single definition. The line index,
the lexer and the diagnostic renderer all use it, and tests lex the whole
specification corpus under each form. CI runs the tests on Linux, macOS and
Windows.

## Parser

Hand-written recursive descent: one function for each production, named
after it. The result is a `rowan` tree that is lossless (its text is the
source text, whitespace and comments included) and total (any tokens give a
tree; what fits nowhere is kept in an `Error` node, with a diagnostic).
`hgl tree` prints it.

An error for something a line break cut short points just after the last
token of the line. One for something the end of the file cut short points
at the end of the file, where TST-13 matches it.

The grammar functions do not build the tree. They record events ("a node
starts", "the next token", "the node ends") and one function replays them
into the tree. That keeps whitespace and comments out of the grammar, and
lets the parser try a reading and take it back by cutting the list short.

A documentation comment directly before a declaration it can document is
the first token of that declaration's node (SDOC-1). All other whitespace
and comments belong to the node around them.

### Line breaks

The cursor steps over line breaks; the grammar asks about the ones before
a token. LEX-5 (owner decision, 2026-11-14: a line break is ignored where
the statement plainly goes on) is met in four places:

| Case | How |
| --- | --- |
| Inside parentheses, brackets, angle brackets and braces that hold a list | The delimiter sets a context in which no line break counts |
| The declaration or statement is incomplete: `let a = b +` | Where the grammar needs a token next it takes it across one line break |
| The next token cannot begin a declaration or statement: `+ c`, `.name`, `=> body`, `else` | Where a statement could end, the grammar asks whether the token after the line break could begin one, and goes on when it could not |
| Two or more line breaks between two tokens, as around a blank line | Nothing is taken across them |

So a line that begins with a name, a literal, `(`, `[` or `!` begins a
statement. One that begins with `-` continues the line before: a statement
that began with it would have no effect. `\` before a line break joins the
two lines whatever the next one begins with (LEX-26): the lexer makes the
`\` and its line break one token, and the parser sees no line break there. A comment that holds a
line break counts as one (LEX-21), and a line that holds only a comment is
a line. A body's opening brace stays with its header.

An opening bracket touches what it applies to (LEX-27; owner decisions,
2026-11-14): the `(` of a call or a parameter list, the `[` of an index,
the `<` of generic parameters or arguments, the `{` of a cardinality. So
`f<T>(x)`, never `f <T>(x)` or `f<T> (x)`. The parser asks whether the two
tokens are neighbours in the lexer's list. A gap is reported and the
construct read all the same. On a later line `(x)` and `[i]` begin a
statement of their own, and in an expression a `<` apart from its name is a
comparison.

### Generic arguments or comparison

In an expression, `name <` opens generic arguments or is a comparison.
GEN-14 decides by what `name` resolves to, which only the analyser knows.
The parser therefore guesses and the analyser confirms (owner decision,
2026-11-13):

- The parser reads the tokens both ways and takes each reading back. The
  generic reading is possible when `<` directly follows the name, generic
  arguments parse, and `(` directly follows their `>` (LEX-27). The
  comparison reading is possible when the same tokens parse as expressions. When only one is possible, that reading
  is the result: `Box<i64>(7)` is a `TypedConstructor`; `a < b`, and
  `a < b > (c)` with its `(` apart from the `>`, are comparisons.
- When both are, as in `f(a<b, c>(d))` and `a<b>(c)`, the parser records
  its guess, the generic reading, in an `UndecidedConstructor` node that
  keeps its tokens.
- The analyser (planned) resolves `name`. A generic type confirms the guess;
  any other name re-reads the node as comparisons. A generic type followed
  by a `<` that cannot open its arguments is an invalid application,
  reported with no fallback to comparison (GEN-14).
- The formatter (planned) runs without the analyser, so it keeps the
  author's spacing inside an undecided node.

The language is exactly GEN-14; only the stage that applies it differs.

A bare name as a generic argument, as in `list<T, N>`, is read as a type.
Whether it names a type or a constant is also the analyser's to say.

### Readings

Where the chapters leave a choice the parser takes one reading, listed in
[hgl_spec issue 13](https://github.com/hhenson/hgl_spec/issues/13) for the
specification to decide:

| Subject | Reading |
| --- | --- |
| EXPR-10: "immediately after its token" | Position, not adjacency. `{}` is an empty configuration only when an operand follows it on the line |
| FN-10: "Parenthesize an anonymous function" | Rejected bare where it is called, indexed, reached into, or the operand of an operator; accepted bare as a whole value, such as an argument |
| `error` in a type position | The built-in type, though `named_type` matches it too. It is the one built-in type name that is not reserved (issue 13, item 15) |

The grammar's `value_type` is parsed as `type`. Which types are value types
is the analyser's to check: the types chapter places that rejection under
TYPE-12 and TYPE-14.

### Limits

| Limit | Default | Option | Bounds |
| --- | --- | --- | --- |
| Nesting of expressions, types, blocks and constraints | 100 | `--nesting-limit` | The parser's own call depth |
| Depth of the tree | 500 | `--tree-depth-limit` | The call depth of anything that walks the tree, and of freeing it |

Past either the parser reports `syntax.nesting_limit` and goes on; a tree
is never deeper than the second. An `else if` ladder and a chain such as
`a + b + ...` count towards the second only. The language sets no limit.
These are the compiler's and the caller sets them (`hgl_parser::Limits`):
configurable, starting from these defaults (owner decision, 2026-11-14).

Each level costs stack: in an unoptimised build, about 1.5 KiB for a level
of nesting and 0.4 KiB for a level of tree depth. `Limits::stack_size`
allows ten times that, and `hgl` runs a command that parses on a thread
with a stack of that size, so raising a limit needs nothing else. The
defaults fit in half of the smallest stack the compiler meets, the 1 MiB of
a Windows main thread. The analyser walks the tree the same way and is
tested to fit that half at the default limits.

## Analyser

The first stage that reads a program whole: every file of every module at
once, because its answers cross files. It reads the tree through typed
views (`hgl_parser::ast`): a function's name, a file's imports. It gives
the *analysed program*: the modules, what each declares, what each
import names, and what each name used in it is bound to.

It is built in passes:

| Pass | What it settles | Rules | State |
| --- | --- | --- | --- |
| Modules | Files grouped into modules and parts by their `module` lines | UNIT-1 | done |
| Declarations | Each module's declarations, test helpers and tests; its declaration scope and test scope; a name declared twice | UNIT-5, UNIT-12, FN-4, TST-1, TST-2, NATIVE-2 | done |
| Imports | The module and the declarations each `use` names; cycles; the operator of each `impl fn` | UNIT-2, UNIT-3, GEN-4 | done |
| Resolution | Every name in a signature, body, type and constraint, to the one binding in scope; a binding that takes a name already there; undecided generic applications | UNIT-5, UNIT-6, UNIT-7, UNIT-10, UNIT-12, FN-14, GEN-1, GEN-14, TST-2 | done |
| Forms | What the grammar admits of a declaration and a rule does not: a pack with a default, const, or before a fixed parameter; a named pack of no type pack; the hooks of a native implementation | FN-8, NATIVE-3 | done |
| Types, phases, constants | The rest of the rules | | planned |

`hgl check` says which passes ran: names are resolved, and types, phases
and constants are not checked yet.

### Resolution

Scopes nest as the source does: a declaration (its generic parameters and
parameters), each block, the pattern of a `for`, an anonymous function. A
name is looked up innermost first, then among the imports of its file,
then among its module's declarations (UNIT-10). A binding that would take
a name any of those has is rejected (UNIT-12), and the first keeps it.

A `switch` case has no scope of its own (UNIT-12): what a case declares
is declared in the block the `switch` stands in, so two cases may not
declare one name.

What the pass decides beyond finding a name:

| Subject | Rule | What is done |
| --- | --- | --- |
| A local used before its statement | UNIT-6 | Rejected. A module declaration may be used before it is written |
| A default | UNIT-7 | Reads no parameter of its declaration and no field of its struct |
| An anonymous function | FN-14 | Uses parameters, generic parameters, `let` locals and iteration bindings of what is around it; a `var`, `state`, `cache` or injected capability is rejected |
| A test helper used outside the tests | TST-2 | Rejected. A test and a helper see the helpers |
| `alias::name` | UNIT-2 | The alias is one of its file; the name a public declaration of that module |
| `name<...>(...)` that reads two ways | GEN-14 | A constructor when `name` is a generic type, comparisons otherwise; recorded for the passes that read it. The names inside are the same under either reading |
| A generic type before a comparison's `<` | GEN-14 | Rejected: an application its arguments do not complete |
| `inject` | TIME-8 | Names one of the six capabilities. Any other name is rejected, under a local code |

What a name *is* does not matter to this pass: a type, a function and a
local are found alike. Whether what was found fits where it is used is
for the passes that know types.

### Order

Nothing depends on the order the files are given in (UNIT-1). Where a
catalogue row reports the later of two, the order is the catalogue's: a
module's unnamed file, then its parts by part name; within a file, source
order.

| Problem | Where it is reported |
| --- | --- |
| Two declarations with one name | The later. The first keeps the name |
| A test helper with the name of a declaration | The helper, wherever it stands (TST-2) |
| A declaration with a name its module imports | The declaration, in whichever file the import is (UNIT-12) |
| Modules that import one another | Once: at the first import into the group, in its first module by path |

The other place is shown with the diagnostic, as "first declared here".

### Prelude

Every module sees the public declarations of core's mandatory module
without importing them (UNIT-11). Core declares none yet. Until it does, a
name that nothing in the program binds is not reported: the analysed
program lists it as left for the prelude, once for each module, and
`hgl check` prints the list. Over core's conformance cases and the
specification's complete examples the list is `delta_value`, `get`,
`info`, `invalidate`, `modified` and `valid`.

### Leaving declarations out

A file may be given with a list of declarations to leave out, and the
program is analysed as if they were not written. A test runner checks a
rejection case that way (TST-13).

## Test runner

`hgl test` runs the cases of a program (TST-12 to TST-15). The runner is
not a stage: it drives the stages. A case is one of two kinds:

| Kind | What it is | State |
| --- | --- | --- |
| Rejection | An import, declaration or named test expected to be rejected, marked with `# expect-error(category, "code")` | checked |
| Executable | A named test | listed and reported `not run`: running one needs a target |

A rejection case is checked in four steps (TST-13):

1. Every `# expect-error` comment is read. It applies to the next line
   that is not an annotation.
2. The *owner* of each is found: the import, declaration or named test
   whose lines include that line. The `module` line, a blank or comment
   line, and the `test {` and `}` lines of an unnamed context have no
   owner. An item owns a case only when its lines are its own: one that
   shares a line with another item, the `module` line or a context's
   brace owns none, and that line has no owner.
3. The program is analysed with every owner left out. This *surviving*
   source must be accepted.
4. For each owner the program is analysed again with that owner alone
   put back. Its errors and its annotations must match one to one, by
   category, code, file and line.

An annotation finds its owner by line. An error of the lexer or parser is
an owner's only when it is inside the owner: within its node, or just
after its last token for something missing there. A line is too coarse
for that: text the parser could place nowhere may stand after an item on
its line, and is still there with the item left out. Every other such
error is the surviving source's.

An error at the end of the file is on the file's last line (TST-13). It
belongs to the item that ends the file, which also owns that line; the
comments after an item the end of the file cut short are inside it. When
an unnamed test context or unplaced text ends the file, nothing owns the
line and the error is the surviving source's, whichever item's line the
file ends on.

Invalid metadata, an annotation with no owner, an unknown selector and an
error in the surviving source stop the run before any case (TST-14). A
case that does not match fails alone.

Only a catalogued category and code can be expected (TST-12).
`hgl_diagnostics::catalogue` lists them, and a test compares the list
with the pinned chapter. Every code of the lexer and the analyser is one,
so a case can expect a name or module rejection as it can a lexical one.
The parser's three local codes and the analyser's one can be expected by
no case.

| Exit status of `hgl test` | When |
| --- | --- |
| 0 | Every selected case passed, or none was selected, which the output says |
| 1 | A case failed, or the run was stopped |
| 2 | Nothing failed and a selected test could not be run: there is no target yet. `--rejections-only` selects no test |

The program is analysed once more for each rejection case. That is simple
and exact; it is revisited if a suite grows large enough to notice.

## Diagnostics

A diagnostic has a category from the specification's list, a code, a primary
location, and the rule it enforces. It may point at related places as
well, such as the first of two declarations. A code is *catalogued* (it appears in the
specification's catalogue) or *local* (the specification rejects the
condition but names no code). Only a catalogued code may satisfy
`# expect-error` (TST-12). Every lexical code is catalogued. The parser
reports the catalogue's one code for the grammar, `syntax.expected_token`,
and three local ones: `doc.unattached` and `doc.repeated` (SDOC-1), and
`syntax.nesting_limit`. The analyser reports 27 catalogued codes and one
local one, `inject.unknown_capability` (`hgl_analyser::codes`), and its
tests check each diagnostic's category and code against the catalogue.
`crates/hgl_lexer/tests/spec_tables.rs` compares the lexer's codes, reserved
words and symbols with the pinned chapters, so a pin that changes one of
them fails a test.

Where two catalogue rows cover one condition the lexer takes one reading,
listed in [hgl_spec issue 11](https://github.com/hhenson/hgl_spec/issues/11)
for the specification to decide:

| Condition | Code used |
| --- | --- |
| Text block that reaches the end of the file | `string.unclosed` |
| Empty zone component, as in `@[Europe//London]` | `date_time.invalid_zone` |
| Backslash directly before a line break in a text block | `string.unknown_escape` |

## Adding a transpiler (planned)

A transpiler is a crate that depends on the analysed-program crate and the
diagnostics crate, and on no other transpiler. `hgl_cli` selects one by
target name. The front end does not change.

First target (owner decision, 2026-11-13): Python wiring for
[hgraph](https://github.com/hhenson/hgraph) running on its C++ runtime.
Behaviour that Python wiring cannot express faithfully is added to hgraph on
its `0.9.x` branch, not approximated in generated code.

## Finding source files

The compiler is given one or more source roots. It reads every `.hgl` file
under them and groups the files by the `module` line each declares. File and
directory names carry no meaning to the compiler; keeping one module's files
together is a convention for readers. The language does not define a file
layout, so this is a compiler rule (owner decision, 2026-11-13).

`hgl check` does this. A root is a directory, searched with its
subdirectories, or a single file. Linked directories are followed. A file
reached twice is read once. A module that an import names must be under
one of the roots given: nothing is searched for elsewhere.

## Planned order

1. Lexer (done).
2. Parser for every grammar in the specification (done); formatter.
3. Analyser: modules, declarations, imports and `hgl check` (done);
   `# expect-error` matching and `hgl test` for rejection cases (done);
   name resolution (done); the checks the parser left (FN-8, NATIVE-3;
   done); types for the scalar slice.
4. Transpiler for hgraph; `hgl test` running the core conformance cases.
5. Wider language coverage; optimiser passes; a second transpiler.

## Specification pin

`spec/hgl_spec` and `spec/hgl_core` are submodules. Their commits are the
specification revision this compiler implements. Tests read HGL from them;
no test depends on a file outside this repository.

## Dependencies

An established tool is preferred to hand-written code. A tool that cannot do
the job the way the design needs is dropped.

| Crate | Used by | For |
| --- | --- | --- |
| `clap` | `hgl_cli` | command-line parsing and help text |
| `codespan-reporting` | `hgl_diagnostics` | source excerpts and underlines in rendered diagnostics |
| `petgraph` | `hgl_analyser` | the graph of imports and the groups of modules in a cycle |
| `rowan` | `hgl_parser` | the lossless syntax tree: nodes, tokens, byte ranges, traversal |
| `walkdir` | `hgl_cli` | every file under a source root, linked directories included |

Hand-written where no tool fits:

- The lexer. Durations, temporal literals, "reject the whole literal, never
  split it" and exact error spans do not fit a generated scanner's
  regular-expression table.
- Line lookup. `codespan-reporting` splits lines on LF only, so it is given
  the compiler's own line index through its `Files` trait.
- The parser. Line breaks that count by position, names that are keywords by
  position, and `name<...>(...)` read two ways (GEN-14) do not fit a parser
  generator's grammar file, and recovery after an error is easier to keep
  exact by hand. The tree it builds is `rowan`'s.

## Toolchain

`rust-toolchain.toml` pins the current stable Rust, so everyone and CI build
and lint with the same release. Dependabot proposes the next Rust release,
crate updates and workflow action updates weekly
(`.github/dependabot.yml`). No minimum supported Rust version is declared.
