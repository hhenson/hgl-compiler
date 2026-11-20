//! Evaluation-thread-only handles into HGraph. All input data passed at graph
//! construction is copied by the bridge. Callback views never outlive the call.
use std::{ffi::c_void, marker::PhantomData, ptr, rc::Rc, slice, str};
#[repr(u32)]
#[derive(Copy, Clone)]
/// Payload schemas accepted by the bridge. Discriminants match its C ABI.
pub enum Type {
    /// Signed 64-bit integer.
    Integer,
    /// UTF-8 string.
    Text,
    /// Boolean.
    Flag,
    /// Dictionary of string keys to integer values.
    Dictionary,
}
#[repr(C)]
/// A node view borrowed only for the current callback on the evaluation thread.
pub struct Frame {
    opaque: [u8; 0],
    thread: PhantomData<Rc<()>>,
}
#[repr(C)]
/// Prepared builders under synchronous construction on the calling thread.
pub struct Plan {
    opaque: [u8; 0],
    thread: PhantomData<Rc<()>>,
}
/// Lifecycle callback. Zero means success, other codes report an HGL failure.
pub type Hook = extern "C" fn(&Frame) -> i32;
/// Generated application function that supplies builders to the executor.
pub type Wire = extern "C" fn(&mut Plan);
/// Recorded (microsecond offset, integer publication) pairs.
pub type Trace = Vec<(i64, i64)>;
#[repr(C)]
#[derive(Copy, Clone)]
/// Logical child builder ID and destination input slot.
pub struct Port {
    /// Graph-local builder identifier.
    pub node: u32,
    /// Destination input ordinal.
    pub slot: u32,
}
#[repr(C)]
struct Entry {
    step: i64,
    integer: i64,
    text: *const u8,
    len: usize,
}
/// One owned-or-borrowed input value, copied into the native replay builder.
pub enum Scalar<'a> {
    /// Integer publication.
    Integer(i64),
    /// Text publication borrowed until builder construction returns.
    Text(&'a str),
    /// Boolean publication.
    Flag(bool),
}
unsafe extern "C" {
    fn hg_replay(p: &mut Plan, t: Type, entries: *const Entry, n: usize) -> u32;
    fn hg_node(
        p: &mut Plan,
        child: bool,
        inputs: *const Type,
        n: usize,
        out: *const Type,
        state: bool,
        start: Option<Hook>,
        eval: Hook,
        stop: Option<Hook>,
    ) -> u32;
    fn hg_edge(p: &mut Plan, child: bool, from: u32, to: u32, slot: u32);
    fn hg_each(p: &mut Plan, dict: u32, key: Port, value: Port);
    fn hg_record(p: &mut Plan, from: u32);
    fn hg_valid(f: &Frame, i: u32) -> bool;
    fn hg_modified(f: &Frame, i: u32) -> bool;
    fn hg_integer(f: &Frame, i: u32) -> i64;
    fn hg_flag(f: &Frame, i: u32) -> bool;
    fn hg_text(f: &Frame, i: u32, len: &mut usize) -> *const u8;
    fn hg_state(f: &Frame) -> i64;
    fn hg_set(f: &Frame, v: i64);
    fn hg_has(f: &Frame, s: *const u8, n: usize) -> bool;
    fn hg_get(f: &Frame, s: *const u8, n: usize) -> i64;
    fn hg_put(f: &Frame, s: *const u8, n: usize, v: i64);
    fn hg_publish(f: &Frame, v: i64);
    fn hg_publish_flag(f: &Frame, v: bool);
    fn hg_publish_text(f: &Frame, s: *const u8, n: usize);
    fn hg_update(f: &Frame, s: *const u8, n: usize, v: i64);
    fn hg_remove(f: &Frame, s: *const u8, n: usize);
    fn hg_run(w: Wire, ctx: *mut c_void, observe: extern "C" fn(*mut c_void, i64, i64)) -> i32;
}
impl Frame {
    /// Whether the input holds a value at evaluation time.
    pub fn valid(&self, i: u32) -> bool {
        unsafe { hg_valid(self, i) }
    }
    /// Whether the input published at evaluation time.
    pub fn modified(&self, i: u32) -> bool {
        unsafe { hg_modified(self, i) }
    }
    /// Read a held integer or its scalar delta.
    pub fn integer(&self, i: u32) -> i64 {
        unsafe { hg_integer(self, i) }
    }
    /// Read a held boolean or its scalar delta.
    pub fn flag(&self, i: u32) -> bool {
        unsafe { hg_flag(self, i) }
    }
    /// Borrow the UTF-8 payload from immutable input storage for this callback.
    pub fn text(&self, i: u32) -> &str {
        let mut n = 0;
        unsafe {
            let p = hg_text(self, i, &mut n);
            str::from_utf8(slice::from_raw_parts(p, n)).expect("invalid engine UTF-8")
        }
    }
    /// Read the integer cell of this node instance.
    pub fn state(&self) -> i64 {
        unsafe { hg_state(self) }
    }
    /// Assign the integer cell of this node instance.
    pub fn set(&self, v: i64) {
        unsafe { hg_set(self, v) }
    }
    /// Test the current run’s global store for a key.
    pub fn has(&self, key: &str) -> bool {
        unsafe { hg_has(self, key.as_ptr(), key.len()) }
    }
    /// Read an integer from the store or report an absent key (CAP-6).
    pub fn get(&self, key: &str) -> Result<i64, i32> {
        if self.has(key) {
            Ok(unsafe { hg_get(self, key.as_ptr(), key.len()) })
        } else {
            Err(2)
        }
    }
    /// Assign an integer to the store for this run.
    pub fn put(&self, key: &str, v: i64) {
        unsafe { hg_put(self, key.as_ptr(), key.len(), v) }
    }
    /// Publish an integer now, including equal consecutive values.
    pub fn publish(&self, v: i64) {
        unsafe { hg_publish(self, v) }
    }
    /// Publish a boolean now.
    pub fn publish_flag(&self, v: bool) {
        unsafe { hg_publish_flag(self, v) }
    }
    /// Copy text into the output and publish it now.
    pub fn publish_text(&self, v: &str) {
        unsafe { hg_publish_text(self, v.as_ptr(), v.len()) }
    }
    /// Publish one dictionary entry update.
    pub fn update(&self, key: &str, v: i64) {
        unsafe { hg_update(self, key.as_ptr(), key.len(), v) }
    }
    /// Publish the removal of one dictionary key.
    pub fn remove(&self, key: &str) {
        unsafe { hg_remove(self, key.as_ptr(), key.len()) }
    }
}
/// Checked i64 addition (NUM-2).
pub fn add(a: i64, b: i64) -> Result<i64, i32> {
    a.checked_add(b).ok_or(1)
}
/// Node schema and callbacks already specialised by the HGL compiler.
pub struct Node<'a> {
    /// Input types in callback slot order.
    pub inputs: &'a [Type],
    /// Compute result, or no result for a sink.
    pub output: Option<Type>,
    /// Allocate an integer cell for each node instance.
    pub state: bool,
    /// Initialisation and start hook.
    pub start: Option<Hook>,
    /// Admitted evaluation callback.
    pub eval: Hook,
    /// Hook run at instance disposal.
    pub stop: Option<Hook>,
}
impl Plan {
    /// Append a finite root source and return its logical ID.
    pub fn replay(&mut self, ty: Type, events: &[(i64, Scalar<'_>)]) -> u32 {
        let entries: Vec<_> = events
            .iter()
            .map(|(step, v)| {
                let mut entry = Entry {
                    step: *step,
                    integer: 0,
                    text: ptr::dangling(),
                    len: 0,
                };
                match v {
                    Scalar::Integer(i) => entry.integer = *i,
                    Scalar::Flag(b) => entry.integer = i64::from(*b),
                    Scalar::Text(s) => {
                        entry.text = s.as_ptr();
                        entry.len = s.len();
                    }
                }
                entry
            })
            .collect();
        unsafe { hg_replay(self, ty, entries.as_ptr(), entries.len()) }
    }
    /// Register a prepared node in the root or current child graph.
    pub fn node(&mut self, child: bool, n: Node<'_>) -> u32 {
        unsafe {
            hg_node(
                self,
                child,
                n.inputs.as_ptr(),
                n.inputs.len(),
                n.output.as_ref().map_or(ptr::null(), ptr::from_ref),
                n.state,
                n.start,
                n.eval,
                n.stop,
            )
        }
    }
    /// Connect the whole source output to one input slot.
    pub fn edge(&mut self, child: bool, from: u32, to: u32, slot: u32) {
        unsafe { hg_edge(self, child, from, to, slot) }
    }
    /// Finalise a dictionary child and attach its key, element and key-set routes.
    pub fn each(&mut self, d: u32, key: Port, value: Port) {
        unsafe { hg_each(self, d, key, value) }
    }
    /// Attach a sink that observes integer publications at their engine times.
    pub fn record(&mut self, from: u32) {
        unsafe { hg_record(self, from) }
    }
}
/// Each call constructs a fresh executor. The sink borrows the vector only while
/// the executor invokes it synchronously on its evaluation thread.
pub fn run(wire: Wire) -> Result<Trace, String> {
    extern "C" fn observe(ctx: *mut c_void, step: i64, value: i64) {
        unsafe { &mut *ctx.cast::<Trace>() }.push((step, value));
    }
    let mut trace = Trace::new();
    let status = unsafe { hg_run(wire, ptr::from_mut(&mut trace).cast(), observe) };
    if status == 0 {
        Ok(trace)
    } else {
        Err("HGraph execution failed".into())
    }
}

/// Print a committed CRUN-8 publication in the same UTC format as the CLI.
pub fn print_publication(step: i64, value: i64) {
    unsafe extern "C" {
        fn hg_print(step: i64, value: i64);
    }
    unsafe { hg_print(step, value) }
}
