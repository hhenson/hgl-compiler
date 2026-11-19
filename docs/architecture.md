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
    |  one transpiler per target (`hgl_hgraph`)
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
| `hgl_runner` | The cases of a program and the outcome of each | rejection cases (`# expect-error`); executable cases handed to a target to run |
| `hgl_cli` | The `hgl` command | `hgl tokens`, `hgl tree`, `hgl check`, `hgl test`, `hgl emit`, `hgl run` |
| formatter | Syntax tree to source text | planned |
| optimiser | Analysed program to analysed program | planned |
| `hgl_hgraph` | The hgraph target: the analysed program to Python wiring, and its tests run on a Python that has hgraph | first slice: scalar and set nodes, value functions, graphs that call nodes, tests with dense evals |

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
| Signatures | What a callable's signature must hold once its types are read: a generic value parameter's default is of its type; an option's name is one kind across an operator's candidates | GEN-13, GEN-6 | in part: a default that names something is not checked |
| Documentation | The keys of a documentation comment's `Args:` and `Type Args:` sections, against the declaration's parameters and generic parameters; nothing recorded | SDOC-3, SDOC-6 | in part: `Requires:` and `Properties:` keys are not checked |
| Value expressions | The type of every expression of a value function and of every constant; each body written down typed | EXPR-2, EXPR-3, EXPR-5, EXPR-7, EXPR-9, NUM-1, FN-4, FN-7, TYPE-5, TYPE-7, TYPE-14, ERR-6 | in part: literals, names, operators, tuples, sequences of values, `let`, `var`, assignment, `return`, calls, constructions, members, elements |
| Phases | Composition and runtime bodies, temporal shapes, lifting | FN-2 | in part: each temporal body is classified a graph or a node. What each admits is planned: see "Temporal bodies" below |
| Generics and operators | Generic resolution, `requires`, candidate selection | | planned |
| Constant cycles | A constant whose value depends on itself, directly or through what working it out reads. It reads the typed program, so it runs last | UNIT-13 | done: see "Constants" below |
| Tests and eval | Test blocks, `assert`, eval targets and traces | TST-3, TST-4, TST-5, TST-7, TST-8 | done: see "Eval" below. Running a test is the runner's |

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
| `T?` | TYPE-14, TYPE-15 | What `?` is applied to is an ordinary value type throughout, but on the whole type of a temporal function's input, where `ref<f64>?`, `rolling_window<f64, 5>?`, `atomic<Quote>?` and `signal?` are inputs that may be omitted, and of a struct field, which is then optional. In a result, a local's annotation or a part of another type they are rejected. A set's element and a dictionary's key take no `?` |
| `atomic<S>` | TYPE-9 | `Type::atomic` forms it: S itself where S is no composite, a scalar, an enum, a native type, a `result`, an `error`, a delta that is a type of its own, or a nullable of one; `Type::Atomic` for a tuple, a struct, a list, a dictionary, a set, a nullable of one, and a type not yet known. It is formed where the type is written and again where a generic parameter is replaced, and an `atomic<T>` given a value of a type that is its own atomic says what `T` is |
| A list's size | TYPE-10 | A count that is not negative, or `unbounded` |
| A window's sizes | TYPE-11 | Each a tick count or a duration, both of one kind, in bounds. A minimum equal to the maximum is the window with none written |
| A default | UNIT-8 | Any parameter but a pack may have one. A temporal input with one keeps its temporal type; whether the default can be wired as a constant of that type is for the pass that evaluates constants |
| A cardinality | GEN-10 | `{n:m}` has m at least n |

A size is a constant expression, and no expression has a type yet. The
pass folds what needs none: integer and duration literals, a minus
before one (LEX-7), `+`, `-` and `*` between two, parentheses, and the
name of a generic parameter. Any other size is kept as written for the
pass that evaluates constants.

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
| A temporal input read in a `cache`'s initialiser | local `cache.input_read` | TIME-1 |
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

A test's block is then read as a graph's body is (`Typer::test`, into
`Module::test_bodies`): a test has a name and neither a parameter nor a
result. `assert` is a statement there (TST-4): its condition is a `bool`
(`type.mismatch`) known when the test is built, and no connection
(`connection.as_constant`). The block of an `assert raises` is read with
the test's scope.

### Eval

`eval(target(...))` is read in a named test (`Typer::eval`, TST-5). Its
first argument is a call of a function of a module, the temporal
definition where the name has both (FN-4); anything else, a call of an
operator among them, is `eval.target`. The call is read as any call of
that function is, so positional and named arguments, defaults, generic
arguments written after the name, and a generic target's parameters are
handled as any call's are. The one difference is what a parameter
takes: a `const` parameter takes configuration, an ordinary value, and
every other parameter a trace. A second argument is the run's
configuration, read as a value of core's `RunConfig` (RUN-1); an eval
takes no third (`argument.unexpected`). The typed eval is
`ExprKind::Eval`, with its call and its configuration where it writes
one.

| Form | Rule | Read as |
| --- | --- | --- |
| A trace's type | TST-3, TST-5 | `Type::Trace`, with the type of a cell and the style. It is a type of this compiler's: source writes none. A cell is one cycle's publication, the `delta<T>` of the parameter's type for an input and of the target's result for what the eval gives |
| `[7, _, 9]`, `[0s: 7, 2s: 9]` where a trace is expected | TST-5, TST-7 | `ExprKind::Trace`: dense, or timed. Each cell is read as the cell's type. A timed trace's keys are all durations or all datetimes, constant keys strictly increase (`trace.key_order`), and it has no `_` (`trace.placeholder`) |
| A `_` anywhere else | EXPR-8 | `trace.placeholder` |
| An eval's traces | TST-7 | Of one style, the expectation included (`trace.style`, at the later trace) |
| An eval of a target with no result | TST-8 | A statement. Bound or compared, it is `eval.no_result` |
| `eval(...) == [...]`, `seen != other` | TST-4 | `ExprKind::TraceCompare`, a `bool`: the harness's own comparison, selected through no operator. The side that gives a trace says what the other, a sequence written out, is read as |
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
`hir::Run` in `Module::run_body` (RUN-1). Its first argument is a call
of an entry, a temporal function with no temporal parameters; anything
else is `run.entry`. The call is read as a graph's call of the entry is,
and the configuration as an eval's is.

### Integration tests

`integration_test name { ... }` is a test (TST-17): `Test::integration`
marks it, and everything a test is held to holds of it. In a test's
block the parser reads `run(` as the run form, so the analyser sees a
`RunExpr` there and nowhere else in a body. It is read as an eval is,
`ExprKind::Eval` with `real_time` set, and in a test that is no
integration test it is `run.position` (RUN-3).

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
| `return` with no value | Ends the source. One with a value is read and not judged: what it is there is proposed | SOURCE-4 |
| `while { }` with no condition | A loop with none | CTRL-15 |

A temporal function called there is `call.role`, a `?`
`result.propagation_phase`, and a `capture` `capture.phase`, as in a
hook.

A node's body is then read into the same typed form as a value
function's (`hir::Body`): a `state` or a `cache` is a local that is
assigned as a `var` is, each injected capability is a local, and each
hook is a `Stmt::Hook` with its condition and its statements.

| In a hook | Read as | Rule |
| --- | --- | --- |
| A temporal parameter | A value of its declared type, as in a value function | FN-3 |
| `valid(value)`, `modified(value)`, `scheduled()` | A bool: an operation on the endpoint, which is the language's own and no call of a function | TIME-9 |
| `delta_value(value)`, `last_modified(value)` | The endpoint's `delta<T>` and its retained publication time as a `datetime?`, respectively | TIME-9, END-3, END-6 |
| `is_active(input)` | A bool for a direct temporal input. In a guard it adds no subscription or validity requirement | TIME-9, END-5 |
| `valid()`, `valid(a, b)`, `valid([])` | A selector of a `when` header: every temporal input, those named, none | TIME-3 |
| No `modified` selector | Subscribes to every temporal input. An active invalidation can wake the node and reach a handler whose other conditions hold | TIME-3 |
| `modified()`, `modified(a, b)` | Subscribes to every temporal input or to the named inputs, respectively. At least one selected input must have published this cycle | TIME-3 |
| `modified([])` | Subscribes to no temporal input and selects a scheduler or alarm wake | TIME-3 |
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
| A capability retained: the value of a `state` or a `cache`, an entry of a tuple, list, set, dictionary or struct, returned, or published: `(clock, value)`, `return clock` | `capability.as_value` | TIME-8 |
| `alarm` and `scheduler` injected in one node | `inject.alarm_and_scheduler` | TIME-8 |
| The run's own key, `"run.output"`, written out as the key of a `get`, `has` or `put` on the global store. `Typer::store_keys` judges it where the call is finished; a key that is a `const` parameter is known when the graph is built, and is not judged | `store.reserved_key` | TIME-8 |

`modified([])` subscribes to no input, so only the node's scheduler or
alarm wakes the handler (TIME-3). Where the node injects neither, or its
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

A guard is `valid(input)`, on the input by its name, proved where the
read stands (TIME-4) by the paths a null test is followed along: the
branch it holds for, the right operand of the `&&` it guards, and from a
branch that returns to the end of its block. A read with none is
`payload.unguarded`. A saved bool proves nothing. A header that is
itself rejected says nothing sure of what it admits, and its body's
reads are not judged.

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

The endpoint operations are core's, declared as such functions: all but
`delta_value`, whose result core has not settled. In a node's hooks
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
| Its value, where a type needs one | `read::constant_value` | A list's size, a generic value argument in angle brackets or by name, and a tuple's index that name a constant follow it to its value, through the constants it names in turn. Only an `i64` or a `duration` written with literals, constants and `+`, `-`, `*` is followed: it is all that folds with no types. A constant given as an operator's option (`Typer::constant_option`) is that value, or what its expression is written as where that is a literal or a `.member` |
| A cycle | `cycles::check` | Asked of the typed program, after the values pass. Each constant, function body, parameter default and struct's field defaults is a node of a graph, with an edge to each it reads: a constant named, the definition a call selected (the value one of a pair, FN-4; the candidate of an operator, by name or as a symbol, OPS-2), and the defaults of the parameters a call leaves out (UNIT-8). A constant in a cycle of that graph depends on itself (`constant.cycle`). `Expr::walk` visits every expression of a typed body. A constant named where the typed program keeps only its value, a size or a generic value argument, is an edge too, from the part the name is written in. The graph is of the whole program: an operator's candidates are found wherever they are declared (GEN-5), so a value may be worked out through a module that does not import the constant's |

Not done: the value of a constant of any other type where a type needs
one; the value of a `switch` label that is a constant, so two labels of
one value are not seen as that. The cycle check counts every field
default of a struct a constant constructs, given or not, and does not
follow the body of an anonymous function, which is not typed.

### Value expressions

The body of each value function, `const fn`, is read into a typed form,
`hgl_analyser::hir`: every local with its type, every expression with
its type, and a node wherever the source leaves a step unwritten, such
as an `i64` taken as an `f64`. It names no target. It is what the stages
after the analyser read; the syntax tree stays for the formatter.

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
| `delta<T>` | TYPE-16 | `Type::delta` forms it: for most T another type of the language, and `Type::Delta` for a set, a dictionary, a struct, and a T not yet known. It is formed where the type is written and again where a generic parameter is replaced, so `TimedValue<f64>` has a `value: f64`. A named type carries what it is declared as, `NamedKind`, so that this needs no look at the program |
| A delta's property, `change.added`, `change.value`, `change.bid`; its constructor, `delta<set<i64>>(added: [1, 2])`, `delta<i64?>()` | TYPE-16 | `ExprKind::Field`, the property by its position, and `ExprKind::Delta`, each property given at its position and one left out empty or null. `Typer::delta_properties` says what a delta has: a set's `added` and `removed`; a dictionary's `updated`, a `dict<K, delta<V>>`, and `removed`; a nullable's `value`, a `delta<V>?`; a struct's one for each field, a `delta<F>?`. A nullable's delta is an optional of its own type, `Type::Delta` of the nullable: a `delta<V>` is admitted where it is expected (`Typer::admits`), and null is not, since null in a delta's place is nothing published. A trace's cell of a nullable type is read as the nullable it publishes (TST-5) |
| What a node publishes | TIME-7, TYPE-9, END-3 | A `return` in a hook, an assignment to `out` and a source's `yield` take a complete value of the result, a V for an `atomic<V>`, or the result's `delta<T>`. `delta_value(input)` is the `delta<T>` of the input's type |
| `return`, `=> value` | FN-7 | Of the function's result type. A `return` with no value is rejected where a result is declared, and one with a value where none is |
| A call | EXPR-2, FN-4, UNIT-8 | Of the value function its name is bound to. Each argument is given to a parameter, by place and then by name; one given twice, to no parameter, or too many is rejected, and so is a parameter left with nothing that has no default and no `?`. A temporal function is not called from a value function |
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
| `value?` | ERR-5 | The T of a `result<T, E>`. It stands in a value function whose result is a `result<U, E>` of the same E, and in a `when` on a `result<T, error>`; elsewhere it is `result.propagation_phase`, and on another type `result.propagation_type` |
| `capture(callable, args...)`, `seen.value`, `seen.error` | ERR-7 | In a graph, the call the first argument names, read from the same list less that name as the call itself is. It is bound with `let`, and read through `.value`, the call's output, and `.error`, an `error`, each a connection. Outside a graph, of no callable, or of a call that composes nothing it is `capture.phase`; `.value` of an outputless call is `capture.output_field` |

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
sequence with a `_`, a call of a name nothing binds or of an operator
that has a pack, a generic function's parameter and result types, the
payload of a nullable, and a conversion written `str(value)`. The arguments of a call that is not
read are read themselves. After a form that is not read every `var` is
taken to be assigned. An operand that is nullable is not judged: its
guard says what it admits. One that is a generic parameter has the
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
`requires` they state too. Nothing is reported where it cannot be told.
A generic parameter of the declaration a call is written in is no one
type, so a membership of it is not told; a named operator requirement
of it is, by what that declaration states (GEN-7), a struct's as a
function's. A generic value is read however it is written (GEN-8): a
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
candidate is `operator.no_candidate`, and a tie `operator.ambiguous`. The
analyser has no local code.
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
`Executor`, which says what became of each. `hgl test --target hgraph`
gives it one made of the hgraph target and a Python interpreter;
without `--target` it is given none, and every test is `not run`.

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
are all catalogued (`hgl_analyser::codes`), one of them a warning, and its
tests check each diagnostic's category and code against the catalogue.
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

## Transpilers

A transpiler is a crate that depends on the analysed-program crate and the
diagnostics crate, and on no other transpiler. `hgl emit --target NAME
--out DIR` selects one by name and writes what it gives. The front end
does not change, and no stage before a transpiler knows a target.

A transpiler is given a program the analyser accepted. What it cannot
write is no fault of the source, so it is reported in the category
`build`, which no `# expect-error` names, with a code of the target's
own. `hgl emit` then writes nothing: half a program is of no use to run.
`hgl test --target NAME` does run on such a program the tests that need
nothing the target reported.

### hgraph

First target (owner decision, 2026-11-13): Python wiring for
[hgraph](https://github.com/hhenson/hgraph) running on its C++ runtime.
Behaviour that Python wiring cannot express faithfully is added to hgraph on
its `0.9.x` branch, not approximated in generated code.

`hgl_hgraph` writes every file under one Python package, `_hgl`, so
that a program's modules are in a namespace of their own: a module named
`hgraph`, or as a module of Python's library is, takes the place of
nothing Python or hgraph imports. Each module is a package
under it, each part of its path with `_m_` before it: `examples.tests`
is `_hgl/_m_examples/_m_tests/__init__.py`. A package holds a module's
declarations and the packages of the modules under it side by side, and
the `_m_` keeps the two apart, so `module demo` may declare a `util`
beside `module demo.util`. Beside the modules are the support files a
generated module imports: `_hgl/_core_native.py` and `_hgl/_harness.py`.
A name that starts with `_` is the target's own: no module's or
declaration's Python name does.

An hgraph node or graph is written with the types of its inputs, so a
temporal function or candidate with generic parameters is written once
for each set of generic arguments it is called with (GEN-2). Before any
module is written the target reads the typed program for those calls
(`instances.rs`): a call in one module asks for an instance that is
written in the declaration's module. It starts from every body that
is not itself written by instance and from the tests, asks the analyser
for each instance (`Program::instance`), and goes on through the calls
of each instance it is given. A value function with generic parameters is
written once where it can be, as Python takes a value of any type. It
is written by instance too where one call of it says something of its
body:

| The body | Why one call says something of it |
| --- | --- |
| Calls an operation the declaration requires, or hands a requirement on to what it calls | Which candidate that is depends on the types (GEN-16) |
| Reads a generic value parameter as a value | It is the constant the call gives (GEN-13) |
| Calls a function written by instance with its own generic parameters | Which instance that is is known only where they are. This makes more functions so, until it makes no more |

All three are read from the typed program.

An instance is written with the types of the module that calls it, so
its module then imports that module, which imports it: two generated
modules may import one another where the source's do not. A call that
selects a candidate of a module that imports the caller's does the same
(GEN-5). Python reads a function's annotations where the function is
defined, so each generated file defines its types before it imports
another generated module. Whichever is loaded first, a type is there
when a function that names it is defined, and what a function calls is
looked up when it runs.

Core is a module as any other, `_hgl/_m_core/__init__.py`, written when
a module of the program imports it. It holds the functions and the
candidates core has an HGL body for; core's native ones are the
provider's. What core holds that the target does not write is reported
only where the program reaches it, at core's source.

| HGL | Python |
| --- | --- |
| `const fn` | A plain function |
| A node, a `fn` with a `when` | `@_hg.compute_node`, or `@_hg.sink_node` where it has no result. hgraph is imported as `_hg` |
| A `when` header's selectors | The node's `active=` is the union of handler subscriptions and its `valid=` holds the validity requirements common to all handlers (TIME-3, TIME-4). In source order, each handler checks its remaining validity requirements and condition. Explicit `modified()` and `modified(a, b)` read the current `valid` and `modified` flags on each selected input. A publication on any selected input satisfies the guard. An omitted selector adds no publication check |
| `inject out` | `_output: _hg.TS_OUT[type] = None`: `out` is `_output.value`, `valid(out)` is `_output.valid`, and `invalidate(out)` is `_output.invalidate()` |
| A graph | `@_hg.graph` |
| In a graph, a value function or a candidate of core given a connection (FN-6) | A node of its own, `_lift_N_name`, written before the graph: an input `name: _hg.TS[type]` for each connection, a scalar parameter for each ordinary argument, and a default left out written where the node calls. An input whose parameter has a default is not required valid, `valid=(...)`, and is read as `(name.value if name.valid else default)` (FN-11). The same lifting twice in one graph is one node |
| In a graph, such a call given no connection | The call itself, made where the graph is built |
| `eval` of a value function | The function lifted the same way, each trace an input |
| A temporal parameter | `name: _hg.TS[type]`; in a hook its payload is `name.value` |
| A `const` parameter of a temporal function | A scalar parameter, of the node and of its `start` and `stop`, which are functions of their own to hgraph. One that is a list, a tuple or a dictionary is an `object` to hgraph: a call of the node gives it inside `_core.Held(...)`, and each of the three functions takes it out first, `items = items.value`. Its type is the instance's: a parameter declared a `T` is given so where `T` is a list |
| A value function's parameter written `T?` | The parameter as any other. A call that leaves it out gives it `None` (UNIT-8) |
| A parameter with a default, of a value function or a `const` one of a temporal function | The parameter as any other. A call that leaves it out is written with the default in its place, which the analysed program holds as a typed value (`Module::defaults`). Python is given no default of its own, so it has none to put in order or to evaluate where the function is defined. A default that leaves itself out is reported |
| A function of another module | `alias.name`, the module imported under a name of its own, `import _hgl._m_demo._m_util as _hgl_4demo_4util`: two modules may each have a `pick`. Each part of the path has its length before it in the alias, so no two paths give one |
| A sequence that is a set | `_py.frozenset((1, 2))` |
| `[key: value, ...]`, and `[]` where a dictionary is expected (EXPR-9) | `{key: value, ...}`, `{}` |
| In a value function and a node's hooks, `for item in items`, `for key, value in entries`, `while condition` (CTRL-12) | `for item in items:`, `for key, value in entries.items():`, `while condition:` |
| `switch selector { case label: ... default: ... }` there (CTRL-6) | `_selected_N = selector`, N the depth it is written at, then `if _selected_N == label:` for the first case, `elif` for each after, and `else:` with the `default`, or with `_core.unmatched(_selected_N)`, which fails |
| `bool`, `i64`, `f64`, `str` | `_py.bool`, `_py.int`, `_py.float`, `_py.str`: Python's built-ins are reached through `import builtins as _py`, so a program's own `float` or `frozenset` is free |
| `state name: T = value`, `cache name: T = value` (TIME-6) | A field of the node's `STATE`, given its value in the node's `start`, in the order written. hgraph's `STATE` is the node's own and is recorded nowhere, so a `cache` is rebuilt on every start, as it is to be |
| `inject clock`, `inject logger`, `inject global_state` (TIME-8) | `_clock: _hg.EvaluationClock = None`, `_logger: _hg.LOGGER = None`, `_store: _hg.GlobalState = None`: a parameter of the node, of its `start` and of its `stop`. hgraph gives a capability for one call and no longer, which is the language's rule too: it "is not retained" |
| `if flag { a } else { b }`, `switch route { ... }` in a graph, over a connection (CTRL-13 to CTRL-15) | `_hg.switch_(flag, {True: _case_0_0, False: _case_0_1}, value=value)`: a `@_hg.graph` for each branch, written before the statement, that takes the connections the branches read. hgraph starts a branch's graph where its key is selected and stops it where another is. A branch that returns takes the rest of the block for the other branch; branches that assign a `var` give it, and it is assigned what `switch_` gives. Over an ordinary value an `if` is Python's own |
| `let seen = capture(child, value)`, `seen.value`, `seen.error` in a graph (ERR-7, ERR-8) | `seen = _core.captured(_hg.try_except(child, value=value))`, `seen.out`, `_core.captured_error(seen.exception)`. hgraph catches an evaluation failure of the child, which runs on; start and stop failures escape. It gives a failure as its traceback's text, so an `HglError` writes its error as a JSON note the traceback ends with, which `captured_error` reads back with the node, phase and time. The phase of a failure with no note is told by the traceback's first frame, the function hgraph called: `_start_name` or `_stop_name` for a hook. hgraph also catches the failure of a `start` or `stop` the evaluation triggers, a `switch_` branch selected or left: `captured` wires a sink beside the capture that raises such a failure again. An outputless child's failures are put in a bundle, `_hg.combine(exception=...)`. A value function lifted over a connection is captured as its lifted node. A temporal `error` and `result` are `_hg.TS[_core.Error]` and `_hg.TS[_core.Result]`; `is_ok(value)` of a connection is `_core.lifted_is_ok(value)` |
| `let seen = capture(scaled, value, 2)`, `scaled` a graph with a `const` parameter (ERR-7) | `try_except` takes no graph with a scalar input: `_capture_0_by = 2`, a `@_hg.graph` `_capture_0(value)` that returns `scaled(value=value, by=_capture_0_by)`, and `seen = _core.captured(_hg.try_except(_capture_0, value=value))`, all where the `let` is. Each argument is evaluated in its order before the `let`, a wiring-time value and a connection that is no local or literal held under such a name; a default left out is in the call of the child |
| `failure.context`, `context.node`, `.phase`, `.evaluation_time` (ERR-4) | The provider's `Error.context`, `None` or an `ErrorContext`, whose three parts are attributes; two contexts are equal when their parts are |
| `inject scheduler`, `inject alarm` (TIME-9, CAP-4, CAP-5) | `_scheduler: _hg.SCHEDULER = None` for either: hgraph has one scheduler for a node, and no cheaper one. `schedule`, `un_schedule` and `reset` are the provider's functions on it, and an alarm's one wake is kept under a tag of its own, the earlier of the one pending and the one asked for |
| `when scheduled() { ... }`, `when modified([]) { ... }` (TIME-3) | One `if` over the handler: `if _scheduler.is_scheduled_now:` with a scheduler, and `if _scheduler.is_scheduled_now and not (price.modified or price.invalidated):` with an alarm, which is told from the inputs. A header that is `scheduled()` alone requires no input valid, so the node is `valid=()` and every other handler is under its own guard. One with no `modified` selector is run by an input's wake: `if (price.modified or price.invalidated) and price.valid:`. In a node with more than one handler, what woke it is read once where the node is entered, `_timer_woke = _scheduler.is_scheduled_now`, and each handler asks that: a handler before it may passivate the input that woke the node, or ask again under the tag that is due |
| A node with no temporal input that a scheduler or an alarm wakes | hgraph takes no node without an input. The node is `_node_name`, with an input `_begin` that never wakes it, `active=()`, and `name` is a graph that gives it `_hg.const(True)` |
| `when !scheduled() { ... }`, `when modified(a) \|\| scheduled() { ... }` (TIME-3, TIME-9) | `scheduled()` is a bool, what woke the node, read where it is entered: `_timer_woke = _scheduler.is_scheduled_now`, with an alarm `and not (a.modified or a.invalidated ...)`. The handler keeps the defaults: `if (a.modified or a.invalidated or b.modified or b.invalidated) and ((a.modified and a.valid) or _timer_woke):`, in a node that requires every input valid. In a handler's body it is `_timer_woke` too |
| A handler or a node that nothing wakes (TIME-3) | A handler that subscribes to no input, in a node that injects neither a scheduler nor an alarm, is not written. A node none of whose handlers subscribes to an input is `active=()`; one with no input is `_node_name` with `_begin`, as above |
| A source, a `fn` with a `yield` (SOURCE-1) | Three functions and a graph. `_yields_name(config, _state)` is a Python generator that holds the source's statements, each `yield time: value` a `yield (time, value)`. `_source_name` is a `@_hg.compute_node` that resumes it, `_core.resume(_state._source, _clock.evaluation_time, _scheduler)`, and returns what is due. Its `start` gives each `cache` and `state` its value, runs the source's `start` and makes the generator; its `stop` is the source's. `name`, a `@_hg.graph`, is what a call names: the node, given `_hg.const(True)`. hgraph takes no node without an input, so the constant is its one input, and wakes it once as the graph starts; its own scheduler wakes it after |
| `const fn bump(var count: i64, by: i64)`, and `bump(sum, value)` (FN-15) | `def bump(count, by): ...; return (None, count)`: Python has no place to give a value to, so a function with `var` parameters returns its result and then what each holds, at every way out. The call is the whole of a statement: `_given = bump(count=sum, by=value)` and then `sum = _given[1]`, each place in the order of the parameters; a place inside a value is assigned as any is, `book = _core.with_field(book, orders=_given[1])`. A call's own result is `_given[0]`. A function that fails raises, and assigns nothing |
| `clock.evaluation_time`, `logger.level` (CAP-1) | `_core.Clock_evaluation_time(_clock)`, and `_core.member(LogLevel, _core.Logger_level(_logger))`: a property of a native type is the provider's to read, named for the type and the property |
| `log(logger, .info, message)`, a native function of core (NATIVE-7) | `_core.fn_log(logger=_logger, level=LogLevel["info"], message=message)`: the provider's function, `fn_` and the name it binds |
| `get<i64>(global_state, "key")`, `has(...)`, `put(...)` (CAP-6) | Core's candidates for the store, the provider's as any native candidate: `_core.get_GlobalState_str(store=_store, key="key")` |
| `stop { if valid(input) { ... } }` (TIME-5) | `@name.stop` on a function that takes the node's `const` parameters, its inputs as the node has them, its state, and its output where the node injects `out`. An input is read there as in a `when`, `input.value` under `input.valid`, and `out` as `_output.value` under `_output.valid` (TIME-7). `start` is given no input and no output |
| `valid(x)`, `modified(x)`, `delta_value(x)` | `x.valid`, `x.modified and x.valid`, `x.delta_value` |
| `activate(input)`, `passivate(input)`, `is_active(input)` (END-5) | `input.make_active()`, `input.make_passive()`, `input.active`. A passive input can satisfy a handler's modification check when another input wakes the node in the same cycle |
| `last_modified(input)`, `last_modified(out)` (END-6) | The input or injected output's `last_modified_time` while valid and `None` while invalid, in a handler or in `stop` |
| A symbol or a call that selects a native candidate of core | A function of `_hgl._core_native`, named for the candidate and its parameter types: `add__i64_i64` |
| `a =={abs: t} b`, `a !={rel: r} b` (NUM-4, EXPR-13) | The provider's function for the comparison with the tolerance where the source has it, `_core.eq__f64_f64_f64_f64(lhs=a, abs=t, rhs=b, rel=0.0)`: Python evaluates the arguments in that order. The function makes the test in exact arithmetic (NUM-5). Lifted, the tolerance is configuration of the node (NUM-11) |
| A generic value a native candidate is given, the `zero` of a division (NUM-9) | An argument by its name: a member of an enum as its name, `zero="ieee"`, and an integer as itself. The analyser says which member a constant comes to, however it is written (TYPE-7), and the target reads none of its spelling. `a / b` is `_core.div__f64_f64(lhs=a, rhs=b, zero="ieee")`, the default its candidate declares |
| `convert<T>(value)` (OPS-4) | The provider's function for the candidate, named with what it gives too, `_core.convert_i64_to_f64(value=count)`: core's candidates of `convert` take one type and give different ones. One that can fail takes `invalid` as division takes `zero`, `_core.convert_str_to_bool(value=text, invalid="error")`, and gives a `result` under `"error"` and the plain value under `"fail"` (OPS-8) |
| `impl const fn` with a body, of core or of a program (GEN-4) | A function of its module, `_impl_` and then the operator and the candidate's parameter types: `_impl_add__f64_i64`. A candidate binds no name (UNIT-12), and an operator may have many |
| `impl fn` with a body (GEN-4) | A node or a graph of its module, as a temporal function is, under the same kind of name: `_impl_join__i64_i64`. A call in a graph that selects it calls it, and is not lifted. Where a value candidate has the same parameter types (GEN-5), the value one is `_value_impl_join__i64_i64` |
| A temporal function or a temporal candidate with generic parameters (GEN-2) | Written once for each set of generic arguments the program calls it with, in the declaration's module: `_inst_`, the name it would have, and the arguments, `_inst_identity_sum__f64` and `_inst__impl_add__T_T__f64__identity`. A call names the instance for its arguments. An operation the declaration requires is, in each instance, the call of the candidate that answers there (GEN-16): `_core.add__f64_f64` in the node for `f64`. A generic value read as a value is the constant the call gives, `by` as `3` in `_inst_scaled__3` (GEN-13) |
| A value function or candidate with generic parameters (GEN-2, GEN-16) | One Python function, where one call says nothing of its body. Where it calls an operation it requires, hands a requirement on, reads a generic value, or calls an instance with its own generic parameters, one for each set of generic arguments it is called with, as a temporal one: `_inst_twice__i64` calls `_impl_join_i64_i64`. A value function, a node's hook, a constant and a lifted node call the instance for their arguments |
| `operator` | Nothing: its candidates are the functions |
| `enum Side { buy, sell = 5 }` (TYPE-7) | `Side = _enum.Enum("demo.orders::Side", [("buy", 0), ("sell", 5)])`, made by a call so that a member may have any name, a Python keyword among them. The class is named with its module's path: hgraph knows a Python enum by its class's name, so two modules' `Side`s would be one type to it, and core's `CmpResult` hgraph's own. hgraph takes it as a scalar: `_hg.TS[Side]`. A module's enums are written before its functions, whose annotations name them where they are defined |
| `.buy`, `Side.buy`; `Side(1)`, `Side("buy")` | `Side["buy"]`; `_core.member(Side, value)`, which fails for a value that is no member's. A member's name that starts with `_` or is `mro`, which `Enum` keeps, has `hgl_` before it |
| `struct Point { x: f64, y: f64 = 0.0 }` (TYPE-3) | `Point = _core.struct("Point", ("x", "y"))`: a class the provider makes, with every field the struct has, a parent's first. Its values are never changed, and two are equal when each field is. A module's structs are written before its functions, with its enums. An abstract struct is a class too, of which no value is made. Equality is the provider's, field by field with no shortcut for one object compared with itself: a field may be a NaN (NUM-5) |
| A value of a generic struct, `Box(value: 1)` (TYPE-3) | `_core.applied(Box, "i64")(value=1)`: each application is a class of its own, made once, so a `Box<i64>` is no `Box<f64>` where the two meet as values of a parent. The arguments are written as text no other type has, a named type with its module's path |
| `Point(x: 1.0)` (TYPE-5) | `Point(x=1.0, y=0.0)`: the arguments in the order written, which is the order Python evaluates them in, then each field left out with its default in its place, read from the analysed program as a parameter's is (`Module::defaults`, `Member::default`), or `None` for an optional field with none |
| `point.x`; `point.x = 1.0`, `line.end.x += by` (EXPR-3) | `point.x`; `point = _core.with_field(point, x=1.0)`, and at depth `line = _core.with_field(line, end=_core.with_field(line.end, x=...))`. The place's root is given a new value and the old one is as it was, for whatever else holds it (TYPE-5: "later mutation of a source cannot change an earlier retained value"). A `state` is assigned the same way |
| `items[1]`, `pair[0]`; `sizes[name]` (EXPR-7, ERR-6) | `_core.item(items, 1)`, which fails for an index outside the sequence, a negative one among them; `_core.entry(sizes, name)`, which fails with `value.missing_key` for a key that is not there |
| `get(sizes, name)`, core's candidate for a dictionary (ERR-6) | `_core.get_dict_K__T__K(store=sizes, key=name)`: the value, or `None` |
| `items[0] = 1`, `sizes[name] += 1`, `rows[1][0] = 9` (EXPR-3) | `items = _core.with_item(items, 0, 1)`, `sizes = _core.with_entry(sizes, name, ...)`, and at depth as a field is: a list, a tuple and a dictionary are never changed, and the place's root is given a new one. A key the dictionary has not is added, and an index outside a list fails |
| `a <=> b`, `compare_(a, b)` (NUM-6, OPS-3) | `_core.member(_hgl_4core.CmpResult, _core.compare__T_T(lhs=a, rhs=b))`. A native candidate that gives an enum gives its member's name, as one that takes an enum is given it: the provider declares no enum of the program's, and the member is read from the enum the program has, core's here |
| `ok(value)`, `err(value)`, `error(message: m, code: c)` (ERR-2, ERR-4) | `_core.Ok(value)`, `_core.Err(value)`, `_core.Error(message=m, code=c)`: values of the provider's, equal when their parts are |
| `is_ok(r)`, `is_err(r)`; `r.value`, `r.error`; `e.message`, `e.code` (ERR-3, ERR-4) | `_py.isinstance(r, _core.Ok)`, `_py.isinstance(r, _core.Err)`; the attribute of the same name |
| `r?` in a value function (ERR-5) | `_core.propagated(r)`, which gives a success's value and raises for a failure. The function's body is in a `try`, and returns the failure raised |
| `r?` in a `when` (ERR-5) | `_core.succeeded(r)`, which fails the node with the error's message and code |
| A named test | A function `test_name` |
| `assert condition` | `if not (condition): raise _py.AssertionError()`. Python's own `assert` is removed when the interpreter is run with `-O` or `PYTHONOPTIMIZE`, and a test would then pass whatever it asserts |
| `eval(target(input: [7, _, 9]))` | `_harness.evaluate(target, {"input": _hgl_4core._inst_replay__i64}, _hgl_4core._inst_record__i64, input=[7, None, 9])` of `_hgl._harness`: the target; core's `replay` for each temporal input the eval gives a trace, by the input's name; core's `record` for the target's result, or `None`; and then the arguments. Core has the two as HGL for any type, so each is the instance written for the type it is of (GEN-2). The harness wires the run from those |
| `eval(target(input: [7]), RunConfig(length: 2us))` (RUN-1, core's CRUN-2) | `_harness.evaluate(target, {...}, ..., input=[7], _config=...)`: the configuration last, by a name no input has, so that it is evaluated after the call's arguments (EXPR-13). The harness reads its `start`, `end` and `length`, refuses an `end` with a `length` (`run.end_and_length`), and runs the entry from the start to the end, which hgraph also takes as exclusive |
| `eval(...) == [...]`, `seen != other` (TST-4) | `_harness.same(left, right)`, and `not _harness.same(...)` for `!=` |
| `const scale: i64 = 2`, and `scale` read (UNIT-13) | `@_core.constant` over `def scale(): return 2`, and `scale()`. The provider runs the function once, when the constant is first read, so its expression may name what is written after it (UNIT-6) |
| A trace given to a `const fn` (TST-16) | `_harness.publications(trace)`: a list with a tuple for each publication, its instant and what was published |
| `assert raises("code") { ... }` (TST-10, TST-11) | A `try` around the block. Its handler calls `_harness.raised(error, "code")`, which lets the error out again unless it is an HGL error with that code, or has one as its cause, as a node's failure in an `eval` has; its `else` calls `_harness.not_raised("code")`, which fails the test |
| A temporal `i64?`; a `const` one (TYPE-14) | `_hg.TS[_py.int \| None]`, hgraph's value that holds one `int` or is unset; `_py.int \| None`. A null is `None` as a value. Where it is published it is hgraph's `Null`: a node that returns `None` publishes nothing, and one that returns `Null` ticks a value that is unset. So a `return`, a `yield` and an assignment to `out` of a nullable result go through `_core.published(value)`, and `null` in a trace is `_hg.Null` where `_` is `None` |
| `delta<i64?>()`, `delta<i64?>(value: v)`, `change.value` (TYPE-16) | `_hg.Null`, `_core.published(v)`, `_core.unpublished(change)`: a nullable's delta is hgraph's own, `Null` or the value, so `delta_value(input)` is `input.delta_value` as it is |
| A temporal `set<i64>`; a `const` one (TYPE-9) | `_hg.TSS[_py.int]`, and `_hg.TSS_OUT[_py.int]` for the node's own output; `_py.frozenset[_py.int]` |
| `delta<set<i64>>(added: [1, 2], removed: [3])` (TYPE-16) | `_hg.set_delta(added=_py.frozenset((1, 2)), removed=_py.frozenset((3,)), tp=_py.int)`: hgraph's own set delta, in a node, in a trace and as a value. The properties are given by name in the order they are written, so that they are evaluated in it (EXPR-13), and one left out is an empty `_py.frozenset()` |
| A temporal `Quote`, `struct Quote { bid: i64, ask: i64 }` (TYPE-9) | `_hg.TSB[_core.schema(Quote, {"bid": _hg.TS[_py.int], "ask": _hg.TS[_py.int]})]`, and `_hg.TSB_OUT[...]` for the node's own output: hgraph's bundle over a schema the provider makes once for the class, each field temporalized in turn. The class and the schema are of the generated module that declares the struct, which hgraph knows them by with their names, so two modules' `Quote`s are two types. A whole value published to it is `_core.bundled(value)`, a `dict` of its fields, and `_core.bundled(value, {"quote": {"bid": None}, ...})` where a part is composite: a struct field by its fields, a set field as the set it is, which hgraph's `TSS` takes as its members now |
| A temporal `Box<i64>`, `struct Box<T> { value: T }` (TYPE-3, TYPE-9) | `_hg.TSB[_core.schema(_core.applied(Box, "i64"), {"value": _hg.TS[_py.int]})]`: the schema of the class for the arguments, each field of the type they give it |
| A temporal `Shape`, `abstract struct Shape { id: i64 }`, and a `Circle : Shape` given where one is expected (TYPE-3) | `_hg.TSB[_core.schema(Shape, {"id": _hg.TS[_py.int]})]`, of its own fields; in a graph `_core.narrowed(circle, bundle, ("id",))`, hgraph's `from_ts` of the circle's fields, no node. A whole value published sets those fields, `_core.bundled(value, {"id": None})`; one read whole is reported |
| A temporal `tuple<i64, str>` (TYPE-9, TYPE-10) | `_hg.TSB["_0": _hg.TS[_py.int], "_1": _hg.TS[_py.str]]`, hgraph's bundle with no name, matched by its fields. Published whole `_core.bundled((a, b,), (None, None,))`; its delta, `(a, None,)`, as it is; read `(pair["_0"].value, pair["_1"].value,)`, `pair[0]` as `pair["_0"]`; `delta_value(pair)` `_core.positions(pair.delta_value, (None, None,))`, the tuple of hgraph's `dict`. `(a, b)` of connections in a graph is `_hg.TSB[...].from_ts(_0=a, _1=b)` |
| `out.ask = v`, `out[0] = v`, `out[key] = v` (TIME-7) | `_output["ask"].value = v`, `_output["_0"].value = v` for a tuple and `_output[0].value = v` for a fixed list, `_core.child(_output, i)` by an index that is no constant; a dictionary's `_output.value = {key: v}`. A place inside a part that publishes whole, `out.ask` of an `atomic<Quote>`, is `_output.value = _core.with_field(_output.value, ask=v)`. A fixed list's `out` is `_hg.TS_OUT[_hg.TSL[...]]`: hgraph has no `TSL_OUT` |
| `quote.bid` of a temporal `Quote`, in a graph and in a hook | `quote["bid"]`, the field's port taken by name, so a field may be named `value`; in a hook `quote["bid"].value`, and `quote` read whole `Quote(bid=quote["bid"].value, ask=quote["ask"].value)`; `out` as `_output` |
| A temporal `atomic<Quote>`, and `quote.bid` of it (TYPE-9) | `_hg.TS[Quote]`; in a graph `_hg.getattr_[_hg.SCALAR: _py.int](quote, "bid")`, told the field's type, and in a hook `quote.value.bid` |
| `delta<Quote>(bid: 1)`, `change.bid` (TYPE-16) | `_core.struct_delta(bid=1)`, `_core.changed(change, "bid")`: hgraph's delta of a bundle, a `dict` of each field that changed, a null property left out |
| A temporal `list<i64, 2>` (TYPE-9, TYPE-10) | `_hg.TSL[_hg.TS[_py.int], _hg.Size[2]]`, `_hg.TS_OUT[_hg.TSL[...]]` for `out`. A whole value published is `_py.tuple(value)`; a delta, `_core.list_delta(updated={1: v})`, a `dict` of each position. Read whole `_py.list(pair.value)`; `valid(pair)` is `pair.valid`, true once a position is, and `all_valid(pair)` `pair.all_valid`; `pair[0]` in a graph is the child's port |
| A temporal `list<i64>` (TYPE-10) | `_hg.TS[_py.tuple[_py.int, ...]]`: hgraph has no `TSL` that grows, so a tick is the whole list, published as `_py.tuple(value)`, a copy, and read as `_py.list(values.value)`. A delta published is applied to `_output`'s last tuple, `_core.list_applied(_output, delta)`, a shrink among them. Its delta read, its children and composite elements are reported, and so is an eval's replay or record of one, which publish and read deltas |
| A temporal `dict<str, i64>` (TYPE-9) | `_hg.TSD[_py.str, _hg.TS[_py.int]]`; read whole `_py.dict(prices.value)`; `prices[key]` in a graph is the provider's `_core.entry_of(prices, key)`, a key known where the graph is built `_hg.const(key, tp=_hg.TS[_py.str])`: it publishes where the key or the selected child ticks, and fails with `value.missing_key` where the key is absent (ERR-6), which hgraph's own selection would not. An unsized list as a child, a dictionary of composites read by key, and a whole dictionary published are reported |
| `delta<dict<str, i64>>(updated: u, removed: r)`, `change.updated`, `change.removed` (TYPE-16) | `_core.dict_delta(updated=u, removed=r)`, `_core.updated(change)`, `_core.removed(change)`: hgraph's delta of a `TSD`, one `dict` with `REMOVE` for each removed key |
| A name | Itself, but where Python or the target would take it. A Python keyword, and one with any number of `_` after it, gets one more `_`. Every name the target makes up starts with `_`, and a test is `test_name`, so a name that starts with `_` or `test_`, under any number of `hgl_`, gets one more `hgl_`. No two names give one |
| A value and a temporal definition of one name (FN-4) | The temporal one keeps the name; the value one is `_value_name` |

Core's native functions are the target's to provide (NATIVE-7):
`_hgl/_core_native.py` is that provider, written by hand in the crate and
copied out beside the modules. It holds the `i64` operations to core's
numeric contract, which neither wraps nor promotes. The transpiler reads
the names it defines, and a candidate it has no function for is reported.
It is written when a module calls a function of it, and whenever the
harness is written: the harness imports it, to know an HGL error by its
class.

hgraph has a generator node of its own, which is not what a source is
written as:

| | hgraph's `@generator` | A source (SOURCE-1 to SOURCE-4) |
| --- | --- | --- |
| `start` and `stop` | It has neither | Both, and `stop` always runs |
| A target that has passed | Published at once | Skipped |
| A target not after the one before | "Python generator output times must be strictly increasing" | `yield.non_increasing_time` |
| A negative duration | Not checked | `yield.negative_duration` |

So the provider resumes the generator itself, `resume`, and holds each
target to the rules. hgraph gives a capability to one call and
refuses it afterwards, so the generator, which lives across
evaluations, reads the clock, the logger and the store from the node's
state, where the node puts the ones of the evaluation it is resumed in.

`_hgl/_harness.py` holds `evaluate`, which runs an eval as core has one
(CEVAL-1): "a run of an entry it generates: core's `replay` wired to
each temporal input of the target, and the target's result as the
entry's".

| Step | What the harness does |
| --- | --- |
| Entries | Each present dense cell or timed entry gives replay one publication at its absolute time (CEVAL-2) |
| Wiring | A graph: a replay for each temporal input, the target, and a record on its result under `"run.output"` (CRUN-8) |
| Run | hgraph's `evaluate_graph`, a simulation from the run's start to its end, with a global state of its own (CRUN-7) |
| Output | What the store has under `"run.output"` once the run has stopped; none is a failure (CEVAL-6) |
| Trace | The recording keeps its run start and one timed entry per publication, through the last publication (TST-6, TST-7) |

hgraph's `eval_node` is not used. It keeps a dense recording counted
from the earliest start whatever start a run has, so it gives no trace
for a run that starts at a calendar date; here a recording holds times,
and the dense trace is counted from the run's start.

`replay` and `record` are core's, and core has them as HGL, for any
type. The target writes each from core's own source, as it writes any
generic temporal function: `replay` a source, and `record` a node that
keeps its recording in a `cache` with core's `append` and puts it in the
store at stop. Before any module is written the target finds the
instances a program asks for, and an eval asks for these two, each for
the type it is of. Core does not export them: a run wires them, and no
module names them.

A list and a dictionary are Python's, each one object that every name
of it sees change, and a value of HGL's does not change (LIST-3: "a value
taken from the place before the call is as it was"). The target follows
the owner's rule for it (issue 151): "all var mutations are mutations,
not copies. The copies only occur when: 1. the value is passed to a
function and the function also recieves a var reference to the item or
items it contains, othewise they are safe to pass through. 2. let
assignment". So a `var`, a `state` and a `cache` change what they hold
where it is, with no check when the program runs, and the compiler says
where a copy is made: where a reference is made to what a place holds
and the place then changes, and where a reference is kept. A struct and
a tuple are never changed: a change to one makes a new one.

| What is written | Where |
| --- | --- |
| `entries`, as it is | An argument, the read of an element or a field, an operand: a reference, which the function is given and does not change |
| `before = _core.copied(entries)` | A `let` of what a place holds, or of a value written with it, where the place is changed while the name is in scope: in the rest of the `let`'s block, or anywhere in a loop the `let` is inside. `hgl_hgraph`'s `copies` reads each body for it |
| `last = _core.item(rows, 2)` | A `let` whose place changes no more in its scope: a reference into the place |
| `for item in _core.copied(values):` | A loop whose own body changes the place it runs over. Any other loop reads each entry where it is |
| `push_all(items=rows, more=_core.copied(rows))` | The owner's first case: an argument read from a place that the same call gives to a `var` parameter. A second `var` parameter given the same place takes a copy too, and the place is given them in the order of the parameters (FN-15) |
| `mine = _core.copied(values)`, `rows = _core.set_entry(rows, "b", _core.copied(items))` | What a place keeps, assigned to it or into it: a copy, where something else holds the value. A value written out and what a function of the program returns are held by nothing else, and are kept as they are |
| `_core.fn_append(values=rows, item=_core.copied(row))` | What a native function with a `var` parameter puts into its place, where the place may hold it: `append` keeps its item, and `extend(values, more)` keeps the numbers `more` holds and takes `more` as it is |
| `return _core.copied(value)`, `return out` | A result is the caller's to keep: a copy of what the function was given, and its own `var` or `let` as it is |
| `_output.value = _core.copied(...)`, `yield`, a hook's `return` of a `state` | What is published is the runtime's to keep. The provider copies what it puts in a run's store |
| `items = _core.set_item(items, i, value)`, `book = _core.set_entry(book, key, value)` | An element or an entry assigned: the place's own collection, changed where it is. A `var` parameter is the caller's place, and is changed where it is too |
| `book = _core.set_at(book, (("f", "bids"), ("k", price),), lambda: size)` | A place further in: the provider goes in and changes each list and dictionary on the way where it is. The value is asked for once the way in has been read |
| `_given = _core.given_at(book, (("f", "orders"),), _core.fn_append, "values", lambda: _py.dict(item=order))` | A native function of core given a place further in, and then `book = _given[1]` |

`_core.copied` shares no list and no dictionary with what it copies; a
struct, a tuple and a result are made again only to hold the copies of
what is inside them, and a value that holds no collection is given as it
is. A way in whose index is an expression after an element read is
given new values on the way, so that the index is evaluated only once
what it is an index of has been read. The provider reads the way in,
then asks for the value, and only then changes anything: the value may
hold a collection on the way in, `append(rows[0], rows)`, of which the
compiler gives a copy, taken before the change. So a recording grows
with no copy, a list passed to a function is not copied, a function's
own result is kept as it is, and a `let` costs a copy only where its
place changes while it is in scope.

FN-15: "A function that fails leaves the place holding what its
parameter held when it failed: no change it had made is undone." A list
and a dictionary are changed where they are, so what a function did to
one stays where it fails. Left: a `var` parameter of any other type, an
`i64` or a struct, is given back when the function returns, and is as
it was where the function fails; and a struct of more than two scalar
fields, which the owner's rule has changed where it is too, is still
made again for each change.

A `tenor` (TYPE-17) is the provider's `Tenor`: a count of months and a
count of days, or a count of business days. Its literal is written by its
counts, `_core.Tenor(18, 0)` for `1Y6M` and `_core.Tenor(business=2)` for
`2B`. Its own sum, order and text are the provider's functions (core's
TEN-2, TEN-3, TEN-9). `date + tenor` is core's HGL, in
`<core>/core/calendar.hgl`: the target writes it as it writes any generic
value function, once for each calendar and rolling rule a call gives, and
what it writes asks the program's own `business_day` and `roll`. The four
parts that HGL is written with are the provider's too: `fn_moved`,
`fn_business_days`, `fn_weekend` and `fn_same_month`. A tenor as the type
of a temporal input or result is not written yet.

hgraph makes a value of its own of what it is given to keep, a node's
configuration and what its global state holds, and has none for a tuple
that holds a set's delta. An object of a class it does not know is kept
as it is. So a list, a tuple or a dictionary that is a node's
configuration, `replay`'s list of entries among them, is given to the
node inside one, the provider's `Held`, and taken out at the head of
the node, of its `start` and of its `stop`; to hgraph such a parameter
is an `object`. What the store is given is inside one too.

The harness compares traces with `same` (TST-4). It compares each
publication's time and value. A trailing placeholder adds no publication.
hgraph's set delta has `Removed(1)` equal to `1`, so `same` compares
additions and removals separately. A `const fn` given a recording receives
its publications and their recorded UTC instants through `publications`
(TST-16). A written trace uses the earliest start for relative keys or
dense positions (CRUN-3) and preserves absolute datetime keys.

Reported as not written yet (`target.unsupported`): as the type of an
input, an output or configuration, any but the four scalars above, an
enum, a set of one of those and a nullable scalar or enum, a struct
among them; a
delta that is not a set's; a struct made in a graph;
a generic struct made in a function generic in its arguments;
a native declaration of a program,
and a candidate with the parameter types of another of its module and
its role;
a call of a function written by instance whose generic arguments are
not known; an operation a generic declaration requires that no
candidate answers;
a default of a temporal input; a pack parameter; a node that no input
wakes, `modified([])`;
a bare payload read of `out` in `stop`;
in a graph, any expression but a name, a literal and a call, an `if`
that is not a whole statement or the whole value of a `let` or a
`return`, and branches that assign more than one `var`; a pack in a lifted
call; a temporal function called in a test outside an `eval`;
a loop and a `switch` in a graph or a test;
a zoned datetime, a zoned time and a time zone; a `result` or an `error` as the type of an input or an output;
and a candidate of core the provider has no function for.

A module is written a piece at a time: each declaration, then each
named test. A piece with a form that is not written is left out of the
file, and every piece records the functions it names. A test is
*blocked* when it, or a function it reaches through those names in any
module, was left out; it then carries the first such report.

`hgl_hgraph::run` runs the tests that are not blocked. hgraph is run
from Python, so the host is a Python interpreter that has hgraph:
`--python PATH`, or the one the environment variable
`HGL_HGRAPH_PYTHON` names, or `python3` (`python` on Windows). The
program is
written to a temporary directory, or to the one `--keep DIR` names, and
the interpreter is started once, with `-B` so that it writes no
bytecode files, on a driver the target carries, `_hgl/_run_tests.py`.

| File in the directory | Written by | Holds |
| --- | --- | --- |
| `_plan.txt` | the compiler | The tests to run, each as its Python module and its function, `test_name` |
| `_results.txt` | the driver, as it runs | For each test `start`, then `passed`, `failed` with a line and the error's text, or `unloaded` when its module does not import |

The results are a file, so nothing a program prints is taken for an
outcome. A test with a `start` and nothing after it was running when the
interpreter stopped; it and the tests after it are not run. For a test
that failed, the driver gives the line of the test's own function that
was running. The emitter recorded the line each statement of a test
starts on, so the failure is reported at that statement in the source:
`cases.hgl:9:5: the assertion does not hold`. An error that escapes,
such as an `i64` result out of range (NUM-2), is reported the same way
with its text.

An integration test (TST-17) is held to a time limit: the duration
`@timeout(5s)` gives on the line before it, or a minute where it writes
none. The driver runs such a test on a thread of its
own and waits for it no longer than its limit. One still running then
has failed, at the statement it was at: its run in real time is asked to
stop, which ends it as its end does, and the test goes on to its own
end. One that does not end soon after is left, and the interpreter ends.
A test that is no integration test has no limit yet.

A trace is kept as its publications and when each was, and never as a
cell for each microsecond: a publication a second after a run's start
would be a million cells on. A trace written out dense, `[7, _, 9]`, is
read with cell i at i smallest steps from the start. A timed trace,
`[0s: 7, 2s: 9]`, gives only the publications (TST-7). Its keys are
durations from the run's start or absolute datetimes. Computed keys must
strictly increase. Replay rejects an input key before the run's start
(CEVAL-2). Two recordings match when the publications have the same
values and offsets from their respective starts.

#### An application

`hgl run ROOT... [--application PATH] [--arg NAME=VALUE]... [--config
FIELD=VALUE]...` runs an application (UNIT-14) on hgraph: the program's
one application, or the one named.

What starts an application "may replace an argument of its entry or a
field of its configuration with a constant checked against the type it
replaces, and what the application writes is the default" (RUN-2).
`--arg` replaces an argument and `--config` a field. The value is HGL,
one expression on one line. `hgl_cli`'s `replace` puts it, in
parentheses, in the text of the application's run statement: in place
of the value the application wrote for that parameter or field, or as
an argument by name where it wrote none. The program is then read and
checked again as if the application had written that, so the analyser
holds a replacement to every rule it holds a written argument to, and
no stage knows of replacements.

```text
eval(counted(upto: 3))                         the application
--arg upto=5 --config length=10s               what starts it
eval(counted(upto: (5)), RunConfig(length: (10s)))
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
generic arguments, the program is rejected. A configuration that is not written as
`RunConfig(...)`, a constant's name, has its fields replaced by nothing
yet.

| Step | What does it |
| --- | --- |
| The run statement is written as a function, `_application`, in the application's own Python module | The transpiler |
| What sets the run's store, `inject global_state` and the `put` calls before the run statement, is written as a function of the store, `_application_store` | The transpiler |
| The function calls the harness's `run` with the entry, core's `record` for the entry's result, the mode, the entry's `const` arguments and the configuration | The generated code |
| `run` makes the run's store, calls `_application_store` with it, wires the entry and `record` into a graph, runs it with hgraph, and gives what `record` put under `"run.output"` | `support/harness.py`: the run an eval is too |
| A driver loads the module, calls the function on a thread of its own, and writes each publication to a results file | `support/run_application.py` |
| `hgl run` starts the driver, and prints the publications once the run has stopped | `hgl_hgraph::run_application`, `hgl_cli` |

```text
1970-01-01T00:00:01.000001Z  2
1970-01-01T00:00:02.000001Z  4
ran demo.ticks.backtest on hgraph: 2 publications
```

The run's output goes to standard output, a line for each publication:
its instant and what was published, written as HGL writes a value where
it has a way to. What the program logs and hgraph prints goes to standard
error, as the run goes.

`eval` is a simulation, and `run` is in real time (RUN-3). An interrupt
of `hgl run`, or a request for it to end, is the host's stop of a run in
real time. hgraph waits for a run's next cycle where no signal reaches
Python, so the driver keeps its main thread for the signal, and asks the
run to stop through a source hgraph wakes for: the run ends as it does
at its end, every node is stopped, and what was recorded up to then is
printed. `hgl` takes the signal too, and passes the stop to the driver
down a pipe: a signal sent to `hgl` alone reaches no other process. It
outlives the signal to print the output. A stop of a simulation, and a
second one of any run, end the run at once with no output.

| Exit status of `hgl run` | When |
| --- | --- |
| 0 | The run ended |
| 1 | The program is rejected, as written or with the replacements given, or the run failed |
| 2 | No application was chosen, a replacement names nothing or is not one expression, the target does not write all it needs, or the host did not run it |

Not written: outside events in real time, and a view of the output
while the run goes.

The crate's tests compare the Python written for a small module with
what is expected, check how dense scalar traces are written, and check
which tests of a program with unwritten forms are
blocked. Tests that run on hgraph need a Python that has it, named by
the environment variable `HGL_HGRAPH_PYTHON`, and are run with
`cargo test -p hgl_hgraph -p hgl_cli -- --ignored`. The `hgraph` job of
the CI workflow runs them on Linux for every pull request. Its backend is
hgraph 0.9.3, the first whose inputs say `invalidated` in the cycle they
lose their value, where `modified` is of a publication alone. The harness preserves a published null and omits silence
(TST-6).

The job also builds `hgl` and runs every test in the pinned core's
`tests/conformance` and `tests/implementation`, including integration tests.
Run the same check with a Python that has hgraph installed:

```sh
python crates/hgl_hgraph/ci/check_core.py target/debug/hgl spec/hgl_core
```

The check compares the runner's full discovery and execution lists and prints
the outcomes and counts for each suite. A case can be reported as not run
only for its source-located hgraph target diagnostic listed in
[`unsupported_cases.toml`](../crates/hgl_hgraph/ci/unsupported_cases.toml).
An assertion or source failure, an unavailable host, a new gap, or a listed
case that passes or disappears fails the check. Remove a listed gap when its
case starts passing. The backend owns this list alongside its target tests.

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
   done); types for the scalar slice.
4. Transpiler for hgraph (first slice done); `hgl test` running tests
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
| `codespan-reporting` | `hgl_diagnostics` | source excerpts and underlines in rendered diagnostics |
| `ctrlc` | `hgl_cli` | taking an interrupt or a request to end while an application runs, to stop the run and still print its output |
| `petgraph` | `hgl_analyser` | the graph of imports and the groups of modules in a cycle |
| `rowan` | `hgl_parser` | the lossless syntax tree: nodes, tokens, byte ranges, traversal |
| `tempfile` | `hgl_cli` | the directory a program is written to for a test run or an application's run, removed after it |
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
