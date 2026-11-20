# Native HGL edit to verified result

Rust is the native emitter for the first scalar and keyed slice. Both emitters run the
original HGL assertions. The scalar case checks equal publications on either side of
silence. The dictionary case checks the child's count, starts and stops across update,
removal and reinsertion. Each assertion gets a fresh HGraph executor and run store.
[Scalar golden](../../crates/hgl_lower/tests/scalar.golden) · [Keyed golden](../../crates/hgl_lower/tests/keyed.golden)

## Measured turnaround

Milliseconds from writing an HGL edit to verifying every UTC timestamp and integer
printed by `hgl run --target hgraph-native`. Each warm cell gives the median and
full range of 15 samples. Each graph imports its node declarations from its fixture
and has a finite HGL application with sparse sources. The driver alternates the two
versions of each edit and the order of the applications. [Driver](hgl_turnaround.py) · [Raw samples and phase timings](hgl-turnaround.csv)

| HGL change | Scalar, ms | Keyed, ms | Cargo units rebuilt |
| --- | ---: | ---: | --- |
| Unchanged | 43.6 (41.1–45.9) | 47.0 (44.7–48.5) | None |
| Application wiring | 95.6 (93.0–96.8) | 98.6 (94.3–101.4) | Application |
| Imported node body | 108.6 (106.6–110.3) | 111.9 (108.5–113.9) | Nodes and application |
| Public signature and caller | 106.9 (102.8–109.0) | 112.5 (109.1–118.0) | Nodes and application |

The wiring edit changes one connection in the application's entry. Both replay
sources are fixed. Scalar selects an input with an extra publication, and keyed
selects a key stream with a different third key. The benchmark verifies that the
imported HGL and the source definitions are identical across this edit.

On wiring edits, front-end work took 3.3 and 5.1 ms for scalar and keyed
respectively. Lowering, emission and file writes together took under 0.3 ms at the
median for each. Cargo took 69.5 and 70.7 ms. Launch and execution took
20.5 and 20.6 ms.
An execution trace of a warm scalar rerun showed Cargo and the built application,
with zero compiler or linker processes.

With an installed SDK and a prebuilt C bridge, the first build in an empty application
workspace took 309 ms scalar and 311 ms keyed, one observation each. That includes
building the Rust facade in the generated workspace. Starting from the installed SDK
and building the bridge as well took 4.09 s for the scalar application, one observation.
The published timings cover Linux x86-64, the release HGL compiler, rustc 1.100.0,
LLD 23.1.5, GCC 15, an optimised provider, development application and node crates,
and native Arrow 25. SDK provisioning is outside these intervals.
[Prior target-source comparison](README.md)

## Integration

- **Lowering** resolves the reachable calls, specialises const inputs, assigns graph-local IDs, describes handler admission, and records state, lifecycle, publication and keyed boundary operations. Both backends consume this representation for the migrated roots. Python retains its existing emitter for other roots. [Lowered form](../../crates/hgl_lower/src/lib.rs)
- **Emission** writes a Cargo application and a node library. The application supplies the prepared builders and edges. The library supplies the Rust callbacks. The generated workspace includes the Rust toolchain pin and facade source. [Rust emitter](../../crates/hgraph_native/src/lib.rs) · [Python rendering](../../crates/hgl_hgraph/src/emit/lowered.rs)
- **Runtime** owns the C ABI, safe Rust callback views, native schemas, replay, recording and keyed map factory. It submits logical connections to HGraph's `PreparedGraph::finish()` and child boundary slots to `finish_child()`. HGraph owns ranking, scheduling and child lifetimes. The compiler tree currently hosts the bridge and facade. [Runtime component](../../runtime/hgraph_native) · [Prepared builder API](https://github.com/hhenson/hgraph/pull/1828)
- **Build and run** write changed generated files, hold a per-workspace lock, ask Cargo to build once for the selected roots, then launch the executable Cargo reports. A failed compile or link stops launch. The normal cache is under `$XDG_CACHE_HOME/hgl/native` or `~/.cache/hgl/native`. `--keep DIR` selects a reusable generated workspace. `HGL_NATIVE_CACHE` overrides the default cache root. [Driver](../../crates/hgl_cli/src/native.rs)

## Reproduce

Use a shared HGraph SDK built from [HGraph PR #1828](https://github.com/hhenson/hgraph/pull/1828) or a revision that includes it. The timing samples used
the first implementation (`bba44c48d`, based on `c9a70d5e0`). The differential
tests also pass with the rebased implementation, `4fe47d1da`. Use absolute paths.
Set `HG_SRC` to the HGraph checkout, `SDK` to the
chosen install prefix, `DEPS` to the native dependency prefix, and `WORK` to a
build directory. Run these commands from the compiler root on Linux.

```sh
cmake -S "$HG_SRC" -B "$WORK/sdk-build" -G Ninja \
  -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$SDK" \
  -DCMAKE_PREFIX_PATH="$DEPS" -DHGRAPH_BUILD_SHARED=ON \
  -DHGRAPH_BUILD_PYTHON_BINDINGS=OFF -DHGRAPH_ENABLE_PYTHON_USER_NODES=OFF \
  -DHGRAPH_USE_PYARROW_ARROW=OFF -DBUILD_TESTING=OFF
cmake --build "$WORK/sdk-build" --parallel
cmake --install "$WORK/sdk-build"

export HGL_HGRAPH_SDK="$SDK" CMAKE_PREFIX_PATH="$DEPS"
export LD_LIBRARY_PATH="$DEPS/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
cargo build --locked --release -p hgl_cli
target/release/hgl test --target hgraph-native \
  --test replay_preserves_equal_ticks --test a_dictionary_has_a_child_for_each_key \
  spec/hgl_core/tests/conformance/scalar_eval.hgl \
  crates/hgl_hgraph/tests/fixtures/demo.graph_for.hgl
```

An SDK install is reusable. To share the compiled bridge across application workspaces,
build the runtime component once. Set `HGL_HGRAPH_BRIDGE` to the directory containing
its shared library. Subsequent consumers use rustc and the platform linker.

```sh
cmake -S runtime/hgraph_native -B "$WORK/bridge" -G Ninja \
  -DCMAKE_BUILD_TYPE=Release -DCMAKE_PREFIX_PATH="$SDK;$DEPS" \
  -DHGRAPH_USE_PYARROW_ARROW=OFF
cmake --build "$WORK/bridge"
export HGL_HGRAPH_BRIDGE="$WORK/bridge"
lld_dir="$(rustc --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin/gcc-ld"
export RUSTFLAGS="-C linker=cc -C link-arg=-B$lld_dir -C link-arg=-fuse-ld=lld"
python3 experiments/hgraph-native/hgl_turnaround.py target/release/hgl "$WORK/samples"
```

`hgl emit --target hgraph-native --test NAME --out DIR ROOT` writes the Rust workspace
for inspection. `hgl run --target hgraph-native ROOT` builds and runs a supported
finite simulation. To run the differential and failed-build checks, set
`HGL_HGRAPH_PYTHON` to an interpreter with the Python backend's HGraph installed
and run `cargo test -p hgl_cli --features native-sdk-tests --test native`.

The current slice records integer outputs, supports dense scalar test inputs and
finite constant-yield sources, a single integer state cell per node, and a
`dict<str, i64>` iteration with an outputless child. General collection reads,
temporal captures, multiple handlers, configured runs, real-time applications,
and other core types remain outside this slice.

## Validation scope

The selected scalar and keyed tests pass on both targets. The integration suite also
changes an imported HGL body, compares the complete timestamped CLI outputs and forces
a failed link after a successful run. At core `40c3e6f` and spec `a3716cc`, the Python
target passed all 131 executable conformance cases, all five rejection cases and all
41 implementation cases. Native validation covers the two migrated cases and the
finite applications above. [Executable checks](../../crates/hgl_cli/tests/native.rs)
