# Native HGraph backend

**Decision.** Rust is the emitted native language. The scalar and keyed slices run
through shared lowering on both targets. Generated Rust constructs prepared HGraph
nodes and child bindings. HGraph ranks them, constructs the graph and runs it.
The compiler can use an installed shared C bridge or build one against an SDK
containing the prepared ranking API. [Implementation and timings](../experiments/hgraph-native/hgl-turnaround.md)

The remaining sections describe the proposed wider rollout.

## Ownership and scope

| Owner | Deliverable | Boundary |
| --- | --- | --- |
| Compiler optimiser stage | Target-independent lowered program | Gives both the Python and native emitters the same HGL decisions, with source spans. |
| HGraph | Installed C++ SDK, engine services and builder factories | Owns the ranker, storage, scheduling and execution. The prepared-builder API shares the existing ranker. The proposed C bridge and safe Rust facade live with the SDK. |
| Compiler native runtime component | HGL core provider, graph assembly and runner | Collects prepared nodes, routes, dependencies and capture descriptions, submits them for ranking, and runs the returned builder. Implements and tests core adaptations independently of code generation. |
| Compiler `hgraph_native` emitter | Module libraries and application or test entry point | Emits value and node bodies, factory calls and graph composition from the lowered program. |
| Compiler target build driver | Runnable artifact and launch information | Provisions dependencies, compiles changed units, links, then stages the runtime. Run and test consume the successful build result. |

Maintain one native emitter after the comparison. A replacement engine, Python
wiring in the native path and a public binary
format for HGL modules are outside this work. The spec and core retain authority for
language behavior and conformance. [Native boundary rules](../spec/hgl_spec/spec/language/native-boundaries.md) ·
[Core execution contract](../spec/hgl_core/spec/execution.md)

### Lower once for both targets

Move the semantic decisions now spread across the Python emitter's copy analysis,
instance selection, node handlers, graph composition and publication into the planned
optimiser stage. Its lowering is required even with optimisations disabled. Record
copy/retention points, concrete declaration identities, subscriptions and handler
admission, child captures and routes, and each output action (silence, publication or
invalidation) with its value or delta shape. Keep Python expressions, HGraph handles
and foreign calling conventions in the respective targets.

Test the stage directly with a stable text dump and checked-in golden fixtures. Use
symbolic declaration/instance IDs, deterministic graph-local IDs, explicit routes and
relative source spans. Snapshot the copy/borrow decision, handler admission, child
capture, publication and delta shape, and resolved calls for the scalar, keyed and
aliasing fixtures. Review intentional dump changes with the lowering change. The dump
is a test format internal to the compiler.

After the language comparison, migrate Python in vertical slices. For a complete input
closure, the driver selects either the existing analysed-program emitter or the
lowered-program emitter. Each invocation has one input. Run both paths on migrated
fixtures in CI, comparing traces and build diagnostics. Retire the legacy entry point
and remove the Python emitter's dependency on analysed bodies once every construct it
currently supports passes its fixtures via lowering and its existing rejection and
unsupported-form diagnostics match. Make this a prerequisite for native work beyond
the two comparison cases. Thereafter both emitters accept only the lowered program.
[Copy analysis](../crates/hgl_hgraph/src/copies.rs) · [Instance selection](../crates/hgl_hgraph/src/instances.rs) · [Python emitter](../crates/hgl_hgraph/src/emit)

## Milestone 0 — builder and provider inventory

The upstream implementation is in review. The provider audit and hand-written
comparison record their original pins. [Prepared-builder implementation](https://github.com/hhenson/hgraph/pull/1828) · [HGraph issue 1824](https://github.com/hhenson/hgraph/issues/1824) ·
[Provider audit](hgraph-adaptations.md#milestone-0-audit) · [Comparison and measurements](../experiments/hgraph-native/README.md)

HGraph 0.9.5 is the first SDK candidate. Header inspection gives the following starting
point. Trace each operation through wiring to its factory and list any inputs only
wiring can produce. Prove the scalar path first. Before enabling each later construct,
run the corresponding probe against the installed SDK using explicit builders and edges.

| Operation | Available at the pin | What the consumer must prove |
| --- | --- | --- |
| Ordinary nodes and edges | `NodeBuilder::native`, descriptor hooks, `GraphBuilder::add_node` and `add_edge` | Stateful nodes, port schemas and nested endpoint paths work with prepared builders. [Node][node] · [Graph][graph] |
| Rank, push sources and feedback | Ranking and cycle checking live in `Wiring::finish`. Feedback source and sink builders are exported. | Order a diamond, place push sources first, reject a same-cycle loop with source locations and run an explicitly delayed feedback pair. [Ranking][ranking] · [Feedback][feedback] |
| Push-source threading | `PushSourceSender` admits producer-thread sends. The queue tests exercise full capacity, blocked sends and stop. | Specify thread affinity for every exposed handle and callback. Keep node state and borrowed port views on the evaluation thread. Pass owned payloads through a sender, grant Rust `Send`/`Sync` only for supported operations, and test concurrent producers, full queues, refusal to block the evaluation thread, and a blocked sender racing shutdown. [Sender][push] · [Queue tests][push-tests] |
| Nested graphs and switch | A child builder and its input/output bindings fit `SingleNestedGraphNodeSpec`. `switch_node` accepts branch specs. | Bind a parent capture, switch branches, verify child teardown and route the selected result. [Nested graph][nested] · [Switch][switch] |
| Keyed or positional graph loops | `map_node` and `tsl_map_node` take child templates. | Supply key/element routes and key-set inputs as required. Remove and re-add a key or position and observe fresh child state. [Keyed map][map] · [List map][list-map] |
| Failure capture | `try_except_node` wraps a child and converts exceptions into node errors. | Only an HGL execution failure may be captured. Exercise both failure paths below before adopting this factory. [Capture][capture] |

The upstream entry point shares the implementation used by top-level and nested
wiring. The application supplies prepared builders with local IDs, data edges
and endpoint paths, same-cycle versus delayed dependencies, and logical parent/child
captures. HGraph resolves order, the push-source prefix, cycle diagnostics, capture
bindings and index remapping into a completed builder. Return offending IDs so the
compiler can attach source spans. Compare acceptance, rejected cycles and execution
traces with existing HGraph wiring on the probes above. The hand-written comparison listed nodes and boundary bindings in known order.
Generated Rust submits logical nodes and bindings through the prepared-builder API.
[Current ranker][ranking] · [HGraph issues](https://github.com/hhenson/hgraph/issues)

Audit the pinned Python provider, emitted support calls and test harness against their
fixtures. For each behavior-changing adapter, record the HGL rule and status, engine
behavior, and an observable test or coverage gap in the shared register. Include
selectors, delta application, failure translation and scheduler/teardown paths. Close
the audit when every such adaptation has an entry or a reference to one.
[Adaptation register](hgraph-adaptations.md) · [Python provider](../crates/hgl_hgraph/support) · [Emitter](../crates/hgl_hgraph/src/emit)

## Provision once, reuse across applications

The native-enabled compiler build provisions a locked SDK and, on the Rust path, its
matching C bridge. Use the upstream Conan recipe with build/host profiles and
`--build=missing`. Disable Python bindings, Python user
nodes and PyArrow discovery (`HGRAPH_USE_PYARROW_ARROW=OFF`). The recipe still requires
shared native Arrow with Compute and Acero. Include these in the binary closure and
record download size, installed size and cold provisioning time. An Arrow-free SDK
would need a separately tested upstream profile. Verify installation by building and
running a clean consumer of `find_package(hgraph CONFIG REQUIRED)` and `hgraph::core`
with PyArrow absent. [SDK recipe][sdk] · [Arrow discovery][arrow]

| Reusable unit | Distribution and invalidation |
| --- | --- |
| Native SDK | Publish libraries, headers, CMake exports and transitive dependencies per HGraph commit, dependency/recipe lock, platform/deployment baseline, C++ compiler and standard-library ABI, linkage, profile and features. |
| C bridge (Rust option) | Publish a native library and C headers keyed by SDK identity, bridge sources, ABI version, native toolchain and build flags. This is the binary contract for Rust consumers. Keep HGraph objects opaque across it. |
| Facade and HGL provider | Distribute the Rust facade and provider as pinned source and cache optimised products by source, core contract, bridge identity, rustc, target, features, profile and flags. For C++, build the optimised provider against the SDK. Application-specific HGL instances belong to generated module code. |
| Emitted modules and application | Key generated units by their contents and dependency interfaces, toolchain/profile and selected provider/SDK. Keep stable output paths and rewrite only changed contents. Reuse Cargo's target directory or CMake/Ninja's objects and link dependencies across invocations. |

Resolve each required native binary from a verified local entry, then a release archive
by locked digest, then locked sources. Lock per key and install atomically. Offline,
identify a missing key. Record the resolved identities in the compiler installation
for the app linker. Check the bridge ABI and its SDK fingerprint before use. Native CI
publishes the same archives, rejects corrupt and incompatible inputs, restores on a
second machine and runs the relocated executable with its packaged shared libraries.
Python-only builds continue to use their existing dependencies.

Cold SDK or bridge builds require C++. With those binaries installed, Rust application
edits need Cargo/rustc and a platform linker. Generated C++ edits need a compatible C++
compiler and linker. The HGL compiler itself is already built with Rust. A rustc
upgrade can reuse a compatible SDK and C bridge while rebuilding local Rust products.
[Rust calling conventions](https://doc.rust-lang.org/reference/items/external-blocks.html#abi)

## Provider and runner

Use ordinary owned target-language values for pure value functions where their
representation fits HGL. Convert at endpoint, callback and store boundaries. The facade
exposes typed, callback-scoped views for reads and writes and owned values for retention.
The provider implements HGL copy points from lowering, including nested snapshots and
reference identity. Each node builder owns its immutable definition. Each instance
owns its state through construction, start, evaluation, stop and destruction. The
Rust facade must contain unsafe access and keep callbacks and handles alive across
partial initialisation, failure and teardown. [Type registry][types] · [Node hooks][node]

The provider owns HGL adaptations on either native path. Both HGraph targets use one
[adaptation register](hgraph-adaptations.md) and an observable regression per entry.
Inventory core native declarations by resolved identity, role and concrete types. A
missing binding diagnoses the declaration before execution. Compile ordinary core HGL,
including replay and record, as module code. Track engine-service coverage separately:
typed scalar and container I/O, activity, clocks, alarms, scheduling, logging, sources,
sinks, nested graphs, services, observers, checkpointing and extensions. Each supported
service needs a native entry point and a direct consumer test.

Pass recoverable HGL execution failures explicitly from generated nodes and value
functions (a Rust `Result` or a tagged C++ return). Preserve code, node and phase.
An ordinary HGL `result<T,E>` remains program data. Catch unwinding panics and
unclassified C++ exceptions on their originating side of the foreign boundary. Set a
separate fatal run status and stop execution before capture or recording can turn
the fault into HGL data. The driver reports an aborting process as infrastructure
failure. Inject a known HGL failure and an internal fault inside the same capture
scope. Only the former can satisfy an HGL capture or `assert raises`.
[ERR-1](../spec/hgl_spec/spec/language/errors-and-diagnostics.md) · [Failure cases](../spec/hgl_core/tests/conformance/captured_failures.hgl)

The runner accepts a reusable builder and configuration, selects simulation or real
time, sets bounds, supports host stop and reads owned store values after stop, before
graph release. The assembly and running nodes must see the same run store. Expose
typed put/get for named keys, type checking on subsequent writes, and presence
independent of stored null. An embedding host can seed its own graph. For HGL,
application store statements supply the initial contents, eval starts empty and
`run.output` is reserved. Run one builder twice to verify fresh node and store state.
[Executor][executor] · [Global state][store] · [CRUN-6–8](../spec/hgl_core/spec/execution.md)

## Emit, build and run

Emit module libraries and an application entry point that returns the root builder.
Emit **only concrete HGL instances**, including value helpers and core calls. Collect
reachable instances to a fixed point before writing modules. Key each by its declaring
module, selected declaration and canonical concrete arguments (including values and
packs). Consume the analyser's specialised body, including the candidate selected for
each `requires` operation. Generated calls name that concrete symbol directly.

The declaring module's generated library owns the definition. Two importers of the
same instance both reference it. Distinct arguments produce distinct definitions.
If a concrete type or candidate introduces a reverse library dependency, build the
mutually dependent modules as one generated crate or library, preserving their module
names and instance ownership. Test both importers, a caller-defined type, and two
instances whose required operation resolves to different HGL candidates. Verify one
definition per identity and the expected calls and results.
[Instance collection](../crates/hgl_hgraph/src/instances.rs) · [Generic resolution](../spec/hgl_spec/spec/language/generics-and-operators.md)
Portable pre-transpilation artifacts belong to [compiler issue 132][artifacts].

A proposed `hgl build` dispatches to the target. For Python it returns the emitted
package. For native it compiles affected libraries and the application, links against
the selected provider and SDK, and returns a runnable only on success. Record that
artifact and its build-input fingerprint after the link succeeds. An exact cache hit
can launch it without invoking Cargo or Ninja. `hgl run` and executable tests require
a current build result. A failed rebuild must prevent launch of an older binary.
Preserve application selection, overrides, timeouts and HGL source diagnostics.

Build one selectable test executable per HGL test module. Select cases at run time
and create a fresh graph and store for each eval. A rejection case stops at analysis.
A full module run should issue at most one application link for that module, regardless
of case count.

## Delivery and acceptance

1. **Audit and provision.** Request the prepared-builder ranker in HGraph, run the
   provider audit, and restore an SDK offline. Application and wrapper edits must
   cause zero SDK compiles. Exercise Python build and CLI without the native SDK.
2. **Choose the language before lowering.** Hand-write both cases below in direct C++
   and safe Rust over the smallest bridge they need. Include the module boundary,
   builders and node bodies each emitter would have to produce. Run the comparison
   on those four programs. Implement only the chosen native emitter.
3. **Integrate the chosen path.** Add lowering goldens, migrate the Python cases and
   emit both native cases from the same lowered form. Verify their original traces
   and test the reported cycle with a deliberately wrong silent cell in a copy. Repeat
   the edit timings end to end through `hgl run`. Run an imported stateful node with a
   C++ node, seed and read the store, distinguish captured failure from fatal fault,
   and repeat the run. Verify rejection without linking and failed relink with an old
   executable present. Retire Python's legacy emission path before extending coverage.
4. **Compare and extend.** Run the same pinned core suite through Python and native.
   Compare each target to core's expectations, then compare traces by evaluation time,
   publication presence, value/delta (including invalidation) and failure category/code.
   Report agreed passes, proposed results, unsupported cases and discrepancies by
   target. Add engine-only tests separately. Start with Linux x86-64 and verify
   relocation before publishing further profiles, including macOS arm64 and Windows x64.
   [Core cases](../spec/hgl_core/tests/conformance) · [Store cases](../spec/hgl_core/tests/conformance/run_store.hgl)

### Turnaround comparison

| Hand-written case | Observable result |
| --- | --- |
| `replay_preserves_equal_ticks` (core) | Relay `[7, _, 7, 9]` exactly, preserving the silent cycle and repeated value. Check the companion publication counter. [Scalar case](../spec/hgl_core/tests/conformance/scalar_eval.hgl) |
| `a_dictionary_has_a_child_for_each_key` (compiler fixture) | Send dictionary deltas for two keys through a keyed child graph. Compare all three traces for child state, starts and stops. Updating a key retains its state, removal stops its child, and reinsertion starts fresh while the other key stays live. [Keyed case](../crates/hgl_hgraph/tests/fixtures/demo.graph_for.hgl) |

Use the same application/module split, host, SDK and parallelism. For the first Linux
x86-64 comparison, pin **LLVM LLD 23.1.5**. Point both compiler drivers at the same
linker binary and record its digest, flags and compiler versions. Prebuild the SDK,
bridge, facade and provider with optimisation enabled. Use the
development profile only for code that would be generated. Give C++ narrow provider
headers, one translation unit per module and incremental Ninja builds. Give Rust
per-module crates and a persistent Cargo target directory. Apply equivalent edits on
each side and verify the changed result. Time build invocation to verified result in
alternating runs, reporting median and spread in milliseconds, compile/link/launch
times and units rebuilt. This first comparison measures target-source edits. The
chosen emitter's end-to-end measurement also includes HGL analysis and emission.

| Scenario | What it measures |
| --- | --- |
| Empty dependency cache, then fresh application with SDK installed | One-time provisioning cost separately from first application build. |
| Unchanged rerun | Launch and driver overhead. Expect zero compiler and linker invocations. |
| Application wiring edit | Normal edit-to-result latency with the provider and module libraries warm. |
| Imported node body edit, then a separate public signature edit | Cost of a local implementation change and the cost of recompiling consumers. Verify changed output. |

**Proposed throughput gate, set before timing.** Require Rust to achieve at least
**80% of direct C++ input ticks/second on each case**. Use release builds of the
renderings for this measurement. Replay a long identical stream with the same scalar
mix or keyed churn and the same observable counts and checksums. Warm up, exclude
construction and external I/O, then take the median of at least five paired Rust/C++
ratios per case. Report absolute rates, ratios and spread, plus the existing Python
target for context. A failure on either case rules out that Rust implementation. If
variation straddles the floor, repeat before deciding.

Among paths with correct traces and acceptable throughput, prefer a repeatable warm
edit advantage. Include three costs in the decision record.

- **Bridge breadth.** Count distinct C operations, facade methods and ownership or
  lifetime conversions required for scalar and keyed cases. Inventory what references,
  windows and remaining container/delta shapes would add, distinguishing measured
  work from estimates.
- **Consumer toolchains.** Build the C++ consumer with each advertised SDK-compatible
  compiler and standard-library ABI. Build the Rust consumer against the C ABI with
  the prebuilt bridge and a platform linker. Record the toolchains users must install
  and which combinations require another SDK binary. With the C bridge, Rust callers
  depend on the platform C ABI. The bridge producer owns C++ ABI compatibility.
- **Aliasing defects.** Translate a retained-value fixture in both languages, mutate
  the original and check the retained snapshot. Also try a scratch variant that holds
  a safe borrow across conflicting mutation. Record what rustc rejects and which
  mistakes still need behavior or FFI-lifetime tests on each path. [Copy-point fixture](../crates/hgl_hgraph/tests/fixtures/demo.copy_points.hgl)

If timings overlap or the cases favor different languages, weigh those three costs
explicitly. Prefer direct C++ on a tie after accounting for deployment and safety.

### Risks and decision ownership

The compiler owner chooses the emitted language after the comparison. Report link
time from the hand-written probes and HGL analysis time once the chosen path is
integrated. The compiler issue tracker owns the resulting build work, including
the separate proposal for reusable analysed modules. HGraph maintainers own builder
additions, adoption of the proposed C ABI and facade, and any smaller Arrow profile.
Test an SDK upgrade against the bridge and builder probes before changing the pin.
Record unresolved decisions in the relevant tracker. [Compiled modules][artifacts] ·
[HGraph issues](https://github.com/hhenson/hgraph/issues)

[sdk]: https://github.com/hhenson/hgraph/blob/3ee23f860ad62c032482a00f10f99105cb733609/conanfile.py
[arrow]: https://github.com/hhenson/hgraph/blob/3ee23f860ad62c032482a00f10f99105cb733609/cmake/HgraphArrow.cmake
[graph]: https://github.com/hhenson/hgraph/blob/3ee23f860ad62c032482a00f10f99105cb733609/include/hgraph/runtime/graph.h
[node]: https://github.com/hhenson/hgraph/blob/3ee23f860ad62c032482a00f10f99105cb733609/include/hgraph/runtime/node.h
[ranking]: https://github.com/hhenson/hgraph/blob/3ee23f860ad62c032482a00f10f99105cb733609/src/hgraph/types/graph_wiring.cpp
[push]: https://github.com/hhenson/hgraph/blob/3ee23f860ad62c032482a00f10f99105cb733609/include/hgraph/runtime/push_source_node.h
[push-tests]: https://github.com/hhenson/hgraph/blob/3ee23f860ad62c032482a00f10f99105cb733609/tests/cpp/test_push_source_queues.cpp
[feedback]: https://github.com/hhenson/hgraph/blob/3ee23f860ad62c032482a00f10f99105cb733609/include/hgraph/runtime/feedback_node.h
[nested]: https://github.com/hhenson/hgraph/blob/3ee23f860ad62c032482a00f10f99105cb733609/include/hgraph/runtime/nested_graph_node.h
[switch]: https://github.com/hhenson/hgraph/blob/3ee23f860ad62c032482a00f10f99105cb733609/include/hgraph/runtime/switch_node.h
[map]: https://github.com/hhenson/hgraph/blob/3ee23f860ad62c032482a00f10f99105cb733609/include/hgraph/runtime/map_node.h
[list-map]: https://github.com/hhenson/hgraph/blob/3ee23f860ad62c032482a00f10f99105cb733609/include/hgraph/runtime/tsl_map_node.h
[capture]: https://github.com/hhenson/hgraph/blob/3ee23f860ad62c032482a00f10f99105cb733609/include/hgraph/runtime/try_except_node.h
[types]: https://github.com/hhenson/hgraph/blob/3ee23f860ad62c032482a00f10f99105cb733609/include/hgraph/types/metadata/type_registry.h
[executor]: https://github.com/hhenson/hgraph/blob/3ee23f860ad62c032482a00f10f99105cb733609/include/hgraph/runtime/executor.h
[store]: https://github.com/hhenson/hgraph/blob/3ee23f860ad62c032482a00f10f99105cb733609/include/hgraph/runtime/global_state.h
[artifacts]: https://github.com/hhenson/hgl-compiler/issues/132
