# Control flow and sources

`control-flow-and-sources.md`

## CTRL-1

analyser: done

- In a graph an `if` is told apart by its condition: an ordinary `bool` composes one branch, a connection selects a child graph at runtime; in a value function and a hook it is ordinary control flow.

## CTRL-2

transpiler: planned

## CTRL-3

analyser: done

- A temporal `if` with no `else` whose value is used is a connection of `T`, not a `T?` (with CTRL-13)

transpiler: planned

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

- A label is a compile-time constant of the selector's type (local `switch.label_constant`, UNIT-13), read by the one reader (`constant`) and compared as the selector's `==` compares them (`ConstantValue::equals`): a literal however written, an enum member however written, a constant followed to its value, and a constant expression of those
- A repeated value is rejected (`switch.duplicate_label`), `case 1:` beside `case 0x1:` among them, and an enum's coverage by such labels stands for a `default`
- Two labels are duplicates where `==` of the selector's type holds for them, as CTRL-6 has it; a selector is a `bool`, an `i64`, a `str` or an enum (CTRL-12), whose `==` is their sameness as constants (UNIT-13)
- Left: a label whose value is not worked out, a generic value parameter's or one of a candidate of the program's own, whose repeat is not judged

transpiler: planned

## CTRL-7

analyser: done

- What a `for` in a graph receives: ordinary values over ordinary data, a connection of `T` over a connection of `list<T, N>`, and over a connection of `list<T>`, `set<T>` or `dict<K, V>` one child for each member, with the set's member and the dictionary's key ordinary values (with CTRL-15)

transpiler: planned

## CTRL-8

analyser: partial

- In a loop that makes one child for each member, an assignment to a `var` declared outside it and a `return` are rejected (`loop.carried`)
- Iteration over a `ref` parameter reads it, and is `ref.read` (FN-16)
- Left: the traversal forms `elements`, `keys`, `values` and `items`

transpiler: planned

## CTRL-9

parser: done

- The first `{` outside parentheses, brackets and configuration opens the body of an `if`, `for`, `switch`, `when` or `while`

## CTRL-10

analyser: partial

- In a value function: a block's value; a `bool` condition; the type of an `if` whose value is used, with an `else` and without
- Left: temporal bodies

transpiler: planned

## CTRL-11

analyser: partial

- In a value function: what is assigned after an `if`, a `switch` and a loop; a `var` written `T?` is null from its declaration
- Left: temporal bodies

## CTRL-12

analyser: done

- In a value function: `for` over a list, a set or a dictionary; `while` with its condition (`while.condition`); a `switch`'s selector, and each label a constant of its type (CTRL-6)

transpiler: planned

## CTRL-13

analyser: done

- In a graph the condition of an `if` is a `bool`, a value or a connection; the `if` has its branches' one type, and is a connection when its condition or a branch is one; with no `else` it is a `T` connection under a temporal condition, and a `T?` or a `T` connection under a wiring-time one by what its branch is

transpiler: planned

## CTRL-14

analyser: done

- A `var` of a graph keeps the category of its first value, and one that holds an ordinary value is not assigned in a branch selected at runtime (`binding.category`); a `return` with a value in such a branch needs every path to return (`return.missing`)
- The same holds in the cases of a `switch` selected at runtime

transpiler: planned

## CTRL-15

analyser: done

- In a graph: a `switch` over an ordinary value or over a connection, its labels compile-time constants (`connection.as_constant`, local `switch.label_constant`); what a `for` binds by what it loops over, and a loop that makes one child for each member carrying nothing (`loop.carried`); a `while` with an ordinary condition (`connection.as_constant`, `while.condition`)

transpiler: planned

## SOURCE-1

analyser: done

- A source's shape: no temporal parameter (`source.input`), no `when` (`source.when`), no `var`, and no `inject` of `out`, `scheduler` or `alarm` (`source.statement`)
- One with a declaration or a hook has its declarations, `start`, one block that holds every `yield` (`{ }`, `while` or `for`), and `stop`, in that order and nothing else: a statement outside the block, a second block, a hook out of order and a `yield` in a hook are `source.statement`
- Its declarations and hooks stand in its body itself and each hook once, as a node's do (TIME-5)
- Its body is read: its declarations and hooks as a node's, and its other statements as value code, with each `yield`'s value held to the result's type
- It injects neither `scheduler` nor `alarm`, so `scheduled()` in it is `inject.no_scheduler` (TIME-9)

transpiler: planned

## SOURCE-2

transpiler: planned

## SOURCE-3

transpiler: planned

## SOURCE-4

analyser: done

- A source's `return` with a value, and a `yield` in a block or an `if` whose value is used, rejected: the local `source.return_value` and `yield.in_value`

transpiler: planned

