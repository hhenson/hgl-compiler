//! Hand-written scalar and keyed child bodies.
use facade::Frame;
const BODY_BIAS: i64 = 0;
/// Return the application parameter used for reproducible interface edits.
pub fn wire_offset(base: u32) -> u32 {
    base
}
/// Count admitted publications, including equal values.
pub extern "C" fn count(f: &Frame, _: u32) {
    let next = f.state() + 1;
    f.set_state(next);
    f.publish(next);
}
/// Forward a scalar publication.
pub extern "C" fn relay(f: &Frame, offset: u32) {
    f.publish(f.integer(0) + i64::from(offset) + BODY_BIAS);
}
/// Translate a key and price to an HGL dictionary delta.
pub extern "C" fn priced(f: &Frame, _: u32) {
    let key = f.text(0);
    let price = f.integer(1);
    if price < 0 {
        f.remove(key);
    } else {
        f.update(key, price);
    }
}
/// Increment the child start count.
pub extern "C" fn start(f: &Frame, _: u32) {
    f.put(b"started", f.get(b"started") + 1);
}
/// Count evaluations in this child and expose the count for key a.
pub extern "C" fn child(f: &Frame, _: u32) {
    let seen = f.state() + 1;
    f.set_state(seen);
    if f.text(0) == b"a" {
        f.put(b"a", seen);
    }
}
/// Increment the child stop count.
pub extern "C" fn stop(f: &Frame, _: u32) {
    f.put(b"stopped", f.get(b"stopped") + 1);
}
/// Sample one of the three keyed observations.
pub extern "C" fn read_store(f: &Frame, lane: u32) {
    let keys: [&[u8]; 3] = [b"a", b"started", b"stopped"];
    f.publish(f.get(keys[(lane % 3) as usize]) + i64::from(lane / 3) + BODY_BIAS);
}
