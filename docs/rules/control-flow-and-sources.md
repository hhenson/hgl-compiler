# Control flow and sources

`control-flow-and-sources.md`

## CTRL-1

analyser: done

- In a graph an `if` is told apart by its condition: an ordinary `bool` composes one branch, a connection selects a child graph at runtime; in a value function and a hook it is ordinary control flow.

## CTRL-2

transpiler: done

- hgraph: an `if` and a `switch` over a connection are `switch_`, which runs the selected branch's graph alone, keeps it while the selector is published again as it was, and stops it and starts the newly selected one afresh where it changes; what is composed outside is the enclosing graph's

## CTRL-3

analyser: done

- A temporal `if` with no `else` whose value is used is a connection of `T`, not a `T?` (with CTRL-13)

transpiler: done

- hgraph: a consumed `if` with no `else` under a temporal condition has `_hg.nothing` for its false branch; an outputless one is a `switch_` of graphs with no output, the omitted branch a graph with nothing in it

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

- A label is a compile-time constant of the selector's type (local `switch.label_constant`, UNIT-13), compared by its value: a literal, an enum member, a constant followed to its value, integers joined by `+`, `-` and `*`
- A repeated value is rejected (`switch.duplicate_label`), and an enum's coverage by such labels stands for a `default`
- Left: a generic value parameter's label, whose value only an instance has (the hgraph target reports a repeat there)

transpiler: done

- hgraph: in a value function, a node's hooks and a graph's `switch` over an ordinary value the selector is evaluated once and each case is an `if` on it, so there is no fallthrough; what no case matches runs the `default`, or fails (`_core.unmatched`)
- Over a connection, `switch_`, whose `default` where none is written is a graph that fails the same way when it is selected
- Two labels of one value in an instance are reported

## CTRL-7

analyser: done

- What a `for` in a graph receives: ordinary values over ordinary data, a connection of `T` over a connection of `list<T, N>`, and over a connection of `list<T>`, `set<T>` or `dict<K, V>` one child for each member, with the set's member and the dictionary's key ordinary values (with CTRL-15)

transpiler: partial

- hgraph: a `for` over a connection of `list<T>`, `set<T>` or `dict<K, V>` is `map_` of a graph for the body: a child for each key, kept while the key stays, stopped where it is removed, started afresh where it is added again
- A list is its positions (`convert` to a `TSD`), a set's members are the keys, and a connection from outside is given whole (`pass_through`)
- Left: a key or a member read other than as the argument of a temporal parameter (`map_` gives it as a port), and a list of nullables

## CTRL-8

analyser: partial

- In a loop that makes one child for each member, an assignment to a `var` declared outside it and a `return` are rejected (`loop.carried`)
- Left: the traversal forms `elements`, `keys`, `values` and `items`; no implicit crossing of a reference

transpiler: partial

- hgraph: in a node's hooks a `for` over a temporal input reads its current value, a list's in index order; a loop that makes a child for each member carries nothing and infers no reduction
- Left: `elements`, `keys`, `values` and `items` (with the analyser); a reference

## CTRL-9

parser: done

- The first `{` outside parentheses, brackets and configuration opens the body of an `if`, `for`, `switch`, `when` or `while`

## CTRL-10

analyser: partial

- In a value function: a block's value; a `bool` condition; the type of an `if` whose value is used, with an `else` and without
- Left: temporal bodies

transpiler: partial

- hgraph: in a value function and a node's hooks, a block or an `if` as the whole value of a `let`, a `var`, an assignment to a name or a `return`: each branch's statements, then the value given; with no `else`, null
- An `if` with no `else` in an expression is `None` where its condition fails
- Left: a block with statements inside another expression; in a graph, as CTRL-13

## CTRL-11

analyser: partial

- In a value function: what is assigned after an `if`, a `switch` and a loop; a `var` written `T?` is null from its declaration
- Left: temporal bodies

## CTRL-12

analyser: done

- In a value function: `for` over a list, a set or a dictionary; `while` with its condition (`while.condition`); a `switch`'s selector, and each label a constant of its type (CTRL-6)

transpiler: done

- hgraph: in a value function and a node's hooks, `for` over a list, a set and a dictionary, `while` with its condition, and `switch`; a label that is a generic value parameter is the instance's constant

## CTRL-13

analyser: done

- In a graph the condition of an `if` is a `bool`, a value or a connection; the `if` has its branches' one type, and is a connection when its condition or a branch is one; with no `else` it is a `T` connection under a temporal condition, and a `T?` or a `T` connection under a wiring-time one by what its branch is

transpiler: partial

- hgraph: an `if` over a connection is hgraph's `switch_`, with a graph for each branch that takes the connections the branches read; an ordinary value in a branch is `_hg.const`, and a branch with no value, and no `else`, is `_hg.nothing`
- An `if` over an ordinary value is Python's own, which composes the one branch, and an `if` over a connection in it is `switch_`
- A connection whose name `switch_` keeps for itself, `key` or `name`, is taken as `_taken_key`
- Where the `if` is a `T?`, a branch's connection of `T` is given as one (`_core.nullable`) and an ordinary value, null among them, as `_hg.const` of `T?`; so is an ordinary `T?` a graph returns
- Left: an `if` that is no whole statement and no whole value of a `let`, a `var` or a `return`

## CTRL-14

analyser: done

- A `var` of a graph keeps the category of its first value, and one that holds an ordinary value is not assigned in a branch selected at runtime (`binding.category`); a `return` with a value in such a branch needs every path to return (`return.missing`)
- The same holds in the cases of a `switch` selected at runtime

transpiler: partial

- hgraph: a branch that returns takes the rest of the block for the other branch's graph, and the graph returns what `switch_` gives
- Branches that assign one `var` each give it as they leave it, the binding it had before where they may leave it unassigned on some path, and it is assigned what `switch_` gives; an `else if` is the one statement of its `else`; a read after a branch assigns the `var` on every path is no input of its graph
- A `return` in an `if` that gives a `let` or a `var` its value: each branch's graph gives the name its value and has the rest of the block
- Left: branches that assign more than one `var`; an `if` that gives a `let` or a `var` its value and whose branch assigns a `var` declared before it; such a `return` where a branch has no value, and under a branch selected at runtime in a loop unrolled where the graph is built

## CTRL-15

analyser: done

- In a graph: a `switch` over an ordinary value or over a connection, its labels compile-time constants (`connection.as_constant`, local `switch.label_constant`); what a `for` binds by what it loops over, and a loop that makes one child for each member carrying nothing (`loop.carried`); a `while` with an ordinary condition (`connection.as_constant`, `while.condition`)

transpiler: partial

- hgraph: a `switch` over a connection is `switch_` with a graph for each case, and `_hg.DEFAULT` for its `default`; over an ordinary value, Python's `if` on the selector
- A `for` over ordinary data and over a connection of `list<T, N>`, and a `while`, are Python's loops, which compose the body on each pass; over a connection of `list<T>`, `set<T>` or `dict<K, V>` `map_` (CTRL-7)
- Left: what CTRL-7 and CTRL-14 leave

## SOURCE-1

analyser: done

- A source's shape: no temporal parameter (`source.input`), no `when` (`source.when`), no `var`, and no `inject` of `out`, `scheduler` or `alarm` (`source.statement`)
- One with a declaration or a hook has its declarations, `start`, one block that holds every `yield` (`{ }`, `while` or `for`), and `stop`, in that order and nothing else: a statement outside the block, a second block, a hook out of order and a `yield` in a hook are `source.statement`
- Its declarations and hooks stand in its body itself and each hook once, as a node's do (TIME-5)
- Its body is read: its declarations and hooks as a node's, and its other statements as value code, with each `yield`'s value held to the result's type

transpiler: partial

- hgraph: a source is a Python generator that holds its statements, a node that resumes it, and a graph that wakes the node once as it starts; a `cache` and a `state` are fields of the node's state, given their values in its `start`, which then runs the source's `start`; the source's `stop` is the node's, and runs when the node ends whatever the generator has done
- A capability is put in the node's state for the evaluation the generator is resumed in; a source injects no other than `clock`, `logger` and `global_state` (the analyser's `source.statement`)
- Left: recording a `state`

## SOURCE-2

transpiler: done

- hgraph: `yield time: value` gives the time, then the value; a duration is counted from the evaluation the source is resumed in, with checked arithmetic, and an instant is taken as it is; a negative duration is `yield.negative_duration`

## SOURCE-3

transpiler: done

- hgraph: a target not after the one before it, a skipped one among them, is `yield.non_increasing_time`; a past target is skipped, a current one published at once, and a future one keeps its payload until the node's scheduler wakes it there

## SOURCE-4

analyser: done

- A source's `return` with a value, and a `yield` in a block or an `if` whose value is used, rejected: the local `source.return_value` and `yield.in_value`

transpiler: partial

- hgraph: a bare `return` and the end of the statements finish the generator; a `while` with no condition is unbounded
- Left: restoring a source
