"""Time emitted Python node bodies on HGraph with the native probes' inputs and sinks."""

import argparse
import json
from pathlib import Path
from time import perf_counter_ns

import hgraph as hg
from _hgl import _m_conformance as scalar
from _hgl import _m_core as core
from _hgl._m_demo import _m_graph_for as keyed

MASK = (1 << 64) - 1
INITIAL = 14695981039346656037
PRIME = 1099511628211


def measure(case, repetitions):
    lanes = [[0, INITIAL] for _ in range(3)]
    clock = {}
    ticks = 0

    @hg.sink_node
    def record(value: hg.TS[int], lane: int, _clock: hg.EvaluationClock = None):
        index = (_clock.evaluation_time - hg.MIN_ST) // hg.MIN_TD
        count, digest = lanes[lane]
        digest = ((digest ^ index) * PRIME) & MASK
        lanes[lane] = [count + 1, ((digest ^ value.value) * PRIME) & MASK]

    @record.start
    def start_timer(lane: int):
        clock.setdefault("start", perf_counter_ns())

    @record.stop
    def stop_timer(lane: int):
        clock["stop"] = perf_counter_ns()

    def stream(pattern):
        nonlocal ticks
        entries = [
            (hg.MIN_ST + hg.MIN_TD * i, value)
            for i, value in enumerate(pattern * repetitions)
            if value is not None
        ]
        ticks += len(entries)
        return entries

    @hg.graph
    def graph():
        if case == "scalar":
            count = core._inst_replay__i64(stream([7, None, 7, 7, None]))
            relay = core._inst_replay__i64(stream([7, None, 7, 9, None]))
            record(scalar.count_publications(count), 0)
            record(scalar.pass_i64(relay), 1)
        else:
            key = core._inst_replay__str(
                stream(["a", None, "b", None, "a", None, "a", None, "a", None])
            )
            price = core._inst_replay__i64(
                stream([1, None, 7, None, 2, None, -1, None, 5, None])
            )
            probe = core._inst_replay__bool(stream([None, True] * 5))

            # One emitted keyed child graph, shared by all three observation lanes.
            @hg.graph
            def per_key(name: hg.TS[str], value: hg.TS[int]):
                keyed.counted(name=name, price=value)

            hg.map_(
                per_key, value=keyed.priced(key=key, price=price), __key_arg__="name"
            )
            for lane, name in enumerate(("a", "started", "stopped")):
                record(keyed.stored(tick=probe, key=name), lane)

    with hg.GlobalState():
        hg.evaluate_graph(
            graph,
            hg.GraphConfiguration(
                start_time=hg.MIN_ST,
                end_time=hg.MIN_ST
                + (10 if case == "keyed" else 5) * repetitions * hg.MIN_TD,
            ),
        )
    return {
        "ticks": ticks,
        "ns": clock["stop"] - clock["start"],
        "outputs": [field for lane in lanes for field in lane],
    }


if __name__ == "__main__":
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("native_results", type=Path)
    cli.add_argument("output", type=Path)
    cli.add_argument("--samples", type=int, default=5)
    args = cli.parse_args()
    native = json.loads(args.native_results.read_text())
    scalar.test_replay_preserves_equal_ticks()
    keyed.test_a_dictionary_has_a_child_for_each_key()
    results = []
    for case in ("scalar", "keyed"):
        expected = next(r["cpp"] for r in native["rates"] if r["case"] == case)
        measure(case, 5)
        for _ in range(args.samples):
            sample = measure(case, native["repetitions"])
            if (sample["ticks"], sample["outputs"]) != (
                expected["ticks"],
                expected["outputs"],
            ):
                raise ValueError((case, sample, expected))
            results.append(dict(case=case, **sample))
            print(case, sample["ticks"] * 1e9 / sample["ns"], flush=True)
    args.output.write_text(json.dumps(results, indent=2) + "\n")
