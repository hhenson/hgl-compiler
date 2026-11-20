# The C++ target

**Status: proposed design.** The compiler's native target writes C++ against
[hgraph](https://github.com/hhenson/hgraph)'s graph, node, type and runtime
layers. It replaces the Python target, which gets no further work: it
stays as it is until this target runs the cases, and is then deleted. A
case of a rule it does not follow is listed for it
(`crates/hgl_hgraph/ci/unsupported_cases.toml`). A runtime in
Rust ([hgl-runtime](https://github.com/hhenson/hgl-runtime)) follows later,
with this target as its reference for behaviour and for speed.

## Decisions taken

| Decision | By | Consequence |
| --- | --- | --- |
| Transcribe the target to C++ on hgraph's own C++ API | Owner | One unknown at a time: statically typed emission on the engine that works, a new engine afterwards |
| Nothing from either engine's standard library | Owner | Operators are HGL's: core's HGL sources compiled, and core's 155 native declarations written in C++ here. Nothing under hgraph's `include/hgraph/lib/` is used, its test helpers included |
| The integration point is hgraph's graph, node, type and runtime layers, with its structural nodes (`map_`, `switch_`, `reduce`, nested graphs, capture) | Owner | Generated code needs `libhgraph_runtime` and `libhgraph_wiring` only |
| What hgraph lacks is added to hgraph, from a specified issue | Owner | No adaptation layer is designed. Where a behaviour differs from the specification, the difference is raised on hgraph, and a workaround here is temporary and names its issue |
| GCC 15 or later on Linux; no `manylinux` build | Owner | C++23 and its standard library are assumed |

A trial transcription of four programs (scalar, keyed children, a struct with
generic functions and a failure, a stateful `switch`) matched the Python
target's traces exactly, and gave the costs below.

## Pipeline

```text
analysed program
    |  hgl_lower   (shared facts: copies, instances, categories, child graphs)
lowered facts
    |  hgl_cpp     (one C++ unit for each HGL module)
C++ sources
    |  build driver in hgl_cli (precompiled header, ninja, a fast linker)
one executable for each test module or application
    |  links libhglrt (this repository's C++ runtime library) and hgraph
```

| Part | Where | What it owns |
| --- | --- | --- |
| `hgl_lower` | `crates/hgl_lower` | What both targets decide the same way, moved out of `hgl_hgraph` unchanged: the copy rule, instance collection and canonical names, what blocks a piece, eval wiring, the child-graph analyses, HGL type facts. The analyser records what it works out on the analysed program: whether each expression is a connection (`hir::Expr::category`), which calls are lifted (the call's `lifted`), what is a source (`Module::is_source`) |
| `hgl_cpp` | `crates/hgl_cpp` | C++ for types, value functions, nodes, graphs, tests and applications. No semantic decision that `hgl_lower` or the analyser can make |
| `libhglrt` | `runtime/cpp` | Core's native declarations; checked reads and failures; sources and wakes; the test and application host (traces, comparison, outcomes); glue for `switch` and `map` until hgraph has it |
| Build driver | `crates/hgl_cli` | Finds the SDK, writes the build, compiles what changed, links, runs. A failed build never runs an older executable |

## What is written

| HGL | C++ |
| --- | --- |
| Module | One translation unit and one header; its generic instances with it, collected for the whole program before anything is written |
| Value function, each concrete instance | A function. `var` parameters are references; a `let` or a parameter is a `const` reference; a copy is made where the copy rule says and nowhere else |
| Scalars | hgraph's `Int`, `Float`, `Bool`, `Str`, `Bytes`, `Date`, `Time`, `DateTime`, `TimeDelta`, `ZonedDateTime`. `tenor`: a type of this library until hgraph has one |
| Struct | A generated struct with HGL's equality. As a temporal value, a bundle of its fields, written and read field by field until hgraph takes the struct whole |
| Enum, tuple, optional, list, fixed list, result | `enum class`, `std::tuple`, `std::optional`, `std::vector`, `std::array`, `std::expected` |
| Set, dictionary, abstract struct, `delta<T>` | To be designed with the collections work: the containers must give the order and equality the specification states, and a delta converts to and from hgraph's own delta value |
| Node | A struct with `eval`, `start` and `stop`. A handler header is the input's declared activity and validity; handlers are tested in source order. State and cache are one generated struct for each node |
| Graph | A function over `Wiring&` and typed ports, calling `wire<Node>`. Identical value nodes are interned by hgraph |
| `if`, `switch`, `for` over connections | hgraph's `switch_node`, `map_node` and `tsl_map_node`, each child a compiled sub-graph with its captures |
| Lifted call | A generated node that calls the instance |
| Source | A node with no input that schedules itself. The body is a C++23 coroutine (`std::generator` of a time and a payload) held in the node's state, made in `start` and destroyed in `stop`: `yield` inside loops and branches is written as it reads, with the body's locals in the coroutine's frame. A trial ran one on hgraph: the trace was right, a failure inside the body was the node's failure, and the frame was freed after a finished and after a failed run |
| `ref` parameter and result | hgraph's `REF` input and output. Returning a whole `ref` parameter publishes the reference that input holds when the `return` runs, and no more: a `REF` output keeps what it was given, which is the specification's rule since hhenson/hgl_spec#147. Returning an ordinary input or a child of one designates that endpoint, which is followed through a re-binding: the node is active on that binding, publishes the new designation there and runs no handler for it (FN-16). A designation the result holds already is not published again (TIME-10) |
| Failure | Thrown as hgraph's `ExecutionFailure` carrying `CanonicalError` (code, message, node, phase, time). `capture` is `try_except_node` with canonical errors. `assert raises` finds the failure in the exception it catches |
| Test | Core's `replay` and `record` are compiled like any module. The compiler writes each eval's entry graph, runs the executor in simulation and reads the recording from the run's store |
| Application | The same, in real time, stopped through the engine's control |

## Safety of generated C++

C++ does not check what Rust would. Three rules stand in for it.

- Generated code holds no view of an input or an output beyond the hook call
  it was given in; what is kept is copied. A source's coroutine takes its
  configuration by value, and reaches the clock, the scheduler and the store
  through a slot the node fills before each resumption.
- An internal fault is no HGL failure (ERR-1): only `ExecutionFailure` may be
  captured. What `try_except_node` does with any other exception is checked
  before `capture` is written, and raised on hgraph if it captures it.
- CI runs the whole suite a second time under the address and
  undefined-behaviour sanitizers.

## Building and turnaround

Measured on the trial, at `-O0`:

| Step | Time |
| --- | --- |
| One unit against a precompiled header of hgraph's includes | 1.5 to 2.0 s, and about 0.1 s for each node |
| Link with `lld` (GNU `ld`: 0.12 s) | 0.015 s |
| Edit to result | 1.8 s |
| Whole suite (about 135 modules), 12 jobs, estimated from the trial | 40 to 55 s (the Python target: about a minute) |
| Whole suite on a four-core CI runner, estimated | about 100 s |

The driver keeps these by: one unit for each module; `libhglrt` and core's
instances built once for an SDK; a precompiled header of the fixed include
set; one executable for each test module with its cases chosen when it
runs; `ninja` for what changed; `lld`. A rejection case never builds.
A lighter front end in hgraph for generated code is the cheapest further
saving ([hgraph 1838](https://github.com/hhenson/hgraph/issues/1838)).

## What hgraph is asked for

The SDK is hgraph 0.9.10.

| Issue | Asked | Until then |
| --- | --- | --- |
| [1833](https://github.com/hhenson/hgraph/issues/1833) | `switch_` and `map_` wiring outside the operator library | `libhglrt` carries the glue |
| [1822](https://github.com/hhenson/hgraph/issues/1822), [1827](https://github.com/hhenson/hgraph/issues/1827) | The growing list's first empty publication and its validity, a shrink under a map or an index, invalidation emptying a collection, and tuple deltas, as the specification has them | `libhglrt` adapts each |
| [1848](https://github.com/hhenson/hgraph/issues/1848) | A generator that yields a time already past skips it and goes on | The source driver of `libhglrt` skips it |
| [1834](https://github.com/hhenson/hgraph/issues/1834) | A C++ struct as a bundle's whole value | Field by field |
| [1838](https://github.com/hhenson/hgraph/issues/1838) | A lighter front end for generated code | A precompiled header |
| [1839](https://github.com/hhenson/hgraph/issues/1839) | A runtime library without Arrow | Arrow is installed with the SDK |

Delivered: `add_node` keeps a builder's own scalars
([1837](https://github.com/hhenson/hgraph/issues/1837)) and
`try_except_<G>` takes options for canonical errors
([1835](https://github.com/hhenson/hgraph/issues/1835)), both in 0.9.9;
a `tenor` scalar ([1836](https://github.com/hhenson/hgraph/issues/1836))
in 0.9.10. Nothing is asked
on references: a `REF` output holds the reference it was given, which is
the specification's rule for a returned `ref` parameter. Where the pinned
specification and hgraph differ, this target builds the specification's
rule, and the difference goes to the specification's issue tracker to be
decided there, not here.

## Progress and acceptance

The measure is cases passing on this target: core's runtime cases (182 today)
and the tests of the fixture programs (about 370 in about 100 programs), all
of which are HGL with their own expectations and need no change. A case this
target cannot yet write is listed with its reason, as `unsupported_cases.toml`
lists them today; a case that passes and is listed fails the check. A
case's own expectation is the judge: the Python target is not kept in
step with rules that change, and is no reference for them.

## Order of work

| Step | Work | Done when |
| --- | --- | --- |
| 0 | `hgl_lower` by moving code (the Python target's output stays byte-identical); `libhglrt` skeleton, SDK provisioning and a CI job; `hgl_cpp` skeleton, `--target cpp` and the build driver | Every case is reported as not yet written on the new target; CI builds the SDK from a cache and runs the empty suite |
| 1 | Scalars end to end: value functions, constants, enums, scalar natives, nodes, graphs, tests with core's `replay` and `record` | Core's scalar, handler, activity, graph and lifecycle cases pass |
| 2 | In parallel lanes: structs, tuples and optionals; sets, dictionaries, lists and deltas; the remaining natives; lifting and capture; `if`, `switch` and `for`; sources; the scheduler and alarm; references; windows; the store, applications and real time | Each lane's cases pass |
| 3 | Parity | Every case passes; the Python target, its provider and its CI job are deleted, and `hgl_hgraph` with them |

## Not decided here

- Containers for a set and a dictionary, an abstract struct's value, and a
  delta's C++ form: with step 2's collections lane.
- Whether instances that two modules both need can be built once across
  programs: after step 1, from measured build times.
