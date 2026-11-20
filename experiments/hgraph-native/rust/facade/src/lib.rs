//! Borrowed, evaluation-thread-only access to prepared builders and native node views.
use std::{
    ffi::{CString, c_char},
    marker::PhantomData,
    ptr,
    rc::Rc,
    slice,
};

#[repr(u32)]
#[derive(Clone, Copy)]
/// Endpoint schemas required by the two fixtures.
pub enum Type {
    Integer,
    Text,
    Flag,
    Dictionary,
}
#[repr(u32)]
/// Replay streams supplied by the common measurement driver.
pub enum Input {
    Count,
    Relay,
    Key,
    Price,
    Probe,
}
#[repr(C)]
/// An opaque HGraph callback view borrowed on the evaluating thread.
pub struct Frame {
    opaque: [u8; 0],
    thread: PhantomData<Rc<()>>,
}
#[repr(C)]
/// A graph under synchronous construction. Graph 0 is the root and graph 1 is the keyed child.
pub struct Plan {
    opaque: [u8; 0],
    thread: PhantomData<Rc<()>>,
}
/// A child node and its input slot, before attaching it to a parent map.
#[repr(C)]
pub struct ChildPort {
    pub node: u32,
    pub slot: u32,
}
/// A node lifecycle or evaluation callback. Panics terminate the benchmark process.
pub type Hook = extern "C" fn(&Frame, u32);
/// Application graph assembly called once before the executor timer starts.
pub type Wire = extern "C" fn(&mut Plan, bool, u32);
#[link(name = "bench_runtime")]
unsafe extern "C" {
    fn hg_source(p: &mut Plan, i: Input) -> u32;
    fn hg_node(
        p: &mut Plan,
        g: u32,
        inputs: *const Type,
        len: usize,
        out: *const Type,
        state: bool,
        eval: Hook,
        start: Option<Hook>,
        stop: Option<Hook>,
        arg: u32,
    ) -> u32;
    fn hg_edge(p: &mut Plan, g: u32, a: u32, b: u32, slot: u32);
    fn hg_map(p: &mut Plan, key: ChildPort, value: ChildPort) -> u32;
    fn hg_keys(p: &mut Plan, a: u32, b: u32);
    fn hg_record(p: &mut Plan, a: u32, lane: u32);
    fn hg_read(f: &Frame, slot: u32) -> i64;
    fn hg_text(f: &Frame, slot: u32, len: &mut usize) -> *const u8;
    fn hg_state(f: &Frame) -> i64;
    fn hg_set_state(f: &Frame, v: i64);
    fn hg_get(f: &Frame, key: *const u8, n: usize) -> i64;
    fn hg_put(f: &Frame, key: *const u8, n: usize, v: i64);
    fn hg_publish(f: &Frame, v: i64);
    fn hg_update(f: &Frame, key: *const u8, n: usize, v: i64);
    fn hg_remove(f: &Frame, key: *const u8, n: usize);
    fn hg_run(n: i32, args: *const *const c_char, wire: Wire) -> i32;
}
impl Frame {
    /// Read a scalar integer input at the indicated bundle slot.
    pub fn integer(&self, slot: u32) -> i64 {
        unsafe { hg_read(self, slot) }
    }
    /// Borrow a text input for this callback.
    pub fn text(&self, slot: u32) -> &[u8] {
        let mut n = 0;
        // The native input string is immutable for this callback. Output operations
        // affect distinct downstream storage. The slice cannot outlive the frame.
        unsafe {
            let p = hg_text(self, slot, &mut n);
            slice::from_raw_parts(p, n)
        }
    }
    /// Read this node instance’s scalar state.
    pub fn state(&self) -> i64 {
        unsafe { hg_state(self) }
    }
    /// Replace the scalar state of this node instance.
    pub fn set_state(&self, v: i64) {
        unsafe { hg_set_state(self, v) }
    }
    /// Read a benchmark store counter, initially zero.
    pub fn get(&self, k: &[u8]) -> i64 {
        unsafe { hg_get(self, k.as_ptr(), k.len()) }
    }
    /// Write a counter in the run-wide HGraph store.
    pub fn put(&self, k: &[u8], v: i64) {
        unsafe { hg_put(self, k.as_ptr(), k.len(), v) }
    }
    /// Publish an integer in this cycle.
    pub fn publish(&self, v: i64) {
        unsafe { hg_publish(self, v) }
    }
    /// Publish an update to one dictionary key.
    pub fn update(&self, k: &[u8], v: i64) {
        unsafe { hg_update(self, k.as_ptr(), k.len(), v) }
    }
    /// Publish removal of one dictionary key.
    pub fn remove(&self, k: &[u8]) {
        unsafe { hg_remove(self, k.as_ptr(), k.len()) }
    }
}
impl Plan {
    /// Append one input replay builder and return its graph-local index.
    pub fn source(&mut self, i: Input) -> u32 {
        unsafe { hg_source(self, i) }
    }
    /// Append a prepared node with its callback, schemas and optional lifecycle hooks.
    #[expect(
        clippy::too_many_arguments,
        reason = "The probe mirrors the direct C++ constructor for the timing comparison."
    )]
    pub fn node(
        &mut self,
        g: u32,
        inputs: &[Type],
        out: Option<Type>,
        state: bool,
        eval: Hook,
        start: Option<Hook>,
        stop: Option<Hook>,
        arg: u32,
    ) -> u32 {
        unsafe {
            hg_node(
                self,
                g,
                inputs.as_ptr(),
                inputs.len(),
                out.as_ref().map_or(ptr::null(), ptr::from_ref),
                state,
                eval,
                start,
                stop,
                arg,
            )
        }
    }
    /// Bind an ordinary output to a target input slot.
    pub fn edge(&mut self, g: u32, a: u32, b: u32, slot: u32) {
        unsafe { hg_edge(self, g, a, b, slot) }
    }
    /// Build the sink map with explicit key and value destinations in the child.
    pub fn map(&mut self, key: ChildPort, value: ChildPort) -> u32 {
        unsafe { hg_map(self, key, value) }
    }
    /// Bind the dictionary key-set projection to the map’s lifecycle input.
    pub fn keys(&mut self, a: u32, b: u32) {
        unsafe { hg_keys(self, a, b) }
    }
    /// Attach the common timestamped checksum sink.
    pub fn record(&mut self, a: u32, lane: u32) {
        unsafe { hg_record(self, a, lane) }
    }
}
/// Run the wired case and verify its recorded output.
pub fn run(wire: Wire) -> i32 {
    let args: Vec<_> = std::env::args().map(|s| CString::new(s).unwrap()).collect();
    let pointers: Vec<_> = args.iter().map(|s| s.as_ptr()).collect();
    unsafe { hg_run(pointers.len() as i32, pointers.as_ptr(), wire) }
}
