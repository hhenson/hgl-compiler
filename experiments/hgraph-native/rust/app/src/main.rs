//! Assemble the prepared node builders and edges in known rank order.
use facade::{Input, Plan, Type::*};
extern "C" fn wire(p: &mut Plan, keyed: bool, base: u32) {
    const WIRING_BIAS: u32 = 0;
    let offset = nodes::wire_offset(base) + WIRING_BIAS;
    if !keyed {
        let a = p.source(Input::Count);
        let r = p.source(Input::Relay);
        let count = p.node(
            0,
            &[Integer],
            Some(Integer),
            true,
            nodes::count,
            None,
            None,
            0,
        );
        let relay = p.node(
            0,
            &[Integer],
            Some(Integer),
            false,
            nodes::relay,
            None,
            None,
            offset,
        );
        p.edge(0, a, count, 0);
        p.edge(0, r, relay, 0);
        p.record(count, 0);
        p.record(relay, 1);
        return;
    }
    let key = p.source(Input::Key);
    let price = p.source(Input::Price);
    let probe = p.source(Input::Probe);
    let prices = p.node(
        0,
        &[Text, Integer],
        Some(Dictionary),
        false,
        nodes::priced,
        None,
        None,
        0,
    );
    p.edge(0, key, prices, 0);
    p.edge(0, price, prices, 1);
    let child = p.node(
        1,
        &[Text, Integer],
        None,
        true,
        nodes::child,
        Some(nodes::start),
        Some(nodes::stop),
        0,
    );
    let map = p.map(
        facade::ChildPort {
            node: child,
            slot: 0,
        },
        facade::ChildPort {
            node: child,
            slot: 1,
        },
    );
    p.edge(0, prices, map, 0);
    p.keys(prices, map);
    for lane in 0..3 {
        let read = p.node(
            0,
            &[Flag],
            Some(Integer),
            false,
            nodes::read_store,
            None,
            None,
            lane + 3 * offset,
        );
        p.edge(0, probe, read, 0);
        p.record(read, lane);
    }
}
fn main() {
    std::process::exit(facade::run(wire));
}
