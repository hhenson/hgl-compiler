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

- hgraph: a call's arguments are written in the order they are given, and Python evaluates each once in that order; an index of a place a call assigns, and of a compound assignment's place, is evaluated once and held, with what it is an index of where reading that may fail
- Left: a pack; a default evaluated where the argument it stands for would be

## EXPR-3

analyser: partial

- A local has the type written or its first value's; only a `var` is assigned, with a value of its type; a `var` is read only once assigned, through `if`, `switch` and loops (CTRL-11)
- In a graph a local is a connection or an ordinary value by its first value
- A `var` of a graph declared with no value takes the category of its first assignment
- In a value function, a hook and a source an assignment names a place inside a value, `point.x = 1`, `row[0] += 1`, `book["a"] = 1`: the root is a `var`, a `state`, a `cache` or `out`, a `let`, a parameter and an iteration binding are read-only all the way down (`binding.immutable`), the value is of the place's type, and a compound assignment reads the place first
- Left: an annotation checked against the category of its value

transpiler: partial

- hgraph: an assignment to a field or an element, at any depth, leaves what took the old value as it was: a list's and a tuple's element by its index, which fails outside the sequence, and a dictionary's value by its key, which is added where it is not there
- An element of the list, and an entry of the dictionary, that a `var`, a `var` parameter, a `state` or a `cache` holds is assigned where the collection is, with no copy; what took the old value is a copy already (LIST-3)
- A place further inside the value, `book.bids[price] = size`, `rows[key][0] = 2`, is assigned by the provider going in (`_core.set_at`): each collection on the way is changed where it is
- What a place is assigned is a copy, where something else holds it, and so is a key a dictionary is given
- A field of the struct a `var`, a `var` parameter, a `state` or a `cache` holds is assigned where the struct is (`_core.set_field`, `_core.set_at`), and what took it took a copy (the owner's rule, issue 151); a struct of at most two scalar fields is a value, and is given a new one, as a tuple is, though a parent's type stands for it
- Left: `out` assigned piecewise; a way in whose index is an expression after an element read, which is given new values

## EXPR-4

analyser: partial

- In a graph each value is a connection or an ordinary value: a temporal parameter and the result of a temporal or lifted call are connections
- A reassignment across the two is rejected (`binding.category`, with CTRL-14)
- Left: reassigning a graph `var` rewires

## EXPR-5

analyser: partial

- A value fits the type its place expects, in a value function, a hook and a graph; an integer literal is an `f64` where one is expected, the `f64` of its value (UNIT-13: `let x: f64 = 1` binds `1.0`), and a value that has a type is not converted; `null`, a tuple and a sequence take the expected type, and so does a child read checked when it runs (ERR-14)
- What is read again to learn its place's type is counted for each statement, so a statement is judged the same whatever comes before it; one that needs more than its bound, 10,000 and 16 for each node of its syntax, is local `reading.limit`, and is not accepted
- Left: const expression restrictions

## EXPR-6

analyser: partial

- The Boolean connectives and `!` take bools, in value functions
- A compile-time constant short-circuits (`constant`): the right operand of `false &&` and `true ||` is not needed where it is a parameter, a constant held by its name, or one whose evaluation fails, which is then no failure (`const NO: bool = false && (1 // 0 == 0)` is `false`); one of no type is held to a `bool` still
- Left: a guard precedes the payload read it protects

transpiler: planned

- Short-circuit evaluation; composition reads no payload

## EXPR-7

analyser: partial

- A field of a struct value has its type, the value's generic arguments in place; a list's element by an `i64`, a tuple's by a constant within its length, read by the one reader (`constant`): a literal, a constant of a module, and a constant expression of those
- A set indexed is rejected, to read or to assign
- A member or an element of a nullable's payload is read where a null test proves it present (ERR-12), and a result's payload through its tag (ERR-3)
- A structural projection of a `ref` parameter in a hook, `xs[i]` or `quote.bid`, selects its child, an endpoint, and reads no payload; an atomic projection of one, `quote.bid` of `ref quote: atomic<Quote>`, reads its payload and is `ref.read` (FN-16)
- Left: an index on any other value that has none; temporal projection

transpiler: partial

- hgraph: brackets read a list's and a tuple's element by its index, failing outside the sequence, and a dictionary's value by its key
- Left, reported: an assertion that an index outside a list fails with `value.index_out_of_range`, a code the provider's failure has not (TST-10)
- Left: projection of a connection; a child of a `ref` parameter, which is reported (FN-16)

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

## EXPR-10

parser: done

- A named-entry brace directly after an operator is its configuration; an empty one rejected

analyser: partial

- In a value function, a hook and a graph, a symbol's configuration is bound as the named arguments of the call the symbol stands for: to the options its operator's candidates declare, a generic value parameter before a parameter; an unknown or a repeated entry, and one no candidate for the operands declares, rejected
- Left: the Boolean connectives and `!`

## EXPR-11

parser: done

- `<=>` at the comparison tier

analyser: done

- In a value function, a hook and a graph `<=>` is core's `CmpResult`, through the candidates of `compare_`, and each other comparison a bool; an outcome where a bool is expected is rejected

## EXPR-12

analyser: partial

- In a value function, a hook and a graph, an option of an operator's call, by name or as a symbol's configuration, that a candidate's `requires` reads selects the candidate, and so the result type; one that is a generic value parameter is given a compile-time constant
- Left: an option that is a parameter of its candidates, given what is no constant, which is not judged

## EXPR-13

transpiler: partial

- hgraph: a configured binary expression is written left operand, configuration, right operand, and Python evaluates a call's arguments in that order
- Left: unary configuration, and short-circuiting

## EXPR-14

analyser: done

- In a graph an entry of a symbol's configuration given to a value candidate, which the graph lifts, is given no connection (`connection.as_constant`); a temporal candidate's parameter takes one, and so does a call by name (FN-6)
- In a value function and a hook the entry is a value of the invocation

transpiler: planned

- Configuration bound at graph construction in composition, per invocation elsewhere

## EXPR-15

analyser: done

- In a graph an assignment into a place of a `var` is read as one elsewhere is (EXPR-3), and is an assignment to the `var`: a connection stays one whatever its place is given, an ordinary value given a connection, indexed by one, or assigned in a branch selected at runtime is rejected (`binding.category`), and so is a `var` declared outside a loop that makes one child for each member (`loop.carried`)

transpiler: partial

- hgraph: a field of a struct's connection, a tuple's position and a fixed list's position under a literal index, at any depth, rebind the `var` to a new bundle or `TSL` of the old one's parts but the place's, made where the graph is built (`from_ts`); what read the `var` before keeps its connection, and a branch selected at runtime assigns it as it assigns the `var`
- An ordinary value assigned is a constant connection of the place's type; a nullable scalar place takes a connection of its `T` through `_core.nullable`
- Left, reported: a field of an atomic struct's connection, an entry of a dictionary's, a position of an unsized list's, and a fixed list's or a tuple's position by an index that is no literal
