# Native boundaries

`native-boundaries.md`

## NATIVE-1

parser: partial

- Reads `export` before a native function's declaration. An export with `impl` or before a contract body is rejected. Native functions and candidates read the same configuration parameter lists as HGL declarations (FN-18).

analyser: partial

- A `native impl fn` or `native impl const fn` is a candidate of the operator it names, with its role
- A native function whose declaration is exported is public
- Native calls are checked against the declared parameter and result types (NATIVE-7).

## NATIVE-2

analyser: partial

- An implementation completes one declaration of its name and role; duplicates rejected
- A native implementation's known parameter and result types, parameter forms and order, `var` and `ref` marks, and generic kinds match one declaration of its name and role. For a native candidate, other candidates of that operator are alternatives.
- A finite membership of a generic type parameter must be present on both the declaration and its implementation part and admit the same types. The parameter matches by position and membership order is irrelevant (GEN-3, GEN-8).
- Left: general equivalence of written constraints, unresolved type arguments, reflected pack types, and provider selection (#517).

## NATIVE-3

parser: done

- Contract body forms: none, `{}`, hook markers

analyser: done

- `start` and `stop` need `when` and are unique; a const implementation has no hook
- Native temporal function and candidate implementations retain graph or node phase
  in `Module::phases`, selected by their contract's `when;` marker. Declarations
  without a body and native const implementations have no temporal phase.
- Native contracts keep their own hook/inject validation rather than HGL node-body checks.

## NATIVE-4

driver: planned

- Target selection is explicit

transpiler: partial

- Selects the bundled C++ runtime bindings by core identity and signature, checking for
  a provider before emitting a native call.
- Left: Selection and matching of user native implementation parts, including native
  temporal hooks, remains pending.

## NATIVE-5

analyser: partial

- A native value function's contract injects nothing (`node.in_value_function`, with FN-9)
- Fixed and packed endpoint parameters borrow hook inputs. An ordinary parameter receives a payload and follows the same read checks as ordinary HGL code (FN-8, TIME-9).

## NATIVE-6

parser: done

- A native type's body of properties, one to a line, each a name and a type

analyser: partial

- A native type's properties are read: each an ordinary value type, no two of one name (`field.duplicate`); `value.property` has the property's type, and a name the type does not declare is `member.unknown`
- Assigning a native property is `binding.immutable`, including through a struct field or list index. A call of a native type's name is `call.not_callable`.
- Left: provider identity and capabilities as a payload or key

## NATIVE-7

analyser: done

- A native declaration is checked from what it declares, with no provider: its role, its signature, a native type's properties, an operator's native candidates
- One with no implementation part keeps its role

transpiler: partial

- Emits a core native call only when the provider advertises the binding. The transpiler
  reports a missing binding and excludes code that reaches it.
- Left: User-supplied native functions, candidates and types, non-HGL temporal bodies,
  and native arguments with no provider representation are reported. Runtime providers
  for further core operations are tracked under those operations.
