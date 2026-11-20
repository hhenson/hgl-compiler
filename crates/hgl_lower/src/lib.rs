//! Lowering: the facts every target decides the same way.
//!
//! ```text
//! (analyser) --> analysed program --[hgl_lower]--> lowered facts --> (transpilers)
//! ```
//!
//! This stage sits where the pipeline has its optimiser, between the
//! analyser and the transpilers. It reads the analysed program and says
//! what follows from the language's rules alone, with no target's
//! knowledge: where a value is copied, which values a struct is, and
//! what a type holds. A transpiler asks it, and writes what the answers
//! mean on its target; no two targets work them out twice.
//!
//! It reads only the analysed program (`hgl_analyser`) and the
//! diagnostics model (`hgl_diagnostics`), and depends on no target.
//!
//! * [`copies`]: where a value is copied, and which structs are values
//!   (the owner's rule on the compiler's issue 151; FN-13, FN-15);
//! * [`instances`]: which instances of generic declarations a program
//!   asks for, and what an eval's run is wired with (GEN-2, CEVAL-1);
//! * [`calls`]: what a call gives a parameter it leaves out, and the
//!   value of a duration it is given (UNIT-8, TIME-8);
//! * [`canonical`]: the text that tells one instance from another;
//! * [`graphs`]: what a graph's branches and loops over connections
//!   take, bind and return, as child graphs (CTRL-7, CTRL-14);
//! * [`nodes`]: what a node's handlers subscribe to and require, and
//!   which of its locals are its inputs (TIME-3, TIME-4);
//! * [`pieces`]: what a piece of a program reaches, and which tests and
//!   applications reach a form a target does not write;
//! * [`types`]: what a type of the program holds, and which of its
//!   children `all_valid` asks (TYPE-3, TYPE-9, TIME-1, TIME-9).

pub mod calls;
pub mod canonical;
pub mod copies;
pub mod graphs;
pub mod instances;
pub mod nodes;
pub mod pieces;
pub mod types;
