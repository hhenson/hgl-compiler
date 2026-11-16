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
| `hgl_analyser` | Syntax trees of a program to the analysed program | modules, declarations, imports, name resolution, written types, the expressions and calls of value functions; control flow, phases and constants planned |
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
| Modules | Files grouped into modules and parts by their `module` lines | UNIT-1 | done |
| Declarations | Each module's declarations, test helpers and tests; its declaration scope and test scope; a name declared twice | UNIT-5, UNIT-12, FN-4, TST-1, TST-2, NATIVE-2 | done |
| Imports | The module and the declarations each `use` names; cycles; the operator of each candidate, `impl fn` or `impl const fn`, native or not | UNIT-2, UNIT-3, GEN-4 | done |
| Resolution | Every name in a signature, body, type and constraint, to the one binding in scope; a binding that takes a name already there; undecided generic applications | UNIT-5, UNIT-6, UNIT-7, UNIT-10, UNIT-12, FN-14, GEN-1, GEN-14, TST-2 | done |
| Forms | What the grammar admits of a declaration and a rule does not: a pack with a default, const, or before a fixed parameter; a named pack of no type pack; the hooks of a native implementation | FN-8, NATIVE-3 | done |
| Written types | What each written type means; each declaration's signature or fields; what a type may be made of | UNIT-4, UNIT-8, TYPE-10, TYPE-11, TYPE-12, TYPE-14, GEN-2, GEN-13 | done |
| Declared types | Struct parents and fields; structs that hold themselves; enum numbering | TYPE-3, TYPE-4, TYPE-6, TYPE-7 | done |
| Generic kinds | Whether a plain generic parameter is a type or a value; each generic argument against its parameter's kind | GEN-13, GEN-14 | done |
| Signatures | What a callable's signature must hold once its types are read: a generic value parameter's default is of its type; an option's name is one kind across an operator's candidates | GEN-13, GEN-6 | in part: a default that names something is not checked |
| Value expressions | The type of every expression of a value function and of every constant; each body written down typed | EXPR-2, EXPR-3, EXPR-5, EXPR-7, EXPR-9, NUM-1, FN-4, FN-7, TYPE-5, TYPE-7, TYPE-14, ERR-6 | in part: literals, names, operators, tuples, sequences of values, `let`, `var`, assignment, `return`, calls, constructions, members, elements |
| Phases | Composition and runtime bodies, temporal shapes, lifting | FN-2 | in part: each temporal body is classified a graph or a node. What each admits is planned: see "Temporal bodies" below |
| Generics and operators | Generic resolution, `requires`, candidate selection | | planned |
| Tests and eval | Test blocks, `assert`, eval targets and traces | | planned |

`hgl check` says which passes ran: names are resolved and written types
are read, and what an expression is is not checked yet.

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
arguments: a constant among them by its value, or as it is written
while this pass holds no value for it. What is read of each
declaration is kept for the passes after: a callable's generic
parameters, parameters and result; a struct's parents and members.
`hgl check --modules` prints each signature as it was read.

What the pass checks as it reads:

| Subject | Rule | What is done |
| --- | --- | --- |
| A name in a type position | UNIT-10, GEN-2, GEN-13 | Names a struct, an enum, a native type or a generic parameter that is a type. A function, operator, local, generic value parameter, or a type pack where one type is needed, is rejected |
| A generic type's arguments | GEN-2, GEN-10 | As many as it declares, within its pack's cardinality. An argument that names a type pack counts for what the pack may hold, and is rejected only when no such length fits. A parent and a constructor are no different |
| An ordinary value type | TYPE-12, FN-1, grammar `value_type` | A set's element, a dictionary's key, a window's value, an atomic or result payload, a `const` parameter, a value function's parameters and result, a generic value parameter and a property domain hold no `rolling`, `ref`, `atomic` or `signal`, at any depth |
| `signal` | TYPE-12 | The whole type of a temporal function's input, or of each input of a positional pack. Not a local's annotation or an anonymous function's parameter |
| `T?` | TYPE-14, TYPE-15 | What `?` is applied to is an ordinary value type throughout, but on the whole type of a temporal function's input, where `ref<f64>?`, `rolling<f64, 5>?`, `atomic<Quote>?` and `signal?` are inputs that may be omitted, and of a struct field, which is then optional. In a result, a local's annotation or a part of another type they are rejected. A set's element and a dictionary's key take no `?` |
| A list's size | TYPE-10 | A count that is not negative, or `unbounded` |
| A window's sizes | TYPE-11 | Each a tick count or a duration, both of one kind, in bounds. A minimum equal to the maximum is the window with none written |
| A default | UNIT-8 | Any parameter but a pack may have one. A temporal input with one keeps its temporal type; whether the default can be wired as a constant of that type is for the pass that evaluates constants |
| A cardinality | GEN-10 | `{n:m}` has m at least n |

A size is a constant expression, and no expression has a type yet. The
pass folds what needs none: integer and duration literals, a minus
before one (LEX-7), `+`, `-` and `*` between two, parentheses, and the
name of a generic parameter. Any other size is kept as written for the
pass that evaluates constants.

A type the pass cannot know, because its name is left for the prelude or
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

An enum's number is folded as a size is. One this pass cannot fold is
not known, and neither are those numbered from it; the pass that
evaluates constants reads them.

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

Nothing more is read of a temporal body yet. The order planned, and
what the chapters leave open for it, is in hhenson/hgl_spec#53: the
structure of a node's body; hooks read as value code, with the
operations on an endpoint; graphs, with connections, lifting and an
operator's temporal candidates; conditionals and loops in composition.

### Value expressions

The body of each value function, `const fn`, is read into a typed form,
`hgl_analyser::hir`: every local with its type, every expression with
its type, and a node wherever the source leaves a step unwritten, such
as an `i64` taken as an `f64`. It names no target. It is what the stages
after the analyser read; the syntax tree stays for the formatter.

The pass is built form by form. So far it reads:

| Form | Rule | What is done |
| --- | --- | --- |
| A literal | TYPE-1, TYPE-2, LEX-11 | The type its form gives: `1` is `i64`, `@2026-09-03T09:30` a `civil_datetime`, with an offset a `datetime`, with a zone too a `zoned_datetime` |
| `null` | TYPE-14 | The nullable type its place expects. Where the type expected has no `?`, or nothing gives it a type, it is rejected |
| A name | UNIT-10, GEN-13 | A parameter, a local, or a generic value parameter, with its type |
| A symbol | NUM-1, OPS-1, OPS-2, OPS-3, EXPR-10, GEN-5 | The call of the core operator it stands for, `add_` for `+`: its operands select one of that operator's value candidates, core's or a module's own, and the result is that candidate's. No candidate for the operands is a rejection. Configuration, `a /{zero: .error} b`, gives the call its named arguments. See "Symbols" below |
| `&&`, `\|\|`, `!` | EXPR-6, OPS-5 | Read as they are written: their operands are bools, and the right operand of a connective is evaluated only where the left does not decide |
| A tuple, a sequence of values | EXPR-5, EXPR-9 | Their elements take the types an expected tuple, list or set gives. With none, a sequence is a list of the one type its values have: `f64` when its integer literals stand among `f64` values, and `T?` when a `null` stands among values of `T`. `[]` and `[null]` give no type, and are rejected |
| A test against null | ERR-12, ERR-3 | `value == null` and `value != null` prove an immutable nullable parameter, `let` or loop binding is not null: in the branch the test holds for, in the right operand of `&&` or `\|\|` it guards, and from a branch that returns to the end of the block the `if` stands in. There the binding is read as its payload's type. A `var`, a saved bool and a test of anything else prove nothing |
| `let`, `var`, assignment | EXPR-3 | A local has the type written or its first value's. Only a `var` is assigned, with a value of its type, and it is read only once assigned |
| `return`, `=> value` | FN-7 | Of the function's result type. A `return` with no value is rejected where a result is declared, and one with a value where none is |
| A call | EXPR-2, FN-4, UNIT-8 | Of the value function its name is bound to. Each argument is given to a parameter, by place and then by name; one given twice, to no parameter, or too many is rejected, and so is a parameter left with nothing that has no default and no `?`. A temporal function is not called from a value function |
| A construction | TYPE-5, TYPE-3, GEN-2 | Its arguments are named, each for a field the struct has, a parent's included, in any order; one with no name is rejected. A generic struct takes its arguments from `Name<...>`, from the type expected, or from the fields given. An abstract struct is not constructed |
| `Side.buy`, `.buy` | TYPE-7 | A member of the enum named, or of the enum the place expects: an annotation, a parameter, a field, a result, the other operand of a comparison |
| `value.field` | EXPR-7 | A field the struct has, a parent's included, with the value's generic arguments in place |
| `base[index]` | EXPR-7, ERR-6 | An element of a list by an `i64`, of a tuple by a constant within its length, or the value a dictionary has for a key |

A value fits its place when its type is the one expected, when it is a
`T` where a `T?` is expected, or when it is a struct where an abstract
struct it inherits from is expected. An integer literal, under a minus
or not, is an `f64` where one is expected, and is written down as
widened (EXPR-5). A value that has a type is not converted: an `i64`
binding where an `f64` is expected is rejected. A `T?` where a `T` is
expected, or as an operand no candidate takes, is
`nullable.required_presence` (ERR-12): it may be null there.

What the pass does not read yet has no type, and nothing is reported of
it or of what uses it (ERR-9): `&&`, `||` or `!` given configuration, a
pair sequence, a call of a name left for the prelude or of an operator
that has a pack, a generic function's parameter and result types, the
payload of a nullable or a result. The arguments of a call that is not
read are read themselves. After a form that is not read every `var` is
taken to be assigned. An operand that is a generic parameter, or
nullable, is not judged: its constraints, or its guard, say what it
admits.

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
| An operand that is a generic parameter | Its constraints say what it admits | The symbol as written |
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
not.

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
candidate is `operator.no_candidate`, and a tie `operator.ambiguous`. The
analyser has no local code.
A symbol selects as a call by name does: see "Symbols" above.

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

Core declares part of its module so far: the comparison outcome and the
symbol operators. A name that nothing binds is therefore not reported:
the analysed program lists it as left for the prelude, once for each
module, and `hgl check` prints the list. Over core's conformance cases
and the specification's complete examples the list is `delta_value`,
`get`, `info`, `invalidate`, `modified` and `valid`.

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
The parser's code for its own nesting limit is local and can be expected
by no case. Nor can an error that no item owns: a repeated documentation
comment, or a second unnamed file of a module.

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
and its two for documentation comments, `doc.unattached` and
`doc.repeated` (SDOC-1). Its one local code is `syntax.nesting_limit`:
the limit is the compiler's, not the language's. The analyser reports 70
codes, all catalogued (`hgl_analyser::codes`), and its tests check each
diagnostic's category and code against the catalogue.
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
