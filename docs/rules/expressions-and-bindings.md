# Expressions and bindings

`expressions-and-bindings.md`

## EXPR-1

parser: done

- Precedence, associativity, grouping; a tuple has a comma

analyser: done

- A chained comparison is typed as written, `(a < b) < c`, not as a conjunction

## EXPR-2

analyser: partial

- In a value function, a hook and a graph, the arguments of a value function's call, positional before named, and of a struct's construction, all named; duplicate, unknown, missing, surplus and mistyped arguments rejected
- A named operator call, and a symbol's configuration, may name an option its candidates declare
- Binds, indexes and spreads homogeneous and reflected packs. Ordinary generic
  calls use their fitted parameter types. FN-17 and GEN-7 record remaining callback
  and forwarded type-evidence limits.

transpiler: partial

- Materializes arguments that can change state or fail in written order, before filling
  omitted defaults. Applies this to calls, constructors, lifted operands and entry
  calls. A scalar output payload read in a hook can fail when unset (TIME-7).
- Emits statement-block field defaults for concrete and specialized structs
  (UNIT-8, TYPE-5). In the emitted forms an omitted field default can propagate
  a failure. Calls with no resolved target or argument slot have defensive reports.

## EXPR-3

analyser: partial

- A local has the type written or its first value's; only a `var` is assigned, with a value of its type; a `var` is read only once assigned, through `if`, `switch` and loops (CTRL-11)
- In a graph a local is a connection or an ordinary value by its first value
- A `var` of a graph declared with no value takes the category of its first assignment
- In a value function, a hook and a source an assignment names a place inside a value, `point.x = 1`, `row[0] += 1`, `book["a"] = 1`: the root is a `var`, a `state`, a `cache` or `out`, a `let`, a parameter and an iteration binding are read-only all the way down (`binding.immutable`), the value is of the place's type, and a compound assignment reads the place first
- A graph local's annotation checks its type. A typed binding keeps its
  initializer's value or connection category.

optimiser: done

- Plans an in-place append for a local `str` variable with the built-in `+=` and splits independent string concatenation pieces (TYPE-1). An expression that can write the destination retains the compound assignment so the old place is read first.

transpiler: partial

- Emits immutable and mutable bindings, assignments to supported places, and copy-
  before-write where later mutation would otherwise change a held value.
- Left: Unsupported assignment places include abstract struct fields and graph
  connection children (EXPR-15). A connection local must be a `var` for whole
  reassignment. Dictionary entries can be assigned, but cannot be passed as a `var`
  argument (FN-15).

## EXPR-4

analyser: partial

- In a graph each value is a connection or an ordinary value: a temporal parameter and the result of a temporal or lifted call are connections
- A reassignment across the two is rejected (`binding.category`, with CTRL-14)

C++ rebinding is recorded under CTRL-14. A whole connection reassignment
rewires subsequent reads, and earlier aliases keep their original connection.
A supported temporal branch selects the replacement or the previous connection.

## EXPR-5

analyser: partial

- A value fits the type its place expects, in a value function, a hook and a graph; an integer literal is an `f64` where one is expected, the `f64` of its value (UNIT-13: `let x: f64 = 1` binds `1.0`), and a result or nullable value takes the form its place expects (ERR-12, ERR-15); `null`, a tuple and a sequence take the expected type, and so does a child read checked when it runs (ERR-14)
- A graph instance rechecks newly known values against established field, tuple, list, set and dictionary entry types (GEN-7).
- A graph instance refits an assigned list literal's known entries against the element type established by the inferred variable's initializer (EXPR-3, GEN-7).
- What is read again to learn its place's type is counted for each statement, so a statement is judged the same whatever comes before it; one that needs more than its bound, 10,000 and 16 for each node of its syntax, is local `reading.limit`, and is not accepted
- A written generic value argument that is a live connection rejects with
  `generic.argument_constant` (GEN-8).
- Left: A required-constant call to a `const fn` can pass checking without its
  failure being reported (UNIT-13, ERR-9).

## EXPR-6

analyser: partial

- The Boolean connectives and `!` take bools, reading a fallible or nullable operand where a bool is expected (ERR-12, ERR-15).
- Composition selects core's Boolean operators, with unparenthesized chains selecting `all_` or `any_` over their operands. A graph instance selects after any deferred operands settle (FN-6, GEN-7, OPS-5).
- Boolean category probes defer nested scalar-flow retries to the real read, so 24 nested guards can each read the binding they refine without exponential rereading. Scalar subtrees retain their own short-circuit facts inside a temporal outer form (ERR-12, FN-6).
- A compile-time constant short-circuits (`constant`): the right operand of `false &&` and `true ||` is not needed where it is a parameter, a constant held by its name, or one whose evaluation fails, which is then no failure (`const NO: bool = false && (1 // 0 == 0)` is `false`); one of no type is held to a `bool` still
- In a handler with manual validity, a leading `valid(input)` guards a read of
  that input on the right of `&&`. The reversed order reports `payload.unguarded`.

transpiler: done

- Value code and hooks use short-circuit Boolean expressions in operand order, including
  guarding a payload read.
- A Boolean call over connections wires every connected operand as an input (FN-6, FN-11).

## EXPR-7

analyser: partial

- A field of a struct value has its type, the value's generic arguments in place; a list's element by an `i64`, a tuple's by a constant within its length, read by the one reader (`constant`): a literal, a constant of a module, and a constant expression of those
- When a graph's deferred collection type settles, its bracket reads get the element type and take the implicit reads needed by the base, index or key (GEN-7, ERR-12, ERR-15).
- A tuple that settles in an instance checks the same constant index and bounds. A checked position is recorded as its integer value.
- A graph member read with a deferred base retains its member name until selection can check the field or property and its implicit reads (GEN-7, ERR-12, ERR-15).
- An index whose base has a known type outside lists, tuples and dictionaries reports `type.mismatch`, including when that base settles in a graph instance (GEN-7). A set has no indexed place to assign (EXPR-3).
- A member or an element of a nullable's payload is read where a null test proves it present (ERR-12), and a result's payload through its tag (ERR-3)
- A structural projection of a `ref` parameter in a hook, `xs[i]` or `quote.bid`, selects its child, an endpoint, and reads no payload; an atomic projection of one, `quote.bid` of `ref quote: atomic<Quote>`, reads its payload and is `ref.read` (FN-16)
- Left: A bare type parameter can still be indexed without a requirement
  establishing a valid bracket base (GEN-7).

transpiler: partial

- Writes ordinary checked list and dictionary indexing, tuple and struct projection,
  temporal child access, atomic struct projection, and keyed graph references.
- A hook can read a guarded field or tuple position of a fixed-list child.
  The emitter reports an unresolved tuple position defensively.
- Left: Reports a list child at a connection-valued index in a graph, a nested
  dictionary entry's payload read in a hook, and other unsupported endpoint
  operations and projection forms.

## EXPR-8

analyser: done

- A `_` stands in a trace of a test: in a sequence that is none it is `trace.placeholder`

## EXPR-9

parser: done

- Brackets delimit a sequence whose entries are all values or all pairs

analyser: partial

- A sequence of values is a list or a set by the type expected, and with none a list of its values' one type, nullable when a `null` is among them; entries of two types, and entries that give no type, rejected
- An inferred list with unfinished entries keeps its element type open until graph specialization can join them and widen eligible literals (EXPR-5, GEN-7).
- Pairs are a dictionary where one is expected, each key and value read as its type; with nothing expected they are `sequence.context_type`
- A named function's empty sequence argument takes the parameter type when another argument supplies the missing type in a graph instance (GEN-2, GEN-7).
- A written `set<str>` rejects a repeated constant string at the later member (`set.duplicate_member`, UNIT-13).
- In a trace, timed pairs have explicit publications. `_` is admitted in a dense
  trace and rejected in a timed trace or ordinary sequence (EXPR-8, TST-7).
- Left: Repeated constant dictionary keys and repeated constant non-string set
  members can still pass source checking.

transpiler: partial

- Writes list, set and dictionary literals, and sequence-of-pairs where a timed trace is
  expected.
- A fixed list literal of connections wires each entry in order to its own list position.
- A set retains a member once if two value-dependent entries turn out to be equal.
- Left: Reports dictionary literals whose keys might coincide.

## EXPR-10

parser: done

- A named-entry brace directly after a symbolic operator holds its configuration. The brace needs at least one entry.
- Reads the same configuration before a call's parentheses. In a control header a configured call must be parenthesized.

analyser: partial

- A brace entry names a configuration parameter first, then a generic value parameter, then an ordinary parameter. Functions, named operators and symbol operators use this order. Rejects repeated and unknown entries and entries no candidate for the operands declares.
- Configuration on Boolean connectives and `!` is pending. Configuration after a callable name awaits #363.

## EXPR-11

parser: done

- `<=>` at the comparison tier

analyser: done

- In a value function, a hook and a graph `<=>` is core's `CmpResult`, through the candidates of `compare_`, and each other comparison a bool; an outcome where a bool is expected is rejected

## EXPR-12

analyser: partial

- In a value function, a hook and a graph, an option of an operator's call, by name or as a symbol's configuration, that a candidate's `requires` reads selects the candidate, and so the result type. A selecting option is a compile-time constant. The use of the result chooses its form (ERR-15).
- A runtime value given as division's `zero` option rejects with
  `generic.argument_constant` in both the named and symbol forms (GEN-8).

## EXPR-13

transpiler: done

- Preserves the order of operator operands and configuration when hoisting evaluations,
  and of eval configuration before target arguments and defaults.

## EXPR-14

analyser: done

- A value function's declared configuration slots take graph-construction values. Both spelling forms reject a connection in such a slot (`connection.as_constant`).
- An ordinary slot can receive a lifted input, including through a symbol's braces (FN-6). The selected declaration decides when candidates use the same option name in different slots.
- An instance checks a configuration argument again if its category was unsettled at declaration (GEN-7). In a value function or hook it is a value of the invocation.

transpiler: done

- Holds numeric tolerances at graph construction. Lifts an ordinary connection such as `calendar` or `utc` as an input. Value and hook calls evaluate their configuration at invocation.

## EXPR-15

analyser: done

- In a graph an assignment into a place of a `var` is read as one elsewhere is (EXPR-3), and is an assignment to the `var`: a connection stays one whatever its place is given, an ordinary value given a connection, indexed by one, or assigned in a branch selected at runtime is rejected (`binding.category`), and so is a `var` declared outside a loop that makes one child for each member (local `loop.carried`, #373)

transpiler: planned

- Copy-on-write updates of fields and indexed places of a graph connection. Whole
  connection rebinding is under CTRL-14.

## EXPR-16

parser: done

- Reads `..<` only in a loop's iterable, with additive expressions as bounds. In a value position the parser reports the illegal range as one expression.

analyser: done

- A range binds one `i64` in each pass. A live connection cannot supply a bound in composition.

transpiler: done

- Evaluates both bounds once in order and visits the increasing values before the exclusive end. A graph composes its passes while it is built.

## EXPR-17

parser: done

- Reads `[for name in iterable: expression]`.

analyser: done

- Types each entry in the iteration scope. A fixed result gets the range's constant length or its fixed iterable's extent, which must agree with the destination after selection.
- A range in a forwarded body uses its enclosing instance's fitted bounds when the body is read again (GEN-7).
- In composition, ordinary entries over a range make an ordinary list that a `const` parameter can take. A connection in a range's entries or the use of a fixed-list connection as the iterable makes a fixed list connection.

transpiler: partial

- Builds growing and fixed ordinary lists. Composes a nonempty fixed list of connections over a range or a fixed input list.
- Temporal conditionals and switches in an entry capture that pass's range binding or list child.
- Cold constant initializers and field defaults keep their comprehension bindings. The target visits the source's extent and checks the count before constructing a fixed result.
- The pinned hgraph SDK requires a positive extent for a fixed temporal list.
