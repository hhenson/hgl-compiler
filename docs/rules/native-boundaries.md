# Native boundaries

`native-boundaries.md`

## NATIVE-1

parser: partial

- Reads `export` before a native function's declaration. An export with `impl` or before a contract body is rejected. Native configuration parameters await #363.

analyser: partial

- A `native impl fn` or `native impl const fn` is a candidate of the operator it names, with its role
- A native function whose declaration is exported is public
- Left: a native declaration checked as an HGL callable

## NATIVE-2

analyser: partial

- An implementation completes one declaration of its name and role; duplicates rejected
- A native candidate's implementation needs a candidate of its name and role
- Left: the exact match of signatures, which says which candidate an implementation completes, their `var` and `ref` marks among them (FN-15, FN-16); selection

## NATIVE-3

parser: done

- Contract body forms: none, `{}`, hook markers

analyser: partial

- `start` and `stop` need `when` and are unique; a const implementation has no hook
- Left: graph or node by body, where a later pass reads it

## NATIVE-4

driver: planned

- Target selection is explicit

transpiler: planned

- The provider meets the declared contract

## NATIVE-5

analyser: partial

- A native value function's contract injects nothing (`node.in_value_function`, with FN-9)
- Left: borrowed access stated in the contract; a scalar parameter gains no endpoint privilege

## NATIVE-6

parser: done

- A native type's body of properties, one to a line, each a name and a type

analyser: partial

- A native type's properties are read: each an ordinary value type, no two of one name (`field.duplicate`); `value.property` has the property's type, and a name the type does not declare is `member.unknown`
- `name.property = value` on a local of a native type, a capability among them, is `binding.immutable`, and a call of a native type's name `call.not_callable`
- Left: a property assigned through a longer place, which is not read yet; identity and capabilities as a payload or key

## NATIVE-7

analyser: done

- A native declaration is checked from what it declares, with no provider: its role, its signature, a native type's properties, an operator's native candidates
- One with no implementation part keeps its role

transpiler: planned

- Execution needs a compatible provider; an absent binding fails
