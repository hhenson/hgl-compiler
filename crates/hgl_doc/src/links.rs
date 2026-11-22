//! Finds declarations named by code spans in their source file (SDOC-8).
//! The analysed module scopes provide the bindings. An unresolved span
//! stays code and produces no diagnostic.

use hgl_analyser::{Declaration, Detail, ImportKind, Module, ModuleFile, Program};
use hgl_diagnostics::Location;
use hgl_parser::HasSpan;

use crate::LinkContext;

/// Returns the location of the name or member a documentation span denotes.
pub(crate) fn resolve(program: &Program, context: LinkContext<'_>, name: &str) -> Option<Location> {
    let module = program
        .modules
        .iter()
        .find(|module| module.path == context.module)?;
    let origin = module
        .files
        .iter()
        .position(|file| file.file == context.at.file)?;
    let file = &module.files[origin];

    if let Some((alias, subject)) = name.split_once("::") {
        let import = file
            .imports
            .iter()
            .find(|import| import.kind == ImportKind::Alias && import.name == alias)?;
        let imported = &program.modules[import.module?];
        return subject_location(subject, |head| {
            public(imported, head).map(|item| (imported, item))
        });
    }

    let enclosing = module
        .declarations
        .iter()
        .enumerate()
        .find(|(_, declaration)| {
            declaration.file == origin
                && declaration.syntax.span().start <= context.at.span.start
                && context.at.span.end <= declaration.syntax.span().end
        });
    let head = name.split('.').next()?;
    if enclosing
        .and_then(|(index, _)| module.details.get(index))
        .is_some_and(|detail| parameter_name(detail, head))
    {
        return None;
    }
    let in_test = enclosing.is_some_and(|(_, declaration)| declaration.test_only)
        || module.tests.iter().any(|test| test.at == context.at);
    if let Some(location) =
        subject_location(name, |name| lookup(program, module, file, name, in_test))
    {
        return Some(location);
    }

    // SDOC-8 also accepts spelling a module path directly with dots.
    // Try the longest visible module path before looking up its subject.
    for (dot, _) in name.rmatch_indices('.') {
        let prefix = &name[..dot];
        let Some(target) = program
            .modules
            .iter()
            .position(|module| module.path == prefix)
        else {
            continue;
        };
        let target_module = &program.modules[target];
        let same = target_module.path == module.path;
        if same
            || Some(target) == program.core()
            || file
                .imports
                .iter()
                .any(|import| import.module == Some(target))
        {
            let subject = &name[dot + 1..];
            return subject_location(subject, |name| {
                if same {
                    local(module, name, in_test).map(|item| (module, item))
                } else {
                    public(target_module, name).map(|item| (target_module, item))
                }
            });
        }
    }
    None
}

fn parameter_name(detail: &Detail, name: &str) -> bool {
    match detail {
        Detail::Callable(signature) => {
            signature
                .parameters
                .iter()
                .any(|parameter| parameter.name == name)
                || signature
                    .generics
                    .iter()
                    .any(|generic| generic.name == name)
        }
        Detail::Struct(shape) => shape.generics.iter().any(|generic| generic.name == name),
        _ => false,
    }
}

fn lookup<'a>(
    program: &'a Program,
    module: &'a Module,
    file: &ModuleFile,
    name: &str,
    in_test: bool,
) -> Option<(&'a Module, &'a Declaration)> {
    if let Some(import) = file.imports.iter().find(|import| import.name == name) {
        if import.kind != ImportKind::Selective {
            return None;
        }
        let target = &program.modules[import.module?];
        return public(target, name).map(|item| (target, item));
    }
    if module.lookup(name).next().is_some() {
        return local(module, name, in_test).map(|item| (module, item));
    }
    let core = &program.modules[program.core()?];
    public(core, name).map(|item| (core, item))
}

fn local<'a>(module: &'a Module, name: &str, in_test: bool) -> Option<&'a Declaration> {
    unique(
        module
            .lookup(name)
            .filter(|declaration| in_test || !declaration.test_only),
    )
}

fn public<'a>(module: &'a Module, name: &str) -> Option<&'a Declaration> {
    unique(
        module
            .lookup(name)
            .filter(|declaration| declaration.is_public()),
    )
}

fn unique<'a>(mut declarations: impl Iterator<Item = &'a Declaration>) -> Option<&'a Declaration> {
    let declaration = declarations.next()?;
    // An unqualified code span gives no call form to choose between
    // the declarations of an FN-4 pair.
    declarations.next().is_none().then_some(declaration)
}

fn subject_location<'a>(
    subject: &str,
    find: impl FnOnce(&str) -> Option<(&'a Module, &'a Declaration)>,
) -> Option<Location> {
    let (head, member) = subject
        .split_once('.')
        .map_or((subject, None), |(head, member)| (head, Some(member)));
    let (module, declaration) = find(head)?;
    let Some(member) = member else {
        return Some(declaration.at);
    };
    let index = module
        .declarations
        .iter()
        .position(|item| std::ptr::eq(item, declaration))?;
    match module.details.get(index)? {
        Detail::Struct(shape) => shape
            .members
            .iter()
            .find(|candidate| candidate.name == member)
            .map(|candidate| candidate.at),
        Detail::Enum(shape) => shape
            .members
            .iter()
            .find(|candidate| candidate.name == member)
            .map(|candidate| candidate.at),
        Detail::Native(shape) => shape
            .properties
            .iter()
            .find(|candidate| candidate.name == member)
            .map(|candidate| candidate.at),
        _ => None,
    }
}
