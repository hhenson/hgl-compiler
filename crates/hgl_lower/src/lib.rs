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
//! * [`canonical`]: the text that tells one instance from another;
//! * [`types`]: what a type of the program holds (TYPE-3).

pub mod canonical;
pub mod copies;
pub mod instances;
pub mod types;
