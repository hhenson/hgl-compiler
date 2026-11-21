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
- Left: what a pack holds; a generic function's parameter types

transpiler: partial

- Materializes arguments that can change state or fail in written order, before filling
  omitted defaults. Applies this to calls, constructors, lifted operands and entry
  calls.
- Left: Statement-emitting or propagating defaults remain unsupported (UNIT-8, TYPE-5).
  Calls with no resolved target or argument slot have defensive reports.

## EXPR-3

analyser: partial

- A local has the type written or its first value's; only a `var` is assigned, with a value of its type; a `var` is read only once assigned, through `if`, `switch` and loops (CTRL-11)
- In a graph a local is a connection or an ordinary value by its first value
- A `var` of a graph declared with no value takes the category of its first assignment
- In a value function, a hook and a source an assignment names a place inside a value, `point.x = 1`, `row[0] += 1`, `book["a"] = 1`: the root is a `var`, a `state`, a `cache` or `out`, a `let`, a parameter and an iteration binding are read-only all the way down (`binding.immutable`), the value is of the place's type, and a compound assignment reads the place first
- Left: an annotation checked against the category of its value

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
- Left: reassigning a graph `var` rewires

## EXPR-5

analyser: partial

- A value fits the type its place expects, in a value function, a hook and a graph; an integer literal is an `f64` where one is expected, the `f64` of its value (UNIT-13: `let x: f64 = 1` binds `1.0`), and a result or nullable value takes the form its place expects (ERR-12, ERR-15); `null`, a tuple and a sequence take the expected type, and so does a child read checked when it runs (ERR-14)
- A graph instance rechecks newly known values against established field, tuple, list, set and dictionary entry types (GEN-7).
- What is read again to learn its place's type is counted for each statement, so a statement is judged the same whatever comes before it; one that needs more than its bound, 10,000 and 16 for each node of its syntax, is local `reading.limit`, and is not accepted
- Left: const expression restrictions

## EXPR-6

analyser: partial

- The Boolean connectives and `!` take bools, in value functions, reading a fallible or nullable operand where a bool is expected (ERR-12, ERR-15)
- A graph instance fits newly known operands of `!`, `&&`, and `||`, with the right operand of a connective still short-circuited (GEN-7).
- A compile-time constant short-circuits (`constant`): the right operand of `false &&` and `true ||` is not needed where it is a parameter, a constant held by its name, or one whose evaluation fails, which is then no failure (`const NO: bool = false && (1 // 0 == 0)` is `false`); one of no type is held to a `bool` still
- Left: a guard precedes the payload read it protects

transpiler: done

- Value code and hooks use short-circuit Boolean expressions in operand order, including
  guarding a payload read.

## EXPR-7

analyser: partial

- A field of a struct value has its type, the value's generic arguments in place; a list's element by an `i64`, a tuple's by a constant within its length, read by the one reader (`constant`): a literal, a constant of a module, and a constant expression of those
- When a graph's deferred collection type settles, its bracket reads get the element type and take the implicit reads needed by the base, index or key (GEN-7, ERR-12, ERR-15).
- A graph member read with a deferred base retains its member name until selection can check the field or property and its implicit reads (GEN-7, ERR-12, ERR-15).
- A set indexed is rejected, to read or to assign
- A member or an element of a nullable's payload is read where a null test proves it present (ERR-12), and a result's payload through its tag (ERR-3)
- A structural projection of a `ref` parameter in a hook, `xs[i]` or `quote.bid`, selects its child, an endpoint, and reads no payload; an atomic projection of one, `quote.bid` of `ref quote: atomic<Quote>`, reads its payload and is `ref.read` (FN-16)
- Left: an index on any other value that has none; temporal projection

transpiler: partial

- Writes ordinary checked list and dictionary indexing, tuple and struct projection,
  temporal child access, atomic struct projection, and keyed graph references.
- Left: Reports a nonliteral tuple index in HIR, a hook child of a list of structs or
  tuples, a graph child of a list at a connection-valued index, nested dictionary entry
  reads and unsupported endpoint operations on children. Unsupported fields and element
  forms are reported at the projection.

## EXPR-8

analyser: done

- A `_` stands in a trace of a test: in a sequence that is none it is `trace.placeholder`

## EXPR-9

parser: done

- Brackets delimit a sequence whose entries are all values or all pairs

analyser: partial

- A sequence of values is a list or a set by the type expected, and with none a list of its values' one type, nullable when a `null` is among them; entries of two types, and entries that give no type, rejected
- Pairs are a dictionary where one is expected, each key and value read as its type; with nothing expected they are `sequence.context_type`
- Left: `_`; pairs as a timed trace; ordering and uniqueness

transpiler: partial

- Writes list, set and dictionary literals, and sequence-of-pairs where a timed trace is
  expected.
- Left: Reports dictionary keys that might coincide and sequence types without a
  constructor. Constant duplicate dictionary key rejection is pending (TYPE-1).

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
- Left: an option that is a parameter of its candidates, given what is no constant, which is not judged

## EXPR-13

transpiler: done

- Preserves the order of operator operands and configuration when hoisting evaluations,
  and of eval configuration before target arguments and defaults.

## EXPR-14

analyser: done

- In a graph an entry of a symbol's configuration given to a value candidate, which the graph lifts, is given no connection (`connection.as_constant`); a temporal candidate's parameter takes one, and so does a call by name (FN-6)
- In a value function and a hook the entry is a value of the invocation

transpiler: partial

- Holds scalar options such as numeric tolerances and zoned comparison's `utc` as
  lifted-call configuration. Value and hook calls evaluate the same arguments at
  invocation.
- Left: Explicit temporal configuration parameters await #363.

## EXPR-15

analyser: done

- In a graph an assignment into a place of a `var` is read as one elsewhere is (EXPR-3), and is an assignment to the `var`: a connection stays one whatever its place is given, an ordinary value given a connection, indexed by one, or assigned in a branch selected at runtime is rejected (`binding.category`), and so is a `var` declared outside a loop that makes one child for each member (local `loop.carried`, #373)

transpiler: planned

- Copy-on-write updates of fields and indexed places of a graph connection. Whole
  connection rebinding is under CTRL-14.

## EXPR-16

parser: planned

- #368 tracks ranges as a loop sequence in value code and in composition.

analyser: planned

transpiler: planned

- Range iteration awaits #368.

## EXPR-17

parser: planned

- #368 tracks lists built by loops in value code and in composition.

analyser: planned

transpiler: planned

- List construction by a loop awaits #368.
