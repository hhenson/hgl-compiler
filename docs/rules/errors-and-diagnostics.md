# Errors and diagnostics

`errors-and-diagnostics.md`

## ERR-1

diagnostics: done

- Every diagnostic has a category and a code; a warning has its code alone, is listed apart from the source codes, and does not reject

transpiler: done

- hgraph: a capture takes an HGL error alone; any other failure a node raises, a form the target does not write or a fault of its own, is raised again beside every capture (`_core.Escaped`), with no code

## ERR-2

analyser: partial

- `ok(value)` and `err(value)` take one value by position and no generic argument (`generic.argument_count`), and both types from the complete `result<T, E>` the place expects, a nullable one included; with none it is `result.context_type`
- The payload is read as its side's type
- Left: a result expected through a generic parameter still being inferred; equality

transpiler: partial

- hgraph: `ok` and `err` are values of the provider's, equal when their tags and then their payloads are, and unhashable
- A temporal `result` publishes whole, a `_core.Result`

## ERR-3

transpiler: done

- hgraph: `is_ok` and `is_err` test which the value is, and `.value` and `.error` read its payload
- In a graph a test of a connection is the provider's lifted node, `_core.lifted_is_ok`

analyser: partial

- `is_ok` and `is_err` take one result by position and give a `bool`
- A direct guard on an immutable result binding proves its tag: in the branch it holds for, across `!` and the Boolean connectives, and after a branch that returns
- `binding.value` and `binding.error` need the matching fact (`result.required_variant`), and have the T and the E
- In a graph a test of a connection gives no fact
- Left: a fact carried through a `switch` or a loop

## ERR-4

analyser: done

- `error(message: text, code: optional_code)`: each by its name, `message` a `str` and required, `code` a `str?`; `.message` is a `str` and `.code` a `str?`
- `.context` is an `error context?`, a type of this compiler's, whose `.node` and `.phase` are a `str` and `.evaluation_time` a `datetime`, read where a null test proves it present (`nullable.required_presence`) and never assigned (`binding.immutable`)

transpiler: partial

- hgraph: an `error` is a value of the provider's with a message and a code that may be null, equal when both are
- `.context` is null for a constructed error; a captured failure's holds its node's wiring path, its phase and evaluation time, kept when it is propagated again, two equal when their parts are
- Left: a failure inside a branch of a `switch_` names the `switch_` node, the one hgraph reports

## ERR-5

analyser: partial

- `value?` has the T of a `result<T, E>`
- In a value function the enclosing result is a `result<U, E>` of the same E, and in a `when` the E is `error` (`result.propagation_type`, which what is no result, a nullable included, is too); in a graph, `start`, `stop`, a node's declarations and a source it is `result.propagation_phase`
- Left: an anonymous function

transpiler: done

- hgraph: `?` evaluates its operand once and gives a success's value
- In a value function a failure leaves it and is returned as it is; in a `when` it fails the node with the error's message and code

## ERR-6

analyser: partial

- A list is indexed by an `i64` and a dictionary by its key, for the element or the value
- `get(book, key)` is core's candidate, a `V?`
- Left: `at`, and `get` with a fallback (`lookup.default_type`, `nullable.required_presence`)

transpiler: partial

- hgraph: a dictionary's brackets fail with `value.missing_key` for a key that is not there, and core's `get` gives null
- A temporal dictionary's key selected in a graph is a child that is not valid while the key is absent (EXPR-7, TIME-1), and is no strict read
- A whole read checked when it runs, `rows[i]`, gives `err` or fails where a required part of the child is not valid, as a child's read does
- Left: `at`; `get` with a fallback

## ERR-7

analyser: partial

- `capture(callable, args...)` in a graph reads the call it names, from the same arguments less the name, as that call is read; in a value function or a hook, of what names no callable, and of a call that composes nothing it is `capture.phase`
- Bound with `let`, `.value` is the call's output and `.error` an `error`, both connections; `.value` of an outputless call is `capture.output_field`
- Left: a capture used other than through a `let` and its two connections is not judged

transpiler: partial

- hgraph: in a graph, `capture` of a graph or a source given no wiring-time value is `_hg.try_except` of it; of a node, a value function lifted over a connection, or a graph given a wiring-time value, `try_except` of a graph of the target's own that calls it and binds the values, so that no call outside the capture shares the node it covers
- `.value` is its `out` and `.error` its failures, each made an `error` with the failing node's provenance by `_core.captured_error`
- A temporal `error` and `result` publish whole
- Left: a capture of any but a graph given no wiring-time value other than as a `let`'s value; one in a test outside an eval; a call with a pack; a lifted operator or native function

## ERR-8

transpiler: done

- hgraph: `try_except` catches an evaluation failure once, at the nearest boundary, and the child is evaluated again on its next input; start and stop failures escape, those of a `switch_` branch the evaluation starts or stops too, raised again by `_core.captured` through every capture
- Each capture sees its own call's failures, two of one call each
- A failed keyed child stays live for its key, and a capture in a keyed child is that child's
- A retained error propagated by `?` keeps its first provenance
- A node's write to `out` and a source's publication made before the failure in its pass stand

## ERR-9

analyser: partial

- A type that is not known is reported once, and nothing is reported of its uses
- A module constant whose evaluation fails is rejected where it is declared (local `constant.failure`, UNIT-13), and nothing is reported of its uses
- Left: other required constant failures, a generic value argument's or a default's written in place, which is `generic.argument_constant`

optimiser: planned

- Folding keeps the failure phase of an executed operation

## ERR-10

parser: done

- `@suppress_warning("...")` before a named function

analyser: partial

- `@suppress_warning("node.no_activation")` before a function acknowledges its dormant-node warning
- Left: a string that is no warning's code; the warning `statement.no_effect`

## ERR-11

transpiler: done

- hgraph: NaN, an infinity and `CmpResult.unordered` publish as any value does, from a node and from a lifted value function, and a capture sees no failure; under `.fail` a zero divisor is a failure a capture sees

## ERR-12

analyser: done

- In a value function, a hook and a graph a direct test against null refines an immutable nullable parameter, `let` or loop binding to its payload's type: in the branch the test holds for, across `!` and the Boolean connectives, and after a branch that returns
- A nullable where its payload is required, as an operand no candidate takes, or read for a member or an element of its payload, is `nullable.required_presence`

## ERR-13

parser: done

- The result forms are calls of a name, and `?` a postfix suffix: `ok(value)`, `is_err(value)`, `capture(callable, args)`, `value?`

analyser: done

- `ok`, `err`, `is_ok`, `is_err` and `capture` as the callee of a call are the forms, looked up nowhere; no declaration, import alias, parameter or local takes one of the five names (`name.duplicate`)

## ERR-14

analyser: done

- A child read checked when it runs (`CheckedChild`) where the place expects `result<T, error>`, T the child's type, is of that result (EXPR-5); elsewhere it is the T

transpiler: partial

- hgraph: `ok` of the child's value, or `err` of an `error` coded `value.unset_read` where the child is not valid (`_core.checked_child_result`); elsewhere the read fails with that code.; a dictionary's entry whose key is absent is a child that is not valid (TIME-1), and is read as one
- Left, reported: a window child's value where a `result` is expected: ERR-14 does not say what a window that is valid and not all valid gives (TYPE-11)
