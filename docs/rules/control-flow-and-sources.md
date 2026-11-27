# Control flow and sources

`control-flow-and-sources.md`

## CTRL-1

analyser: done

- In a graph an `if` is told apart by its condition: an ordinary `bool` composes one branch, a connection selects a child graph at runtime; in a value function and a hook it is ordinary control flow.

## CTRL-2

transpiler: partial

- A temporal `if` or `switch` wires only the selected child to run. It starts a new
  child on selection changes and leaves outside producers running.
- A window result starts with the newly selected child's samples (hgraph 0.9.32).
- Left: The proposed mapping-child lifetime is tracked in #370. A selected child's
  store read that determines wiring is reported (#541). An ordinary computation that
  may fail while building the selected child is also reported (#896, FN-9, ERR-5).

## CTRL-3

analyser: done

- A temporal `if` with no `else` whose value is used is a connection of `T`, not a `T?` (with CTRL-13)

transpiler: done

- An omitted `else` of a consumed temporal `if` is a typed branch with no publication.
  An outputless `if` needs no synthetic result.

## CTRL-4

analyser: done

- A branch's locals do not escape
- A `var` assigned under a temporal condition has a binding before the `if`, or every branch assigns it (with CTRL-14)
- Assignments to multiple outer connections are checked separately. Emission of
  those bindings is tracked under CTRL-14.

## CTRL-5

analyser: done

- In a value function, a path that has returned takes no part in what is assigned or in the type of an `if`
- In a graph a `return` with a value in a branch selected at runtime needs the continuation to return (CTRL-14)
- In a hook, a return leaves the continuation and its assignment checks to the
  paths that reach it. C++ continuation limits are tracked under CTRL-13 and CTRL-14.

## CTRL-6

analyser: partial

- A label is a compile-time constant of the selector's type (`switch.label_constant`, CTRL-12 and UNIT-13), read by the one reader (`constant`) and compared as the selector's `==` compares them (`ConstantValue::equals`): a literal however written, an enum member however written, a constant followed to its value, and a constant expression of those
- A repeated value is rejected (`switch.duplicate_label`), `case 1:` beside `case 0x1:` among them, and an enum's coverage by such labels stands for a `default`
- A generic graph reads an implied enum label when its selector settles and checks repeats against that enum's resolved labels (TYPE-7, GEN-7).
- Two labels are duplicates where `==` of the selector's type holds for them, as CTRL-6 has it; a selector is a `bool`, an `i64`, a `str` or an enum (CTRL-12), whose `==` is their sameness as constants (UNIT-13)
- Left: A label supplied by a generic value parameter can escape duplicate
  checking when its instance is made (#982). Labels using module constants
  computed by calls still depend on constant evaluation (#240).

transpiler: partial

- Value and temporal switches select by typed label value, with no fallthrough. An
  unmatched selector takes `default` if there is one, or fails.
- Left: The unmatched failure lacks `switch.unmatched` in value code, hooks and both
  forms of composition (#983). Reading a case binding after an ordinary or wiring-time
  switch is reported (#984, UNIT-12). The target reports equal labels in a graph
  instance, while a value instance can select the first equal label (#982).

## CTRL-7

analyser: partial

- A graph loop binds values from ordinary data and child connections from a fixed-size temporal list.
- Struct fields and rejecting a temporal sequence with changing membership await #373. The compiler still accepts dynamic temporal loop forms. Core still tests the dictionary form.

transpiler: partial

- A graph unrolls an ordinary collection or a local fixed-list connection. The current
  growing-list and dictionary forms use hgraph child graphs.
- The legacy dynamic list and dictionary loops run. A set connection or a dynamic loop
  key used as an ordinary value is reported.
- Left: Struct-field loops and rejection of changing membership await #373. The
  corresponding Core mapping forms are tracked in #370. The target reports a nonlocal
  fixed-list connection (#986) or a list size left unresolved by constant evaluation (#240).

## CTRL-8

analyser: partial

- Iteration over a `ref` parameter reads it, and is `ref.read` (FN-16)
- The Core `keys` operator on a dictionary supplies a set to ordinary hook traversal.
- Left: Dedicated hook-scoped views remain pending. `elements`, `values` and `items`
  have no callable binding in the pinned Core.

transpiler: partial

- Hooks loop over the supported whole list, set and dictionary reads, retaining list
  index order.
- A hook can traverse the ordinary value from a guarded whole-struct read
  (CTRL-12).

## CTRL-9

parser: done

- The first `{` outside parentheses, brackets and configuration opens the body of an `if`, `for`, `switch`, `when` or `while`

## CTRL-10

analyser: done

- In value functions and hooks: a block's value, the Boolean condition, required result
  and nullable reads (ERR-12, ERR-15), and the type of a consumed `if`. A block with no
  value rejects in a value position.

transpiler: partial

- Emits value blocks and typed `if`, including a nullable result for an omitted `else`.
  A block with a `let` and a value can be used as a call argument, a module initializer
  or a parameter default.
- Left: An early return inside a consumed conditional used as a call argument is
  reported (#985).

## CTRL-11

analyser: done

- In value functions and hooks, tracks assignment after conditionals and loops,
  ignoring paths that return. An uninitialized `var` written `T?` starts as null.

## CTRL-12

analyser: partial

- In a value function: `for` over a list, a set or a dictionary; `while` with its condition (`while.condition`); a `switch`'s selector, and each label a constant of its type (CTRL-6)
- Value code and hooks traverse a known nominal struct in parent-first
  declaration order. A one-name pattern binds the field name. A two-name
  pattern also binds the field at its own type (CTRL-15). An explicitly
  written value argument to an ordinary function can use the field name as
  a constant string in each pass (GEN-8).
- Left: Traversal over a generic parameter constrained to the struct
  category still rejects at its declaration (#373, #751).

transpiler: partial

- Loops over ordinary lists, sets and dictionaries in value code and hooks.
- An ordinary struct iterable is held once. Each field pass has its own
  bindings and reads from that held value.

## CTRL-13

analyser: done

- In a graph the condition of an `if` expects a `bool`, a value or a connection, reading a result or nullable where necessary (ERR-12, ERR-15); the `if` has its branches' one type, and is a connection when its condition or a branch is one; with no `else` it is a `T` connection under a temporal condition, and a `T?` or a `T` connection under a wiring-time one by what its branch is
- When an instance selects a deferred operator call, the resulting categories settle a no-else expression. Its type propagates through inferred tuple and list members, block tails and local reads. The analyser fits newly known types at their conditions, assignments, arguments and returns, including any required implicit reads (GEN-7, EXPR-5).
- A consumed `if` with an `else` reconciles newly known branch types using the source rules. When one branch returns, the surviving branch supplies the type. A dropped `if` still has no common type requirement (CTRL-10, GEN-7).

transpiler: partial

- A graph selects an `if` during wiring for an ordinary condition or wires a runtime
  selector for a connection. A consumed branch can have no publication (CTRL-3).
  A returning `if` bound directly by a `let` can give the graph's result or continue
  with the other branch's value.
- Left: A returning temporal `if` nested in another expression and a consumed temporal
  `if` that also assigns an outer `var` are reported (#986).

## CTRL-14

analyser: done

- A `var` of a graph keeps the category of its first value, and one that holds an ordinary value is not assigned in a branch selected at runtime (`binding.category`); a `return` with a value in such a branch needs every path to return (`return.missing`)
- The same holds in the cases of a `switch` selected at runtime
- After specialization, the analyser checks the assignment against the first value's category and against any enclosing branch now selected at runtime (GEN-7, `graph_instances_recheck_assignments`).
- An instance applies the same temporal-branch restrictions to assignments through a call's `var` parameter (FN-15).
- A valued return from a branch that becomes temporal in an instance requires a complete continuation (FN-7). A final switch whose cases all return can complete it when its enum coverage becomes known, including at the end of an arm of a final two-branch conditional (CTRL-6).

transpiler: partial

- Rebinds a whole connection in wiring-time branches. A temporal branch selects one
  assigned `var` or the result of its return and continuation. A return from a wiring-time
  loop can skip its remaining passes. Ordinary `var` assignments under a temporal
  selector and a bare return in a graph with a result reject in the analyser.
- Left: A temporal branch assigning multiple outer `var`s and a return from a temporal
  branch inside an unrolled loop or inside an outputless graph are reported (#986).

## CTRL-15

analyser: partial

- A graph supports `switch` over an ordinary value or a connection with compile-time labels, and `while` with an ordinary condition. It binds loop items by sequence type.
- A graph instance rechecks a `while` condition after selecting deferred operator calls. A connection there is `connection.as_constant` (FN-3, GEN-7).
- A `switch` with a newly settled selector or label rechecks the selector type and fits the affected labels against it (CTRL-12, GEN-7).
- Selection also settles each `for` binding's type and category from the sequence before the instance checks its uses (CTRL-12, FN-3, GEN-7).
- A loop that selects a temporal sequence now creating children checks outer writes, including through `var` parameters, and returns against the existing `loop.carried` restrictions.
- CTRL-15 allows an outer `var` to carry across passes over a connection of `list<T, N>` or a range with constant bounds (EXPR-16). Each pass rebinds the `var` at wiring time (EXPR-15).
- Struct and atomic struct iteration and the proposed `for.sequence` diagnostic await #373. Dynamic loops continue to accept temporal collections and report local `loop.carried` for an outer assignment or a return.

transpiler: partial

- Emits wiring-time and temporal switches, unrolls range and fixed-list connection loops, and
  emits the existing dynamic list and dictionary forms.
- Left: Field traversal and the changing-membership rejection await #373, with dynamic
  children moving to #370. The loop size, nonlocal list, set, key and branch return
  restrictions are listed under CTRL-7 and CTRL-14.

## CTRL-16

analyser: partial

- A computed pack reads only the branch selected in each concrete pack-key pass. A chain with no selected branch and no `else` supplies no entry (FN-19, GEN-7).
- A pack-key condition with a resolved module constant or generic value parameter checks only the selected branch. A generic pack supplies its keys in the making instance. Ordinary `const` and runtime parameters cannot supply the key (FN-1, GEN-17).
- An ordinary `if` with no selected branch and no `else` in a pass gives `null`. A nullable place supplies its type. A `let` with no type context reports `null.context_type` (CTRL-10, TYPE-14).
- In an ordinary struct pass, a resolved constant condition checks its
  selected branch. Type queries in that condition read the current field
  bindings (TYPE-18). A switch label can read the constant field name.
- Left: A module constant computed by a call can remain unresolved in a generic
  value argument within a computed spread or in a pack-key condition (#240). Ordinary
  graph ranges can unroll a bound supplied by such a constant.
- Left: A nominal struct type or constructor with a field name as its
  written generic value argument still rejects before the pass (#373).

transpiler: partial

- C++ emits the entries supplied by the selected branches of a computed pack.
- C++ emits the selected branch of a resolved pack-key condition in a graph, node or value function.
- C++ emits selected constant branches of each pass over an ordinary struct.

## SOURCE-1

analyser: done

- A source's shape: no temporal parameter (`source.input`), no `when` (`source.when`), no `var`, and no `inject` of `out`, `scheduler` or `alarm` (`source.statement`)
- One with a declaration or a hook has its declarations, `start`, one block that holds every `yield` (`{ }`, `while` or `for`), and `stop`, in that order and nothing else: a statement outside the block, a second block, a hook out of order and a `yield` in a hook are `source.statement`
- Its declarations and hooks stand in its body itself and each hook once, as a node's do (TIME-5)
- Its body is read: its declarations and hooks as a node's, and its other statements as value code, with each `yield`'s value held to the result's type
- An anonymous function has its own body (FN-14). Its `yield` is rejected with `yield.outside_source`, including where the enclosing function is a source. Only the source's own `yield` counts towards its source classification (FN-2).
- It injects neither `scheduler` nor `alarm`, so `scheduled()` in it is `inject.no_scheduler` (TIME-9)
- A source with no result arrow is rejected at its name with the proposed code `source.result`.

transpiler: done

- Emits source bodies as coroutines, with state, cache, and lifecycle hooks. Supplies
  clock, logger and store capabilities.
- Disallowed injections, `when` and a missing result arrow reject before emission.

## SOURCE-2

analyser: done

- A `yield` time is a `duration` or `datetime` (`yield.time_type` for other types). A nullable or fallible time is read as its payload (ERR-12, ERR-15). Any failure is raised in the source (ERR-5).
- Agreed. A time of a generic type is admitted when its declared type bounds prove it can only be `duration` or `datetime`, for example `T == duration` or `T in {duration, datetime}` (GEN-7). Positive conjunctions intersect known bounds. In a disjunction, every branch must provide a bound. The typed body retains the time's generic type for each instance.

transpiler: done

- Evaluates each yield's time and payload once in order, then resolves relative or
  absolute time and validates it. Each generic instance uses the relative or
  absolute form selected by its actual time type.

## SOURCE-3

transpiler: done

- Skips past targets, publishes current ones, and holds a future payload until its wake.
  The generator resumes after that yield and checks strictly increasing targets.

## SOURCE-4

analyser: done

- Proposed SOURCE-4 rejects a source's `return` with a value and a `yield` inside a block or an `if` whose value is used. The proposed catalogue codes are `source.return_value` and `yield.in_value`.

transpiler: done

- Completes on a bare return or body end, retaining locals across suspension. A source
  can loop without a condition. A valued return has a defensive target report.
