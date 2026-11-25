# The C++ target

**Status: built, in part.** The compiler's target writes C++ against
[hgraph](https://github.com/hhenson/hgraph)'s graph, node, type and runtime
layers. A case it does not run yet is listed with its reason
(`crates/hgl_cpp/ci/unsupported_cases.toml`). A runtime in
Rust ([hgl-runtime](https://github.com/hhenson/hgl-runtime)) follows later,
with this target as its reference for behaviour and for speed.

## Decisions taken

| Decision | By | Consequence |
| --- | --- | --- |
| Transcribe the target to C++ on hgraph's own C++ API | Owner | One unknown at a time: statically typed emission on the engine that works, a new engine afterwards |
| Nothing from either engine's standard library | Owner | Operators are HGL's: core's HGL sources compiled, and core's 155 native declarations written in C++ here. Nothing under hgraph's `include/hgraph/lib/` is used, its test helpers included |
| The integration point is hgraph's graph, node, type and runtime layers, with its structural nodes (`map_`, `switch_`, `reduce`, nested graphs, capture) | Owner | Generated code needs `libhgraph_runtime` and `libhgraph_wiring` only |
| What hgraph lacks is added to hgraph, from a specified issue | Owner | No adaptation layer is designed. Where a behaviour differs from the specification, the difference is raised on hgraph, and a workaround here is temporary and names its issue |
| GCC 15 or later on Linux, with no `manylinux` build; macOS too, and Windows later (2026-11-20) | Owner | C++23 and each platform's standard library are assumed: Linux builds with GCC 15 or later and libstdc++, macOS with Apple clang 21 or later and the system's libc++. What one of them lacks is written once in `libhglrt` and used on every platform, not chosen by platform: Apple's libc++ has no `<generator>`, so a source's body is an `hglrt::generator` |

A trial transcription of four programs (scalar, keyed children, a struct with
generic functions and a failure, a stateful `switch`) gave the costs below.

The [release bundles](releases.md) provide hgraph's SDK and the native
dependencies. C++ builds use Apple clang 21 or later on macOS and the
system GCC 15 on Ubuntu 26.04, with CMake and Ninja on `PATH`.

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
| `hgl_lower` | `crates/hgl_lower` | What a target decides from the language's rules alone: the copy rule, instance collection and canonical names, what blocks a piece, eval wiring, the child-graph analyses, HGL type facts. The analyser records what it works out on the analysed program: whether each expression is a connection (`hir::Expr::category`), which calls are lifted (the call's `lifted`), what is a source (`Module::is_source`) |
| `hgl_cpp` | `crates/hgl_cpp` | C++ for types, value functions, nodes, graphs, tests and applications. No semantic decision that `hgl_lower` or the analyser can make |
| `libhglrt` | `runtime/cpp` | Core's native declarations; checked reads and failures; sources and wakes; the test and application host (traces, comparison, outcomes) |
| Build driver | `crates/hgl_cli` | Finds the SDK, writes the build, compiles what changed, links, runs. A failed build never runs an older executable |

## What is written

| HGL | C++ |
| --- | --- |
| Module | A declaration header, a types header where needed, and a translation unit for its definitions and generic instances. A mixed application and test build may add a `test-mod-<module>.cpp` unit for its test definitions |
| Value function, each concrete instance | A function. `var` parameters are references; a `let` or a parameter is a `const` reference; a copy is made where the copy rule says and nowhere else |
| Scalars | hgraph's `Int`, `Float`, `Bool`, `Str`, `Bytes`, `Date`, `Time`, `DateTime`, `TimeDelta`, `ZonedDateTime` and `Tenor`. Tenor literals keep their calendar or business-day form |
| Struct | A generated struct with HGL's equality. As a temporal value, a nominal bundle whose whole value is the struct |
| Enum, tuple, optional, list, fixed list, result | `enum class`, `std::tuple`, `std::optional`, `std::vector`, `std::array`, `std::expected` |
| Set, dictionary | `hglrt::set`, `hglrt::dict`: hash containers, as no order of iteration is promised (CTRL-12), equal whatever the order (OPS-2). A key is told apart by `==` of its type. As a temporal value, hgraph's `TSS` and `TSD`, published through hgraph's own whole write and delta write, with TIME-7's ticks and hgraph's ordering |
| Abstract struct | A class that holds any struct that inherits from it, read through a function of each field's name |
| `delta<T>` | As a value: `hglrt::nullable_delta`, a type of its own and no optional, `hglrt::list_delta`, `set_delta`, `dict_delta`, a struct's own delta struct, a tuple. Converting to and from hgraph's delta value is each temporal form's: a struct's and a tuple's are read from and published to hgraph's bundle field by field (`hglrt/structs.h`), a set's and a dictionary's are in `hglrt/keyed.h`, a list's in `hglrt/list_endpoints.h` |
| Temporal struct, tuple, nullable | A struct is hgraph's `NativeTSB<S>` (an `hgraph::native_bundle<S>` written beside the aggregate), or its `NominalTSB` of each field's own form where a field is optional; a tuple an `UnNamedTSB` of its positions; `atomic<S>` a `TS<S>`; a nullable `TS<Optional<T>>`. A field is a child endpoint, in a hook and in a graph (EXPR-7) |
| Temporal list, fixed list | hgraph's `TSL<S>` and `TSL<S, N>`, built on as they are |
| Node | A struct with `eval`, `start` and `stop`, in its module's unit, and a wiring function, in its header, that wires it: other units call the wiring function and instantiate none of hgraph's templates of the node. A handler header is the input's declared activity and validity; handlers are tested in source order. State and cache are one generated struct for each node |
| Graph | A struct with static `compose` over `Wiring&` and typed ports, calling the wiring functions of what it wires, and a wiring function as a node has. hgraph interns identical nodes and compares native Float zero signs. Nodes with opaque configurations that can hold a float use separate wiring (NUM-2) |
| `if`, `switch`, `for` over connections | hgraph's `compiled::switch_` and `compiled::map_` wiring of compiled children (`types/compiled_subgraph.h`), each child a compiled sub-graph with its captures |
| Lifted call | A generated node that calls the instance |
| Source | A node with no input that schedules itself. The body is a C++23 coroutine, an `hglrt::generator` of a time and a payload (`<hglrt/generator.h>`: `std::generator`'s shape, written once for every platform because Apple clang 21's standard library has no `std::generator`), held in the node's state, made in `start` and destroyed in `stop`: `yield` inside loops and branches is written as it reads, with the body's locals in the coroutine's frame. A trial ran one on hgraph: the trace was right, a failure inside the body was the node's failure, and the frame was freed after a finished and after a failed run |
| `ref` parameter and result | hgraph's `REF` input and output. Returning a whole `ref` parameter publishes the reference that input holds when the `return` runs, and no more: a `REF` output keeps what it was given, which is the specification's rule since hhenson/hgl_spec#147. Returning an ordinary input or a child of one designates that endpoint, which is followed through a re-binding: the node is active on that binding, publishes the new designation there and runs no handler for it (FN-16). A designation the result holds already is not published again (TIME-10) |
| Failure | Thrown as hgraph's `ExecutionFailure` carrying `CanonicalError` (code, message, node, phase, time). `capture` is hgraph's `compiled::try_except_` of a compiled child, with canonical errors, a node of its own. `assert raises` finds the failure in the exception it catches |
| Test | Core's `replay` and `record` are compiled like any module. The compiler writes each eval's entry graph, runs the executor in simulation and reads the recording from the run's store |
| Application | The same, in an executable of its own whose host is `libhglrt`'s, in simulation or in real time; a run in real time its host asks to stop is stopped through the engine's control |

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

A warm build, test or run starts the selected C++ compiler once with
`-dM -E` to confirm its family and version. It then uses the prepared
toolchain and verifies the stamped artifacts by content. The HGL front end
and Ninja are skipped when that check succeeds.

A reused `hgl build --out DIR` directory may retain generated C++ sources,
headers, objects and executables from earlier builds. This includes test
artifacts after a plain build and files for modules removed from the sources.
The `build.ninja` manifest and `hgl build --format json` inventory identify
the units and executables in the current build.

Reuse assumes that the SDK and dependency prefixes are fixed installations.
The key includes the prefixes and hashes the SDK version file and its runtime
and wiring libraries. Use a fresh native cache and workspace after updating
headers or dependencies in place or when checking the output of a different
linker version. An existing verified executable can run without Ninja or the
linker.

Measured at `-O0` with GCC 15 on the SDK built without Arrow and
`libhglrt` on hgraph's front end for generated code (in brackets: on the
SDK with Arrow and `graph_wiring.h`, as before):

| Step | Time |
| --- | --- |
| The precompiled header of `<hglrt/hglrt.h>` | 5.0 s, 254 MB (6.0 s, 300 MB) |
| A unit of nodes, graphs and an eval, against it | 1.55 s (1.75 s) |
| The same unit with no precompiled header | 2.8 s (3.2 s) |
| A module's unit of value functions and a test, against it; with none | 0.57 s (0.81 s); 1.48 s (2.27 s) |
| A test executable's entry unit: `hglrt.h` against it; `host.h` alone, with none | 0.57 s (0.81 s); 0.06 s |
| Edit to result, `hgl test --target cpp` of a small module | 0.65 s (0.95 s) |
| Link with `lld` (GNU `ld`: 0.12 s), on the trial | 0.015 s |
| Whole suite (about 135 modules), 12 jobs, estimated from the trial | 40 to 55 s |
| Whole suite on a four-core CI runner, estimated | about 100 s |

The driver compiles production module units in an ordinary build and adds
test units and entries when requested. It builds `libhglrt` and core's
instances once for an SDK and precompiles the fixed include set. A test
executable runs the cases requested for its module. `ninja` rebuilds changed
inputs, and `lld` links on Linux. A rejection case never
builds. A unit with no precompiled header still takes about twice as long
as one against it, so the header stays.

On a macOS machine (Apple Silicon, Apple clang 21), with hgraph 0.9.13's
SDK and the driver's own commands, beside the same on Linux (GCC 15):

| Step | macOS | Linux |
| --- | --- | --- |
| The precompiled header of `<hglrt/hglrt.h>` | 1.6 s, 54 MB | 5.5 s, 265 MB |
| A module's unit of value functions and two tests, against it; with none | 0.26 s; 1.38 s | 0.60 s; 1.70 s |
| The entry unit, `host.h` alone | 0.11 s | 0.07 s |
| The link (Apple's `ld`; GNU `ld`) | 0.07 s | 0.04 s |
| Edit to result, `hgl test --target cpp` of that module | 0.55 s | 0.70 s |

The compiled capture boundary reduces the cost of including
`<hglrt/hglrt.h>`. Both versions used the hgraph 0.9.23 SDK without Arrow
and differed only in `capture.h`. GCC 15.2 compiled a file containing just
that include at `-std=c++23 -O0` on one x86-64 Linux core. The machine
averaged 93% idle across all threads and the core's sibling thread
averaged 98% idle. Times are medians of five interleaved runs after a
warm-up.

| Step | Old capture boundary | Compiled capture boundary |
| --- | --- | --- |
| Compile without a precompiled header | 3.84 s | 2.21 s |
| Build the precompiled header | 7.23 s, 320.8 MiB | 6.44 s, 278.9 MiB |

## Debugging tests

`hgl test ROOTS --target cpp --debug-build DIR` builds the selected test
modules with debug information and no optimisation. It starts no tests.
Stdout contains the JSON description or is empty. Diagnostics go to
stderr. Use `--test`, repeatable
`--test-path`, `--integration` or `--integration-only` as for an ordinary
test run. Omitting selectors selects the ordinary cases. A bad path, an
unknown test name or conflicting integration flags exits 2 before any build.

The debugger sets breakpoints in the original HGL files, including on the
statements of ordinary and integration tests. Step Over between test
statements stays in HGL. Value functions, their generic instances, and
graph composition open at their HGL declarations. Function and handler
ends map to their HGL closing lines. Step Into on an `eval` can stop at
the Core declaration that its replay or record wiring belongs to. The
debug workspace stores the bundled Core files named by its line tables
under `debug-sources/`.
Breakpoints on declaration lines can bind to
nearby executable lines, including a helper. Set a breakpoint on a body
statement to stop there. The same sources and selection in `DIR` reuse
the same launches without compiling again. A new
selection builds only missing modules and their dependencies. Debug library
selection matches application debug builds.

The description has `version: 1`, `launches`, and `skipped`. Both arrays
can be empty. Each launch contains the fully qualified `module` and its
fully qualified `tests` in execution order. The modules appear in name
order. Pass `arguments` to `executable` as supplied, in `workingDirectory`
with the given `environment`. Those arguments select exactly the listed
tests. `resultsFile` identifies the file the launch writes under `DIR`.
All paths are absolute. `hgraph` reports the linked libraries as in an
application launch. Each skipped entry gives a `name` and `reason` for a
selected rejection case or a test the target cannot write.

A failed selected rejection has `status: "failed"` in its skipped entry
and gives the ordinary rejection details in `reason`. The command exits 1
and still prints the description with any launches it prepared. An error
in the surviving source exits 1 with empty stdout. A native build failure,
a missing C++ compiler, an unwritable build directory or an interrupted
build exits 2 with empty stdout. A failed build supplies no partial launches.

The test executable's result file is text. It starts fresh on each
launch and flushes one record per line. `N` is the zero-based index in the
launch's `tests` array.

| Record | Meaning |
| --- | --- |
| `start N` | The host started processing that test. |
| `passed N` | The test ran to completion. |
| `failed N LINE TEXT` | The test failed. `LINE` is the HGL line of the test statement, or 0 if unavailable. |
| `unloaded N TEXT` | The executable could not find the requested test. |

`TEXT` escapes backslash, newline and carriage return as `\\`, `\n` and
`\r`. Exit 0 means the host finished writing its records. Test failures
appear in those records and the host still runs later tests in the module.
Exit 2 means usage or result-file failure. Debugger launches do not apply
integration tests' host-clock limits. Ordinary runs keep their TST-17
limits. If a launch stops early, an index without an outcome did not
finish. Remove any previous results file before
starting an executable if the debugger needs to distinguish a launch
failure from old results.

## What hgraph is asked for

The SDK is hgraph 0.9.32. It compares native Float zero signs during
interning ([hgraph 1866](https://github.com/hhenson/hgraph/issues/1866)).
HGL tuples and lists can still hide their floats from that comparison,
so their nodes use unique wiring.

| Issue | In hgraph | Here |
| --- | --- | --- |
| [1833](https://github.com/hhenson/hgraph/issues/1833) | `switch_` and `map_` wiring of compiled children with no operator registration (`types/subgraph_wiring.h`) | The control-flow work wires with them; `libhglrt` carries no glue |
| [1822](https://github.com/hhenson/hgraph/issues/1822), [1827](https://github.com/hhenson/hgraph/issues/1827) | The growing list's first empty publication and its validity, a shrink under a map or an index, invalidation emptying a collection, and tuple deltas | The temporal list and tuple are built on hgraph as it is: core's list and tuple cases need no adaptation |
| [1848](https://github.com/hhenson/hgraph/issues/1848) | A generator that yields a time already past skips it and goes on | That is hgraph's Python generator; a C++ node's `NodeScheduler` drops a wake at a past time, so the source driver of `libhglrt` keeps its own skip (SOURCE-3) |
| [1834](https://github.com/hhenson/hgraph/issues/1834) | A C++ aggregate as a nominal bundle's whole value | A struct is written and read whole |
| [1838](https://github.com/hhenson/hgraph/issues/1838) | A lighter front end for generated code (`types/generated.h`) | `libhglrt`'s headers, and so generated units, include it; the precompiled header still pays |
| [1839](https://github.com/hhenson/hgraph/issues/1839) | A runtime and wiring library without Arrow (`HGRAPH_WITH_ARROW=OFF`) | The SDK is built so: 14 MB where it was 54, its prefix fmt, spdlog and simdjson; a test of `libhglrt` fails where Arrow or hgraph's standard library is included or linked |
| [1837](https://github.com/hhenson/hgraph/issues/1837), [1835](https://github.com/hhenson/hgraph/issues/1835), [1836](https://github.com/hhenson/hgraph/issues/1836) | `add_node` keeps a builder's own scalars; `try_except_<G>` takes options for canonical errors; a `tenor` scalar | In use |
| [1871](https://github.com/hhenson/hgraph/issues/1871) | `compiled::try_except_` in the front end for generated code | `capture` wires it; no header of `libhglrt` takes `graph_wiring.h`, which a test checks |
| [1874](https://github.com/hhenson/hgraph/issues/1874) | A `std::optional<U>` member of a mapped aggregate for an `Optional<U>` field | An atomic struct that holds an optional field is written |
| [1880](https://github.com/hhenson/hgraph/issues/1880) | `CanonicalError` hashes, so a node may be configured with one | An `error` configuration is held as `hglrt::configured<error>`, as any value hgraph has no scalar of its own for |
| [1881](https://github.com/hhenson/hgraph/issues/1881) | A `REF<TS<Optional<T>>>` input bound to a `TS<T>` output through the widening | A connection of T given to a `ref` parameter of `T?` is written |
| [1884](https://github.com/hhenson/hgraph/issues/1884) | In the evaluation a wake brings about, that wake is no longer pending | `libhglrt`'s scheduler is hgraph's, with no adaptation (CAP-4) |
| [1885](https://github.com/hhenson/hgraph/issues/1885) | A switch's output is withdrawn when the selected branch has published nothing | Nothing here adapted it |
| [1911](https://github.com/hhenson/hgraph/issues/1911) | A switch's window history starts fresh when its selected branch changes | Temporal `if` and `switch` use the SDK's window result directly (CTRL-2) |

Nothing is asked on references: a `REF` output holds the reference it
was given, which is the specification's rule for a returned `ref`
parameter. Where the pinned specification and hgraph differ, this target
builds the specification's rule, and the difference goes to the
specification's issue tracker to be decided there, not here.

Under CAP-4, the scheduler removes a wake from pending wakes when it
starts its evaluation. `un_schedule()` cancels the next one, and
`is_scheduled_now` holds after `reset`. hgraph keeps no wake under the
empty tag, so a tag is given to it after a mark of its own. The
wall-clock option is hgraph's own, as CAP-4 has it since the owner's
ruling: in real time a wake of it already due comes in the next cycle, and
in simulation the option changes nothing. The alarm keeps its one wake
under a tag of its own in the same `NodeScheduler`, the earliest asked
for (CAP-5: "asking again keeps the earliest"): hgraph's
`SingleShotScheduler` only marks the node, and an evaluation at an
input's tick before the wake takes the mark away, where the node's
scheduler arms the node again.

## Progress and acceptance

The measure is cases passing on this target: core's runtime cases (182 today)
and the tests of the fixture programs (about 370 in about 100 programs), all
of which are HGL with their own expectations and need no change. A case this
target cannot yet write is listed with its reason in
`crates/hgl_cpp/ci/unsupported_cases.toml`. A passing case still on the list
fails the `hgl_build_tool check-core` command. A case's own expectation is the
judge.

## Order of work

| Step | Work | Done when |
| --- | --- | --- |
| 0 | `hgl_lower`; `libhglrt` skeleton, SDK provisioning and a CI job; `hgl_cpp` skeleton, `--target cpp` and the build driver | Every case is reported as not yet written on the new target; CI builds the SDK from a cache and runs the empty suite |
| 1 | Scalars end to end: value functions, constants, enums, scalar natives, nodes, graphs, tests with core's `replay` and `record` | Core's scalar, handler, activity, graph and lifecycle cases pass |
| 2 | In parallel lanes: structs, tuples and optionals; sets, dictionaries, lists and deltas; the remaining natives; lifting and capture; `if`, `switch` and `for`; sources; the scheduler and alarm; references; windows; the store, applications and real time | Each lane's cases pass |
| 3 | Parity | Every case passes |

## Not decided here

- Whether instances that two modules both need can be built once across
  programs: after step 1, from measured build times.
