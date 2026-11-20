# HGL behavior on HGraph

**Status — implementation test inventory.** The Python and native providers share
these regressions. Rule status and expected results come from the compiler's pinned
spec and core. A compiler fixture documents an implementation test. Native coverage
is proposed except where the milestone report records a native probe. The audit below
covers the Python provider and host at compiler revision `9b479e5`.

| Surface | Observation a provider must preserve | Regression |
| --- | --- | --- |
| Composite validity | A single valid child makes the composite valid. Test `all_valid` independently. | Populate one fixed-list position or struct field and probe both predicates. [END-7][end] · [Core cases][collections] |
| List with no valid child | A nonempty list with only invalid positions fails `valid`. A published empty list passes. The distinction also controls handler admission. | Grow by size only, publish an empty list, and inspect validity and admitted ticks. [END-7][end] · [List fixture][lists] |
| Empty-list publication | The first explicit empty list and an explicitly repeated empty list each publish. A silent cycle has no publication. HGraph's unbounded list can coalesce a write that changes no element. | Record two empty publications separated by silence, through a node and a lifted value function. [TIME-7 and TIME-9][time] · [Python adapter][python] · [List fixture][lists] |
| Index after shrink | A dropped position ceases to belong to the list. A saved route cannot keep its old child alive. An evaluated out-of-range value index follows EXPR-3. | Add a differential regression that shrinks below a selected position, probes, then regrows with a new value. Check selection and value indexing separately. [EXPR-3][expressions] · [TIME-10][time] · [Core resize cases][collections] |
| Read of an invalid child | An executed payload read fails with `value.unset_read`. In a context expecting `result<T,error>`, it yields that code as ordinary error data. | Run the paired core cases with a runtime index selecting first a valid and then an invalid position. [END-8][end] · [Core child-read cases][collections] |
| Holes in a whole read | A whole read of a list containing an unset position fails with `value.unset_read`, even when another position makes the list valid. | Fill one position and leave another unset. Read the valid child successfully and check the failing whole read. [END-7–8][end] · [Whole-read fixture][lists] |
| Negative list size delta | A negative `size_modified` reduces the held size by that count, down to zero. The published delta reports the resulting size when it changes and the tail positions dropped. | Replay a reduction by one and a reduction past zero. Check the value and the recorded delta. [TYPE-16][types] · [Core reduction cases][collections] · [List fixture][lists] |
| Entry beyond a fixed list | A constant out-of-bounds delta entry is a rejection (`delta.index_bounds`). A dynamic entry past the end fails when the delta is published. | Check the diagnostic without building, then compute the same index at run time and capture the write failure. [TYPE-16][types] · [Core bounds case][collections] |
| Growing-list size after invalidation | HGraph can retain physical positions when its list is invalidated. The next HGL delta must use the cleared payload as its starting point. | Grow, invalidate the whole list, then update position zero without supplying a size. Check a one-position list with no old tail. Compare with invalidating a single position in a still-held list. [END-4][end] · [Python size adapter][python] |
| Selectors on an invalidated collection | The transition wakes active subscribers. The invalid input has false `valid` and `modified`, and null `last_modified`. Repeated invalidation is silent. | Use permissive admission and an independent probe. Check which handlers ran and the collection and child observations after invalidation and republication. [END-4 and END-6–8][end] · [TIME-9][time] |
| Outputless list-map body on shrink | Each retained position keeps its child. Dropping a position stops that child, including when its body only has side effects. Regrowth creates a fresh child. | Count starts, stops and effects across shrink/regrowth and confirm a surviving sibling keeps state. [CTRL-7 and CTRL-15][control] · [Outputless loop fixture][lists] |
| Reference designation | A reference ticks on a change of designation. Following the selected endpoint can observe a payload publication while the designation stays the same. | Add a regression that selects the same endpoint twice, ticks its payload, then switches to another endpoint with an equal payload. Record designation and follower separately. [TIME-10][time] · [Related core cases][references] |
| Set-delta equality | Compare added and removed members separately, including in a containing value or trace cell. | Compare adding one member with removing that same member, both directly and nested. [TYPE-16][types] · [Equality fixture][equality] |
| Removing an absent dictionary key | TYPE-16 ignores removal of a missing key without failure. | The agreed core case checks that another key survives. A separate proposed case checks whether removal of only an absent key wakes readers. [Spec decision][absent] · [Core dictionary cases][dict] |
| Scheduler delivery and pending state | Snapshot the due wake on entry. The wake being delivered is no longer pending. A coincident input or a new same-tag request during evaluation does not erase the cause of this wake. | Inspect `scheduled()`, `is_scheduled_now` and `next_scheduled_time` before and after rescheduling. Cancel the next pending wake. [CAP-4][caps] · [Core wake cases][wakes] · [Timer regressions][timers] |
| Scheduler tags, cancellation and reset | Distinguish an empty tag from an omitted tag. Exercise same-tag replacement, tagged cancellation, untagged earliest cancellation, and reset on an inputless node. | Schedule two deadlines, replace one by tag, cancel or reset, and verify the times that remain. Keep the core cases' agreed/proposed labels. [CAP-4][caps] · [Core wake cases][wakes] · [Timer regressions][timers] |
| Alarm and deadlines at or before now | An alarm keeps its earliest request. An input tick has precedence for the alarm selector. The Python fixture also records what happens to a pending wake when another is requested at or before now. | Compare overlapping requests, an input coinciding with an alarm, and deadlines already reached. Track the last as observed provider behavior where the pin leaves delivery unspecified. [CAP-5][caps] · [Core wake cases][wakes] · [Timer regressions][timers] |

Run each regression against the expected observations on both targets. If an SDK version
already supplies the required behavior, the regression is sufficient. Track differences
in engine behavior in HGraph and gaps in an HGL contract in its owning repository.

## Milestone 0 audit

Reviewed the five shipped Python support modules, their aliases and the call sites
in the emitter. The inventory covers 122 top-level functions/classes in
`core_native.py`, 101 in `core_support.py`, 25 in the eval harness, and 13 in the two
process drivers. The table groups helpers by observable behavior, including helpers
that implement an HGL rule entirely inside the provider. The preceding rows cover
collection validity, child reads and scheduling in more detail.

| Implementation family | What native must preserve, and how to observe it |
| --- | --- |
| Module constants, enums, nominal structs and generic schemas (`constant`, `member`, `struct` through `changed`) | Lazily evaluate a forward constant once. Check enum construction by exact name and ordinal, inherited fields, nominal equality and distinct applied generic types. An omitted struct delta field preserves the held field. UNIT-13, TYPE-3/7/16. [Struct fixture](../crates/hgl_hgraph/tests/fixtures/demo.struct_review.hgl) · [Constants](../crates/hgl_hgraph/tests/fixtures/demo.constants.hgl) |
| Value retention and mutation (`Held`, `copied`, `set_at`, `given_at`, item/entry helpers) | Retain a nested mutable value, mutate the original, and observe both. For a native call through a nested place, preserve write-back, argument evaluation order and updates made before failure. TYPE-5 and the value-function rules. [Copy points](../crates/hgl_hgraph/tests/fixtures/demo.copy_points.hgl) · [Places](../crates/hgl_hgraph/tests/fixtures/demo.places.hgl) |
| Equality, result data and failures (`_equal` through `lifted_is_err`) | Compare nested NaNs and set changes structurally. Preserve result tags, cause, code, phase and original failing node through capture or propagation. Start/stop and unclassified engine errors must escape a capture. Test wrong-code assertions and failures during nested lifecycle hooks. ERR-1/4/5/7/8. [Python adapter][python] · [Provenance](../spec/hgl_core/tests/conformance/error_provenance.hgl) · [Capture fixture](../crates/hgl_hgraph/tests/fixtures/demo.captures.hgl) |
| Endpoint shapes (`dict_delta` through `unpublished`, `nullable`, emitted publication and composition) | Distinguish a whole replacement, a child delta, published null, silence and invalidation. Strict dictionary reads of missing keys fail. Optional lookup and child endpoint observation have different results. Exercise non-peered bundles, fixed/variable lists, sets and child paths independently. TYPE-9/14/16, END-2–8. [Validity fixture](../crates/hgl_hgraph/tests/fixtures/demo.validity.hgl) · [Publication code](../crates/hgl_hgraph/src/emit/publish.rs) |
| Node and child graph admission | Preserve active/passive inputs, per-handler admission and source-ordered handlers, state/cache lifetime, switch/default failures, nested capture and outputless side effects. Exercise branch exit/re-entry, key reinsertion and parent captures. Child-window support has explicit target diagnostics. TIME-3, CTRL-6/7/14/15. [Node emission](../crates/hgl_hgraph/src/emit/node.rs) · [Composition](../crates/hgl_hgraph/src/emit/compose.rs) · [Graph fixtures](../crates/hgl_hgraph/tests/fixtures/demo.graph_for.hgl) · [Window fixtures](../crates/hgl_hgraph/tests/fixtures/demo.windows.hgl) |
| Resumable sources and capabilities (`Source`, `resume`, `Wake`, clock/logger/scheduler/store bindings) | Resolve relative and absolute yields, skip elapsed targets, preserve monotonic ordering and reject negative delays. Check evaluation versus wall time, log level and literal message text, earliest alarm, isolated run stores, missing keys and owned readback. SOURCE-2/3, CAP-2–6. [Source cases](../spec/hgl_core/tests/conformance/source_timing.hgl) · [Capabilities][caps] · [Store cases](../spec/hgl_core/tests/conformance/run_store.hgl) |
| Native scalar operations and conversions | Check signed-width overflow, integer versus float division and modulo, zero-divisor policies, unordered comparison, exact mixed numeric equality, tolerance and rounding. Check parse/encoding failure as result data versus execution failure and the spelling of converted values. [Numeric contract](../spec/hgl_core/spec/numeric-operators.md) · [Scalar contract](../spec/hgl_core/spec/scalar-operators.md) · [Conversion fixture](../crates/hgl_hgraph/tests/fixtures/demo.converted.hgl) |
| List, dictionary, temporal and calendar natives | Preserve append/extend ownership, optional lookup, microsecond duration range and tie-to-even rounding, UTC offsets and exact zone membership, tenor form/overflow and end-of-month movement. Exercise these through native declarations as well as ordinary HGL wrappers. [Native provider](../crates/hgl_hgraph/support/core_native.py) · [Duration fixture](../crates/hgl_hgraph/tests/fixtures/demo.durations.divided.hgl) · [Core implementation cases](../spec/hgl_core/tests/implementation) |
| Eval and process host | Normalize dense/timed replay and whole/delta expectations, distinguish an unreadable whole from its delta, snapshot at publication and preserve timestamps. Check independent stores, exclusive end time, ordered recordings, escaped failure codes, bounded real-time stop, output framing and diagnostics. CEVAL, CRUN and TST. [Eval harness](../crates/hgl_hgraph/support/harness.py) · [Test host](../crates/hgl_hgraph/support/run_tests.py) · [Application host](../crates/hgl_hgraph/support/run_application.py) |

### Coverage at the original audit revision

At compiler `9b479e5` on HGraph 0.9.5, the Python target passed all **109 executable conformance cases** and
both rejection cases. In core's separate implementation suite, 37 passed. Its declared
gap was `core_implementation::reverse_scaling` (`mul_(str, i64)`). All **125** tests in
the compiler's Python emitter integration test binary passed with its otherwise
ignored runtime tests enabled. [Core allowance](../crates/hgl_hgraph/ci/unsupported_cases.toml) ·
[Emitter tests](../crates/hgl_hgraph/tests/emit.rs)

At the audit revision the list adapter had four explicit unsupported paths. They cover an empty unsized
list nested in a source yield without an output port, an update at or beyond an
explicit new size, a removal beyond the list's extent, and a whole source-yielded
unsized list with nullable or composite elements. Capture the unsupported diagnostic
as a target limitation. Keep a native parity case separate from each Python limit.
The current pin includes the absent-key cases. The native slice and current Python
suite results are recorded in the [integration report](../experiments/hgraph-native/hgl-turnaround.md#validation-scope). [List adapter][python] ·
[Runtime gap fixtures](../crates/hgl_hgraph/tests/fixtures/a_list_form_not_written_fails_as_the_graph_runs)

[end]: ../spec/hgl_core/spec/temporal-endpoints.md
[collections]: ../spec/hgl_core/tests/conformance/collection_ticks.hgl
[time]: ../spec/hgl_spec/spec/language/temporal-model.md
[expressions]: ../spec/hgl_spec/spec/language/expressions-and-bindings.md
[types]: ../spec/hgl_spec/spec/language/types-and-values.md
[control]: ../spec/hgl_spec/spec/language/control-flow-and-sources.md
[dict]: ../spec/hgl_core/tests/conformance/dictionary_ticks.hgl
[absent]: https://github.com/hhenson/hgl_spec/pull/132
[caps]: ../spec/hgl_core/spec/capabilities.md
[wakes]: ../spec/hgl_core/tests/conformance/scheduled_wakes.hgl
[timers]: ../crates/hgl_hgraph/tests/fixtures/demo.timer_fixes.hgl
[references]: ../spec/hgl_core/tests/conformance/reference_ticks.hgl
[python]: ../crates/hgl_hgraph/support/core_support.py
[lists]: ../crates/hgl_hgraph/tests/fixtures/demo.list_delta.hgl
[equality]: ../crates/hgl_hgraph/tests/fixtures/a_set_delta_within_a_cell_is_compared_by_its_changes/demo.cell_deltas.hgl
