# Control flow and sources

`control-flow-and-sources.md`

## CTRL-1

analyser: done

- In a graph an `if` is told apart by its condition: an ordinary `bool` composes one branch, a connection selects a child graph at runtime; in a value function and a hook it is ordinary control flow.

## CTRL-2

transpiler: partial

- A temporal `if` or `switch` wires only the selected child to run. It starts a new
  child on selection changes and leaves outside producers running.
- Left: The proposed child lifetime for core mapping awaits #370. Child graph bodies
  cannot read capabilities captured from the parent or compute ordinary values that may
  fail (FN-9, ERR-5).

## CTRL-3

analyser: done

- A temporal `if` with no `else` whose value is used is a connection of `T`, not a `T?` (with CTRL-13)

transpiler: done

- An omitted `else` of a consumed temporal `if` is a typed branch with no publication.
  An outputless `if` needs no synthetic result.

## CTRL-4

analyser: partial

- A branch's locals do not escape
- A `var` assigned under a temporal condition has a binding before the `if`, or every branch assigns it (with CTRL-14)
- Left: several results kept as separate connections

## CTRL-5

analyser: partial

- In a value function, a path that has returned takes no part in what is assigned or in the type of an `if`
- In a graph a `return` with a value in a branch selected at runtime needs the continuation to return (CTRL-14)
- Left: runtime bodies

## CTRL-6

analyser: partial

- A label is a compile-time constant of the selector's type (`switch.label_constant`, CTRL-12 and UNIT-13), read by the one reader (`constant`) and compared as the selector's `==` compares them (`ConstantValue::equals`): a literal however written, an enum member however written, a constant followed to its value, and a constant expression of those
- A repeated value is rejected (`switch.duplicate_label`), `case 1:` beside `case 0x1:` among them, and an enum's coverage by such labels stands for a `default`
- Two labels are duplicates where `==` of the selector's type holds for them, as CTRL-6 has it; a selector is a `bool`, an `i64`, a `str` or an enum (CTRL-12), whose `==` is their sameness as constants (UNIT-13)
- Left: a label whose value is not worked out, a generic value parameter's or one of a candidate of the program's own, whose repeat is not judged

transpiler: partial

- Value and temporal switches select by typed label value, with no fallthrough. An
  unmatched selector takes `default` if there is one, or fails.
- Left: The failure lacks `switch.unmatched`. A case binding read after its switch and
  equal labels discovered in an instance are reported.

## CTRL-7

analyser: partial

- A graph loop binds values from ordinary data and child connections from a fixed-size temporal list.
- Struct fields and rejecting a temporal sequence with changing membership await #373. The compiler still accepts dynamic temporal loop forms. Core still tests the dictionary form.

transpiler: partial

- A graph unrolls an ordinary collection or a local fixed-list connection. The current
  growing-list and dictionary forms use hgraph child graphs.
- Left: Struct-field loops and replacing the dynamic forms with core mapping await #373
  and #370. A set connection, a fixed-list expression that is not a local, an unresolved
  list size and a dynamic loop key used as an ordinary value are reported.

## CTRL-8

analyser: partial

- Iteration over a `ref` parameter reads it, and is `ref.read` (FN-16)
- Left: the traversal forms `elements`, `keys`, `values` and `items`

transpiler: partial

- Hooks loop over the supported whole list, set and dictionary reads, retaining list
  index order.
- Left: Struct-field traversal awaits #373. Other sequence types and unsupported child
  forms are reported.

## CTRL-9

parser: done

- The first `{` outside parentheses, brackets and configuration opens the body of an `if`, `for`, `switch`, `when` or `while`

## CTRL-10

analyser: partial

- In a value function: a block's value; a `bool` condition, reading a result or nullable where necessary (ERR-12, ERR-15); the type of an `if` whose value is used
- Left: temporal bodies

transpiler: partial

- Emits value blocks and `if` with typed values, including a nullable result for an
  omitted `else`.
- Left: A value expression that needs a statement-emitting block or a block that returns
  can still be reported when there is no statement context. A block with no value cannot
  supply an expression.

## CTRL-11

analyser: partial

- In a value function: what is assigned after an `if`, a `switch` and a loop; a `var` written `T?` is null from its declaration
- Left: temporal bodies

## CTRL-12

analyser: partial

- In a value function: `for` over a list, a set or a dictionary; `while` with its condition (`while.condition`); a `switch`'s selector, and each label a constant of its type (CTRL-6)
- Field iteration in value code and hooks awaits #373.

transpiler: partial

- Loops over ordinary lists, sets and dictionaries in value code and hooks.
- Left: Struct fields await #373. Other loop sequences are reported.

## CTRL-13

analyser: done

- In a graph the condition of an `if` expects a `bool`, a value or a connection, reading a result or nullable where necessary (ERR-12, ERR-15); the `if` has its branches' one type, and is a connection when its condition or a branch is one; with no `else` it is a `T` connection under a temporal condition, and a `T?` or a `T` connection under a wiring-time one by what its branch is

transpiler: partial

- A graph selects an `if` during wiring for an ordinary condition or wires a runtime
  selector for a connection. A consumed branch can have no publication (CTRL-3).
- Left: A consumed branch with an escaping `return`, a temporal `if` used as a value
  that also assigns an outer `var`, and a value-producing branch without a return in its
  generated continuation are reported.

## CTRL-14

analyser: done

- A `var` of a graph keeps the category of its first value, and one that holds an ordinary value is not assigned in a branch selected at runtime (`binding.category`); a `return` with a value in such a branch needs every path to return (`return.missing`)
- The same holds in the cases of a `switch` selected at runtime

transpiler: partial

- Rebinds a whole connection in wiring-time branches. A temporal branch selects one
  assigned `var` or the result of its return and continuation.
- Left: A temporal branch assigning multiple outer `var`s, a return inside an unrolled
  loop or an outputless graph's temporal branch, and a branch that both supplies a value
  and returns without a value on another path are reported. Ordinary `var` assignments
  under a temporal selector remain source rejections.

## CTRL-15

analyser: partial

- A graph supports `switch` over an ordinary value or a connection with compile-time labels, and `while` with an ordinary condition. It binds loop items by sequence type.
- CTRL-15 allows an outer `var` to carry across passes over a connection of `list<T, N>` or a range with constant bounds (EXPR-16). Each pass rebinds the `var` at wiring time (EXPR-15). #368 tracks constant ranges.
- Struct and atomic struct iteration and the proposed `for.sequence` diagnostic await #373. Dynamic loops continue to accept temporal collections and report local `loop.carried` for an outer assignment or a return.

transpiler: partial

- Emits wiring-time and temporal switches, unrolls fixed-list connection loops, and
  emits the existing dynamic list and dictionary forms.
- Left: Field traversal and the changing-membership rejection await #373, with dynamic
  children moving to #370. The loop size, nonlocal list, set, key and branch return
  restrictions are listed under CTRL-7 and CTRL-14.

## CTRL-16

analyser: planned

- #368 tracks checking only the selected branch of a constant condition in an unrolled pass.

transpiler: planned

- Constant branches of compile-time loops await #368.

## SOURCE-1

analyser: done

- A source's shape: no temporal parameter (`source.input`), no `when` (`source.when`), no `var`, and no `inject` of `out`, `scheduler` or `alarm` (`source.statement`)
- One with a declaration or a hook has its declarations, `start`, one block that holds every `yield` (`{ }`, `while` or `for`), and `stop`, in that order and nothing else: a statement outside the block, a second block, a hook out of order and a `yield` in a hook are `source.statement`
- Its declarations and hooks stand in its body itself and each hook once, as a node's do (TIME-5)
- Its body is read: its declarations and hooks as a node's, and its other statements as value code, with each `yield`'s value held to the result's type
- It injects neither `scheduler` nor `alarm`, so `scheduled()` in it is `inject.no_scheduler` (TIME-9)
- A source with no result arrow is rejected at its name with `source.result`.

transpiler: partial

- Emits source bodies as coroutines, with state, cache, and lifecycle hooks. Supplies
  clock, logger and store capabilities.
- Left: Other injections, a `when` and a source with no result have defensive target
  reports. The analyser rejects those forms. Removal of the no-result report
  is tracked in #402.

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
