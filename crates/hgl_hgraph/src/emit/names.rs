//! The names the hgraph target writes: an HGL name as a Python one, a
//! module's alias, a declaration's, an instance's and a candidate's
//! Python names, and a type's text where a name or a class is made of
//! it. Part of the transpiler, the last stage: `emit` writes a module
//! with them.

use super::*;

impl Emitter<'_> {
    /// The Python name of an instance of `declaration` of `module`:
    /// `_inst_`, the name the declaration would have, and the
    /// instance's generic arguments. The `_` first makes the name the
    /// target's own.
    pub(super) fn instance_name(&self, module: usize, declaration: usize, suffix: &str) -> String {
        format!(
            "_inst_{}__{suffix}",
            self.definition_name(module, declaration)
        )
    }

    /// The Python name of what a call of `declaration` of `module`
    /// calls: the function, or for a declaration written by instance,
    /// its instance for the call's generic arguments `generics`.
    pub(super) fn called_name(
        &mut self,
        (module, declaration): (usize, usize),
        generics: &[GenericArgument],
        at: Location,
    ) -> String {
        let program = self.program;
        if !self.instances.instanced(program, module, declaration) {
            return self.function_name(module, declaration);
        }
        let instances = self.instances;
        let Some(instance) = instances.find(program, (module, declaration), generics) else {
            self.unsupported(
                at,
                "a call of a generic temporal function with generic arguments that are not known",
            );
            return "None".to_string();
        };
        let python = self.instance_name(module, declaration, &instance.suffix);
        self.piece
            .uses
            .insert((module, declaration, Some(instance.suffix.clone())));
        if module == self.module {
            return python;
        }
        let path = program.modules[module].path.clone();
        let named = format!("{}.{python}", alias(&path));
        self.imports.insert(path);
        named
    }

    /// The Python name of the enum or the struct `declaration` of
    /// `module`. One of another module is named through that module, as
    /// a function is.
    pub(super) fn type_name(&mut self, module: usize, declaration: usize) -> String {
        let declared = &self.program.modules[module];
        let python = name(&declared.declarations[declaration].name);
        self.piece.uses.insert((module, declaration, None));
        if module == self.module {
            return python;
        }
        self.imports.insert(declared.path.clone());
        format!("{}.{python}", alias(&declared.path))
    }

    /// The Python name `declaration` of `module` is defined under.
    ///
    /// FN-4 lets one name have a temporal and a value definition: "
    /// Composition and eval choose the temporal definition if one
    /// exists; runtime/value evaluation chooses the value definition."
    /// Python has one function to a name, so the value definition of
    /// such a pair is `_value_name`, which no HGL name is written as.
    ///
    /// A candidate of an operator binds no name (UNIT-12), and an
    /// operator may have many. Each is `_impl_` and then the operator
    /// and the types of the candidate's parameters:
    /// `_impl_add__f64_i64`. The `_` first makes the name the target's
    /// own, as no name of a program is.
    ///
    /// GEN-5 lets an operator have a temporal and a value candidate "of
    /// the same parameter types", as FN-4 lets a name have both. The
    /// value one of such a pair is `_value_impl_add__f64_i64`.
    pub(super) fn definition_name(&self, module: usize, index: usize) -> String {
        let declared = &self.program.modules[module];
        let declaration = &declared.declarations[index];
        if let DeclarationKind::Implementation(role, _) = declaration.kind {
            // The candidate `index` of the module as it is named.
            let named = |index: usize| -> String {
                let types: Vec<String> = match declared.details.get(index) {
                    Some(Detail::Callable(signature)) => signature
                        .parameters
                        .iter()
                        .map(|parameter| mangled(&parameter.ty))
                        .collect(),
                    _ => Vec::new(),
                };
                let operator = &declared.declarations[index].name;
                format!("_impl_{operator}_{}", types.join("_"))
            };
            let python = named(index);
            let paired = role == Role::Value
                && declared
                    .declarations
                    .iter()
                    .enumerate()
                    .any(|(other, candidate)| {
                        matches!(
                            candidate.kind,
                            DeclarationKind::Implementation(Role::Temporal, _)
                        ) && named(other) == python
                    });
            return if paired {
                format!("_value{python}")
            } else {
                python
            };
        }
        let python = name(&declaration.name);
        let paired = declaration.kind == DeclarationKind::Function(Role::Value)
            && declared.declarations.iter().any(|other| {
                other.name == declaration.name
                    && other.kind == DeclarationKind::Function(Role::Temporal)
            });
        if paired {
            format!("_value_{python}")
        } else {
            python
        }
    }

    /// The Python name of the function `declaration` of `module`. One
    /// of another module is named through that module, which is
    /// imported under a name of its own: two modules may each have a
    /// `pick`, and this one a `pick` too.
    pub(super) fn function_name(&mut self, module: usize, declaration: usize) -> String {
        let declared = &self.program.modules[module];
        let python = self.definition_name(module, declaration);
        self.piece.uses.insert((module, declaration, None));
        if module == self.module {
            return python;
        }
        self.imports.insert(declared.path.clone());
        format!("{}.{python}", alias(&declared.path))
    }
}

/// Names as a Python tuple of strings: `("a", "b")`.
pub(super) fn tuple(names: &[String]) -> String {
    let quoted: Vec<String> = names.iter().map(|name| format!("\"{name}\"")).collect();
    // A tuple of one needs its comma.
    format!(
        "({}{})",
        quoted.join(", "),
        if quoted.len() == 1 { "," } else { "" }
    )
}

/// The words a generated module leaves to Python: its keywords. A
/// built-in name such as `float` is no keyword, and a program may use
/// it: the target reaches Python's built-ins through a name of its
/// own, `_py`, and never by their bare names.
const PYTHON_WORDS: [&str; 37] = [
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield", "match", "case",
];

/// An HGL name as a Python one. Most are written as they are. The
/// mapping gives no two HGL names one Python name, and gives none a
/// name the target uses itself:
///
/// - A Python word, and one followed by any number of `_`, gets one
///   more `_`: `lambda` is `lambda_`, and `lambda_` is `lambda__`.
/// - Every name the target makes up starts with `_`, and a test is a
///   function `test_name`. So a name that starts with `_` or `test_`,
///   under any number of `hgl_` before it, gets one more `hgl_`:
///   `_kept` is `hgl__kept`, and `hgl__kept` is `hgl_hgl__kept`.
pub(crate) fn name(written: &str) -> String {
    if PYTHON_WORDS.contains(&written.trim_end_matches('_')) {
        return format!("{written}_");
    }
    let mut rest = written;
    while let Some(inner) = rest.strip_prefix("hgl_") {
        rest = inner;
    }
    if rest.starts_with('_') || rest.starts_with("test_") {
        return format!("hgl_{written}");
    }
    written.to_string()
}

/// The name of an enum's member as Python's `Enum` has it. A member is
/// read by name, `Side["buy"]`, so any word is one, a Python keyword
/// among them. `Enum` keeps `mro` and the names that start and end with
/// `_` for itself: a name that starts with `_` or is `mro`, under any
/// number of `hgl_`, gets one more `hgl_`. No two names give one, as
/// for [`name`]. The provider's `member` does the same for a member
/// constructed from its name.
pub(super) fn member_name(written: &str) -> String {
    let mut rest = written;
    while let Some(inner) = rest.strip_prefix("hgl_") {
        rest = inner;
    }
    if rest.starts_with('_') || rest == "mro" {
        return format!("hgl_{written}");
    }
    written.to_string()
}

/// The name a generated module imports the module `path` under:
/// `_hgl_4demo_4util` for `demo.util`. Each part of the path has its
/// length before it, so that two paths never give one name, whatever
/// `_` they hold: `a_.b` is `_hgl_2a__1b`, and `a._b` is `_hgl_1a_2_b`.
pub(super) fn alias(path: &str) -> String {
    let parts: Vec<String> = path
        .split('.')
        .map(|part| format!("{}{part}", part.len()))
        .collect();
    format!("_hgl_{}", parts.join("_"))
}

/// A string as a Python literal.
pub(super) fn string(text: &str) -> String {
    let mut out = String::from("\"");
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // Any other control character, by its number.
            control if control.is_control() => {
                out.push_str(&format!("\\u{:04x}", control as u32));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// A type as part of a native function's name: `i64` as it is, and any
/// other character than a letter or a digit as `_`.
pub(super) fn mangled(ty: &Type) -> String {
    ty.to_string()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect()
}

/// The name a child graph of `switch_` or `map_` takes the connection
/// named `python` by. hgraph's `switch_` and `map_` keep some names for
/// their own parameters, and those of the function they wire with:
/// `switch_(key, cases, ..., reload_on_ticked)`, `map_(func, ...)` and
/// `wire(name, ..., tp, output_type)`; and a branch of `switch_` whose
/// first parameter is `key` is given the selector there. A connection
/// of one of those names is taken under a name of the target's own,
/// `_taken_name`, which the child's first lines give it back
/// (`Emitter::renamed`).
pub(super) fn taken_name(python: &str) -> String {
    const KEPT: [&str; 7] = [
        "key",
        "cases",
        "reload_on_ticked",
        "func",
        "name",
        "tp",
        "output_type",
    ];
    if KEPT.contains(&python) {
        format!("_taken_{python}")
    } else {
        python.to_string()
    }
}

/// A type as text that no other type is written as, for the class of a
/// generic struct's application (see `Emitter::construct`). A named
/// type has its module's path before it: two modules may each declare
/// a `Quote`. `None` for a type that is not whole as the module is
/// written: one that names a generic parameter, or is not known.
pub(super) fn canonical(program: &Program, ty: &Type) -> Option<String> {
    let one = |ty: &Type| canonical(program, ty);
    Some(match ty {
        Type::Scalar(scalar) => scalar.as_str().to_string(),
        Type::Named {
            name,
            module,
            arguments,
            ..
        } => {
            let path = &program.modules[*module].path;
            if arguments.is_empty() {
                format!("{path}::{name}")
            } else {
                let arguments: Option<Vec<String>> = arguments
                    .iter()
                    .map(|argument| canonical_argument(program, argument))
                    .collect();
                format!("{path}::{name}<{}>", arguments?.join(", "))
            }
        }
        Type::Tuple(elements) => {
            let elements: Option<Vec<String>> = elements.iter().map(one).collect();
            format!("tuple<{}>", elements?.join(", "))
        }
        Type::List { element, size } => match size {
            Some(size) => format!("list<{}, {}>", one(element)?, canonical_constant(size)?),
            None => format!("list<{}>", one(element)?),
        },
        Type::Set(element) => format!("set<{}>", one(element)?),
        Type::Dict { key, value } => format!("dict<{}, {}>", one(key)?, one(value)?),
        Type::RollingWindow {
            value,
            maximum,
            minimum,
        } => match minimum {
            Some(minimum) => format!(
                "rolling_window<{}, {}, {}>",
                one(value)?,
                canonical_constant(maximum)?,
                canonical_constant(minimum)?
            ),
            None => format!(
                "rolling_window<{}, {}>",
                one(value)?,
                canonical_constant(maximum)?
            ),
        },
        Type::Ref(target) => format!("ref<{}>", one(target)?),
        Type::Endpoint(target) => format!("endpoint<{}>", one(target)?),
        Type::Signal => "signal".to_string(),
        Type::Atomic(payload) => format!("atomic<{}>", one(payload)?),
        Type::Result { value, error } => format!("result<{}, {}>", one(value)?, one(error)?),
        Type::Error => "error".to_string(),
        Type::ErrorContext => "error context".to_string(),
        Type::Delta(shape) => format!("delta<{}>", one(shape)?),
        Type::Nullable(inner) => format!("{}?", one(inner)?),
        Type::Parameter { .. } | Type::Trace { .. } | Type::Unknown => return None,
    })
}

/// A generic argument as [`canonical`] text.
pub(crate) fn canonical_argument(program: &Program, argument: &GenericArgument) -> Option<String> {
    match argument {
        GenericArgument::Type(ty) => canonical(program, ty),
        GenericArgument::Constant(constant) => canonical_constant(constant),
    }
}

/// A constant inside a type as [`canonical`] text: what it comes to,
/// however it was written. `None` for one whose value is not known as
/// the module is written.
fn canonical_constant(constant: &Constant) -> Option<String> {
    Some(match &constant.value {
        ConstantValue::Int(number) => number.to_string(),
        ConstantValue::Duration(microseconds) => format!("{microseconds}us"),
        ConstantValue::Unbounded => "unbounded".to_string(),
        ConstantValue::Member(member) => format!(".{member}"),
        // A literal of another kind is held as it is written, and
        // compared without the spaces between its tokens.
        ConstantValue::Other(_) => constant.unspaced(),
        ConstantValue::Parameter { .. } | ConstantValue::Unevaluated => return None,
    })
}

/// The name of a temporal tuple's field for the position `index`, in
/// hgraph's bundle with no name that the tuple is (`tuple_annotation`):
/// `_0`, `_1`. A tuple's fields are its positions, and no name of the
/// program's.
pub(super) fn position(index: usize) -> String {
    format!("_{index}")
}

/// An endpoint of a node, an input or its `inject out`, as the node's
/// code names it: an input by its name, and the node's own output as
/// hgraph's `_output`.
pub(super) fn endpoint_name(endpoint: &Local) -> String {
    if endpoint.kind == LocalKind::Inject {
        "_output".to_string()
    } else {
        name(&endpoint.name)
    }
}
