# Architecture

Status: the lexer, the parser, the analyser, the documentation stage,
the test runner and the cpp transpiler are implemented. The analyser,
test runner and cpp transpiler are partial. [rules.md](rules.md) says
what each leaves. Everything marked *planned* is
a plan, not a contract; it changes as stages are built. The target is
designed in [cpp-target.md](cpp-target.md): C++ on hgraph's own layers.

## Pipeline

```text
source text
    |  hgl_lexer
tokens
    |  hgl_parser
syntax tree ---------> formatter (planned) ---------> source text
    |  hgl_analyser
analysed program ----> error reporting (`check`)
    |            ----> hgl_doc ----> HTML reference or JSON model (`doc`)
    |  hgl_lower (in the optimiser's place)
analysed program, and the facts every target decides the same way
    |  one transpiler per target (`hgl_cpp`)
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
| `hgl_analyser` | Syntax trees of a program to the analysed program | modules, declarations, imports, name resolution, written types, and the typed bodies of value functions, nodes, sources, graphs and tests: see "Analyser" below |
| `hgl_doc` | Analysed interfaces and their attached SDOC comments to an HTML reference or JSON API model | [documentation.md](documentation.md) |
| `hgl_runner` | The cases of a program and the outcome of each | rejection cases (`# expect-error`); executable cases handed to a target to run |
| `hgl_cli` | The `hgl` command | `hgl tokens`, `hgl tree`, `hgl check`, `hgl doc`, `hgl test`, `hgl emit`, `hgl run` |
| formatter | Syntax tree to source text | planned |
| optimiser | Analysed program to analysed program | planned |
| `hgl_lower` | Lowering, in the optimiser's place: the facts every target decides the same way, read from the analysed program | implemented for what the cpp target needs: see "Lowering" below |
| `hgl_cpp` | The cpp target: the analysed program to C++ against hgraph's C++ API; its build driver is in `hgl_cli` | in part: it runs core's cases but those its list names; see "cpp" below |

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
durations (LEX-13), tenors (LEX-28), and the contents of strings and text
blocks (LEX-24, LEX-25). Both read shapes through one private module, `shape`, so they agree
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

A `.` where an operand is due begins `.member`, the member of the enum
type its position expects (TYPE-7): `side == .buy`, `/{zero: .error}`.
After an operand a `.` reaches into it, on the same line or the next, so
no statement begins with `.member`. Which enum it names is the analyser's
to say.

The name of a type is a reserved word, and a reserved word names no
parameter, local, function or type. Any reserved word is read as a member
name and as the label of an argument, where it cannot be taken for what
it reserves (LEX-9): `quote.date`, a field called `list`, `outcome.error`,
`order.if`.

### Generic arguments or comparison

In an expression, `name <` opens generic arguments or is a comparison.
GEN-14 decides by what `name` resolves to, which only the analyser knows.
The parser therefore guesses and the analyser confirms (owner decision,
2026-11-13):

- The parser reads the tokens both ways and takes each reading back. The
  generic reading is possible when `<` directly follows the name, generic
  arguments parse, and `(` directly follows their `>` (LEX-27). The
  comparison reading is possible when the same tokens parse as expressions. When only one is possible, that reading
  is the result: `Box<i64>(value: 7)` is a `TypedConstructor`; `a < b`, and
  `a < b > (c)` with its `(` apart from the `>`, are comparisons.
- When both are, as in `f(a<b, c>(d))` and `a<b>(c)`, the parser records
  its guess, the generic reading, in an `UndecidedConstructor` node that
  keeps its tokens.
- The analyser resolves `name`. A generic type, function or operator
  confirms the guess: the node is a constructor, or a call that writes
  its generic arguments (GEN-8). Any other name re-reads the node as
  comparisons. A generic type followed
  by a `<` that cannot open its arguments is an invalid application,
  reported with no fallback to comparison (GEN-14).
- The formatter (planned) runs without the analyser, so it keeps the
  author's spacing inside an undecided node.

The language is exactly GEN-14; only the stage that applies it differs.

A bare name as a generic argument, as in `list<T, N>`, is read as a type.
Whether it names a type or a constant is also the analyser's to say.

### Value types

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

Each level costs stack. In an unoptimised build the parser uses about
1.5 KiB for a level of nesting and 0.4 KiB for a level of tree depth, and
`Limits::stack_size` allows ten times that. The analyser reads the tree
the same way and uses more: about 12 KiB for a level of nesting and 6 KiB
for a level of tree depth, a chain of binary operators costing none since
it is read in a loop; `hgl_analyser::stack_size` allows near three times
that. `hgl` runs a command on a thread with the larger of the stacks its
stages ask for, so raising a limit needs nothing else, and each stage is
tested to fit what it asks for at the default limits.

## Analyser

The first stage that reads a program whole: every file of every module at
once, because its answers cross files. It reads the tree through typed
views (`hgl_parser::ast`): a function's name, a file's imports. It gives
the *analysed program*: the modules, what each declares, what each
import names, what each name used in it is bound to, and what each type
written in it means.

It is built in passes:

| Pass | What it settles | Rules | State |
| --- | --- | --- | --- |
| Modules | Files grouped into modules and parts by their `module` lines, and an application's file, headed `application name`, into a unit of its own | UNIT-1, UNIT-14 | done |
| Declarations | Each module's declarations, test helpers and tests; its declaration scope and test scope; a name declared twice | UNIT-5, UNIT-12, FN-4, TST-1, TST-2, NATIVE-2 | done |
| Imports | The module and the declarations each `use` names; cycles; the operator of each candidate, `impl fn` or `impl const fn`, native or not | UNIT-2, UNIT-3, GEN-4 | done |
| Resolution | Every name in a signature, body, type and constraint, to the one binding in scope; a binding that takes a name already there; undecided generic applications | UNIT-5, UNIT-6, UNIT-7, UNIT-10, UNIT-12, FN-14, GEN-1, GEN-14, TST-2 | done |
| Forms | What the grammar admits of a declaration and a rule does not: a pack with a default, const, or before a fixed parameter; a named pack of no type pack; the hooks of a native implementation | FN-8, NATIVE-3 | done |
| Written types | What each written type means; each declaration's signature or fields; what a type may be made of | UNIT-4, UNIT-8, TYPE-10, TYPE-11, TYPE-12, TYPE-14, GEN-2, GEN-13 | done |
| Declared types | Struct parents and fields; structs that hold themselves; enum numbering | TYPE-3, TYPE-4, TYPE-6, TYPE-7 | done |
| Generic kinds | Whether a plain generic parameter is a type or a value; each generic argument against its parameter's kind | GEN-13, GEN-14 | done |
| Signatures | What a callable's signature must hold once its types are read: a generic value parameter's default, and each constant its `requires` compares one with, is of its type; an option's name is one kind across an operator's candidates | GEN-13, GEN-6, GEN-3 | done |
| Documentation | The keys of a documentation comment's `Args:` and `Type Args:` sections, against the declaration's parameters and generic parameters; nothing recorded | SDOC-3, SDOC-6 | in part: `Requires:` and `Properties:` keys are not checked |
| Value expressions | The type of every expression of a value function and of every constant; each body written down typed | EXPR-2, EXPR-3, EXPR-5, EXPR-7, EXPR-9, NUM-1, FN-4, FN-7, TYPE-5, TYPE-7, TYPE-14, ERR-6 | in part: see "Value expressions" below |
| Phases | Composition and runtime bodies, temporal shapes, lifting | FN-2 | FN-2 done: each temporal body is classified a graph or a node, and a node's body is held to its shape. What each admits: see "Temporal bodies" below |
| Generics and operators | Generic resolution, `requires`, candidate selection | GEN-5, GEN-7, GEN-16 | in part: see "Symbols" and "Requirements" below |
| Constant cycles | A constant whose value depends on itself, directly or through what working it out reads. It reads the typed program, so it runs last | UNIT-13 | done: see "Constants" below |
| Tests and eval | Test blocks, `assert`, eval targets and traces | TST-3, TST-4, TST-5, TST-7, TST-8 | done: see "Eval" below. Running a test is the runner's |

`hgl check` says what it checked: modules, declarations, imports, names,
written types and the bodies of value functions, nodes, sources, graphs
and tests; it runs no test.

`hgl check --json ROOT...` writes one JSON object to stdout. Version 1 has
`version: 1` and a source-ordered `diagnostics` array. Each entry carries
`severity` (error or warning), `category`, `code`, `catalogued`, `message`,
`primary`, nullable `rule`, `notes` and `related`. A location has `file`
and half-open `start` and `end` positions. Both `line` and UTF-16
`character` are zero-based, measured after any leading BOM. A related entry
has `location` and `message`. File names follow the input paths, relative to
the working directory when supplied that way. Bundled core uses its compiler
display names. A clean report has an empty array. Warnings exit 0, source
errors exit 1, and invocation or I/O failures exit 2 with a message on stderr.
`--modules` is a text listing and conflicts with `--json`.

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
| `inject` | TIME-8 | Names one of the six capabilities. Any other name is rejected |

What a name *is* does not matter to this pass: a type, a function and a
local are found alike. Whether what was found fits where it is used is
for the passes that know types.

### Written types

Every type the source writes is read into one form, `Type`. Two types
are the same type when their `Type` values are equal. A struct, enum or
native type is known by the module and declaration that declare it
(UNIT-4), whichever import or alias names it, and by its generic
arguments: a constant among them by its value (`constant`), read as a
value of the type of the parameter it is given to. What is read of each
declaration is kept for the passes after: a callable's generic
parameters, parameters and result; a struct's parents and members.
`hgl check --modules` prints each signature as it was read.

What the pass checks as it reads:

| Subject | Rule | What is done |
| --- | --- | --- |
| A name in a type position | UNIT-10, GEN-2, GEN-13 | Names a struct, an enum, a native type or a generic parameter that is a type. A function, operator, local, generic value parameter, or a type pack where one type is needed, is rejected |
| A generic type's arguments | GEN-2, GEN-10 | As many as it declares, within its pack's cardinality. An argument that names a type pack counts for what the pack may hold, and is rejected only when no such length fits. A parent and a constructor are no different |
| An ordinary value type | TYPE-12, FN-1, grammar `value_type` | A set's element, a dictionary's key, a window's value, an atomic or result payload, a `const` parameter, a value function's parameters and result, a generic value parameter and a property domain hold no `rolling`, `atomic` or `signal`, at any depth. `ref` is no type: the parser rejects `ref<T>` |
| `ref` | FN-16, TYPE-12 | A mode of a node's temporal parameter, `ref low: f64`, and of its result, `-> ref f64`, recorded as `Parameter::reference` and `Signature::reference`; the type is the T after it. A value function, a value candidate, a graph, an operator and a native implementation part that is a graph take none, and no `signal` parameter takes it (`ref.position`); a part of `out` of a `ref` result is not assigned (`binding.immutable`) |
| `signal` | TYPE-12 | The whole type of a temporal function's input, or of each input of a positional pack. Not a local's annotation or an anonymous function's parameter |
| `T?` | TYPE-14, TYPE-15 | What `?` is applied to is an ordinary value type throughout, but on the whole type of a temporal function's input, where `rolling_window<f64, 5>?`, `atomic<Quote>?` and `signal?` are inputs that may be omitted, and of a struct field, which is then optional. In a result, a local's annotation or a part of another type they are rejected. A set's element and a dictionary's key take no `?` |
| `atomic<S>` | TYPE-9 | `Type::atomic` forms it: S itself where S is no composite, a scalar, an enum, a native type, a `result`, an `error`, a delta that is a type of its own, or a nullable of one; `Type::Atomic` for a tuple, a struct, a list, a dictionary, a set, a nullable of one, and a type not yet known. It is formed where the type is written and again where a generic parameter is replaced, and an `atomic<T>` given a value of a type that is its own atomic says what `T` is |
| A list's size | TYPE-10 | A count that is not negative, or `unbounded` |
| A window's sizes | TYPE-11 | Each a tick count or a duration, both of one kind, in bounds; a constant of a module and a duration and an `i64` multiplied, in either order, are followed to their value, and a constant of another type is of the wrong kind. A minimum equal to the maximum is the window with none written |
| A default | UNIT-8 | Any parameter but a pack may have one. A temporal input with one keeps its temporal type; whether the default can be wired as a constant of that type is for the pass that evaluates constants |
| A cardinality | GEN-10 | `{n:m}` has m at least n |

A size and a generic value argument are constant expressions, and no
expression has a type yet. Each is read with `constant::read`, given the
type its place expects: the type of a generic value parameter is read
from how it is written (`constant::written_type`), a scalar, an enum, a
generic parameter, or a nullable, list or tuple of those. An argument of
another type than its parameter's is `type.mismatch`, and one that is no
compile-time constant, or one not worked out, `generic.argument_constant`.

A type the pass cannot know, because nothing binds its name or
it has been reported, is `Unknown`. Nothing more is reported of it
(ERR-9), and what holds it is still checked.

Each of these rejections has a row of the catalogue, and is reported
under its code.

### Declared types

The declarations are then read against one another:

| Subject | Rule | What is done |
| --- | --- | --- |
| A parent | TYPE-3 | An abstract struct, listed once. A concrete struct, an enum, a native type or a generic parameter is rejected |
| Inheritance | TYPE-6 | No struct is its own ancestor. A group that inherit from one another is reported once, at its first struct |
| A field of a struct and of a parent | TYPE-4 | The same type, and so the same optionality, the parent's generic arguments standing for its parameters |
| A field two parents give | TYPE-4 | The same type. One field reached by two paths is one field |
| A default of null | TYPE-4 | Only on an optional field: one whose type is written `T?`, `atomic<Node>?` included, or an `atomic<...>` of a nullable type, `atomic<T?>`. For `name = null` the type is the one the parent gives the field |
| `name = value` | TYPE-4 | Overrides the default of a field a parent declares |
| A member's name | TYPE-4, TYPE-7 | One member of a struct or enum has it |
| A struct that holds itself | TYPE-6 | Through a field with a value that ends the recursion: an optional one, `Node?`, `atomic<Node?>` or `atomic<Node>?`, or one that holds the struct only as elements of a collection that may be empty, `atomic<list<Tree>>`: an unsized list, a list of size zero, a set, a dictionary. Its default, in its struct or one below, is `null` or `[]`, and a null default it is declared with is kept by the structs below. Inside an `atomic<...>` a collection is part of one snapshot; a path through a collection outside one is rejected. The field gives a struct of its cycle each generic parameter whole, not a type built from one |
| An enum's numbers | TYPE-7, LEX-7 | From zero, or the number written, then one more for each member. Each is an `i64` no other member has |

A struct holds the structs its fields hold: its own fields and those it
has from its parents, with the arguments it gives each parent in place
of the parent's parameters. A field whose type is an abstract struct
holds every struct that inherits from it, since its value is one of
them: each that may be the application written, so `struct Child :
Base<i64>` is held by a `Base<i64>` and not by a `Base<str>`. A
parameter of the child stands for one thing wherever the child writes
it, and is matched inside a type: `struct Child<U> : Base<U, U>` is not
held by a `Base<i64, str>`, nor `struct Child<U> : Base<list<U>>` by a
`Base<str>`. Constant arguments are compared by what they come to: a
window with no minimum written has its maximum for one (TYPE-11), and
`1.0` is `1.00` (LEX-20). One type pack a child passes on stands for
the arguments between those before it and those after, as many as its
cardinality admits (GEN-10); where two are passed on, or the field's
type passes one on, the child may be held. A
recursive field stays within its module: a struct of another module is
reached back only through an abstract struct declared there, and that
is rejected. A field holds every struct its type names,
and what a generic struct among them holds once its arguments stand for
its parameters: with `struct Boxed<T> { values: list<T> }`, a
`Boxed<Node>` holds `Node`s as the elements of a list. Structs that hold
one another are found as the strongly connected groups of that graph,
as modules that import one another are.

A fault in a struct's own field is reported at the field, and so is one
in a parent's field whose type holds the child whatever the arguments.
One that is in what a struct gives a parent, as in `struct Child :
Base<Child>` where `Base<T>` has a field of type `T`, is reported where
the struct names the parent.

The fields of each struct are worked out once, from those of its
parents, and a field that reaches a struct by two paths is listed once.
A ladder of structs that each inherit from the two before it would
otherwise double the list at every step.

An enum's members are numbered before the types are read, since a
constant a type writes may name one (`Tagged<Mode(1)>`). Its number is
read as an `i64` constant, a module's constant among them. One that is
not worked out is not known, and neither are those numbered from it.

2^63 is an integer only as the operand of a minus, the least `i64`
(LEX-7). The lexer admits the literal; the analyser rejects it anywhere
else, under the catalogue's `number.integer_range`.

Each has a row of the catalogue: `struct.parent`,
`struct.duplicate_parent`, `struct.inheritance_cycle`, `field.conflict`,
`field.not_inherited`, `field.duplicate`, `field.recursive`,
`field.null_default`, `enum.value`, `enum.duplicate_value`,
`enum.overflow` and `enum.duplicate_member`.

### Generic kinds

A generic parameter is a type, a type pack or a value (GEN-13). `...Ts`
and `const N: V` say which. A parameter written plain, `T`, is what its
signature uses it as:

| Use in the signature | Makes it |
| --- | --- |
| A type: of a parameter, a result, a field, a part of one, after `...` | A type |
| In a size or a default | A value |
| An argument of a generic type | What that type's parameter is, once that is settled |
| In a `requires` clause | Nothing yet: the pass that reads constraints says |

Uses that disagree are rejected. A struct's parameter that nothing in
its fields or a constraint names is rejected too: a struct is always
given its arguments, so nothing later says what it is. A function's is
left unsettled where it is declared, for a call to supply; a call that
supplies no type for it is rejected, which is for the pass that reads
calls. A use in a body does not settle a kind: the rule infers it "from
signature positions and constraints".
Neither does being handed on to a parameter that nothing settles: in
`struct Node<T> { next: atomic<Node<T>?> = null }` nothing says what `T`
is. That is reported at the struct, and not again at each function
that is given one.

A use as an argument depends on another declaration, and structs name
one another freely. So before any type is reported on, the structs are
read again and again, each reading using the kinds the one before
settled, until one settles nothing more. A kind once settled stays, so
this ends after at most as many readings as there are plain parameters.
What those readings report is thrown away; the pass proper reports it
once, with every kind known.

With the kinds known, each generic argument is read for the parameter
it is given to (GEN-14): a value where a type goes, or a type where a
value goes, is rejected. A lone name is read as a type by the parser;
here it is a constant when it names a value parameter, or a plain one
given where a value goes. A name `each T in ...` binds is a type.

Each has a row of the catalogue:

| Code | Rejection |
| --- | --- |
| `generic.kind_conflict` | A generic parameter used as a type and as a value |
| `generic.kind_unresolved` | A struct's generic parameter that no field or constraint names |
| `generic.argument_kind` | A generic argument of the other kind than its parameter |


### A const option in a constraint

A candidate says in `requires` which values of a const option it is for
(GEN-6): `requires zero == .error`, `requires zero in {.ieee, .nan}`.
The parser cannot tell that `zero` from a type, as in `T == i64`, and
reads a lone name among a constraint's operands as a type. This pass
knows what the name is bound to, and confirms or corrects the guess:

| The name is | Read as |
| --- | --- |
| A parameter marked `const`, an operand of `==` or `in` whose other operand holds no type | The option: a value, and no type is recorded |
| A parameter of a value candidate that its operator marks `const` or does not declare, likewise | The option: a value candidate writes it with no marker, and one its operator does not declare is the candidate's own |
| Any other parameter, an option anywhere else in a constraint, or one compared with a type | `type.not_a_type`, with a note of what a constraint may name (GEN-3: it "cannot inspect temporal values") |

Nothing is selected by the option yet: comparing its value at a call is
for the pass that selects candidates.

### Temporal bodies

The body of a temporal function describes one of two things (FN-2), and
`Module::phases` records which for each `fn` and each `impl fn`:

| The body holds | It describes | The function is |
| --- | --- | --- |
| `state`, `cache`, `inject`, `start`, `when`, `stop` or `yield`, anywhere in it | Runtime | A node |
| None of those | Composition | A graph |

"Classification precedes phase checking, even if the trigger occurs in
an invalid nested position", so the pass looks only at which constructs
a body holds, and reports nothing. A conditional or a loop does not
make a body runtime (FN-3).

A node's body is then held to its shape (FN-2, owner's decisions of
2026-11-16 on hhenson/hgl_spec#53): "its `state`, `cache`, and `inject`
declarations and then its `start`, `when`, and `stop` hooks, and nothing
else".

| Written | Rejected as | Rule |
| --- | --- | --- |
| A declaration or a hook inside another block | `node.nested` | TIME-3, TIME-5 |
| `state`, `cache` or `inject` after a hook | `node.declaration_order` | TIME-5 |
| A second `start`, or a second `stop` | `node.duplicate_hook` | TIME-5 |
| Any other statement of the body | `node.statement` | FN-2 |
| `inject out` in a function that declares no result | `inject.out_without_result` | TIME-7 |
| `state`, `cache`, `start`, `when` or `stop` in a `const fn` | `node.in_value_function` | FN-1 |
| `inject` in a `const fn`, or in the contract of a native one | `node.in_value_function` | FN-9, NATIVE-3 |
| `inject`, `state`, `cache` or a hook in an anonymous function written in a node | `node.in_value_function` | FN-9, FN-3 |
| A temporal input read in a `cache`'s initialiser | `cache.input_read` | TIME-5, TIME-6 |
| A connection in a graph given to a value candidate's option by a symbol's configuration, `a =={abs: tolerance} b` | `connection.as_constant` | EXPR-14 |

A value function injects no capability (owner's decision of 2026-11-16
on hhenson/hgl_spec#57): what it needs of one is its argument, a value
the node reads or the capability itself, given to a parameter of the
capability's type (TIME-8).

A named test is a composition block with assertions (TST-3), and the
same pass holds it to that: a `state`, a `cache`, an `inject`, a hook, a
`yield` or a `return` in it, at any depth but inside an anonymous
function, is `test.statement_phase`. `assert raises("code")` names one
execution code of the catalogue, as a string literal written out
(TST-10, `catalogue::EXECUTION_CODES`); anything else is
`test.raises_code`.

A test's block is then read as value code (`Typer::test`, into
`Module::test_bodies`), as an application's statements are (TST-3,
UNIT-14): a test has a name and neither a parameter nor a result. It
calls `const fn`s only, a temporal call there being `call.role`; an
operator selects among all its candidates, a value one outranking a
temporal one of the same parameter types, and a temporal one selected is
`call.role` too (GEN-5, the typer's `value_code`); nothing is lifted. The call
an `eval` or a `run` takes is read as composition is. `assert` is a
statement there (TST-4): its condition is a `bool` (`type.mismatch`).
The block of an `assert raises` is read with the test's scope.

### Eval

`eval target(...)` is read in a named test (`Typer::eval`, TST-5). Its
call is of a function of a module, the temporal definition where the
name has both (FN-4); a call of anything else, an operator among them,
is `eval.target`, and no call after the word is the parser's
`syntax.expected_token`. The call is read as any call of
that function is, so positional and named arguments, defaults, generic
arguments written after the name, and a generic target's parameters are
handled as any call's are. The one difference is what a parameter
takes: a `const` parameter takes configuration, an ordinary value, and
every other parameter a trace. The run's configuration is written in
braces after the word, `eval{length: 4s} target(...)`, as an operator's
is, and read first (RUN-6, EXPR-13): each entry names a property of
core's native `RunConfig` (`argument.unknown`, `argument.duplicate`) and
is value code of its type (`Typer::run_configuration`). The typed eval
is `ExprKind::Eval`, with its call, its entries, and, in an integration
test that injects `run_config`, the local the fields it leaves out are
read from (`inherited`).

| Form | Rule | Read as |
| --- | --- | --- |
| A trace's type | TST-3, TST-5 | `Type::Trace`, with the type of a cell and the style. It is a type of this compiler's: source writes none. A cell is one cycle's publication, the `delta<T>` of the parameter's type for an input and of the target's result for what the eval gives |
| `[7, _, 9]`, `[0s: 7, 2s: 9]` where a trace is expected | TST-5, TST-7 | `ExprKind::Trace`: dense, or timed. Each cell is read as the cell's type. A timed trace's keys are all durations or all datetimes, constant keys strictly increase (`trace.key_order`), and it has no `_` (`trace.placeholder`) |
| A `_` anywhere else | EXPR-8 | `trace.placeholder` |
| An eval's traces | TST-7 | Of one style, the expectation included (`trace.style`, at the later trace) |
| An eval of a target with no result | TST-8 | A statement. Bound or compared, it is `eval.no_result` |
| `eval target(...) == [...]`, `seen != other` | TST-4 | `ExprKind::TraceCompare`, a `bool`: the harness's own comparison, selected through no operator. The side that gives a trace says what the other, a sequence written out, is read as |
| A trace, or a value that holds one, given to a field or to a temporal function | TST-3 | `trace.escape`, reported where the construction or the call is finished (`Typer::traces_given`), so that a generic place and a trace wrapped in a sequence are held to it too |
| A trace given to a `const fn` | TST-3, TST-16 | The list of its publications, a `list<tuple<datetime, delta<T>>>`: `Publications` in the tree, held to the parameter's type |

Not read: an `eval` outside a named test; a trace that is the value of
a block or an `if`, where it is compared; a generic target whose
parameter is given structural cells.

### Applications

An application's file is headed `application name` (UNIT-14). The
modules pass makes it a unit of its own, `Module::application`, so what
it declares is listed, named and typed as a module's is. The passes add
what an application has that a module has not:

| Check | Rule | Code |
| --- | --- | --- |
| Its path is no other file's. The first file of a path is the unit; each later one is reported and left out | UNIT-14 | `application.duplicate_path` |
| It exports nothing | UNIT-14 | `application.export` |
| No `use` names one. The names of such a line are bound to nothing | UNIT-14 | `application.import` |
| It has one run statement | UNIT-14 | `application.run_count`, at the application's path for none and at each statement after the first |
| Its run statement follows its declarations, and a run statement stands in no module | UNIT-14 | `run.position` |

The run statement is kept on its unit, `Module::run`, and read by the
passes that read a test: its names are resolved in the unit's scope, the
types it writes are read, and `Typer::run_statement` types it as
`hir::Run` in `Module::run_body` (RUN-1). Its call is of an entry, a
temporal function with no temporal parameters; anything else is
`run.entry`. The call is read as a graph's call of the entry is. It
gives no configuration (`run.configuration`): what starts the
application supplies it, over the defaults of an `inject
run_config{...}` among the statements before it (RUN-6), whose entries
are an eval's and are the `Stmt::Inject` of `Module::store_body`.
Store values and the entry's arguments may read this `run_config`. The
entry's arguments carry its binding in `Run::locals` and read no other
capability.

| Check | Rule | Code |
| --- | --- | --- |
| `inject run_config` in a function, in a test that is no integration test, or after an integration test's other statements (the phase pass) | RUN-6 | `inject.position` |
| Braces after any capability but `run_config` | RUN-6 | `inject.configuration` |
| A key of the global store written out, used with two types in one program (`store_keys`, once every body is typed); a `get` of `T` beside a key of `T?` is not judged, awaiting hhenson/hgl_spec#165 | TIME-8 | `store.key_type` |

### Integration tests

`integration_test name { ... }` is a test (TST-17): `Test::integration`
marks it, and everything a test is held to holds of it. `run` is a
reserved word (LEX-10), and the parser reads `run target(...)` wherever
an operand is due, as `eval target(...)` is. It is read as an eval is,
`ExprKind::Eval` with `real_time` set, and in a test that is no
integration test it is `run.position` (RUN-3). An integration test may
inject `run_config` before its other statements (TST-17): `hgl test`
supplies nothing to it, so its braces' defaults are its configuration.

A source, a temporal function whose body has a `yield`, is a node of
another shape (SOURCE-1), and is held to it:

| Written in a source | Rejected as |
| --- | --- |
| A parameter that is not `const` | `source.input` |
| `when`, anywhere | `source.when` |
| `var`, anywhere; `inject` of `out`, `scheduler` or `alarm` | `source.statement` |
| With a declaration or a hook: a statement that is no declaration, `start`, block or `stop`; a second block; `start` after the block or `stop` before it; a `yield` in a hook | `source.statement` |
| A declaration after a hook or the block; a declaration or hook inside a block; a second `start` or `stop` | As in any node: `node.declaration_order`, `node.nested`, `node.duplicate_hook` (TIME-5) |

A source with no declaration and no hook has a body of statements, and
only the first three rows apply. The block is `{ ... }`, a `while` or a
`for`.

A source's body is read into the same typed form (`Typer::source`). Its
declarations and its hooks are read as a node's are. Every other
statement is read as value code, with three things of a source's own:

| In a source | Read as | Rule |
| --- | --- | --- |
| `yield time: value` | `Stmt::Yield`: the value is of the result's type, and is not a capability. What the time may be is proposed, and is not judged | SOURCE-1, SOURCE-2 |
| `return` with no value | Ends the source. One with a value is `source.return_value` | SOURCE-4 |
| A `yield` inside a block or an `if` whose value is used | `yield.in_value` | SOURCE-4 |
| `while { }` with no condition | A loop with none | CTRL-15 |

A temporal function called there is `call.role`, and a `capture` is
`capture.phase`. A result taken as its value raises a failure in the
source's phase (ERR-5, ERR-15).

A node's body is then read into the same typed form as a value
function's (`hir::Body`): a `state` or a `cache` is a local that is
assigned as a `var` is, each injected capability is a local, and each
hook is a `Stmt::Hook` with its condition and its statements.

| In a hook | Read as | Rule |
| --- | --- | --- |
| A temporal parameter | A value of its declared type, as in a value function | FN-3 |
| `valid(value)`, `modified(value)`, `scheduled()` | A bool: an operation on the endpoint, which is the language's own and no call of a function | TIME-9 |
| `delta_value(value)`, `last_modified(value)` | The endpoint's `delta<T>` and its retained publication time as a `datetime?`, respectively. Where its place takes `result<delta<T>, error>`, `delta_value` is of that result, the read held as `ExprKind::CheckedDelta` (ERR-14); a plain read of the delta of a window of a duration is `payload.unguarded`, found once a node's body is typed (`Typer::plain_cycle_reads`) | TIME-9, END-3, END-6, ERR-14, TYPE-11 |
| `removed_value(window)` | Core's native function, typed by its declaration, `endpoint<rolling_window<V, Max, Min>>` to V; where its place takes `result<V, error>` the place says what V is, and the read is a `CheckedDelta`; a plain read of it is `payload.unguarded` | TIME-9, TYPE-11, ERR-14 |
| `is_active(input)` | A bool for a direct temporal input. In a guard it adds no subscription or validity requirement | TIME-9, END-5 |
| `valid()`, `valid(a, b)`, `valid([])` | A selector of a `when` header: every temporal input, those named, none | TIME-3 |
| No `modified` selector | Subscribes to every temporal input. An active invalidation can wake the node and reach a handler whose other conditions hold | TIME-3 |
| `modified()`, `modified(a, b)` | Subscribes to every temporal input or to the named inputs, respectively. At least one selected input must have published this cycle | TIME-3 |
| `modified([])` | Subscribes to no temporal input and selects a scheduler or alarm wake | TIME-3 |
| `modified([])` in a body, under a negation or in a disjunction | A bool: true in the body of the handler that selects it, and elsewhere where no temporal input is modified | TIME-3 |
| `out` | The node's result: read, or assigned | TIME-7 |
| `return`, `return value` | Ends the evaluation; the value is of the result's type and is published | FN-7 |
| A call | Of a value function or an operator's value candidate | FN-3 |

What is rejected there:

| Written | Rejected as | Rule |
| --- | --- | --- |
| An endpoint operation given a state, a `const` parameter or an expression | `endpoint.argument` | TIME-9 |
| A sequence that names inputs, `valid([a, b])`, or any sequence given to another operation | `endpoint.argument` | TIME-3, TIME-9 |
| A second input given to any operation but the selectors `valid` and `modified` | `argument.unexpected` | TIME-9 |
| `out` read or written in `start`; written, or `invalidate(out)`, in `stop`; `return value` in either | `out.lifecycle` | TIME-7 |
| A temporal function called in a hook | `call.role` | FN-3 |
| `scheduled()` in a node that injects neither `scheduler` nor `alarm`: in a hook, a `state`'s or a `cache`'s initialiser, or a source's body | `inject.no_scheduler` | TIME-9, SOURCE-1 |
| `scheduled()` in a value function, a graph or a test, which is no node | `scheduled.outside_node` | FN-9, TIME-9 |
| A capability retained: the value of a `state` or a `cache`, an entry of a tuple, list, set, dictionary or struct, returned, or published: `(clock, value)`, `return clock` | `capability.as_value` | TIME-8 |
| `alarm` and `scheduler` injected in one node | `inject.alarm_and_scheduler` | TIME-8 |
| `scheduled()` in `start` or `stop` | `scheduled.lifecycle` | TIME-9 |
| A wake asked of the scheduler or the alarm with a duration the compiler sees, a literal or a constant, that is not more than zero, but a zero in `start`: `schedule(scheduler, 0s)` in a `when`. `Typer::wake_durations` judges it where the call is finished | `schedule.duration_bounds` | TIME-8 |
| The run's own key, `"run.output"`, written out as the key of a `get`, `has` or `put` on the global store. `Typer::store_keys` judges it where the call is finished; a key that is a `const` parameter is known when the graph is built, and is not judged | `store.reserved_key` | TIME-8 |

`modified([])` subscribes to no input, so only the node's scheduler or
alarm wakes the handler (TIME-3). It is that selector alone or as a
top-level conjunct of its header; read as a bool, in a body or under a
negation or a disjunction, it sets no subscription, and is not warned
of. Where the node injects neither, or its
`start` does not use the one it injects, the selector is warned of,
`node.no_activation`, and the source is accepted. Any use in `start`
counts, by the capability's name or by a local it is bound to: what a
function given it does is not followed. The hooks may be written in any
order, so the warning is given once the whole node is read. A node that
is dormant by design acknowledges it with
`@suppress_warning("node.no_activation")` before the function (ERR-10).

A `when` reads an input's payload only where the input is valid
(TIME-1). Its header says which are (TIME-3):

| Header | Admits | A read of an input in the body |
| --- | --- | --- |
| No `valid` selector alone or as a top-level conjunct: `when { }`, `when modified() { }`, `when !valid(a) { }` | Every temporal input: the default | Free |
| `valid(a)`, `valid(a, b)` | Those named | `a` free; any other needs a guard |
| `valid([])` | None: manual admission | Every one needs a guard |

A guard is `valid(input)` or `modified(input)`, on the input by its
name, proved where the read stands (TIME-4) by the paths a null test is
followed along: the branch it holds for, the right operand of the `&&`
it guards, a header's top-level conjunct, `when modified(a) &&
valid([])`, and from a branch that returns to the end of its block.
`modified(a, b)` guards neither. A read with none is
`payload.unguarded`. A saved bool proves nothing. A header that is
itself rejected says nothing sure of what it admits, and its body's
reads are not judged.

A struct, a tuple or a fixed list read whole, an input or a child
named when the program is compiled, `return quote`, `rows[0]`, is
admitted by its own `all_valid` alone (TIME-1, TIME-4), in a header,
`when all_valid(quote)`, or as a guard; neither the default admission,
`valid(quote)`, nor `all_valid(rows)` admits it. A dictionary and an
unsized list read whole need `valid`, and one at an index known when it
runs, `rows[i]`, is checked then (ERR-14).

The analyser records what the header asks in the `hir::Header` of its
`Stmt::Hook`, read from the typed condition: the inputs whose tick wakes
the handler, what it requires valid (the inputs it admits, and `out`
where `valid(out)` names it), whether a wake of the scheduler or alarm
runs it, and which conjuncts of its condition are checked when it runs.
A transpiler reads it and works none of it out again. The analyser
judges the handler's reads, the header's own among them, by the same
record: the condition is read once to learn it, that reading is taken
back, and the condition is read again under what the record admits.
Core's `valid` is the selector however it is named, `c::valid(a)` with
`use core as c` (TIME-9). A configured `valid(a) &&{cfg} b` is the call
of the candidate of `and_` its configuration selects, which is not read
yet: nothing under it is a top-level conjunct, and the default stands
(TIME-3 does not say in words whether such a `&&` is a conjunct).

A `state`'s initializer reads no temporal input (TIME-5): one that does
is `state.input_read`.

A capability is a value of a type core declares (TIME-8): `inject clock`
binds a local of type `Clock`, which `Program::capability_type` finds in
core by the capability's name with a capital. It is used and not
retained:

| Written | Read as |
| --- | --- |
| `clock.evaluation_time` | A property, of the type the native type declares for it (NATIVE-6) |
| `let bound = clock`, `stamped(clock, value)` | Uses: a `let`, and an argument of a parameter of its type |
| `(clock, value)`, `[clock]`, `["at": clock]`, `Kept(value: clock)`, `held = clock` of a `state` or `cache`, `out = clock`, `return clock` | Retained: `capability.as_value` |
| `boxed(clock)` where the parameter is written as a generic parameter or `any` | `capability.as_value`: such a parameter takes any type, and no capability |
| `Clock` written anywhere but as the whole type of a parameter of a `const fn` or a native value function | `capability.as_value` |

A parameter of a capability's type in a value function is held to the
same rule as the injected local it was given. Where a program's own
`module core` declares no capability type, a capability has no type and
its properties none, and what is retained is still reported.

A native type's properties are kept as `Detail::Native`, each a name
and an ordinary value type, and a member access on a value of the type
reads one. None is assigned: `name.property = value` on a local of a
native type, or on a capability, is `binding.immutable`. A native type is
not constructed: a call of its name is `call.not_callable`. A native
function is private unless its declaration is exported (NATIVE-1), as a
function is; the part that implements it does not repeat `export`.

`endpoint<T>` is `Type::Endpoint`, admitted only as the whole type of a
native value function's parameter (TYPE-12, `endpoint.position`). An
argument given to such a parameter is an endpoint and no value (TIME-9):
`Typer::expression` reads a place that expects an endpoint as
`ExprKind::EndpointOf`, the input named, with no read of its payload. It
is a temporal input of the node by its name, or `out`, in a hook;
anything else is `endpoint.argument`, and so is any argument in a graph
or a value function, which have no input of their own to give. A
generic `T` under the endpoint is inferred from what the input holds.

The endpoint operations are core's, declared as such functions,
`delta_value` and `removed_value` among them. In a node's hooks
their names are read as the forms they are, a selector or a guard among
them, where core's module binds the name or nothing does: a node reads
the same either way.

Not read yet in a node: what `modified` subscribes to.

A graph's body describes composition, and is read into the same typed
form. Every value in it is one of two things (FN-3, EXPR-3, EXPR-4):

| Value | Is |
| --- | --- |
| A temporal parameter | A connection |
| A `const` parameter, a literal | An ordinary value, known while the graph is built |
| A call of a temporal function, or of an operator's temporal candidate | A connection |
| A call of a value function or a value candidate with a connection among its arguments | A connection: the call is lifted (FN-6) |
| The same with ordinary values alone | An ordinary value: the call is made while the graph is built |
| A call of an operation the graph requires (GEN-7), given no connection | Not settled: a temporal candidate would make it a connection and a value candidate a value, and which it is is selected where the generic parameters are known. It is not judged where a constant is required |
| A local | What its first value is; `hir::Local::category` says which of the three |
| A symbol, `&&`, `\|\|` and `!` among them; a construction, a sequence, a result form, an element or a field; with a connection among its parts | A connection, as a call of its identity given one is |

Each expression's is recorded on it (`hir::Expr::category`), and whether a
call is lifted on the call.

The types are the ones a value function has, so a symbol and a call
select as they do there, with two differences (FN-4, GEN-5): a call
takes the temporal definition of a name where there is one, and an
operator's call selects among its temporal candidates too, one of which
outranks a value candidate of the same parameter types. An ordinary
value given where a connection is expected is wired as a constant, and
nothing is reported of it. A connection given to a `const` parameter is
`connection.as_constant`; one given to a generic value parameter is
`generic.argument_constant`, and one that is a tuple's index
`tuple.index`.

A connection is given to a function's parameter, and returned by the
graph, as the type it is: a `ref` parameter of T takes a connection of
T, which it designates, and a `ref` result is a connection of T to
whoever reads it (FN-16); any connection is given to a `signal`
(`signals`). A generic parameter is inferred from the connection's
type, at a `ref` parameter as at any other (GEN-11). A `?` that marks
an input that may be omitted is no part of either (TYPE-14,
`Type::omission_removed`); the cpp target writes a lifted input by
the same method, so the stages cannot disagree on it.

In a node's hooks a `ref` parameter, a child of one, and `out` of a
`ref` result are endpoints: the endpoint operations take them, and a
read of the value or the `delta_value` of one is `ref.read` (FN-16). A
`return` to a `ref` result, and `out =` of one, names a temporal input
of the node, `ref` or not, or a child of one, and is read as the
designation it is, `ExprKind::EndpointOf` or `ExprKind::ChildEndpointOf`
of the result's type: no payload is read and nothing needs admitting.
A value there is `type.mismatch`.

An `if` in a graph is read as a value function's is, and then told
apart by what its condition is (CTRL-1, CTRL-13; owner's decisions of
2026-11-16 on hhenson/hgl_spec#61):

| Condition | The `if` | With no `else`, its value used |
| --- | --- | --- |
| An ordinary `bool`: a `const` parameter, a literal | One branch is composed while the graph is built | A `T?` where the branch is an ordinary value; a connection of `T` where it is one |
| A connection of `bool` | Each branch is a child graph, selected at runtime | A connection of `T` that never ticks while the condition is false (CTRL-3) |
| Any other type, `bool?` included | `type.mismatch` | |

With both branches it has their one type, and is a connection when its
condition or either branch is one.

A `var` of a graph keeps the category of its first value (CTRL-14,
EXPR-3, EXPR-4):

| Written | What is done |
| --- | --- |
| `var level = 1.0` then `level = price` | `binding.category`: a connection to a `var` that holds an ordinary value. The reverse likewise |
| `var level = 1.0` then, `rising` a connection, `if rising { level = 4.0 }` | `binding.category`: its value would be chosen at runtime |
| `var chosen = price` then `if rising { chosen = scale(price) }` | One connection after the `if` |
| `var chosen: f64`, assigned in every branch | Takes the category of its first assignment. Assigned in one branch only and then read: `binding.unassigned` |
| A `var` declared in the branch | The branch's own |

A `return` with a value in a branch selected at runtime makes the
graph's result the continuation's where the branch is not selected, so
a graph that can then reach its end with no value is `return.missing`.

A `switch` and the loops of a graph are told apart the same way
(CTRL-15):

| Written | What is done |
| --- | --- |
| `switch` over an ordinary value | One case is composed: read as in a value function |
| `switch` over a connection | Each case is a child graph selected at runtime: an assignment or a `return` in a case follows the rules of a branch selected at runtime |
| A label that is a connection | `connection.as_constant` |
| A label that is no compile-time constant: a `let`, a `const` parameter, a call (UNIT-13, GEN-8), here and in a value function or a hook | `switch.label_constant` |
| `for item in items`, `items` an ordinary list, set or dictionary | Unrolled while the graph is built: ordinary values bound, and a `var` may be carried from pass to pass |
| `items` a connection of `list<T, N>` | Composed `N` times: `item` is a connection of `T` |
| `items` a connection of `list<T>`, `set<T>` or `dict<K, V>` | One child for each member. `item` is a connection of `T` for a list and the member, an ordinary `T`, for a set; `key` is an ordinary `K` and `value` a connection of `V`. An assignment to a `var` declared outside the loop, and a `return`, are `loop.carried` |
| `items` a connection of a tuple, or of any other type | `type.mismatch` |
| `while condition`, the condition a connection | `connection.as_constant`: the condition is tested while the graph is built |
| `while { }` | `while.condition`, as in a value function |

Not read yet in a graph: whether a graph that returns nowhere under a
temporal condition gives a result on every path; the traversal forms
`elements`, `keys`, `values` and `items` (CTRL-8).

### Constants

A constant, `const name: T = expression` (UNIT-13), is a declaration of
its module. What is read of it:

| What | Where | How |
| --- | --- | --- |
| Its type | `Detail::Constant` | The type it is written with, an ordinary value type (`type.not_ordinary`) |
| Its expression | `Module::constants` | Read as a value of that type by a typer with no parameter, so it names no parameter and no local. A call of a temporal function is `call.role` |
| A read of it | `ExprKind::Constant` | By its name, of its own module or one imported. It is a constant expression (FN-1), so a `const` parameter takes it, and a compile-time constant (GEN-8) |
| Its value, where one is needed | `constant::read` | Every place that needs a constant's value reads it with the one reader of `constant`, given the type the place expects: a generic value argument, a `requires` operand, a generic value parameter's default, a parameter's default a candidate's condition reads, a `switch` label, a list's and a window's size, a delta's key, a tuple's index and an enum's number. A constant of a module is followed to its value, read as a value of its own type; the ones it names are worked out first, so a chain of any length is read once each and on no call stack. One computed by a call is held by its name (`ConstantValue::Named`). One whose evaluation fails is `constant.failure` where it is declared (UNIT-13, ERR-9), and one the reader does not work out otherwise is not valued (`ConstantValue::Unevaluated`). An expression is read in a loop along a chain of operators, and by a call for what nests in the source, so the reader's depth is the source's nesting, which the parser bounds |
| A cycle | `cycles::check` | Asked of the typed program, after the values pass. Each constant, function body, parameter default and struct's field defaults is a node of a graph, with an edge to each it reads: a constant named, the definition a call selected (the value one of a pair, FN-4; the candidate of an operator, by name or as a symbol, OPS-2), and the defaults of the parameters a call leaves out (UNIT-8). A constant in a cycle of that graph depends on itself (`constant.cycle`). `Expr::walk` visits every expression of a typed body. A constant named where the typed program keeps only its value, a size or a generic value argument, is an edge too, from the part the name is written in. The graph is of the whole program: an operator's candidates are found wherever they are declared (GEN-5), so a value may be worked out through a module that does not import the constant's |

A constant's value is a `ConstantValue`: the value the type of its
place gives it, an integer, a float, a string's characters, a bool,
`null`, a duration's count, a tenor's counts, a date, time or instant's
parts, an enum member by its name, a list or a tuple; a generic value
parameter, or a symbol over one, as written (`N + 1`), which is worked
out where the parameter is given a value; `Unevaluated` for what is not
worked out; `Unknown` for what was reported. The written text stays
beside it (`Constant::text`), for messages, and nothing compares or keys
by it. Two questions are asked of two constants, each by one function.
`ConstantValue::same` says whether they are the same constant: the same
value of the type (`1.0` is `1.00`, `60s` is `1m`, two datetimes as
instants, a zoned datetime as an instant and a zone), which for an `f64`
is not `==`: `-0.0` is not `0.0`, and a NaN is a NaN (`same_float`,
UNIT-13). Type identity and a target's instances ask it (TYPE-3,
GEN-13). `ConstantValue::equals` says whether they are equal as the
language's `==` compares them, `-0.0` equal to `0.0` and a NaN to
nothing: what the program compares asks it, a `requires` (GEN-3), and,
as CTRL-6 and TYPE-16 have it, a `switch` label's duplicates and a
delta's overlap. Each gives `None` where either
is not known, which is neither equal nor unequal. `ConstantValue::key`
is the text a target keys an instance by, one for each constant.

| A caller | Asks | Where a value is not known |
| --- | --- | --- |
| A type's identity | `declared::agree`: known to be the same | Not the same; `Unknown` agrees with any (ERR-9) |
| A field's conflict, a parent's argument | `declared::may_be_one`: not known to differ | No conflict |
| `requires` | `holds`: `equals` of the option's value and each operand | Not told for a generic parameter the call passes on: told for each instance a call that gives every argument makes (`requirements::instances_hold`). Not told for a constant not worked out: `requires.unmet` |
| A call's generic value argument | GEN-8 | `generic.argument_constant`: what is no constant, and one not worked out |
| A `switch` label | CTRL-6: `equals` of two labels | No constant: `switch.label_constant`. One not worked out is a label whose repeat is not judged |
| A delta's key, a tuple's index | TYPE-16: `equals` of two keys; EXPR-7 | No overlap, and the element's type not known |

Not done: an operation the reader does not work out (one given
configuration, a date moved by a tenor, a set's constant). The cycle check counts every field
default of a struct a constant constructs, given or not, and does not
follow the body of an anonymous function, which is not typed.

### Value expressions

The body of each value function, `const fn`, is read into a typed form,
`hgl_analyser::hir`: every local with its type, every expression with
its type, and a node wherever the source leaves a step unwritten, such
as an `i64` taken as an `f64`. It names no target. It is what the stages
after the analyser read; the syntax tree stays for the formatter.

It is the module `values` (`crates/hgl_analyser/src/values.rs`): its
`Typer` reads one body, and each part of what it reads is a module of
`values/`: `statements`, `expressions`, `symbols`, `calls`,
`arguments`, `inference`, `operators`, `candidates`, `requirements`,
`call_checks`, `constants`, `lifting`, `control`, `members`, `deltas`,
`fit`, `results`, `evals`, `node`, `endpoints`, `children`,
`capabilities` and `sizes`.

The pass is built form by form. So far it reads:

| Form | Rule | What is done |
| --- | --- | --- |
| A literal | TYPE-1, TYPE-2, LEX-11 | The type its form gives: `1` is `i64`, `@2026-09-03T09:30` a `civil_datetime`, with an offset a `datetime`, with a zone too a `zoned_datetime`. A temporal literal's value is `hir::Temporal`, the parts it writes: a date, a time, an offset in minutes, a zone. Whatever reads the typed program has no literal to read again |
| `null` | TYPE-14 | The nullable type its place expects. Where the type expected has no `?`, or nothing gives it a type, it is rejected |
| A name | UNIT-10, GEN-13, UNIT-13 | A parameter, a local, a constant of a module (`ExprKind::Constant`), or a generic value parameter, with its type |
| A symbol | NUM-1, OPS-1, OPS-2, OPS-3, EXPR-10, GEN-5 | The call of the core operator it stands for, `add_` for `+`: its operands select one of that operator's value candidates, core's or a module's own, and the result is that candidate's. No candidate for the operands is a rejection. Configuration, `a /{zero: .error} b`, gives the call its named arguments. See "Symbols" below |
| `&&`, `\|\|`, `!` | EXPR-6, OPS-5 | Read as they are written: their operands are bools, and the right operand of a connective is evaluated only where the left does not decide |
| A tuple, a sequence of values | EXPR-5, EXPR-9 | Their elements take the types an expected tuple, list or set gives. With none, a sequence is a list of the one type its values have: `f64` when its integer literals stand among `f64` values, and `T?` when a `null` stands among values of `T`. `[]` and `[null]` give no type, and are rejected |
| A sequence of pairs, `[key: value]` | EXPR-9 | A dictionary where one is expected (`ExprKind::Pairs`): each key is read as the key type and each value as the value type, and no entries make an empty one. With nothing expected the pairs are `sequence.context_type`: syntax alone selects no dictionary |
| A test against null | ERR-12, ERR-3 | `value == null` and `value != null` prove an immutable nullable parameter, `let` or loop binding is not null: in the branch the test holds for, in the right operand of `&&` or `\|\|` it guards, and from a branch that returns to the end of the block the `if` stands in. There the binding is read as its payload's type. A `var`, a saved bool and a test of anything else prove nothing |
| `let`, `var`, assignment | EXPR-3 | A local has the type written or its first value's. Only a `var` is assigned, with a value of its type, and it is read only once assigned |
| An assignment into a place, `point.x = 1`, `row[0] += 1` | EXPR-3 | `Stmt::AssignPlace`. The place is read as the expression it is, so a field or an element takes what it is said to be where it is read. Its root is a `var`, a `state`, a `cache` or `out`; a `let`, a parameter and an iteration binding are read-only all the way down. A compound assignment is the symbol with the place as its left operand. In a graph it is copy on write (EXPR-15): the same statement, held as an assignment to the `var`, to its category (CTRL-14) and to what a loop that makes one child for each member admits (CTRL-15) |
| `delta<T>` | TYPE-16 | `Type::delta` forms it: for most T another type of the language, and `Type::Delta` for a set, a dictionary, a struct, and a T not yet known. It is formed where the type is written and again where a generic parameter is replaced, so `TimedValue<f64>` has a `value: f64`. A named type carries what it is declared as, `NamedKind`, so that this needs no look at the program. A trace's cell is `Type::trace_cell` of what the trace is of: its `delta<T>`, and for a tuple `Type::Delta` of the tuple, which names T where the tuple's delta would not, so that a cell written as a T is read whole (TST-5) |
| A delta's property, `change.added`, `change.value`, `change.bid`; its constructor, `delta<set<i64>>(added: [1, 2])`, `delta<i64?>()` | TYPE-16 | `ExprKind::Field`, the property by its position, and `ExprKind::Delta`, each property given at its position and one left out empty or null. `Typer::delta_properties` says what a delta has: a set's `added` and `removed`; a dictionary's `updated`, a `dict<K, delta<V>>`, and `removed`; a nullable's `value`, a `delta<V>?`; a struct's one for each field, a `delta<F>?`. A nullable's delta is an optional of its own type, `Type::Delta` of the nullable: a `delta<V>` is admitted where it is expected (`Typer::admits`), and null is not, since null in a delta's place is nothing published. A trace's cell of a nullable type is read as the nullable it publishes (TST-5) |
| What a node publishes | TIME-7, TYPE-9, TYPE-11, END-3 | A window's result takes a V, which arrives, and nothing else. A `return` in a hook, an assignment to `out` and a source's `yield` take a complete value of the result, a V for an `atomic<V>`, or the result's `delta<T>`. `delta_value(input)` is the `delta<T>` of the input's type |
| `return`, `=> value` | FN-7 | Of the function's result type. A `return` with no value is rejected where a result is declared, and one with a value where none is |
| A call | EXPR-2, FN-4, UNIT-8, FN-7 | Of the value function its name is bound to. Each argument is given to a parameter, by place and then by name; one given twice, to no parameter, or too many is rejected, and so is a parameter left with nothing that has no default and no `?`. A temporal function is not called from a value function. A callee that names no declaration is read as any expression, for what it reads (FN-14), and one that is a value of a type, `plus_one(x)()` or `(1 + 2)()`, is `type.mismatch`: no value is callable |
| A construction | TYPE-5, TYPE-3, GEN-2 | Its arguments are named, each for a field the struct has, a parent's included, in any order; one with no name is rejected. A generic struct takes its arguments from `Name<...>`, from the type expected, or from the fields given. An abstract struct is not constructed |
| `Side.buy`, `.buy` | TYPE-7 | A member of the enum named, or of the enum the place expects: an annotation, a parameter, a field, a result, the other operand of a comparison |
| `Side(1)`, `Side("buy")` | TYPE-7 | The member of the enum that has the number, an `i64`, or the exact name, a `str`: one argument, by position. A constant that is no member's is `member.unknown`; a value of the run is checked where it is evaluated |
| `value.field` | EXPR-7 | A field the struct has, a parent's included, with the value's generic arguments in place |
| `base[index]` | EXPR-7, ERR-6 | An element of a list by an `i64`, of a tuple by a constant within its length, or the value a dictionary has for a key |
| An argument of a `const` parameter | FN-1 | What is fixed when the graph is built. In a graph any ordinary value is, and a connection is `connection.as_constant`. In a hook, a source and a value function it is a constant expression, of literals, enum members, generic value parameters and the enclosing function's own `const` parameters; anything else is `argument.constant` |
| `ok(value)`, `err(value)` | ERR-2, ERR-13 | A result of the complete `result<T, E>` the place expects, with the value read as its side's type. With no such place it is `result.context_type` |
| `is_ok(value)`, `is_err(value)` | ERR-3 | A `bool`, of one result. A direct guard on an immutable result binding proves its tag as a test against null proves presence, over the same paths |
| `binding.value`, `binding.error` | ERR-3 | The T, or the E, of a binding proved a success, or a failure. With no such proof, or on what is no binding, it is `result.required_variant` |
| `error(message: text, code: optional_code)`, `.message`, `.code`, `.context` | ERR-4 | An `error`, each part given by its name; the message a `str` and the code a `str?`. The context is an `error context?`, a type of this compiler's that source writes no name for: proved present by a null test on its binding, its `.node` and `.phase` are a `str` and its `.evaluation_time` a `datetime`. It and its parts take no assignment |
| A result given to a typed place | ERR-5, ERR-15 | A result place takes the whole result. A place for its success type takes the value or raises the failure. A nullable place for a non-nullable success of `result<T, error>` takes the value or null. A raise of a mismatched error type is `result.propagation_type`. |
| `capture(callable, args...)`, `seen.value`, `seen.error` | ERR-7 | In a graph, the call the first argument names, read from the same list less that name as the call itself is. It is bound with `let`, and read through `.value`, the call's output, and `.error`, an `error`, each a connection. Outside a graph, of no callable, or of a call that composes nothing it is `capture.phase`; `.value` of an outputless call is `capture.output_field` |

A value fits its place when its type is the one expected, when it is a
`T` where a `T?` is expected, or when it is a struct where an abstract
struct it inherits from is expected. An integer literal, under a minus
or not, is an `f64` where one is expected, and is written down as
widened (EXPR-5). A value that has a type is not converted: an `i64`
binding where an `f64` is expected is rejected. A `T?` where a `T` is
expected is read for its value and fails with `value.null` when null
(ERR-12). A fallible nullable success can require both a result read
and a nullable read (ERR-15).

What the pass does not read yet has no type, and nothing is reported of
it or of what uses it (ERR-9): `&&`, `||` or `!` given configuration,
an anonymous function's body, a call of a name nothing binds or of an
operator that has a pack, and a conversion
written `str(value)`. The arguments of a call that is not
read are read themselves. After a form that is not read every `var` is
taken to be assigned. An operand that is a generic parameter has the
operations its declaration requires: see "Requirements" below.

A generic application that has been reported, `Box<7>(value: 1)`, makes
a value whose type is not known: nothing more is reported of it.

Each rejection has a row of the catalogue: `type.mismatch`,
`null.context_type`, `sequence.context_type`, `enum.context_type`,
`binding.unassigned`, `binding.immutable`, the five `argument.` codes,
`call.role`, `struct.abstract`, `member.unknown` and `operator.no_candidate`.

#### Symbols

The compiler has no table of what an operator admits. A symbol stands
for an operator of core's module (the generics chapter: "Syntax binds
these core identities independently of local same-named functions"),
and is read as the call of it (owner's direction of 2026-11-15 and
2026-11-16):

| Written | Read as | Recorded as |
| --- | --- | --- |
| `a + b` | `add_(a, b)` | The call of the candidate selected, marked as written with `+` |
| `-a` | `neg_(a)` | The same, marked as written with `-` |
| `a /{zero: .error} b` | `div_(a, b, zero: .error)` | The same: the entry is a generic value parameter's constant, and no argument |
| `a =={abs: 0.01} b` | `eq_(a, b, abs: 0.01)` | The same, with its arguments in the order they are evaluated (EXPR-13): `a`, `0.01`, `b` |
| `total += b` | `total = total + b` | An assignment of that call, marked as written `+=` |

So what `+` admits is what the candidates of `add_` declare: core's,
for numbers, strings, durations and instants, and a module's own, `impl
const fn add_(lhs: Money, rhs: Money) -> Money`. No candidate for the
operands is `operator.no_candidate`, citing OPS-1, or OPS-2 for a
comparison, and written with the symbol: "`+` takes no `str` and
`i64`".

A configuration's entry is checked as a call's named argument is
(EXPR-10): its name is an option a candidate declares, or it is
`argument.unknown`; given once, or `argument.duplicate`; a candidate
that does not declare it is not for the symbol; and what selects is a
compile-time constant (GEN-8).

Not read as calls:

| Form | Why | What stands |
| --- | --- | --- |
| `&&`, `\|\|`, `!` | Their candidates take a pack, `all_(values: ...bool)`, and a connective does not evaluate its right operand where the left decides (EXPR-6) | The symbol as written, with bool operands |
| An operand that is nullable | "null presence checks ... have their own language rules" | The symbol as written; a comparison is a bool |
| An operand of no known type | Nothing can be selected | The symbol as written |

A symbol is its call in every respect (owner's decisions of 2026-11-16
on hhenson/hgl_spec#51):

| Respect | So |
| --- | --- |
| Its operands are read as the call's arguments are | One with no type of its own, a `.member` or a sequence written out, is read again as the selected candidate's parameter: `xs == [1, 2]` beside a `list<f64>` |
| The type its place expects is requested of the candidates (GEN-5) | `let x: f64 = n + 1` selects the candidate for an `i64` and an `f64`. A link inside a chain is an operand, and is asked for nothing. `count += 1.5` asks for the `var`'s type |
| No candidate gives what the place asks | `operator.no_candidate`, as for the call: "`<=>` takes no `i64` and `f64` to give `bool`" |

What an integer literal is at a call, by name or by symbol, is settled
by the candidates' result and parameter types before their conditions
(GEN-5). Where one that gives the output requested takes every literal
as an `i64`, none is widened, so `n //{zero: .nan} 2` is rejected as
`n //{zero: .nan} n` is. Where literals are widened and candidates tie,
differing only in which, the one that widens every one is selected:
`let x: f64 = 1 + 2` is the candidate for two `f64`.

An abstract struct widens what an earlier argument decided (GEN-2):
`round == shape` compares two `Shape`, as `shape == round` does. What
the call writes, or its place expects, is not widened.

A parameter that may be omitted comes last (UNIT-8): one with a default
or a type written `T?` follows every parameter that has neither. The
`forms` pass checks it. Fields are declared in any order (TYPE-5): a
constructor names each field it gives, so a struct that inherits fields
places none of its own before or after them.

Its rejection is `default.order`, and a tuple's index that is no
constant or is outside the tuple is `tuple.index`.

Control flow in a value function is read as it is written (CTRL-10 to
CTRL-12, FN-7). A block has the value of its last statement when that is
an expression, and so has the function's body, which returns it. An `if`
has a `bool` condition and the one type its branches have: a null branch
makes it `T?`, as does a missing `else`, and a branch that returns takes
no part. `for` binds each element of a list or a set, or a dictionary's
key and value; `while` has a `bool` condition; a `switch` selects on a
`bool`, `i64`, `str` or enum value and its labels are of that type. What
is assigned is followed along each path: after an `if` or a `switch`,
what every branch that does not return assigns, an enum's coverage
standing for a `default`; nothing a loop's body assigns; and a `var`
written `T?` from its declaration, where it is null. A function that
declares a result returns on every path, or is `return.missing`. The
typed body keeps blocks, `if`, the loops and `switch` as they are
written. Not read yet: whether a `switch` label is a constant, and the
warning for a statement with no effect (ERR-10).

A call of a generic value function resolves each of its generic
parameters (GEN-1, GEN-2, GEN-8), from these in order:

| Source | Example, for `const fn same<T>(left: T, right: T) -> T` |
| --- | --- |
| What the call writes after the name, which may be the first of the parameters only | `same<f64>(1, 2)` |
| The type the call's place expects of the result | `let real: f64 = same(1, 2)` |
| The arguments, in declared parameter order (GEN-12) | `same(1, 2)`; in `same(right: 1, left: 2.0)` it is `left` that decides |

The first that says what a parameter is decides it, and every argument
is then checked against its parameter's type with that in place: in
`same("a", 1)` the `1` is a `type.mismatch`, and in
`let real: f64 = same(count, 2)` so is `count`, which is no literal to
widen. The call has the result's type with what was found in place, and
the typed body records what each parameter was resolved to. More generic
arguments than the callee has parameters is `generic.argument_count`; a
type for a value parameter, or a value for a type, is
`generic.argument_kind`. A struct's fields have no order of their own,
so a construction's say what a parameter is in the order written.

A parameter nothing resolves rejects the call as `generic.unresolved`
(GEN-8). A call read before its place is known, as an argument is, puts
the report off for a parameter its result names, which that place may
yet resolve. Once the enclosing call is settled and the place stays
unknown, the argument is read as one of which nothing is expected and
the report is made: in `same(repeat(1), repeat(2))` with nothing
expected, each `repeat`. It is not reported where something else was
reported for the call.

A named argument may give a generic value parameter of the function
(GEN-8): the name is looked for among the function's generic value
parameters and then among its parameters. What it gives is a
compile-time constant, or `generic.argument_constant`; it is held to
the parameter's type; it decides before anything is inferred; and it is
kept among the call's generic arguments, not its arguments. One given
among the generic arguments too, `scale<4>(n, factor: 5)`, is
`argument.duplicate`. A generic value parameter the call does not give,
and no argument's type says anything of, takes its default. So for
`const fn scale<const factor: i64 = 3>(value: i64)`, `scale(n)` has
`factor` 3 and `scale(n, factor: 5)` has it 5.

Any generic value parameter may have a default (GEN-2, owner's decision
of 2026-11-16 on hhenson/hgl_spec#52):

| Declared | Written | Read as |
| --- | --- | --- |
| `struct Window<const size: i64 = 3>` | `value: Window`, `Window(values: xs)` where nothing says the size | `Window<3>`. A struct with a parameter that has no default is still not named bare |
| `operator window<T, const size: i64 = 3>(value: T) -> list<T, size>` | `window<f64>(x)` | The output requested is a `list<f64, 3>` |

An operator's generics are resolved in GEN-2's order: what the call
writes, then what its place expects, then its arguments, and a default
where none of them says (GEN-8). The output requested is matched as far
as it is known, so `window(x)` asks for a list of three of any type.

Not read yet: a function with a type pack or a pack parameter, whose
generic types are then not known; the type of a generic argument written
after the callee's name, `scale<"wide">(n)`, against its parameter's; an
operator's own generic value parameter given by name.

A call of an operator may write the operator's generic arguments
(GEN-8). Where they settle the operator's result, that is the output
requested of the candidates: with `operator convert<T, V>(value: V) ->
T`, `convert<f64>(count)` selects the candidate that gives an `f64`.
What they say of the operands is not read yet.

A call of an operator by its name selects one of the operator's value
candidates (GEN-5, FN-3). The call is matched against each candidate's
own signature (GEN-6):

| Part of a candidate | How a call gives it |
| --- | --- |
| Its first parameters, which are the operator's, place for place | In position, or by the name the operator gives the parameter |
| An option that selects among candidates: a generic value parameter, `<const zero: Zero = .ieee>` | By its name |
| An option that does not select: a parameter after the operator's | By its name, or in position |

A name is looked for among the candidate's generic value parameters and
then among its parameters (GEN-8); a candidate has only one of the two
under a name, and across an operator's candidates an option's name is
one kind (GEN-6), or the candidate that declares the parameter is
`impl.option_kind`.

A candidate is admitted when its result fits the type the place expects,
each argument fits what it is given to, what the call leaves out has a
default or a type written `T?`, and its `requires` holds. A call that
names an option admits only the candidates that declare it; a name
neither the operator nor any candidate declares is `argument.unknown`.
Candidates may order their options differently, so which option an
argument in position is given to is judged candidate by candidate; only
what is wrong for every candidate is reported of the call itself.

A candidate's `requires` compares an option with constants,
`zero == .error` or `zero in {.ieee, .nan}`, joined by `&&`, `||` and
`!`. It is checked against what the call gives the option, or the
option's default: the candidate's, and for a parameter the operator
declares then the operator's. Constants are compared as the values they
denote: `1_000` is `1000`, a string is its contents, and an enum member
is the same written `.error` or `Zero.error` (TYPE-7). So
`ratio(a, b, zero: .error)` selects the candidate that is for `.error`,
and has its result type (EXPR-12).

What a call gives a generic value parameter is a compile-time constant
(GEN-8): a literal, an enum member, a generic value parameter in scope,
or an expression of those. Any other is `generic.argument_constant`. It
is kept among the call's generic arguments in the typed body, and is not
one of its arguments. What the call writes for it decides before any
argument's type does (GEN-2), and its condition reads the value so
resolved, the default only where nothing else said. Its own type may
name a type parameter, `<T, const fallback: T>`: the parameters decide
`T` first, and what `fallback` is given decides it where they do not.

A default is of its parameter's type (GEN-13). One that is not, as far
as how it is written says, is `type.mismatch` where it is declared, and
a call that would take it is not judged. Where the type names a type
parameter, `<T, const fallback: T = 2>`, the default is checked against
what the call resolves `T` to, and the candidate is not for a call
where it does not fit. Where it is a generic value parameter of the
enclosing function, which candidate it selects is known only where that
function is called, and the call is not judged here.

An operator's own requirement may say what its result is: `operator
convert<T, V, O>(value: V) -> O requires O in {T, result<T, error>}`.
With the generic arguments a call writes in place (GEN-8), a candidate
is admitted only when its result is one of those types, which tells
`convert<f64>(count)` from `convert<str>(count)` where the arguments do
not. `convert<list<B>>(values)` of a `list<A>`, `A` a struct that
inherits from the abstract struct `B`, has no candidate in core (OPS-6):
it is read as `ExprKind::ParentList`, of type `list<B>`, with no option.

A candidate with generic parameters is admitted with each resolved from
the call (GEN-2): from the type the place expects of the result, then
from the arguments in declared parameter order (GEN-12). Its `requires`
may say what a parameter is, `T in {i64, f64, str}` or `T == str`, which
is checked against what the parameter stands for. Among the candidates
a call admits the most specific is selected (GEN-5, GEN-12). Each of the
operator's parameters is compared with the other candidate's, part for
part:

| One candidate has | The other has | More specific |
| --- | --- | --- |
| A concrete type or a structure, `i64`, `list<T>` | A bare variable, `T` | The first |
| A structure, `tuple<i64, T>` | The same structure, `tuple<U, V>` | Whichever is in its parts: here the first |
| `tuple<i64, T>` | `tuple<T, str>` | Neither: each is more specific in one part |
| A concrete type, `i64` | A nullable variable, `T?`, which also takes an `i64` (TYPE-14) | The first: they are compared as the types under the `?` |

#### Requirements

A `requires` may name an operator with the types it is supplied with,
and the result it gives: `requires add_(T, T) -> T` (GEN-16). It is
read from both sides.

| Where | Rule | What is done |
| --- | --- | --- |
| In the body of the declaration that states it | GEN-7, EXPR-2 | A call of that operator, by name or by symbol, whose arguments are of the types supplied is admitted, and is of the arrow's type. An argument is given to the operator's parameter by its position or by that parameter's name, `add_(rhs: b, lhs: a)`. One with no type of its own, a sequence or a tuple written out, is read as the type the requirement supplies in its place: `[a] + [b]` under `requires add_(list<T>, list<T>) -> list<T>`. The typed body records `Callee::Required` with the operator and the requirement's position: which candidate it is is selected where `T` is known |
| The same body, an operation nothing states | GEN-7 | Selected as any call is. `a + b` of two `T` finds no candidate and is `operator.no_candidate`; `a == b` is taken by core's `eq_<T>`, which is for any one type |
| At a call that selects the candidate that states it | GEN-16, GEN-5 | Tested with the candidate's generic parameters in place: `add_(f64, f64) -> f64` holds when one most-specific value candidate of `add_` takes two `f64` and gives an `f64`. A requirement names a value operation whatever declares it, a graph's included: core's OPS-9 has "Addition requires a value `add_(T, T) -> T`". It is tested as the call it names would select: the arrow is what the place expects, and a generic value parameter of the operator that nothing says anything of takes its default (GEN-8) |
| At such a call inside a generic body | GEN-7 | The types still name the body's own parameter, `add_(T, T) -> T`: it holds when the body's declaration states the same requirement, and otherwise when a value candidate takes them as they are, as core's `eq_<T>` takes any one type |

What answers a requirement is kept, so that a later stage can write a
generic body for the types of one call of it. A call of a declaration
that states requirements records an `Answer` for each, in the order
written, counting the constraint calls that hold wherever its
`requires` does: the condition itself and each side of an `&&`.

| `Answer` | When | For `fn sum<T>(a: T, b: T) -> T requires add_(T, T) -> T` |
| --- | --- | --- |
| `Candidate` | One most-specific value candidate takes the types (GEN-5). It has its own requirements answered with it | `sum(1.5, 2.5)` records core's `add_(f64, f64)` |
| `Stated(n)` | The types name a generic parameter of the declaration the call is written in, which states the same requirement as its `n`th (GEN-7) | Inside `fn twice<U>(v: U) -> U requires add_(U, U) -> U`, `sum(v, v)` records `Stated(0)`: a call of `twice` says which candidate |
| `Unread` | A predicate, a requirement supplied with what is no type, or one for which no one candidate is told to be selected | `sum(true, false)`, which is rejected: the requirement does not hold (`requires.unmet`, GEN-3) |

A `Callee::Required` in the body carries the same position, so the
candidate it calls for one call of the declaration is that call's
answer at that position.

A `requires` that does not hold with the generic arguments it is given
is `requires.unmet` (GEN-3), reported where the function is called or
the struct applied:

| Where | What is told there |
| --- | --- |
| A call of a function | A membership or an equality of a type, a generic value parameter compared with constants, and a named operator requirement |
| A construction of a struct, its arguments written or found from its fields | The same |
| A type written that applies a struct, at any depth, in any declaration: a signature, a body, a struct's field, a constant's type | The same |

A function and a struct with no generic parameter are held to a
`requires` they state too. What cannot be told where a generic body
passes on a parameter of its own is told once the typed program is
whole, for each instance a call that gives every argument makes, at any
depth (`requirements::instances_hold`), and reported at that call: of a
struct a type in a generic declaration applies, and a generic argument
worked out from the body's own generic parameters, rejected
(`generic.argument_constant`) where its evaluation fails there. A call
of another generic function in a generic body is judged where the body
is declared, by what its `requires` states (GEN-7,
hhenson/hgl_spec#144), and is not judged again for each instance. A call of an operator in a generic body whose
candidate the body's own parameters would select is judged once, where
the body is declared, under what its `requires` states (GEN-7): a named
operator requirement makes it the required operation; a membership
(`T in {i64, f64}`) authorises it where "each listed type has one"
(GEN-3), and each instance then selects its own
(`values::Selecting`, which `Program::instance` reads for the target);
otherwise it is rejected there. Only
the calls that reach one of those are followed, each instance once
however many calls make it, and at most `hgl_analyser::INSTANCES`
(4096) instances of one declaration from one call, whose arguments are
no longer than 65536 characters written out; a call whose instances go
past that is `generic.instance_limit` (GEN-13), for what is
past it is not told. The target makes no more than that many either
(`hgl_lower::instances`). What
is not told for a constant this compiler does not work out is reported
as not known to hold; nothing else that cannot be told is reported.
A generic parameter of the declaration a call is written in is no one
type, so a membership of it is not told there; a named operator
requirement of it is, by what that declaration states (GEN-7), a
struct's as a function's. A generic value is read however it is written (GEN-8): a
literal of any kind, an enum's member in any spelling, a constant's
name, after the callee's name or by name. One reader does that for a
call, a construction and a written type, so a spelling means the same
in each. A type that writes a struct's arguments is held to them, and
one that writes none to the defaults (GEN-2), which are read where the
struct is declared.

`Program::instance` puts the two together for a target with no generics
of its own. Given a declaration, the generic arguments of one call of
it and that call's answers, it returns the declaration's signature and
body with each generic parameter replaced by what it stands for, each
`Callee::Required` replaced by the call of the candidate that answers
it, each `Stated` answer of a call in the body replaced by what answers
the declaration's own requirement, and a generic value read as a value
replaced by the constant given (GEN-13). So a call in the returned body
names no generic parameter of the declaration, and asks for a whole
instance in turn. Nothing is analysed again.

A call that writes the operator's generic arguments, `make<T>()`, is
the requirement those agree with (GEN-8): what the operator's signature
says of a parameter or of its result with them in place is what the
requirement says, part for part, and what the call does not write
agrees with any.

A requirement is of one call: an operator, the types supplied, and the
output asked, which is its arrow or, where the arrow leaves a generic
value parameter of the operator unsaid, the operator's default (GEN-8).
So `requires window(T) -> list<T, 5>` neither is the body's bare
`window(value)`, which asks for the default, nor meets a candidate's
`requires window(T)`.

A requirement and a call ask the candidates through one function,
`Typer::output_requested`: what the place expects, the types supplied
in the operator's parameter order with an abstract struct widening what
an earlier one decided (GEN-2), the operator's defaults, each of its
parameter's type as resolved (GEN-13), and the operator's own
requirement of its result. So a requirement holds exactly where the
call it names would select one candidate.

Where two requirements name one operator with the same types and
different results, the call is the one that gives what its place
expects (GEN-5), and the first stated where nothing is asked.

A requirement joined to others by `&&` holds wherever the condition
does, and is one the body may use; one under `!` or `||` is tested at a
call and admits nothing in the body. A candidate's requirement may be
met by a candidate that states one of its own, so a test is bounded
eight deep: past that the requirement is not told, and the call is not
judged.

A candidate outranks another when it is as specific in every parameter
and more specific in one. One that nothing outranks is selected; two
are `operator.ambiguous`. Not read yet: a candidate with a generic value
parameter or a type pack, which leaves the whole call unread.

An argument that is read as the type of its place, `null`,
`.member`, or a sequence or tuple written out, is tried against each
candidate's parameter, and what the trial reports is taken back. A
candidate that fits with no integer literal widened is taken before one
that needs it. An argument tried may hold a call that tries its own, so
the readings are counted for the whole function: past a thousand a call
that needs a trial is not judged, and the pass ends whatever it is given.
The typed body records the operator and the candidate selected. No
candidate is `operator.no_candidate`, and a tie `operator.ambiguous`.
A symbol selects as a call by name does: see "Symbols" above.

The five names `ok`, `err`, `is_ok`, `is_err` and `capture` are
contextual (ERR-13). As the callee of a call each is its form: the
resolver looks the name up nowhere, so it is no name the prelude is
waited on for, and the value pass reads the form before it asks what a
name is bound to. A declaration, import alias, parameter or local that
takes one of them is `name.duplicate`; a member may have the name.

What a guard proves is one list of facts (`Fact`): a binding is not
null, an input is valid, a result is a success or a failure. A
condition gives the facts that hold where it does and where it does
not, and a branch, the right operand of a connective and what follows a
branch that returns each read under theirs.

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

Every module sees the public declarations of core's mandatory module,
`core`, without importing them (UNIT-11). The module is source like any
other: core's `hgl` directory is given among the source roots, and a
program given without it has no prelude. When the program has it, a name
nothing nearer binds is its public declaration of that name, an operator
candidate finds its operator there (GEN-4), and outside it no
declaration, import alias or local takes such a name (UNIT-12).

A name that nothing binds is rejected, `name.unknown` (UNIT-10): a
local, a parameter, a generic parameter, a declaration, an import and
the prelude are all there is to bind one. An `impl` of an operator that
nothing declares is `impl.unknown_operator` (GEN-4). Over core's own
source and its conformance cases there is none. Core declares the public
endpoint functions. The analyser recognises endpoint forms such as the
`valid` and `modified` selectors and `scheduled()` in a node (TIME-9).
A call to core's `is_active` follows ordinary name and argument binding.
`types(Ts)` and
`schemas(pack)` are forms of the language too (GEN-9, GEN-10), which no
pass reads yet: neither name is reported. A program given with no module
`core` has no prelude and is no whole program: its unbound names are
listed in the analysed program and not reported.

### Leaving declarations out

A file may be given with a list of declarations to leave out, and the
program is analysed as if they were not written. A test runner checks a
rejection case that way (TST-13).

## Documentation

`hgl_doc` reads the analysed program's interfaces and the lossless trees
it retains. The parser's SDOC layout reader is shared with the analyser's
key checks. It uses CommonMark fence ranges to preserve literal text in
comment and section contents. The result is an [HTML reference or a
JSON API model](documentation.md), a leaf of the pipeline (SDOC-6).

## Test runner

`hgl test` runs the cases of a program (TST-12 to TST-15). The runner is
not a stage: it drives the stages. A case is one of two kinds:

| Kind | What it is | State |
| --- | --- | --- |
| Rejection | An import, declaration or named test expected to be rejected, marked with `# expect-error(category, "code")` | checked |
| Executable | A named test | run on the target `--target` names; listed and reported `not run` with none |

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

The runner knows no target. It hands the selected tests, each named
`module.path::name`, and the surviving source's analysed program to an
`Executor`, which says what became of each. `hgl test --target cpp`
gives it one made of the cpp target and its build driver; without
`--target` it is given none, and every test is `not run`.

An integration test is left out of a run (TST-17): it is no case of the
run, and is not listed. `--integration` asks for them all, and `--test`
with its name asks for that one. A rejection case an integration test
owns runs nothing, and is checked as any other is.

| Outcome of a test | When |
| --- | --- |
| `passed` | It ran to its end |
| `FAILED` | An assertion did not hold or an error escaped. The line under it says which statement of the test, and what the host said |
| `not run` | No target was named; the target does not write a form the test needs, which the line under it names; or the host did not run it to an end. Such a test shows nothing (TST-14) |

Only a catalogued category and code can be expected (TST-12).
`hgl_diagnostics::catalogue` lists them, and a test compares the list
with the pinned chapter. Every code of the lexer and the analyser is one,
so a case can expect a name or module rejection as it can a lexical one.
The parser's code for its own nesting limit is local and can be expected
by no case. Nor can an error that no item owns: a repeated documentation
comment, or a second unnamed file of a module.

| Exit status of `hgl test` | When |
| --- | --- |
| 0 | Every selected case passed, or none was selected, which the output says |
| 1 | A case failed, or the run was stopped |
| 2 | Nothing failed and a selected test was not run. `--rejections-only` selects no test |

The program is analysed once more for each rejection case. That is simple
and exact; it is revisited if a suite grows large enough to notice.

## Running an application

`hgl run ROOT... [--application PATH] [--arg NAME=VALUE]... [--config
FIELD=VALUE]...` runs an application (UNIT-14) on the target, `cpp`
where `--target` names none: the program's one application, or the one
named.

What starts an application "supplies its run's configuration (RUN-6),
and may replace an argument of its entry with a constant checked
against the type it replaces: what the application writes is the
default" (RUN-2). `hgl run` is what starts it, and this is the command
line it gives for both: "Compiler/runner documentation specifies CLI
options" (the program-execution chapter's core boundary):

| Option | What it gives |
| --- | --- |
| `--arg NAME=VALUE` | The argument `NAME` of the entry, in place of the application's |
| `--config start=VALUE`, `--config end=VALUE`, `--config length=VALUE` | That field of the run's configuration, core's `RunConfig`'s, over the default the application's `inject run_config{...}` gives it. An `end` given takes out a default `length`, and a `length` a default `end` (RUN-6: "An `end` or a `length` that what starts an application gives replaces a default of either"); both given are the run's, which refuses them (`run.end_and_length`) |

The value is HGL, one expression on one line. `hgl_cli`'s `replace`
puts it, in parentheses, in the application's text: an argument in the
run statement, in place of the value the application wrote for that
parameter or by name where it wrote none; a field in the braces of the
application's `inject run_config`, in place of its default or after
them, or in an `inject run_config{...}` written before the run
statement where the application has none. The program is then read
and checked again as if the application had written that, so the
analyser holds each to every rule it holds what is written to, and no
stage knows of replacements. `run_config` then reads the configuration
as supplied (RUN-6).

```text
inject run_config{length: 3s}                  the application
eval counted(upto: 3)
--arg upto=5 --config end=@2026-01-02T00:00:00Z
inject run_config{end: (@2026-01-02T00:00:00Z)}
eval counted(upto: (5))
```

A name the entry or the configuration has not, a name given twice, and
a value that is not one expression are refused before the second read.
The value is read as it is put, between its parentheses, where `.buy`
is a member of the enum the place expects; `3, 4` and `3) + (4` are not
one expression there. An error of the second read is reported in the
application's file, with the statement as it was checked. The type a
replacement is checked against is the one it replaces. The generic
arguments an entry's call infers, `<i64>` for `echo(value: 3)`, are
written after the entry's name for the second read, so a replacement
is read as the argument it replaces was expected: `.sell` is a member
of the enum the call made `T`, `3` is the `f64` it made it, and
`"text"` is no `i64`. They are written as a message writes a type,
which the application's file may have no name for: they are left out
where the application, with them written and nothing replaced, does not
read as it did. In either case, where the call with the replacements is
of another entry than the application's, or of the same one with other
generic arguments, the program is rejected.

The target writes the application and runs it once: the cpp target
builds its executable and reads what became of the run from the results
it writes ("The build driver" under "cpp" below).

```text
1970-01-01T00:00:01.000001Z  2
1970-01-01T00:00:02.000001Z  4
ran demo.ticks.backtest on cpp: 2 publications
```

The run's output goes to standard output, a line for each publication:
its instant and what was published, written as HGL writes a value where
it has a way to. What the program logs and hgraph prints goes to standard
error, as the run goes.

`eval` is a simulation, and `run` is in real time (RUN-3). An interrupt
of `hgl run`, or a request for it to end, is the host's stop of a run in
real time: `hgl` takes it and passes it on to the run, which ends as it
does at its end, every node stopped, and what was recorded up to then is
printed. A signal sent to `hgl` alone reaches the run all the same, and
`hgl` outlives it to print the output. A stop of a simulation, and a
second one of any run, end the run at once with no output.

| Exit status of `hgl run` | When |
| --- | --- |
| 0 | The run ended |
| 1 | The program is rejected, as written or with the replacements given, or the run failed |
| 2 | No application was chosen, a replacement names nothing or is not one expression, the target does not write all it needs, or the host did not run it |

Not written: outside events in real time, and a view of the output
while the run goes.

## Diagnostics

A diagnostic is an error, which rejects the source, or a warning, which
does not (ERR-1). The catalogue lists a warning by its code alone
(`catalogue::WARNING_CODES`): it is printed `warning[code]`, `hgl check`
counts it after the errors, and no `# expect-error` names one (ERR-10).

A diagnostic has a category from the specification's list, a code, a primary
location, and the rule it enforces. The list's `build` is for
infrastructure, a transpiler's report of what its target does not write
among them: it is no source category, and no annotation names it. It may point at related places as
well, such as the first of two declarations. A code is *catalogued* (it appears in the
specification's catalogue) or *local* (the specification rejects the
condition but names no code). Only a catalogued code may satisfy
`# expect-error` (TST-12). Every lexical code is catalogued. The parser
reports the catalogue's one code for the grammar, `syntax.expected_token`,
and its two for documentation comments, `doc.unattached` and
`doc.repeated` (SDOC-1). Its one local code is `syntax.nesting_limit`:
the limit is the compiler's, not the language's. The analyser's codes
are catalogued (`hgl_analyser::codes`), one of them a warning. Its local
codes are `loop.carried` (CTRL-15), `reading.limit` (EXPR-5), and
`function.value` and `function.reference_unread` (FN-17).
An expression that takes its type from its place is read again as
each candidate's parameter it is tried as, and those readings nest.
Each statement read where no expression is, each
declaration of a node and each body written as one expression may read
10,000 expressions again, and 16 more for each node of its syntax; one
that needs more is rejected, and what is read before it changes
nothing. Its tests check each diagnostic's category and code against
the catalogue, or that list.
`crates/hgl_lexer/tests/spec_tables.rs` compares the lexer's codes, reserved
words and symbols with the pinned chapters, so a pin that changes one of
them fails a test.

Where two catalogue rows could cover one condition, the specification
names one (LEX-12, LEX-25):

| Condition | Code |
| --- | --- |
| Text block that reaches the end of the file | `string.unclosed` |
| Empty zone component, as in `@[Europe//London]` | `date_time.invalid_zone` |
| Backslash directly before a line break in a text block | `string.unknown_escape` |

## Lowering

`hgl_lower` says what follows from the language's rules alone, before
any target writes a program: no target works it out again. It depends on
`hgl_analyser` and `hgl_diagnostics`, and on no target; a transpiler
depends on it. It rewrites nothing: each module answers questions of the
analysed program.

| Module | What it decides | Rules |
| --- | --- | --- |
| `copies` | Where a value is copied: which `let`s and loops take a copy, what a name refers into, whether what a call gives is its own; which structs are values (`by_value`) | the owner's rule (compiler issue 151), FN-13, FN-15 |
| `instances`, `canonical`, `packs` | Which instances a program asks for, including generic value functions and packed calls. A pack's captured types and keys identify its instance, and each capture becomes an individual parameter. These modules also describe what an eval's run and an application's run are wired with. | FN-8, GEN-2, GEN-10, GEN-13, GEN-16, CEVAL-1, CRUN-8 |
| `pieces` | What a piece a target writes reaches (`Reach`), and whether a test or an application reaches a form the target does not write (`blocked`) | |
| `graphs` | What a child graph of a graph's `if`, `switch` or `for` takes, the value of a `switch` label, the `var`s a branch assigns, which paths return | CTRL-6, CTRL-7, CTRL-11, CTRL-14 |
| `types` | A struct's fields and descendants, whether a struct has none, and which values are read whole only where they are all valid | TYPE-3, TYPE-4, TYPE-9, TIME-1 |
| `nodes` | A `when`'s subscriptions and requirements, what a node's handlers ask together, its timer, which inputs a `ref` result designates and how | TIME-3, TIME-4, TIME-9, FN-16 |
| `calls` | What a parameter left out is given, and a field's default | UNIT-7, UNIT-8, TYPE-5 |

The analyser exports what it works out and a target would otherwise
work out again: which declarations are sources (`Module::is_source`,
SOURCE-1), an integration test's limit (`INTEGRATION_LIMIT`, TST-17),
whether each expression is a connection or an ordinary value
(`hir::Expr::category`, FN-3), and whether each call is lifted (the
`lifted` of `hir::ExprKind::Call`, FN-6). The last two are written on
every expression once its body is read, from the one rule the typing
pass reads by (`values::lifting::category`), and again on an instance's
body (`Program::instance`), where each operation the declaration
requires is the call of its candidate and what it gives is settled.
A lifted call's arguments that are connections are its node's inputs,
and the rest its configuration. The targets read them (compiler issue
270).

## Transpilers

A transpiler is a crate that depends on the analysed-program crate, the
lowering crate and the diagnostics crate, and on no other transpiler. `hgl emit --target NAME
--out DIR` selects one by name and writes what it gives. The front end
does not change, and no stage before a transpiler knows a target. There
is one, `cpp`, which `hgl emit` and `hgl run` use where `--target` names
none. `--target` takes no other value. `--target unknown` is a usage
error, `error: invalid value 'unknown'
for '--target <TARGET>'` with the values it takes, status 2, and nothing
is read.

A transpiler is given a program the analyser accepted. What it cannot
write is no fault of the source, so it is reported in the category
`build`, which no `# expect-error` names, with a code of the target's
own. `hgl emit` then writes nothing: half a program is of no use to run.
`hgl test --target NAME` does run on such a program the tests that need
nothing the target reported.

### cpp

The compiler's target, designed in [cpp-target.md](cpp-target.md): C++ against
hgraph's graph, node, type and runtime layers, linked with `libhglrt`,
the runtime library in `runtime/cpp`. It writes value code whole, value
functions and the tests of them, and the temporal code of scalars,
enums, nullables, structs, tuples, sets, dictionaries and lists: nodes,
graphs, sources and the evals of them; windows; `ref` parameters and
results; schedulers and alarms; an input's activity; lifted calls and
`capture`; control flow over connections; the run's store where a graph
is built, and applications. It reports every other
form, so that `hgl test --target cpp` runs over any program to its end.

| Part | Where | What it does |
| --- | --- | --- |
| Emitter | `crates/hgl_cpp/src/emit.rs` and `emit/` by concern: `names`, `types`, `composites`, `values`, `statements`, `expressions`, `calls`, `nodes`, `graphs`, `sources`, `tests`, `apps`; the temporal forms through the dispatch point, `forms` ("Temporal forms: what a construct lane reads" below), and its families `scalars`, `endpoints`, `nodes`, `sources`, `graphs`, `tests`, `windows`, `references`, `apps`, `scheduling`, `activity`, `lifting`, `control`, `nullables`, `structs`, `keyed`, `lists` | Writes each declaration, instance, named test and application's run statement as a piece: a prototype for the module's header, a definition for its unit, the functions it calls, and the first form in it not written |
| Assembly | `crates/hgl_cpp/src/assemble.rs` | Leaves out each piece that is not written or reaches one that is not, and each instance that no piece it writes reaches (one only a piece left out asks for, an eval's `record` of a type not written, so that it stops no other module's unit, core's among them), puts the rest into each module's types header, header and unit, and writes an entry unit for each module with a test that can be run and for each application that can be |
| Runtime library | `crates/hgl_cpp/src/runtime.rs`, `build.rs` | Carries `runtime/cpp` in `hgl`, and reads from its headers which of core's natives it defines |
| Build driver | `crates/hgl_cli/src/cpp.rs` and `cpp/` | Finds the toolchain, prepares it once, builds a program in a workspace kept between runs, runs the executables and reads their results |

| HGL | C++ |
| --- | --- |
| Module `demo.calc` (UNIT-1, UNIT-2) | `modules/mod-demo-calc.h` and `modules/mod-demo-calc.cpp` in namespace `hgl::demo::calc::m_`. Its header includes `modules/type-demo-calc.h` where it declares types, with each struct after those it holds (UNIT-6) |
| `const fn` (FN-1); an HGL candidate of an operator (GEN-4) | A function, with its HGL name, signature and line in a comment; a candidate an overload of its operator's name in `c_` |
| A generic one (GEN-2) | One function for each instance `hgl_lower` collects (`Instances::typed`), named by its suffix in `i_::name`: `i_::first::of_i64` |
| A parameter; a `var` parameter (FN-15) | By value where it is small (`types::by_value`), a `const` reference otherwise; a reference to the place the call gives |
| `i64`, `f64`, `bool`, `str`, `bytes`, `date`, `time`, `datetime`, `duration`, `civil_datetime`, `zoned_datetime`, `tenor` | hgraph's `Int`, `Float`, `Bool`, `Str`, `Bytes`, `Date`, `Time`, `DateTime`, `TimeDelta`, `CivilDateTime`, `ZonedDateTime`, `Tenor`, and their literals |
| An enum (TYPE-7); one of core's the library declares | An `enum class` of the module's, and beside it its `hgl_enum`, by which the runtime library names it ("The C++ runtime library" below); the library's, `::hglrt::ZeroPolicy` |
| Another module's enum or struct, named | The header includes that module's types header, and no header includes another module's header: an instance (written in its declaration's module) may name a type of a module that imports that one, and a types header includes only the types headers of the modules its types name, which its module imports (UNIT-3: "Import cycles are rejected") |
| A struct (TYPE-3 to TYPE-5) | An aggregate of its fields, a parent's first, with `==` of each (OPS-2), which names the struct as `struct S` so that a field of its name does not hide it; a generic one a class template, `Box<::hgraph::Int>`, whose parameters are `names::template_parameter`s, `T_p_`, a form no other name takes, so that no name the template declares shadows one; `hglrt::fields<S>` naming its members, which its hash and its identity as a key are made from, and `std::hash<S>`. Built as `Point{x, y}`, its fields given in their order, defaults and nulls among them, an optional field's empty default the empty collection, each held first in the order written where that shows (EXPR-2) |
| An abstract struct (TYPE-3) | A class that holds any struct that inherits from it (`hglrt::held_value`), with a function of each field's name and a conversion to each struct it inherits from; `hglrt::parents<S>` lists what a struct inherits from. A struct that holds itself (TYPE-6) is not written yet |
| `set<K>`, `dict<K, V>` (TYPE-14, CTRL-12) | `hglrt::set<K>`, `hglrt::dict<K, V>`: hash containers in no order, equal where their members are (OPS-2); a key is told apart by `==` of its type (NUM-5: a key that holds a NaN is no key's). A dictionary written out with more than one entry is written where its keys are constants that are not `==`: the pinned text does not say what one key written twice gives. `book[key]` is `hglrt::entry_at`, which fails with `value.missing_key` (ERR-6); assigned whole it is `hglrt::entry(book, key) = value`, which adds the key as the value is assigned (EXPR-3); `for` binds each member, or key and value, of the container or of a copy of it where the copy rule says or it is made in the loop's line |
| `delta<T>` (TYPE-16) | A nullable's `hglrt::nullable_delta<T, D>`, of the payload and its delta, which holds an optional, `value`: a type of its own, so that a `delta<i64?>?` is an optional of one and is compared with a `delta<i64?>` as a value, and `get` of a dictionary of them gives one; a list's `hglrt::list_delta`, its `updated` and `removed` read as a dictionary and a set; a set's and a dictionary's `hglrt::set_delta`, `hglrt::dict_delta`; a struct's `d_::S` of its module, an optional of each field's delta; a tuple's a tuple. A dictionary's or a list's whose value or element is an `atomic<V>`, at any depth through dictionaries and lists, is reported: its delta is the whole V, where the library's would be V's delta |
| `Side(n)`, `Side("buy")` (TYPE-7) | `hglrt::enum_member`, from a table of the members' numbers or names; none matching fails uncoded: the catalogue has no code for it |
| `timezone`, `zoned_time` (TYPE-2) | hgraph's `ZoneId` and `ZonedTime`. A literal of either, and of a `zoned_datetime`, is `hglrt::zone_literal`, `zoned_time_literal`, `zoned_datetime_literal`, which check the zone against the catalogue of hgraph's time-zone provider for the process, and a `zoned_datetime`'s offset against the zone, where it is evaluated (LEX-12), failing uncoded: such a literal is one that may fail |
| `T?`, `tuple<A, B>`, `list<T>`, `list<T, N>` | `std::optional<T>`, `std::tuple<A, B>`, `hglrt::list<T>`, `std::array<T, N>` |
| `error`, `result<T, error>`, `?` (ERR-2 to ERR-5) | `hglrt::error`, `hglrt::result<T>`; the result held before its statement, the function left with its failure; a `?` only where it is the first thing its statement evaluates, and reported anywhere else |
| A constant (UNIT-13) | A function giving a reference to one `static` value, made where it is first read; a read of one whose value may fail is one that may fail, which a `?` after it is not run before |
| `let`, `var` (EXPR-3) | `const T` or `const T&` as the copy rule says (`hgl_lower::copies`), `T` |
| `if`, `switch`, `while`, `for` over a list (CTRL-6, CTRL-10, CTRL-12) | `if`, a chain of `if`s on the selector held once, each case a block (a case's binding read after the `switch`, which UNIT-12 binds in the block around it, is reported), `while`, a range `for` over the list or a copy of it |
| A native candidate or function of core | The runtime library's function, named by its operation and its parameters' types ("The C++ runtime library" below): `::hglrt::add__i64_i64`, a generic one's arguments its template's, `::hglrt::div__f64_f64<::hglrt::ZeroPolicy::nan>`; `convert`'s, of one value, given the type it gives first, `::hglrt::convert_i64<::hgraph::Float>` |
| A call with more than one argument that may fail | Its arguments evaluated one statement at a time, in the order written (EXPR-2), in a lambda called where it stands |
| A tuple or a list with an entry after the first that runs statements; an element read whose list and index both may fail, or either runs statements | Each entry held as a value of its own type first, in order; the list held, then the index (`expressions::held_read`), the element a value of its own |
| Named test | A function of `hgl::demo::calc::m_::t_`; each statement told to the library's test host first, `::hglrt::testing::statement()`; `assert` is `::hglrt::testing::check`, `assert raises` `::hglrt::testing::raises` of a lambda (TST-10) |
| The tests of a module | `tests/test-demo-calc.cpp` registers each test by its identity with `hglrt::testing::Registration` and defines the executable's `main` |
| Temporal function `count` (FN-2), and each instance of a generic one (GEN-2) | A struct `n_::count::impl_` of the module's namespace, in its unit (`impl_` in a namespace of the function's name, so that no member, `name` or `compose`, is named as its struct), and a wiring function `w_::count(wiring_, ...)`, declared in its header, which adds it to a graph: a call names the wiring function, so that no other unit instantiates hgraph's templates of it; an instance's are `n_::i_::record::of_i64::impl_`, `w_::i_::record::of_i64` |
| A node (FN-2, TIME-3 to TIME-8) | A static node of hgraph's front end (`<hgraph/types/generated.h>`): `eval`, and `start` and `stop` where it needs them, each given hgraph's views. A temporal parameter is an `hgraph::In` of its temporal form, `Passive` where no handler subscribes to it and `Unchecked` where not every executed handler requires it (TIME-4, `hgl_lower::nodes`); a `const` one an `hgraph::Scalar`; the result `hgraph::Out`, `out_` |
| `state`, `cache` (TIME-5, TIME-6) | Members of the node's one `state_type_`, which hgraph holds as the node's `State`, registered with its type registry; each is given its first value in `start`, before the hook's statements, and bound by reference in each hook |
| `when` (TIME-3) | Tested in source order in `eval`, each guarded by what it asks beyond what the node requires: `::hglrt::fn_valid` of an input it alone requires, `::hglrt::fn_modified` of a `modified` selector, and the rest of its condition. One that `scheduled()` or `modified([])` selects is never run in a node that injects no timer, and is not written |
| `return value`, `out = value`, `invalidate(out)` in a handler (TIME-7, FN-7) | The value published to `out_` as its temporal form publishes, `out_.set(value);`, and for a `return` the evaluation left; `::hglrt::fn_invalidate(out_)` |
| A temporal input read in a hook; `valid`, `modified`, `delta_value`, `last_modified` of one (TIME-1, END-1 to END-6) | The form's checked read of the view, `::hglrt::checked_value(price)`; the library's natives given the view itself, `::hglrt::fn_valid(price)`; `delta_value` in the form its place takes, `fn_delta_value_checked` where the analyser recorded a `result` (ERR-14) |
| `is_active(data)`; `activate(data)`, `passivate(data)` (TIME-9, END-5) | The library's natives given the input's view, `::hglrt::fn_is_active(data)`, `::hglrt::fn_activate(data);`: hgraph's own subscription of that binding, which the input's declared activity starts from, and which the guards read as it is. A change in `stop` is reported: hgraph makes passive at stop only the inputs a node declares active |
| A temporal value of an enum (TYPE-7, TYPE-16) | hgraph's `TS<Side>`, read, published, replayed, recorded and wired as a constant as a scalar is: its delta is itself |
| `inject clock`, `logger`, `global_state`; a property of one (TIME-8, CAP-1) | The hook's hgraph view, a parameter of each hook that uses it, and the library's `::hglrt::Clock` made from it at the top of the hook; `clock.evaluation_time()` |
| `get`, `has`, `put` on the store (CAP-6) | The library's natives, `::hglrt::put_GlobalState_str_V<T>(global_state, key, value)`; every ordinary value is held alike, a struct, a tuple, a set, a dictionary and a delta among them |
| `inject global_state` in a graph; a call on it (FN-9) | `const ::hglrt::GlobalState global_state{wiring_.global_state()};` in `compose`, the store of the wiring the graph is built on, which hgraph carries onto the graph it builds: each call runs once as the graph is built, in the order written, before any node starts (`apps`) |
| `inject scheduler`, `inject alarm`; `schedule`, `un_schedule`, `reset` (TIME-8, CAP-4, CAP-5) | The hook's `::hgraph::NodeScheduler`, and the library's `::hglrt::Scheduler` or `::hglrt::Alarm` made from it with the hook it is made in, `::hglrt::Hook::evaluation`, in every `eval`; the library's natives (`emit/scheduling.rs`) |
| What woke a node with a scheduler or an alarm (TIME-3, TIME-9) | Read as `eval` is entered: `timer_woke_`, the scheduler's `is_scheduled_now()` or, for an alarm, that no input ticked (`::hglrt::ticked`), and `input_woke_`. A handler that `scheduled()` selects or that subscribes to no input is guarded by the first, another with no `modified` selector by the second; `scheduled()` read as a `bool` is `timer_woke_`, and `modified([])` is `true` in the handler that selects it and elsewhere that no input published |
| A source (SOURCE-1 to SOURCE-4) | A node whose static `body_` is an `::hglrt::generator` of `::hglrt::Yielded<T>`, or of `Yielded<delta<T>>` where every `yield` gives a delta that is a type of its own (TIME-7), core's `replay` of a set among them, given its `const` parameters by value; `yield t: v` is `co_yield yielded_type_{t, v};`, a bare `return` `co_return;`. `start` makes the body, held in the node's state with where it is, `eval` steps it with `::hglrt::resume`, `stop` destroys it |
| A source's `state`, `cache`, `inject`, `start` and `stop` (SOURCE-1) | Each `state` and `cache`, and each capability the body reads, a member of a `slot_type_` that the node's state keeps on the heap and the body is given by reference; `start` makes the slot, gives each its first value, makes the body and runs its statements; `eval` lends the body each capability for the resumption (`::hglrt::Lending`); `stop` destroys the body, then runs its statements (`emit/scheduling.rs`) |
| A graph (FN-2, FN-7) | A struct with static `compose(::hgraph::Wiring& wiring_, ...)`, a temporal parameter an `hgraph::Port` and a `const` one a value; a connection `let` a `const ::hgraph::Port<S>`, `return` the port given |
| A lifted call (FN-6, FN-11) | A node of its own (`emit/lifting.rs`), a struct `lifted_00_` in the graph's struct, where the analyser marks the call lifted: each argument that is a connection an input, and each other configuration, an `hgraph::Scalar`; its `eval` calls the function and publishes what it gives, and one with no result is a sink. An input is required valid, but where its parameter has a default or admits null: that one is `InputValidity::Unchecked`, under a name of its own, and binds the parameter to the fallback where it is not valid, `fn_valid(input_00_) ? T{read} : T{default}`. A parameter the call leaves out binds its default as configuration where the graph is built (FN-6), after the arguments, in the order of the parameters (EXPR-2). A struct, a tuple, a fixed list or a window given to one calls nothing where it is valid and not all valid |
| A temporal candidate of an operator, `lhs +{invalid: .identity} rhs` (GEN-4, OPS-9) | A node or a graph of its module, as a temporal function is, using the selected candidate's own name, with each generic instance in its namespace under `c_::i_` |
| A value where a connection is expected (FN-3) | Its temporal form's constant, `::hglrt::constant<::hgraph::Int>(wiring_, 5)` |
| A connection of T where one of `T?` is taken, a value of T too (TYPE-14) | hgraph's own read-only nullable widening, the port as one of `T?`: `(x).as<::hgraph::TS<::hgraph::Optional<::hgraph::Int>>>()` (`graphs::widened_from`); a lifted call's input binds the `T?` itself. Given to a `ref` parameter of `T?`, the node's `REF` input designates that widened view of the producer's endpoint (FN-16) |
| A `ref` parameter, `ref low: i64`, and a `ref` result (FN-16, TIME-10; core's END-10) | The parameter uses an hgraph `REF` input. Default admission permits an empty reference, and an explicit `valid(target)` requires a designation (TIME-3). Returning an empty reference withdraws the result's designation (FN-16, TIME-10). The wiring function exposes the `REF` output as a connection of the target type, such as `.as<::hgraph::TS<::hgraph::Int>>()`. hgraph binds an ordinary reader to its designated endpoint. |
| `return low` of a whole `ref` parameter | `out_.set(::hglrt::designation(low));`, what it designates as the `return` runs, which the result keeps |
| `return price` of an input that is no `ref` parameter, `out =` alike | The input given to the node again as a `REF` input it is active on, `designated_00_`; the node's state keeps which it designates, `designates_`, and where that `REF` ticks the node publishes what it designates now and runs no handler for that wake; `invalidate(out)` follows none |
| `prices[key]` of a dictionary connection in a graph, the key a connection or a value (EXPR-7) | A node of its own, `selected_00_`, written in the graph's struct as a lifted call's is: the dictionary an input observed for its membership (`InputActivity::Structural`), the key an input, a value one a constant connection, and a `REF` result that designates the entry the key names (`::hglrt::select_entry`), none where the dictionary holds none for it; given as a connection of the entry, `.as<...>()` |
| `return items[idx]` of a position of a fixed list that is no `ref` parameter, `out =` alike | Each position given to the node again, `::hglrt::element_port(items, 0)`, as a `REF` input it follows as it follows an input, `designated_01_0_`; the index checked as the `return` runs, `::hglrt::designated_position(idx, 3)` (`value.index_out_of_range` outside the list), and the position it names published, `::hglrt::designation_of(position, ...)`. A designation of another child, of a `ref` parameter's or a field, is reported |
| `if` and `switch` over a connection in a graph (CTRL-2, CTRL-3, CTRL-5, CTRL-6, CTRL-13 to CTRL-15) | A struct `switch_00_` in the graph's struct, a child graph `branch_00_` in it for each branch, and its `wire_`, which compiles each with hgraph's `compile_subgraph` and wires them with `compiled::switch_` (`switch_sink_` where they give nothing) on the condition or the selector. Each branch takes every connection a branch reads that has a value there, whole, and each ordinary value as an `hgraph::Scalar`; which those are, what follows a branch and which paths return are `hgl_lower::graphs`'s. A branch gives its value, the `var` it assigns, or with a `return` in it its statements and then those after the `if` or `switch`, the graph returning what `switch_` gives; the branch of no `else` gives `::hglrt::never`, and a `switch` with no `default` has one that fails with the selector (`::hglrt::unmatched`) |
| `var x = c`, `x = d` of connections (CTRL-14) | `::hgraph::Port<S> x = c;`, `x = d;` |
| `for` over a connection of `dict<K, V>` or `list<T>` (CTRL-7, CTRL-15) | A struct `each_00_` in the graph's struct, its child graph `child_` given the key where the body reads it (only as the whole argument of a temporal parameter, `hgl_lower::graphs::key_read_as_value`: hgraph gives it as a connection), the member, the connections the body takes, whole, and its ordinary values, and its `wire_`, which wires it with `compiled::map_sink_`, one child for each key or position; over `list<T, N>`, the body `N` times, `item` the list's child at each position |
| An `if` over an ordinary value whose value is a connection (CTRL-1, CTRL-13) | C++'s `if` where the graph is built, in a lambda called where it stands; a branch of no value `::hglrt::never` |
| A `const` parameter of a node, a source, a lifted call or a branch (FN-1) | Its `hgraph::Scalar` (`emit/configuration.rs`): of the value's own type where hgraph has a scalar for it, a scalar, an enum, and a list or a nullable of a value the library names (`hglrt::type_name`); and of `::hglrt::configured<T>` for any other value, a tuple, a fixed list, a set, a dictionary, a struct, a list or a nullable of a struct hgraph does not hold as a bundle, which holds it whole as an atomic of hgraph's with the value's hash and equality, so that hgraph interns two nodes as for any configuration. It is given as `::hglrt::configured<T>{value}` where the node is wired, and read as `name.value().held` in a hook; a nullable one is tested against null, and read narrowed, as the optional the `Scalar` holds (ERR-12) |
| A node whose configuration holds an `f64` | Wired with `Wiring::add_unique_node` and not `hgraph::wire`: hgraph interns two nodes whose configurations are `==`, as `0.0` and `-0.0` are ([hgraph 1866](https://github.com/hhenson/hgraph/issues/1866)), until it tells them apart |
| `eval target(value: [1, _, 3])` (TST-5 to TST-8, CEVAL-1) | `::hglrt::testing::evaluate<T>` of an entry that wires core's `replay` instance of each trace written to its input, the target's wiring function, and core's `record` of the result under `"run.output"`; `run target(...)` in real time; for a result whose delta is no T, a node of the entry's own, `held_00_`, which keeps the result read whole at each publication (`::hglrt::testing::hold`, none where the read fails), so that a cell written whole is compared with it (TST-5); a target with no result `::hglrt::testing::run`. A value function's is its lifted node, given the replays as its inputs and each default the call leaves out as configuration: a struct `lifted_0F_O_` (the file and the offset of the call) of the test's namespace, ahead of the test |
| `eval{start: s, length: l} call` (RUN-6, CRUN-2) | The run given `::hglrt::testing::Bounds{.start = s, .length = l}`, each field the configuration names (`lifecycle`); the configuration and then the call's arguments held first, in the order written, where more than one may fail (EXPR-2, EXPR-13). In an integration test that injects `run_config`, a field the eval does not name is the injected one's, `.end = run_config.end()` |
| `inject run_config{length: 3s}` in an integration test (TST-17, RUN-6) | `const ::hglrt::RunConfig run_config = ::hglrt::RunConfig{::hglrt::RunConfig::Fields{.length = ...}};`: `hgl test` supplies nothing, and the defaults stand. `run_config.start` is `(run_config).start()`, a `std::optional` |
| `let seen = eval target(...)` | `const auto seen = ...;`, the recording, read where a trace is |
| Application `demo.app` (UNIT-14) | In its module's unit, `::hglrt::application::Output application_()`, which runs its run statement once; `applications/app-demo-app.cpp`, an entry unit whose `main` is the library's host of that run, `::hglrt::application::main` (`apps`, `assemble`) |
| Its `inject run_config{...}`, `inject global_state` and `put`s before its run statement (UNIT-14, RUN-6) | The function's first statements, in the order written: `const ::hglrt::RunConfig run_config = ...;` of the braces, which `hgl run` has written what starts it gives over, and a store of its own, `::hglrt::application::Store store_`, which the run gives its graph before the entry is wired |
| `eval call`, `run call` (RUN-1, RUN-3) | `::hglrt::application::run<T>(store_, entry, bounds_, mode)`: the entry wires the call and core's `record` of its result under `"run.output"`, as an eval's does; `run` of an entry with no result. The call's arguments are an eval's, held in the order written where that shows (EXPR-2); `bounds_` is each field of `run_config`, or `Bounds{}` where the application injects none. `eval` is a simulation, `run` in real time |
| `let seen = capture(attempt, value)` (ERR-7, ERR-8) | A graph of its own for the call, a struct `captured_00_` in the graph's struct, whose `compose` takes each connection of the call as a port and each other argument as an `hgraph::Scalar`, and calls the callable; the library's `::hglrt::capture<captured_00_>(wiring_, value)`, which is hgraph's `compiled::try_except_` of it with canonical errors (`<hglrt/capture.h>`): a node of its own, which hgraph interns with no other, so that two given `0.0` and `-0.0` stay two |
| `seen.value`, `seen.error` (ERR-7) | `::hglrt::captured_value(seen)`, `::hglrt::captured_error(seen)`: the ports of the boundary's fields |
| A temporal value of `error`, of `result<T, error>` (ERR-2, ERR-4) | `TS<::hglrt::error>`, hgraph's `CanonicalError`; `TS<::hglrt::result<T>>`, registered with hgraph as an atomic; read, published, given as a constant and a trace's cell as a scalar is, each compared by its own equality, which excludes an error's provenance |
| A result taken as its success value in a `when` (ERR-5, ERR-8, ERR-15) | Hold the result before the statement and call `::hglrt::raise(held.error())` on failure. The raised error keeps its context. |
| `failure.context`, and its `.node`, `.phase`, `.evaluation_time` (ERR-4) | `(failure).context`, a `std::optional` of hgraph's `ErrorContext`, and its members of those names |
| A trace written out; `==` and `!=` of two (TST-4, TST-6, TST-7, TST-16) | `::hglrt::testing::Dense<T>`, `Timed<T>`, each cell as its type's family writes it; in an `assert`, `expect_same` or `expect_different` of the two held in order, elsewhere `same`; a trace given to a `const fn`, `publications` |
| A window of a scalar, `rolling_window<i64, 3, 2>`, `rolling_window<i64, 3us, 1us>` (TYPE-11; core's END-9) | hgraph's `TSW<::hgraph::Int, 3, 2>`, `TSWDuration<::hgraph::Int, 3, 1>` (its bounds in microseconds), whose values leave it by age as hgraph has it; a node's `return value` and `out = value` `arrival_.hold(value)`, its `eval` running its handlers in `::hglrt::WindowArrival::run`, which pushes the last value written once they have run, the cycle's one arrival (END-5; `forms::Family::evaluation`), and a source's `::hglrt::window_push(out_, value)`; a read of a node's own window result in its `when`, which would not see what is held, reported; its value read in a hook `::hglrt::window_value(view)`, which fails before it is all valid, or `window_value_checked` where a `result` is taken (`ExprKind::CheckedWindow`); `all_valid` `::hglrt::fn_all_valid`; `delta_value` plain or checked, and `removed_value`, read only into a `result`, `::hglrt::fn_removed_value_checked`; a lifted helper given its `list<V>` (`forms::Family::read_type`), and not called where it is valid and not all valid (FN-11, FN-12; `lifting`); a trace of one, and a source's payload, of its V (`forms::Family::payload`) |
| A temporal struct, tuple and atomic struct (TYPE-9) | `hgraph::NativeTSB<S>`, through `hgraph::native_bundle<S>`, its nominal schema and members, which `structs` writes beside the aggregate with `hglrt::type_named` of it and its delta; a `NominalTSB` of each field's own form where a field is optional; `UnNamedTSB<Field<"0", ...>, ...>`; `TS<S>`, an optional field at any depth among it, which hgraph maps to its `Optional` |
| A read of one whole, a field read in a hook, `out.bid = value`, a field in a graph (TIME-1, TIME-7, EXPR-7) | `::hglrt::whole_value(quote)`, checked; the child's view `::hglrt::child<0>(quote)` read as its form reads; the child's publication; `::hglrt::field_port<0>(port)`, and for an atomic struct `::hglrt::atomic_field<0, S>(wiring_, port)` |
| `all_valid`, a publication, `delta_value` of one (TIME-9, TYPE-16) | `::hglrt::fn_all_valid`, one level deep, an optional field aside; `::hglrt::publish(out_, value);` of a whole value or a delta; `::hglrt::fn_delta_value`, each field that published |
| A temporal nullable scalar or enum (TYPE-14, TYPE-15) | `TS<Optional<T>>`; read `::hglrt::nullable_value(view)`, an optional, `::hglrt::nullable_payload` where a test has proved it holds a value; published `::hglrt::publish_nullable(out_, value);` |
| A temporal value of `set<K>`, `dict<K, V>` (TYPE-9, TIME-1, TIME-7, TIME-9, END-3, END-4) | The `keyed` family: hgraph's `TSS<K>`, `TSD<K, V'>`, `V'` the form of `V`, a scalar, a set, a dictionary or a tuple of such positions (the `structs` family's, an entry read whole as a tuple is), and K a scalar; read whole with `::hglrt::keyed_value(view)`; `return value`, `out = value` as `::hglrt::keyed_publish(view, value)`, hgraph's whole write of a value and its write of a delta (`apply_current_value`, `apply_delta`); `delta_value` an `::hglrt::set_delta` or `dict_delta`; `all_valid` of a dictionary `::hglrt::fn_all_valid`; an entry of a dictionary input in a hook, `book[key]`, read checked as `::hglrt::keyed_entry_value(book, key)` (`keyed_entry_result` into a `result`), and given to `valid`, `modified`, `last_modified` and `delta_value` as `keyed_entry_valid(book, key)` and the like, a key the dictionary does not hold an entry that is not valid (TIME-1, EXPR-7); a trace's cell written whole `::hglrt::testing::Whole<T>{...}`, replayed as its change from the cells before it |
| A temporal `list<V>`, `list<V, N>` (TYPE-9, TYPE-10; `emit/lists.rs`) | hgraph's `TSL<S>`, `TSL<S, N>`, each part of its own form, asked of its own family: a scalar's, a nullable's, a list's, a struct's or a tuple's (`structs`), read and published through it (`endpoints.h`'s `whole` and `publisher`); a part a set, a dictionary, a window or an atomic struct is not written yet, nor a child of a list of structs or tuples named in a hook |
| Its read in a hook, and what a lifted helper is given (TIME-1, FN-12) | `::hglrt::checked_whole(view)`: a growing list needs `valid`, a position that holds no value read as NaN for an `f64` and null for a `V?`, and the read failing for any other; a fixed list needs `all_valid`, which a lifted call's node asks before it calls its helper (FN-11, `lifting`) |
| `return value`, `out = value`, a `yield` of one (TIME-7, TYPE-16) | `::hglrt::publish(out_, value)`: a whole value published as its change, each position given published, a growing list given its size; a delta as that change, a negative size counted from the size held, a fixed list's bounds kept; a delta the list cannot take fails |
| `delta_value`, `all_valid` of one (END-3, END-7) | `::hglrt::fn_delta_value(view)`, `fn_all_valid(view)`: hgraph's own delta of the `TSL`, read as `hglrt::list_delta` |
| `values[i]`, a child named in a hook (TIME-1, TIME-9, END-8, ERR-14) | `::hglrt::child(view, i)`: an endpoint whether or not it is valid, past a growing list's end one that is not, and outside a fixed list `value.index_out_of_range`; given to the operations on an endpoint, and read with `checked_whole` or `checked_whole_result` |
| `pair[0]`, a child of a connection in a graph (TIME-9) | `::hglrt::element_port(pair, 0)`: of a fixed list, the port of the element; of a growing list a node publishes, which has no position when the graph is wired, a node's `REF` designating the position where the list holds it (`GrowingElement`, hgraph's own `getitem_tsl_by_index`), to which a reader is bound |
| A cell of a trace of one (TST-5, TST-9) | `::hglrt::testing::Cell<T>`, of a `Whole<T>` or of the delta |
| Everything else | Reported: `the cpp target does not write ... yet`: a window of any but a scalar; a generic or an abstract struct as one, a list with a part that is no scalar, enum, nullable, list, struct or tuple, a child of a list of structs or tuples named in a hook, a struct or a tuple with a part that is no scalar, enum, nullable, list, struct or tuple, a dictionary whose value is no scalar, set, dictionary or tuple of such; a set's element or a dictionary's key of any but a scalar, a constant connection of a set or a dictionary, an entry of a dictionary's entry; a nullable put in the run's store (hgl_spec issue 165); `all_valid` of a set, a change of an input's activity in `stop`; a `for` over a connection of a set, in a branch or a loop's body the store (a child graph's wiring neither sees nor sets the root's store) or an ordinary value that may fail (hgraph builds every child as the graph around it is built, FN-9), a `return` under a branch selected at runtime in a loop unrolled while the graph is built, branches that assign two `var`s |

Where the driver gives the program's files' names (`hgl_cpp::transpile`,
its `file_name`), each statement written outside a test, a constant's
value, a handler's condition and each part of an application's run
statement is preceded by `#line N "file.hgl"`, its HGL file and line, so
that a failure raised there names them. Where one is written on more
than one line of C++, what it needs run before it and the lines of a
block around the statements inside it among them, each of its lines
that no statement inside it numbers is preceded by the directive again
(`emit::stamped`): C++ counts a line on from the directive before it,
and each line is the statement's own. After each piece, and after
the nodes a test writes ahead of itself, the unit numbers its lines as
its own again (`assemble`, `emit::RESTORE`), since a test's
statements are told to the test host by their lines in the unit, and
the test host names those statements in HGL itself. `hgl_cpp`'s own
tests write no names, and so no directive, but one.

Every C++ name an HGL name gives is made by one function,
`emit::names::identifier`: a plain name (a letter first, a letter or
digit last, no `__`, a lowercase letter in it, none of the words the
C++ standard keeps: its keywords, alternative tokens, attribute tokens,
the identifiers it gives a meaning in places, and its own library's
lowercase macros) is written as it is, and any other with each `_` as
`_1` and a `_` after it, so no two names give one identifier and none
is reserved. The macros one platform's headers define (`issubnormal`,
`htobe16`, `min`) are not listed: every generated file `#undef`s each
plain name of the program it writes, once, after its last `#include`
(`emit::names::program_names`, `assemble::undefined`), and units are
compiled as strict C++23 (`-std=c++23`, never GNU's dialect, in which
`typeof` is a keyword and `linux` a macro). The target's own
namespaces, `m_`, `t_`, `c_` and `i_`, are of a form no name gives, and
so are the names it makes for what a statement holds, `argument_00_` (a
`0` after the `_`).

#### Temporal forms: what a construct lane reads

Step 2's construct lanes each add a *family* of forms to the temporal
code the target writes. This is how that code is shaped and where a
family plugs in.

**What a temporal function is written as.** Temporal function `f` of
module `demo.m`, a node, a source or a graph, is two things:

| Part | Where | What |
| --- | --- | --- |
| Its struct, `::hgl::demo::m::m_::n_::f::impl_` (`nodes::IMPLEMENTATION`, in a namespace of the function's name, so that no member is named as the struct) | The module's unit | A static node (`eval`, `start`, `stop`) or a graph (`compose`), and for a node with `state` or `cache` its `state_type_`, registered with hgraph by a specialization of `hgraph::scalar_descriptor` at the global namespace: the piece closes the module's namespace around it (`nodes::registration`) |
| Its wiring function, `::hgl::demo::m::m_::w_::f(::hgraph::Wiring& wiring_, ...)` | Declared in the header, defined in the unit | Takes a temporal parameter as an `hgraph::Port` of its temporal form and a `const` one as a value function takes it, gives the result's `Port` (or nothing), and is `return ::hgraph::wire<n_::f::impl_>(wiring_, ...)` for a node, `n_::f::impl_::compose(wiring_, ...)` for a graph |

Every call of a temporal function, in a graph or an eval's entry, is a
call of its wiring function (`graphs::temporal_call`), so no other
unit instantiates hgraph's templates of a node, and a node's struct and
state never leave its unit. A generic one is written for each instance
`hgl_lower::instances` collects, `n_::i_::f::of_i64::impl_` and
`w_::i_::f::of_i64`. A node whose configuration holds an `f64` (at any
depth, `nodes::holds_f64`) is wired with `Wiring::add_unique_node`
(`nodes::unique`) and not `hgraph::wire`, until hgraph tells `0.0` from
`-0.0` in its interning ([hgraph 1866](https://github.com/hhenson/hgraph/issues/1866)).
The target's own names in this code end with `_` after a part that has
an `_` not followed by `1`, or a plain word: `wiring_`, `state_`,
`kept_`, `out_`, `state_type_`, `capability_03_`; `names::identifier`
gives no HGL name one of them, and every HGL name is written through it,
a struct's generic parameter through `names::template_parameter`. The
standard library is named from the global namespace, `::std::optional`,
so that a module, a type or a parameter named `std` hides none of it;
a specialization of `std::hash`, written in the global namespace, names
it as it is.

**A lifted call** (FN-6) is a node of its own, a struct `lifted_00_` in
the graph's struct (`lifting::lifted`): each argument the analyser
records as a connection is an `hgraph::In` of its temporal form,
required valid, and each other one configuration, an `hgraph::Scalar<"p",
T>`, given as `static_cast<T>(value)` where the graph is built. Its
`eval` is the call of the function written by the value code's writer
over a body of its own whose locals are the arguments, each an input or
a configuration (`nodes::Role`, below), and a `return` of it, which the `nodes` family publishes. A
lifted input whose parameter has a default or admits null (FN-11) is
not required valid, and binds the fallback where it is not valid (the
table above).

**The context.** While it writes temporal code the emitter holds a
`nodes::Context` (`Emitter::temporal`): its `kind` (`Kind::Hook(start,
when or stop)`, `Source`, `Graph`, `Lifted`, `Entry`), what each local of
the body is (`roles`, a `nodes::Role` each: `Plain`, a C++ local of its
own name, a `state` or `cache` bound by reference among them; `Input`,
the node's view of a temporal input, named as the parameter; `Config`,
a node's `hgraph::Scalar`, read as `configuration::read` has it; `Out`, the view `out_`;
`Connection`, a graph's `Port`), the node's `result`, a graph's
lifted nodes, and a node's `signature`, which says the mode of each
parameter and of the result (FN-16, `references`). A family reads it with `Emitter::kind()`,
`Emitter::role(local)` and `Emitter::view(locals, local)`, and writes
code of another context with `Emitter::in_context`.

**The dispatch point**, `emit/forms.rs`, is where each form is looked
up. A family is a unit struct that implements `forms::Family`; each
method answers one lookup, and every one has a default that answers
nothing, so a family implements only its own:

| Method | Asked | Gives |
| --- | --- | --- |
| `schema(emitter, ty)` | For every temporal parameter, result, `let` of a connection, port and replay, through `Emitter::temporal_schema`; and for the type of a child of a family's own form, through `Emitter::temporal_schema_of`, which reports nothing | hgraph's schema of a temporal value of `ty`, qualified: `::hgraph::TS<::hgraph::Int>` |
| `read(emitter, ty, view)` | For a read of a temporal input's payload in a hook (TIME-1), through `temporal_read` | The C++ of the value, checked when it runs: `::hglrt::checked_value(view)` |
| `publish(emitter, ty, view, value)` | For `return value`, `out = value`, a source's `yield` and a lifted call's result (TIME-7), through `temporal_publish` | One statement that publishes `value`, a T or a `delta<T>`, to `view`: `view.set(value);` |
| `delta(emitter, ty, view, checked)` | For `delta_value` (END-3), through `temporal_delta`, which `Emitter::delta_value` calls in the form the analyser recorded (`ExprKind::CheckedDelta` is the place that takes a `result`, ERR-14) | `::hglrt::fn_delta_value(view)`, or `fn_delta_value_checked` where `checked`: a family whose delta needs a conversion gives the library's `endpoint_read` specialization for its schema (`endpoints.h`) |
| `constant(emitter, ty, value)` | For an ordinary value where a connection is expected (FN-3), through `temporal_constant` | A `Port` of the value wired as a constant on `wiring_`: `::hglrt::constant<T>(wiring_, value)` |
| `payload(emitter, ty)` | For the payload of a source of `ty` and the `T` of a trace of `ty` (`Dense<T>`, `Timed<T>`, `evaluate<T>`), through `Emitter::payload_type` | The C++ type a publication of `ty` takes and a trace of it holds, where it is not `ty`'s own: a window's V (TIME-7, TST-16) |
| `read_type(emitter, ty)` | For each input of a lifted call (FN-6), through `Emitter::read_type` | The HGL type a read of the input's payload gives, which the helper is given, where it is not `ty`: a window's `list<V>` (TYPE-11) |
| `evaluation(emitter, ty, handlers)` | For the `eval` of each node with a result of `ty`, through `Emitter::evaluation` | The body of `eval` around its handlers' C++, where the result's form holds what the evaluation publishes and publishes it once it ends (END-5): a window's, which takes one value a cycle (TYPE-11) |
| `cell(emitter, scope, ty, value)` | For each cell of a trace written out of `ty` (TST-5), through `trace_cell` | The C++ of the cell, an element of `hglrt::testing::Dense<T>` or `Timed<T>`: a T, or for a T whose delta is no T a `Cell<T>` |
| `statements(emitter, scope, statements, depth, text)` | For every run of statements written, a body's, a branch's, a loop's, from the top of `statements::statements` | `true` once it has written the run whole, each statement with what follows it, the statements not its own through `Emitter::statement`; `false` for a run not its own |
| `statement(emitter, scope, statement, depth, text)` | For every statement written, of value code too, from the top of `statements::statement_text` | `true` once it has written the statement into `text`, `depth` levels in; `false` for one not its own |
| `expression(emitter, scope, value)` | For every expression written, of value code too, from the top of `expressions::expression` | The C++ of `value`, or `None` for one not its own |
| `wiring(emitter, scope, value)` | For an expression of a graph that is a connection, through `Emitter::wiring_form` | The C++ of the `Port` it gives |

*Not mine* is `None` (or `false`): the next family is asked. A form that
is the family's and that it cannot write yet is reported by the family,
with `Emitter::unsupported(at, "...")`, or `Emitter::missing(at, "...")`,
which reports and gives `0` to stand in its place; the piece is then left
out, and every test that reaches it is listed with that reason. A
family that writes a form asks the dispatch point for the forms inside
it (`emitter.expression(scope, part)`, `emitter.temporal_schema(element,
at)`, or `emitter.part_schema(element)`, which reports nothing where no
family writes the part, so that the whole form is reported), never another family directly, so that a form is written one way
wherever it is.

`forms::FAMILIES` lists the families, and each lookup asks them in that
order and takes the first that answers: the spine's are, in order,
`scalars`, `tests`, `sources`, `nodes`, `graphs` and `endpoints`, after
`control`, which writes each run of a graph's statements, so that a
branch selected at runtime sees what follows it, and the construct
lanes' ahead of them: `nullables`, `structs`, `keyed`, `lists`. A lane
puts its own ahead of them, since `endpoints` answers for any read of an
input, `graphs` reports any connection in a value's place, and the
value code's writer, after all of them, writes or reports the rest: the
lanes' are, in order, `windows`, `references`, `apps`, `scheduling`,
`activity` (an input's activity, an enum's temporal form), `lifting`
(lifted calls, `capture`, `error` and `result`, a `?` in a `when`) and
`control` (`if`, `switch` and `for` over connections in a graph).

**The hooks** in the value code's writer are one line each:
`statements::statements` asks `Emitter::form_statements` before it
writes a run a statement at a time; `statements::statement_text` asks `Emitter::form_statement` before its
own `match`, inside `statement`, which writes what a statement needs
before it (`Scope::before`) first; and `expressions::expression` asks
`Emitter::form_expression` before its own (after the read of a compound
assignment's own place). So a family sees every statement and every
expression, nested ones among them, in hooks, sources, graphs, tests
and value functions, and answers only for its own. Besides them,
`types::cpp_type` writes core's capabilities as the library's types
(`endpoints::capability_type`) and the delta of a scalar as the scalar.
`composites::struct_declaration` writes what `structs` says of a struct
beside its traits (`Emitter::bundle_traits`), and a family asks the form
of a part of its own with `Emitter::temporal_schema_of`, which reports
nothing. A source's payload is what its yields give, a delta where each
is one (`sources`); an eval whose result's delta is no T keeps the value
its result held, read by a node of the entry's own as its family reads
it and given to `hglrt::testing::hold` (`tests`).

**What no family writes** is reported, never written wrongly: a type
lookup reports "a temporal value of", "a read of a temporal value of",
"a publication of", "the delta of", "a constant connection of" or "a
trace of" the type; a connection no family wires, by its form ("a
`capture`", "an `if` that gives a connection", "a field of a
connection", "this connection": `forms::unwired`); a statement or an
expression no family claims falls to the value code's writer, which
reports each form it does not write ("a hook", "a `yield`", "this
expression"). Each reason is a key of `crates/hgl_cpp/ci/unsupported_cases.toml`.

**Adding a family**, as `nullables` adds `T?`:

1. `crates/hgl_cpp/src/emit/nullables.rs`: a module comment with the
   table of what it writes, and `pub(super) struct Nullables;` with
   `impl Family for Nullables`. `schema` gives
   `::hgraph::TS<::hgraph::Optional<T>>` for `Type::Nullable(inner)`
   where `emitter.temporal_schema(inner, at)` is a `TS` of a scalar, and
   `None` for any other type; `read` gives what a read of the input's
   value is in HGL (a published null is `std::nullopt`); `publish` the
   statement that publishes a `std::optional<T>`; `delta` the library's
   native, once `endpoints.h` reads the form (it does for a nullable
   scalar); `constant` and `cell` likewise.
2. `mod nullables;` in `emit.rs`, and `&super::nullables::Nullables,`
   at the head of `FAMILIES` in `forms.rs`.
3. A fixture and its goldens in `crates/hgl_cpp/tests` (`fixtures/`,
   `HGL_UPDATE_GOLDENS=1 cargo test -p hgl_cpp --test emit`), and the
   fixture run on the SDK in `crates/hgl_cli/tests/cpp.rs`.
4. `crates/hgl_cpp/ci/check_core.py --write-list`, never by hand: the
   cases it now runs come off the list.

A lane that needs a lookup the trait lacks asks for it: it is added to
`Family` and to this table on the spine's branch, so every lane has the
same one.

C++ compiles a unit whole, so a piece that calls one not written is
left out too, and a test that reaches one is listed with the first form
it reaches, as `hgl_lower::pieces` finds it. What a
call that leaves an argument out gives it is `hgl_lower::calls`'s.

What the target takes of the runtime library ("The C++ runtime
library" below): its CMake project, configured with `HGL_HGRAPH_SDK`,
`HGL_HGRAPH_DEPS` and `BUILD_TESTING=OFF`, built and installed;
`hglrt/hglrt.h`, the fixed include set, and `hglrt/host.h`, the test
host's light header, which an entry unit includes alone; core's natives
in `hglrt`, a native being defined where its name followed by `(` is in a
header under `include/`, and core's enums its natives take where it
declares an `enum class` of the name; and its test host,
`hglrt::testing`. An executable is run as `EXE RESULTS NAME...`, and the
results are `start N`, `passed N`, `failed N LINE TEXT` and `unloaded N
TEXT` (`hglrt/host.h`), read by `cpp/results.rs`, with LINE a line of
the module's unit. The
driver runs a test held to a time limit (TST-17) in a process of its
own, which it ends at the limit; the host's own limit, `NAME@MICROSECONDS`,
is not used yet.

#### The build driver

| Variable | What it names |
| --- | --- |
| `HGL_HGRAPH_SDK` | hgraph's C++ SDK prefix. Defaults to `../libexec/hgraph-sdk` beside the resolved executable |
| `HGL_HGRAPH_DEPS` | The SDK's dependency prefix. Defaults to `../libexec/hgraph-deps` beside the resolved executable |
| `CXX` | The compiler, of the platform's family: GCC 15 or later on Linux, Apple clang 21 or later on macOS. Otherwise, on Linux, the first of `g++-17`, `g++-16`, `g++-15`, `g++` on the `PATH` that is GCC 15 or later, and on macOS `clang++`. A compiler is known by the macros it defines (`-dM -E`), not by its name |
| `HGL_CPP_FLAGS` | Flags added to every compile and link, the runtime library's too: `-O2`, `-g`, the sanitizers |
| `HGL_NATIVE_CACHE` | Where builds are kept; otherwise `hgl/native` in the user's cache directory |
| `CMAKE_BUILD_PARALLEL_LEVEL` | How many jobs a build, and the executables run, take at once |

A toolchain is prepared once and kept under the cache in
`toolchains/<hash>`, the hash of the SDK's and the dependencies' paths
and the SDK's own files, the compiler, its family and version and what
it reads from the environment (`MACOSX_DEPLOYMENT_TARGET` and `SDKROOT`
on macOS), the linker, the flags and the runtime library's sources,
under a lock: the runtime
library is configured, built and installed with CMake; a small CMake
project of the driver's own, a program linked with it, is configured,
and CMake's file API describes how CMake compiles and links it, which
is the description every generated unit is built with
(`description.json`); and the library's header is precompiled with it,
beside it as `hgl_pch.h.gch` for GCC and `hgl_pch.h.pch` for clang.
What differs between the two families is said in one place, `Family` in
`crates/hgl_cli/src/cpp/sdk.rs`. On Linux, links add
`-Wl,--disable-new-dtags`, so that the SDK's libraries find their
dependencies, and `-fuse-ld=lld` where `ld.lld` is on the `PATH`; an
executable is run with the libraries' directories in `LD_LIBRARY_PATH`.
On macOS, links are Apple's, and an executable finds every library
through CMake's run paths (`-rpath`), which macOS's loader reads for
each library on the way; it is run with no variable, as
`DYLD_LIBRARY_PATH` naming the dependency prefix would load
conda-forge's libc++ in place of the system's.

A program's workspace is `programs/<hash>` under the cache, keyed by its
source roots, or the directory named by `--out` for `hgl build` and
`--keep` for `hgl run` and `hgl test`. The driver holds the workspace
lock through the build and until its last executable ends.

`hgl build ROOT...` checks the program, writes the generated files
and builds its units and application executables. `--tests` builds a
test executable for each module with tests. The command reports the
workspace and artifacts. `--format json` reports a versioned object
with `workspace`, `target`, `units`, `applications`, `tests` and
`reused`.

After Ninja succeeds, the driver records a stamp and the artifact
inventory. The key includes the source contents, core, the compiler
version and executable contents, the prepared toolchain and flags,
and any application replacements. The inventory records whether the
tests were built. On a matching stamp with the needed artifacts,
`hgl run` and `hgl test` use the existing executables directly. A
rebuild clears the previous stamp before checking and leaves it
absent if the build fails or is interrupted.
An ordinary build or run checks rejection owners as source. When a
test build omitted those owners, its stamp covers test commands.

The driver writes each generated file and `build.ninja` where its
contents change. Ninja compiles each module unit with the precompiled
header. A test entry uses the light host header (`hglrt/host.h`) and
each executable links its entry, module unit and the units it calls
into. A unit is linked whole.
Executables run side by side. File stems replace each dot of a module
or application path with a hyphen and mark folded uppercase letters
with `+`. The `mod-`, `type-`, `test-` and `app-` prefixes
distinguish artifact roles. `tm.pricing.app` and `tm.Pricing.App`
give `bin/test-tm-pricing-app` and `bin/test-tm-+pricing-+app`
on macOS. Reports keep the original HGL paths.

`hgl emit --target cpp --out DIR` writes the files. `hgl run --target
cpp` uses the application's executable, `run/<application-stem>`
(UNIT-14). It runs it as `EXECUTABLE RESULTS` (`cpp/application.rs`).
The entry unit, executable
and run results share a stem such as `app-demo-pricing-app` for
`demo.pricing.app`. The executable's host
(`hglrt/application_host.h`) runs the run statement once and writes
RESULTS (`start`, `published TIME TEXT`, `stopped`, `stopped none`,
`host`, `failed TEXT`). `hgl` reads it and reports the outcome. A failed
run prints its failure to standard error and exits with status 1. If the
application could not be run, `hgl` exits with status 2. TIME is the
instant in UTC, `1970-01-01T00:00:01.000001Z`, and TEXT the value as the
library's `text_of` writes it, as a trace's text is. What the program
and hgraph print goes to standard error. An interrupt of `hgl`, or a
request for it to end, is passed to the executable down its standard
input, `stop`; an interrupt from a terminal reaches it as well. A run in
real time asked to stop is stopped through hgraph's executor, as a test
past its time limit is (one asked before it starts is stopped as it
starts). When the run stops cleanly, `hgl` exits with status 0 and prints
the publications recorded up to the stop. A simulation
asked to stop, and a run interrupted a second time, end the executable
at once, `failed interrupted`, and a second interrupt of `hgl` ends it
too. The executable's standard input closed is such a request: what
started it is gone.

The crate's tests compare the C++ written for its fixture programs,
`demo.calc`, `demo.values` (value code of every form written),
`demo.composites` (structs, sets, dictionaries and deltas),
`demo.temporal` (the temporal forms written), `demo.windows` (windows),
`demo.references` (`ref` parameters and results), `demo.apps` (the
store and applications), `demo.timing` (schedulers, alarms and
sources with hooks), `demo.activity` (an input's activity and an
enum's temporal form), `demo.lifting` (lifted calls and captures),
`demo.control` (control flow over connections), `demo.composites`
(structs, sets, dictionaries and deltas), `demo.structs` (temporal
structs, tuples and nullables), `demo.keyed` (a temporal set and
dictionary), `demo.lists` (the temporal list) and `demo.nested` (forms
made of other families' forms), with goldens under
`crates/hgl_cpp/tests/goldens/` (`HGL_UPDATE_GOLDENS=1 cargo test -p
hgl_cpp --test emit` writes them again; core's module is left out of
them) and check what blocks what. `crates/hgl_cli/tests/cpp.rs` checks
the driver: what is said with no SDK and with a compiler not of the
platform's family, `emit` and `run`; and, given an
SDK, the whole route, that a failed build runs no older executable,
that `demo.values`, `demo.composites`, `demo.temporal`, `demo.windows`, `demo.references`, `demo.timing`, `demo.activity`, `demo.lifting`, `demo.control`, `demo.structs`, `demo.keyed`, `demo.lists` and `demo.nested` pass, where a failure is
reported, applications run with their output, the store and a failure,
and a run in real time stopped by an interrupt of `hgl` alone. Run the SDK
suite with `scripts/check-cpp-sdk.sh 16 8` to share up to 16 native build
jobs among eight concurrent cases. The Linux and macOS C++ jobs run the
SDK suite and `check_core.py` (below), and both again with
`HGL_CPP_FLAGS` naming the address and undefined-behaviour sanitizers:
on macOS one after another, on Linux side by side on separate runners.
The generated code compiles with `-Wall -Wextra -Werror`.

The target's progress is counted in cases: core's runtime cases, and
later the fixture programs' tests. Each case the target does not run is
listed with its reason in
[`crates/hgl_cpp/ci/unsupported_cases.toml`](../crates/hgl_cpp/ci/unsupported_cases.toml).
The list covers the remaining nullable store put case. Build `hgl` and run
`check_core.py`, which runs the pinned core's `tests/conformance`
and `tests/core_conformance` on the target, including integration tests:

```sh
cargo build --locked --bin hgl
python3 crates/hgl_cpp/ci/check_core.py target/debug/hgl spec/hgl_core
```

It compares the runner's full discovery and execution lists and prints
the outcomes and counts for each suite. A case can be reported as not
run only for its source-located diagnostic of the target, `the cpp
target does not write ... yet`, under the form the list gives it. A
rejection case can fail only by not being rejected, and only where it is
listed under the file's `[rejections]`, by `suite::file::declaration`
and a reason that names the rule not yet built; the analyser's and the
driver's corpus tests read the same list. An assertion or source
failure, a host that did not run a case, a new gap, or a listed case
that passes, is rejected or disappears fails the check. `--write-list`
writes the list from a run in place of checking it; read the difference
before it is committed. Its own tests are
`python3 -m unittest discover -s crates/hgl_cpp/ci`.

### Pull request checks

| Platform | Automatic pull request checks | C++ check before merge |
| --- | --- | --- |
| Linux | Hosted formatting, Clippy and Rust workspace tests | Hosted C++ job runs runtime CTests, SDK tests and pinned core cases, plain and sanitized |
| macOS | Hosted Rust workspace tests | A maintainer dispatches **macOS C++ target** on the reviewed repository branch. It adds the workspace tests and the same C++ checks on Apple silicon |
| Windows | Hosted Rust workspace tests | C++ build and execution remain outside CI |

The macOS C++ workflow runs on pushes to `main` and on manual dispatches
of branches in this repository, when `HGL_SELF_HOSTED_MACOS` is `true`.
Once the workflow is on `main` and the reviewed branch contains it, a
maintainer with write access can dispatch that branch.

```sh
gh workflow run cpp-macos.yml --repo hhenson/hgl-compiler --ref BRANCH
```

Check the dispatch result for the commit being merged. Pull request events
use hosted runners only.

### The C++ runtime library

`runtime/cpp` is `libhglrt`, the C++ library that the code of the
[C++ target](cpp-target.md) links: what that code calls and hgraph does
not have. It is
a skeleton that the target's first step fills. It is built on hgraph's
runtime and wiring libraries alone (`hgraph::runtime`, `hgraph::wiring`):
`hgraph::core` would bring hgraph's standard library, which no generated
code uses. Its headers take hgraph's front end for generated code,
`<hgraph/types/generated.h>` (static nodes, `wire<Node>` with complete
positional arguments, compiled children and their error capture), and
not `graph_wiring.h`, whose operator dispatch, named arguments and type
resolution generated code does not use: the test `hglrt_takes_no_stdlib`
fails where a public header brings it in.

| Header (`include/hglrt/`) | Holds | Rules |
| --- | --- | --- |
| `hglrt.h` | The one header generated code includes: all below | |
| `values.h` | HGL's scalars as hgraph's types (`i64`, `f64`, `bool`, `str`, `bytes`, `date`, `time`, `datetime`, `duration`, `civil_datetime`, `zoned_datetime`, `tenor`); `list<T>`, a `std::vector<T>` with a hash, registered with hgraph as an atomic; `delta_t<T>`, a scalar's, a nullable's (`nullable_delta`), a tuple's and a list's delta (`list_delta`); an enum's names, `EnumNames`, which the enum gives through `hgl_enum` (found by argument-dependent lookup) and by which the library names it, prints its members (`Side.buy`) and registers it with hgraph as an atomic; `value_form<T>`, the name, text and hash of a type another header declares (keyed.h's sets and dictionaries, capture.h's errors and results) | TYPE-1, TYPE-2, TYPE-4, TYPE-7, TYPE-16 |
| `failure.h` | `error` (hgraph's `CanonicalError`), `result<T>` (`std::expected`); `Failure`, hgraph's `ExecutionFailure` with the place it was raised at; `fail`, `fail_uncoded`, `raise`; the catalogued codes it raises | ERR-1, ERR-2, ERR-4, ERR-5, ERR-8 |
| `capture.h` | `capture<G>`, hgraph's `compiled::try_except_` of the graph the compiler writes for a captured call, compiled with `compile_subgraph` given the call's other arguments, with canonical errors; `captured_value`, `captured_error`; a `result` registered with hgraph as an atomic; the name, text and hash of an `error` and of a `result` (`value_form`) | ERR-2, ERR-4, ERR-7, ERR-8 |
| `checked.h` | Reads checked when they run: `unset_read`, `checked_value`, `checked_result`, `item` (a reference into a list that outlives the read, the element itself of one that ends with its statement) | TIME-1, ERR-14, EXPR-3 |
| `scalar.h` | Core's natives of numbers and the ordered types: checked `i64` arithmetic, `f64` and mixed arithmetic, every division of numbers under each zero policy, exact numeric equality with tolerances, order and `compare_`, the Boolean forms; core's `ZeroPolicy` and `CmpResult` | NUM-1 to NUM-12, OPS-2, OPS-3, OPS-5 |
| `temporal.h` | Core's natives of durations and instants: a duration's arithmetic, rounded and checked; a `datetime`, `civil_datetime`, `date` or `zoned_datetime` moved by a duration, and the duration between two; `date + time`; zoned equality; a zone's literals, checked against the provider's catalogue and a `zoned_datetime`'s offset against its zone | OPS-1, OPS-2, OPS-10, NUM-12, LEX-12 |
| `text.h` | Core's natives of `str`: concatenation and repetition | OPS-1 |
| `conversions.h` | Every `convert` of core's: those that cannot fail, and those that can under `invalid`; core's `Rounding` and `InvalidPolicy` | OPS-4, OPS-6 to OPS-8 |
| `tenors.h` | Core's natives of `tenor`, which is hgraph's `Tenor` (counts and form alone): its arithmetic, order and text, and the parts core's calendar candidates use, `business_days`, `moved`, `weekend`, `same_month` | TYPE-17, TEN-1 to TEN-9 |
| `lists.h` | `append`, `extend` | LIST-1 to LIST-3 |
| `collections.h` | `set<K>`, `dict<K, V>`; a key's hash, alike for keys that are `==`; `entry_at`, `entry`, core's `get`; `set_delta`, `dict_delta`, a list's delta's properties as HGL's; `fields<S>`, `held_value`, `held_as`, `parents<S>`, `inherits`; `enum_member` | TYPE-3, TYPE-7, TYPE-14, TYPE-16, EXPR-3, ERR-6, OPS-2 |
| `nullables.h` | A nullable scalar's temporal form, `TS<Optional<T>>`: its payload read checked, `nullable_value`, `nullable_payload`; `publish_nullable`; a `std::optional<T>` registered with hgraph as an atomic | TYPE-14, TYPE-15, TIME-1, TIME-7 |
| `structs.h` | A struct's (`NativeTSB`, `NominalTSB`), a tuple's (`UnNamedTSB`) and an atomic struct's temporal forms: `child`, `field_port`, `whole_value`, `publish`, their `endpoint_read` (delta, `all_valid` one level deep with an optional child aside, the value held), `atomic_field`, a trace's cell of a struct published as its change from what each field held (`testing::detail::Publisher`), an atomic struct's whole (`whole_publication`), and a struct's name and text (`value_form`); a struct and a tuple as a part of another form, read whole and published (`whole`, `publisher`), and each part of one through its own form; `list_value`, the value schema a struct's nominal bundle holds a list field as; `atomic_trace<S>`, what a trace of an atomic struct is of, whose cell, the whole struct, is its delta, and its recording compared with a trace of the struct's whole values (`difference`) | TYPE-9, TYPE-16, TIME-1, TIME-7, TIME-9, EXPR-7, TST-9 |
| `keyed.h` | The temporal form of a set and of a dictionary, hgraph's `TSS` and `TSD`: `keyed_form`, an HGL value of a form to and from hgraph's, a tuple's as hgraph's bundle among them; `endpoint_read` of each, its delta as `set_delta` or `dict_delta` and a dictionary's `all_valid`; `keyed_value`, a whole read checked when it runs; `keyed_entry_value`, `keyed_entry_valid` and the like, an entry of a dictionary as an endpoint whether or not it is held; `keyed_publish`, of a whole value through hgraph's `apply_current_value` and of a delta through its `apply_delta`; `fn_invalidate` of each; a trace's whole cell of each published as its change (`testing::detail::Publisher`); their names and text (`value_form`) | TYPE-9, TYPE-16, TIME-1, TIME-7, TIME-9, EXPR-7, END-1 to END-4, END-6, TST-9 |
| `endpoints.h` | Core's nine operations on an endpoint, each a template over the hgraph view it is given; `delta_value` plain and where a `result` is taken (`fn_delta_value_checked`); `endpoint_read`, through which a temporal form says how its delta is read, a scalar's and a nullable scalar's written; and how a form is read whole and published as a part of another, `schema_value`, `whole`, `publisher` and `publish_as_form`, which each form's header specializes: a scalar's and a nullable's here | TIME-9, END-1 to END-10, ERR-14, TYPE-9, TIME-1, TIME-7 |
| `capabilities.h` | `Logger`, `Clock`, `Scheduler`, `Alarm`, `GlobalState`, each made from its hook's hgraph view, a member function for each property; `log`, `schedule` four ways, `un_schedule`, `reset`, the store's `get`, `has`, `put` of every ordinary value; core's `LogLevel` | CAP-1 to CAP-6, TIME-8 |
| `windows.h` | A window, hgraph's `TSW` and `TSWDuration`: `endpoint_read` of one (its delta the value that arrived, none at an exit alone; `all_valid` at its minimum); `WindowArrival`, what an evaluation writes of its window result, held and pushed once (END-5, TYPE-11), or discarded with the samples and their times on `invalidate(out)` (END-4); `window_push`; its value read checked, `window_value` and `window_value_checked`, which stand for the one read in which hgraph differs, reading the values held below the minimum (`tests/hgraph_window_read.cpp` shows it on hgraph alone); `fn_removed_value_checked` | TYPE-11, END-3, END-9, ERR-14 |
| `control.h` | What a branch of an `if` or a `switch` over a connection wires where the language gives it nothing: `never`, a connection that never ticks, the branch of no `else`; `unmatched`, the branch of no `default`, which fails with the selector's value as it is selected | CTRL-3, CTRL-6, CTRL-13 |
| `list_endpoints.h` | The temporal form of a list: what it holds whole (`schema_value`), how it is read whole (`whole`), published (`publisher`, `publish`), and read as its delta and its `all_valid` (`endpoint_read`), each element through its own form's; a child of one (`Child`, `child`) and the port of an element (`element_port`, through `GrowingElement` for a growing list's). A tuple's form is `structs.h`'s. hgraph 0.9.23's `TSL` is built on as it is: none of TIME-7's ticks, TIME-9's validity or TYPE-16's deltas needs an adaptation (hgraph issues 1822, 1827) | TYPE-9, TYPE-10, TYPE-16, TIME-1, TIME-7, TIME-9, END-3, END-7, END-8, ERR-14, TST-5 |
| `sources.h` | `constant`, a node that publishes a value once at the start, each a node of its own (hgraph would intern two whose values are equal under `==`, `0.0` and `-0.0` among them); `resume`, which drives a source's body under the timing of `yield` | FN-3, SOURCE-1 to SOURCE-4 |
| `references.h` | `designation`, what a `REF` input designates now, or the empty reference: what a `ref` result is given; `designated_position`, an index of a fixed list a `ref` result designates, checked, and `designation_of`, the position it names; `select_entry`, a dictionary's entry a key selects in a graph, designated | FN-16, TIME-10, END-10, EXPR-3, EXPR-7 |
| `timing.h` | `ticked`, whether an input published or lost its value in the evaluation, by which a node with a timer tells its wakes apart; `Lent` and `Lending`, a capability lent to a source's body for one resumption | TIME-3, TIME-9, SOURCE-1, TIME-8 |
| `generator.h` | `generator<T>`, the coroutine type of a source's body: `std::generator<T>`'s shape for a value type (a move-only view whose input iterator resumes the body to its next `co_yield`, and throws again what it lets out), which not every standard library has | SOURCE-1 |
| `host.h` | What the entry unit of a test executable needs, and no hgraph header: registration and `main`, which holds a test to its time limit | TST-14, TST-15, TST-17 |
| `traces.h` | Traces, dense and timed, of scalars, nullables, tuples and lists, each cell whole or a delta, a whole one published as its change from what the cells before it left, each position of a list or a tuple followed at any depth (`detail::Publisher`), a list of lists among them; their comparison and where two first differ; replay items and what a `const fn` is given; the text of a value | TST-4 to TST-7, TST-9, TST-16, CEVAL-2 |
| `testing.h` | The rest of the test host: `check`, `raises`, `expect_same`, a run in simulation or in real time, from no earlier than the earliest start to no later than the latest end (one with no end ends there), its recording, and the values its result held | TST-2 to TST-11, TST-17, CRUN-1 to CRUN-8, CEVAL-1 to CEVAL-6 |
| `application.h` | The run of an application: `Store`, what it sets before its run statement; `run<T>`, the test host's run of its entry on a graph given that store, and its output as text, each publication's instant and value | UNIT-14, RUN-1 to RUN-4, CRUN-1 to CRUN-8 |
| `application_host.h` | What an application's entry unit needs, and no hgraph header: `main`, which runs the run statement once, writes what became of it to a results file, and stops a run when its host asks: down standard input, or by an interrupt | RUN-3, CRUN-6 |

A native is a function named as the emitter names the candidate
(`hgl_cpp`, `names::native_candidate`): the operator's name and the type
of each parameter, `add__i64_i64`. One with a generic type parameter is a
template the generated code instances, `lt__T_T<i64>`; one with a generic
value parameter takes it as a template argument,
`div__f64_f64<ZeroPolicy::ieee>`, and so do the candidates that differ in
it alone and in what they give. The candidates of `convert` that take one
type and give different ones share a name, `convert_i64`, which is a
template whose first argument is the type converted to,
`convert_i64<f64>`, the `T` of `convert<T>(value)`; one that can fail
takes `invalid` after it, `convert_str<i64, InvalidPolicy::fail>`. Core's
enums that the natives take or give (`ZeroPolicy`, `CmpResult`,
`Rounding`, `InvalidPolicy`, `LogLevel`) are the library's, their members
in core's order. One that can fail takes the place it is called from, a defaulted
`std::source_location`, which its failure carries: the HGL file and line
where the generated code sets them with `#line`, which `describe` says;
a place in the library itself or in a test's body, which the test host
names by its statement, is no place of the program's and is not said.
The run's store holds every value as itself, in the library's own
`detail::Stored<V>`, which hgraph keeps whole as an atomic, one for
each type: hgraph's own schema of a struct is a bundle, which it would
keep field by field and give back as no struct, and a tuple, a set, a
dictionary and a delta have none. hgraph keeps that place
for a failure of `start` or `stop`, which it nests; one of an evaluation
it throws again as a new `ExecutionFailure`, which has none. A zoned
datetime moved by a duration takes its zone's offset from hgraph's
time-zone provider for the process, as hgraph's own codecs do: a value
function is given no run's store, which a provider of the run's would be
in.

An endpoint operation is given the view the hook has, `In<"price",
TS<i64>>`, `Out<TS<i64>>` or a child of one, and is named as an exported
native is, `fn_valid`. A capability is made at the top of the hook that
injects it, once, from the view hgraph gives that hook:
`hglrt::Scheduler scheduler{view, hglrt::Hook::evaluation}`. hgraph's
`NodeScheduler` has a wake it delivers due and no longer pending in the
evaluation it brings about, so that what is pending is what is still to
come and `is_scheduled_now` stays true after `un_schedule` or `reset`
(CAP-4); a tag goes to hgraph after a mark,
since hgraph keeps no wake under the empty tag. An evaluation with no wake
due and no input modified is no wake: hgraph evaluates the node at a
wake that was cancelled. The alarm keeps its one wake, the earliest asked
for (CAP-5), under a tag of its own in the same `NodeScheduler`, which
arms the node for it again after an evaluation at an input's tick before
it is due; the alarm made in an evaluation takes the wake due then.

An enum is named by `hgl_enum`, found by argument-dependent lookup, which
each of core's enums the library declares has, and which the compiler
writes for each enum of a program, in the enum's namespace and right
after it: `constexpr ::hglrt::EnumNames<Side, 2> hgl_enum(Side) noexcept
{ return {"demo.trade::Side", {{Side::buy, "buy"}, {Side::sell, "sell"}}};
}`, the type's name qualified with its module's path, and each member
with its HGL name. It is all an enum needs: its HGL name, its text
(`Side.buy`) and its hgraph schema come from it.

A list is `hglrt::list<T>` and not a bare `std::vector<T>` because hgraph
hashes a value it holds as configuration, a source's items, through
`std::hash`, which no program may give a `std::vector`.

An executable of a module's tests calls `hglrt::testing::main` from its
own `main`: `EXECUTABLE RESULTS TEST...` runs the tests named, in order, and
writes RESULTS (`start N`, `passed N`, `failed N LINE TEXT`, `unloaded N
TEXT`), which the build driver reads (`crates/hgl_cli/src/cpp/results.rs`).
`LINE` is the line of the
last statement the test marked with `statement()`. A test held to a time
limit (TST-17) is named `NAME@MICROSECONDS`: it runs on a thread of its
own, and at its limit its runs are asked to stop, as a host stops a run in
real time, and it has failed; one that has not ended five seconds later
ends the process, with the status 3. Its entry unit can include `host.h`
alone, which compiles in 0.06 s where `hglrt.h` through the precompiled
header takes 0.57 s (GCC 15, `-O0`).

Built with CMake, once for each SDK. A build is given:

| Variable | Is |
| --- | --- |
| `HGL_HGRAPH_SDK` | The SDK's install prefix (cache variable or environment) |
| `HGL_HGRAPH_DEPS` | The dependency prefix (cache variable or environment) |
| `BUILD_TESTING` | The unit tests (default on) |
| `HGLRT_SANITIZE` | The library and its tests under the address and undefined-behaviour sanitizers, which what links it inherits (default off) |

and gives `libhglrt.so` (`libhglrt.dylib` on macOS), the target `hglrt::hglrt`, and a CMake package of
it (`-Dhglrt_DIR=` the build directory, or its install prefix). The unit
tests use [doctest](https://github.com/doctest/doctest): hgraph's own use
Catch2, which its SDK does not install, and doctest is the smallest
established framework, one header. An installed doctest is used, or its
release is fetched. The library's own C++ is built with `-Wall -Wextra
-Werror`, and laid out by `runtime/cpp/.clang-format`.

#### The SDK

`scripts/provision-hgraph-sdk.sh TAG DIR` builds hgraph's shared C++ SDK
at the tag `TAG`, 0.9.12 or later, and installs it into `DIR`, as hgraph's
CI builds its runtime without Arrow: Release, shared libraries,
`HGRAPH_WITH_ARROW=OFF` ([hgraph 1839](https://github.com/hhenson/hgraph/issues/1839)),
which leaves out Frame, Series, the table codecs, the standard library,
Python and the extensions, against the dependency prefix
`HGL_HGRAPH_DEPS`. Arrow's packages are kept from being found, and the
date/tz library is always hgraph's own pinned release, fetched and built
into its runtime library, so that what the SDK is does not turn on what
else a prefix holds. The SDK's libraries are given the prefix's library
directory as their run path. Where the prefix does not exist the script
makes it with micromamba from conda-forge: fmt, spdlog and simdjson, with
CMake and Ninja, at fixed versions, with micromamba given a home directory
of its own, so that nothing outside the prefix and a temporary directory
is written. The compiler is the caller's: GCC 15 or later on Linux, Apple
clang 21 or later on macOS (`clang++`, the default there); the script runs
under macOS's own bash 3.2. `DIR/hgl-sdk.txt` says the tag, its commit,
`arrow=off`, the compiler and the prefix; an SDK of the tag built
otherwise is not taken for one. The SDK is 14 MB (54 MB with Arrow), and
its runtime and wiring libraries need fmt and spdlog alone; it builds in
28 to 33 s with 12 jobs here (77 s with Arrow). On a macOS machine (Apple
Silicon, Apple clang 21) it is 12 MB and builds in 34 s with 12 jobs, 56 s
with the prefix made and hgraph cloned. On macOS conda-forge's fmt,
spdlog and simdjson bring conda-forge's own libc++ into a process beside
the system's, which the SDK and everything built against it link; it
re-exports the system's libc++abi, so exceptions and type information
have one runtime, and an exception thrown in fmt is caught by type
elsewhere.

The Linux C++ job makes the prefix and SDK with the provisioner and
caches both by tag, provisioning inputs and runner image. The macOS job
uses the runner's configured Apple Clang and SDK and keys the cache by
the tag, provisioning inputs, toolchain and workspace path. The cached
SDK retains absolute dependency paths. The macOS job keeps Rust and
Python tooling in the runner's tool cache and builds in the job's
workspace. Both C++ jobs build `runtime/cpp` and run its unit tests,
and repeat both with sanitizers; the Linux job runs each mode on a
runner of its own, with the SDK tests on one runner and the runtime
library's tests and core's cases on another. The test `hglrt_takes_no_stdlib` fails where the library takes
anything of hgraph's standard library or of Arrow: a header of either that
the public headers bring in at any depth (their include set, preprocessed),
a source that includes one, or a library that `libhglrt`, its tests or
hgraph's runtime and wiring libraries need, read with `readelf` (with
`otool -L` on macOS); an SDK built with Arrow fails it. It runs on GitHub's Ubuntu 26.04 image, whose GCC 15.2 is
Ubuntu's own: nothing is installed for it, it is the compiler a developer
on that release has, and neither a container (which the cache and the
checkout would run inside) nor a compiler in the conda prefix (which adds
its own sysroot and runtime libraries) is needed.

## Finding source files

The compiler is given one or more source roots. It reads every `.hgl` file
under them and groups the files by the `module` line each declares; a file
headed `application name` is a unit of its own (UNIT-14). File and
directory names carry no meaning to the compiler; keeping one module's files
together is a convention for readers. The language does not define a file
layout, so this is a compiler rule (owner decision, 2026-11-13).

`hgl check` does this. A root is a directory, searched with its
subdirectories, or a single file. Linked directories are followed. A file
reached twice is read once. A module that an import names must be under
one of the roots given: nothing is searched for elsewhere.

Core's module is the exception. Core is the part of the language every
program has, and every module binds its public declarations with no
`use` (UNIT-11). So a program is always checked with core's module, and
no option leaves it out:

| The source roots | Core's module |
| --- | --- |
| Hold no `module core` | The compiler supplies its own: the files of the `spec/hgl_core` pin, built into `hgl` when it is compiled |
| Hold a `module core` | That one. This is how core itself is checked |

A module the compiler supplied is not counted in `hgl check`'s summary
or listed by `--modules`; a diagnostic in it names its file as
`<core>/...`. It is read under the default limits whatever
`--nesting-limit` and `--tree-depth-limit` say: those are for the
program's own source.

The files are `hgl_analyser::CORE_FILES`. The analyser reads only what
it is given, so whoever calls it parses these with the program's files
and passes them along: `hgl` does, and so does the helper the
analyser's own tests check a program with, so a test's program has
core as every program does. A test holds the list to the pin: a pin
that adds a file to core fails it until the list has the file.

## Planned order

1. Lexer (done).
2. Parser for every grammar in the specification (done); formatter.
3. Analyser: modules, declarations, imports and `hgl check` (done);
   `# expect-error` matching and `hgl test` for rejection cases (done);
   name resolution (done); the checks the parser left (FN-8, NATIVE-3;
   done); the typed bodies of functions and tests (in part).
4. Transpiler for C++ on hgraph (in part); `hgl test` and `hgl run`
   on it (done for what the target writes); the forms the rest of
   core's conformance cases need.
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
| `serde` | `hgl_cli`, `hgl_doc`, `hgl_runner` | serialization of documentation and built test records |
| `serde_json` | `hgl_cli`, `hgl_doc` | check diagnostics, API documentation and native build records as JSON |
| `blake3` | `hgl_cli` | content fingerprints for native build reuse |
| `codespan-reporting` | `hgl_diagnostics` | source excerpts and underlines in rendered diagnostics |
| `ctrlc` | `hgl_cli` | waiting for an interrupted native build to finish, or passing a stop request to an application |
| `petgraph` | `hgl_analyser` | the graph of imports and the groups of modules in a cycle |
| `rowan` | `hgl_parser` | the lossless syntax tree: nodes, tokens, byte ranges, traversal |
| `pulldown-cmark` | `hgl_parser`, `hgl_doc` | CommonMark fenced block ranges and HTML rendering of documentation comments |
| `tempfile` | `hgl_cli` | a file of a build written beside the one it replaces and renamed onto it, so that nothing reads half of it |
| `walkdir` | `hgl_cli` | every file under a source root, linked directories included |
| `dirs` | `hgl_cli` | the user's cache directory, where the cpp target's builds are kept |
| `which` | `hgl_cli` | the programs the cpp target builds with, found on the `PATH` |

The cpp target's build driver runs programs, not crates: GCC 15 or
later on Linux or Apple clang 21 or later on macOS, `ninja`, `cmake`
(once for each SDK) and, on Linux, `ld.lld` where it is on the `PATH`;
see "cpp" under "Transpilers".

The C++ target's runtime library, `runtime/cpp`:

| Dependency | For |
| --- | --- |
| hgraph's C++ SDK, its runtime and wiring libraries | the graph, node, type and runtime layers the library and generated code are built on |
| fmt, spdlog, simdjson | hgraph's own dependencies without Arrow: fmt's and spdlog's headers are in hgraph's, and its runtime library needs both when it runs. Its date/tz library is its own pinned release, built into it |
| GCC 15 or later on Linux, Apple clang 21 or later on macOS | C++23, and the standard library the SDK links: libstdc++, or the system's libc++ |
| CMake, Ninja | building the SDK and the library |
| micromamba, conda-forge | making the dependency prefix where there is none |
| doctest | the library's unit tests |

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

### Native application debug launch

`hgl build --debug --application NAME --out DIR --format json ROOT...`
builds native symbols with HGL source mappings and reports the selected
application's launch. With an implicit workspace, the debug build uses
`debug-programs/<hash>` under the cache. Its stamp includes the absolute
HGL source paths used by the debugger.

The debugger starts the reported absolute `executable` with its
`arguments`, `workingDirectory`, and `environment` overrides. A client
creates its own session directory to keep that image for the duration
of the debug session. The legacy `hgl run --application NAME
--debug-build DIR ROOT...` spelling creates a fresh session under DIR
and reports the same launch fields with `version: 1`. The debugger's
console mirrors the results protocol. After the run
returns it prints the recorded publications. The debugger owns process termination and may close stdin.
Ordinary application runs keep their host stop pipe. HGL statements use the
existing C++ `#line` mappings with absolute application and module paths.
After each mapped statement, generated code uses its own C++ file and line.
Bundled core uses display names such as `<core>/core/core.hgl`.
With `--arg` or `--config`, breakpoints use the original file and line. A
configuration inserted by the driver has only a generated C++ location.
