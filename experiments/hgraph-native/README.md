# HGraph prepared-builder comparison

**Milestone 0 result.** Proceed with Rust for the first emitter slice. It shortened
all three warm edits on this Linux x86-64 profile and passed the predeclared 80%
throughput floor on both graphs. Direct C++ launched an unchanged application sooner.
These measurements start at hand-written target source. The Rust emitter now has
[end-to-end measurements from edits to HGL source](hgl-turnaround.md).
[Backend plan](../../docs/hgraph-native.md) · [Upstream ranking request](https://github.com/hhenson/hgraph/issues/1824)

## Turnaround

Median milliseconds from invoking the build to a verified result over 15 alternating
pairs. Each build produces one application with both cases. The two case columns add
their respective launch and verification time to the same build measurement.

| Development edit | C++ scalar | Rust scalar | C++ keyed | Rust keyed |
| --- | ---: | ---: | ---: | ---: |
| No change | 14.0 | 34.6 | 13.9 | 31.9 |
| Application wiring | 97.2 | 78.4 | 94.1 | 73.5 |
| Imported node body | 100.7 | 90.1 | 98.2 | 85.5 |
| Public signature and caller | 101.8 | 94.0 | 99.1 | 90.3 |

Each edit changes an expected output. The no-change builds compiled and linked zero
units. An application edit rebuilt one application unit on both paths. On a body edit
Ninja compiled the node unit, archived it and relinked. Cargo rebuilt the node and
application crates. Median time inside LLD was 4.5–4.6 ms for C++ and 11.5–11.7 ms
for Rust across the changed builds. Scalar wiring edits ranged from 95.5–104.9 ms
for C++ and 76.3–83.0 ms for Rust. [All build samples and rebuilt units](turnaround.csv)

With the installed SDK and optimised layers warm, the first application build took
207 ms for C++ (including CMake configuration) and 84 ms for Rust (Cargo had already
built the facade). Separately, configuring and compiling the common optimised C++
provider and bridge took 3.47 s. Building the optimised Rust facade took 141 ms.
Those are single observations.

## Throughput and correctness

Median external input **publications per second**. The Rust/C++ column is the median
of paired ratios. Each native run repeated the fixture stream 50,000 times after a
separate warm-up. Python used five samples with the same stream length. Every run
verified the publication counts and timestamp/value checksums for every output lane.

| Graph | Input publications/run | Direct C++ | Rust | Python target | Paired Rust/C++ (range) |
| --- | ---: | ---: | ---: | ---: | ---: |
| Scalar | 300,000 | 3,194,232 | 3,207,625 | 315,858 | 99.95% (96.73–106.03%) |
| Keyed dictionary | 750,000 | 961,539 | 958,073 | 183,215 | 99.50% (97.94–103.61%) |

The short runs also compare literal output times and values from the fixtures. The
scalar counter records `[1, _, 2, 3]` and the relay records `[7, _, 7, 9]`. On the
five keyed probes the observed count for “a” is `1, 1, 2, 2, 1`, starts are
`1, 2, 2, 2, 3`, and stops are `0, 0, 0, 1, 1`. Both renderings build a real HGraph
sink map, supply the key and value child input slots, and connect the dictionary's
key-set projection to the lifecycle input. HGraph owns child allocation, scheduling
and disposal. [Scalar fixture](../../spec/hgl_core/tests/conformance/scalar_eval.hgl) ·
[Keyed fixture](../../crates/hgl_hgraph/tests/fixtures/demo.graph_for.hgl) · [All throughput samples](throughput.csv)

The native timer surrounds the executor run. SDK loading, graph construction and
verification occur outside it. Both paths use the same optimised C++ replay, record,
typed accessors and map factory. The C++ node bodies call the C++ interface directly.
Rust implements its own bodies and wiring and calls through the C bridge. Python
uses emitted replay and node bodies on HGraph with a Python checksum sink. Its timer
runs from sink start to stop, after wiring. The workloads use the same observations
and publication counts. Source and sink overhead differs by language.

## Bridge, toolchain and audit findings

- The scalar path needs nine C operations for assembly, execution, scalar I/O and
  state. The keyed case adds seven for map construction, key-set binding, string
  input, dictionary update/removal and global-store read/write. The string view is
  borrowed for one callback. A dictionary mutation copies its key at the boundary.
  Reference designation/lifetime, windows, arbitrary nested routes, general deltas,
  recoverable errors and producer-thread senders still need bindings or probes. The
  benchmark bridge terminates the process on an unexpected foreign exception.
  [Bridge](bridge.cpp) · [Rust facade](rust/facade/src/lib.rs)
- Both comparisons used GCC 15.2, rustc 1.100.0 and the same LLD 23.1.5 binary
  (SHA-256 `ed377d194e3f4f2a21452caa9301579624517d4879c578db56dde81bbcb6dd6f`).
  A separate SDK consumer built with GCC 15. GCC 14 failed to link with its default
  libstdc++ because the SDK required `GLIBCXX_3.4.34`. GCC 14 linked and ran with
  the GCC 15 library selected. A fresh Rust consumer ran both graphs against the
  prebuilt bridge using `cc` as its linker driver. Its build invoked no C++ compiler.
- Keeping a shared borrow of a list element, mutating the list and then observing the
  borrowed element produced Rust diagnostic E0502. C++ accepted the reference and
  its retained-value assertion failed. Taking an owned copy passed in both languages.
  Generated aliasing checks remain necessary for FFI views and HGL snapshots.
- The Python provider audit records the core outcomes, compiler integration tests and
  explicit unsupported paths. [Provider audit](../../docs/hgraph-adaptations.md#milestone-0-audit)

## Reproduce

Use the HGraph 0.9.5 sources at `3ee23f860ad62c032482a00f10f99105cb733609` and a
native dependency prefix with Arrow 25.0.0, Compute and Acero plus HGraph's other
build dependencies. The measured SDK was a shared Release CMake build with Python
bindings, Python user nodes, PyArrow discovery and upstream tests disabled. The
source build and install took 62.2 s with 24 workers and an existing dependency
prefix. The installed SDK occupied 54 MiB. Its gzip archive was 11,470,906 bytes
and was restored locally for the measured builds. The external dependencies were
already installed. A second-host restore and a fully cold dependency build remain
untested.

From this directory, with `HG_SRC`, `DEPS`, and a fresh `WORK` set to absolute paths:

```sh
cmake -S "$HG_SRC" -B "$WORK/sdk-build" -G Ninja \
  -DCMAKE_PREFIX_PATH="$DEPS" -DCMAKE_INSTALL_PREFIX="$WORK/sdk" \
  -DCMAKE_BUILD_TYPE=Release -DHGRAPH_BUILD_SHARED=ON -DBUILD_TESTING=OFF \
  -DHGRAPH_USE_PYARROW_ARROW=OFF -DHGRAPH_BUILD_PYTHON_BINDINGS=OFF \
  -DHGRAPH_ENABLE_PYTHON_USER_NODES=OFF
cmake --build "$WORK/sdk-build" -j24
cmake --install "$WORK/sdk-build"
python3 compare.py "$WORK/sdk" "$DEPS" "$WORK/comparison" --pairs 15 --repetitions 50000
```

The driver copies the hand-written sources to scratch space. It pins both drivers to
the installed Rust toolchain's LLD, prebuilds the provider and facade with optimisation,
then times debug/incremental application builds and release throughput runs.
The reported run used two available CPU cores for both native paths. The raw result
includes the LLD version and digest, per-unit rebuilds and individual samples.

For the Python reference, set `HGL` to the pinned compiler executable and `PYTHON`
to a Python interpreter with HGraph 0.9.5 installed. Run from this directory:

```sh
"$HGL" emit ../../spec/hgl_core/tests/conformance/scalar_eval.hgl \
  ../../crates/hgl_hgraph/tests/fixtures/demo.graph_for.hgl --out "$WORK/emitted"
PYTHONPATH="$WORK/emitted" "$PYTHON" python_reference.py \
  "$WORK/comparison/results.json" "$WORK/python-rates.json"
```
