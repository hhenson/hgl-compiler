//! What a type of the program holds, as the language says it: a
//! struct's fields and its descendants. Part of lowering: a transpiler
//! asks it what a type is before it writes the type's form.

use hgl_analyser::{Detail, Program, Type};

/// True where another struct of the program inherits from the struct
/// `declaration` of `module`: a value of that type may be of the other
/// struct, with fields this one has not.
pub fn inherited(program: &Program, module: usize, declaration: usize) -> bool {
    program.modules.iter().any(|module_of| {
        module_of.details.iter().any(|detail| {
            matches!(detail, Detail::Struct(other) if other.parents.iter().any(|parent| {
                matches!(parent, Type::Named { module: parent_module, declaration: parent_declaration, .. }
                    if *parent_module == module && *parent_declaration == declaration)
            }))
        })
    })
}
