//! Resolves what `use` lines name, and the operators of `impl fn`.
//!
//! This pass runs after every module's scope is built, because an import
//! reads another module's scope.

use std::collections::{HashMap, HashSet};

use hgl_diagnostics::{Category, Diagnostics, Location};
use petgraph::algo::kosaraju_scc;
use petgraph::graph::{DiGraph, NodeIndex};

use crate::program::{DeclarationKind, ImportKind, Module, OperatorRef, PreludeName, Program};
use crate::{RESULT_FORMS, codes, error};

/// Resolves the imports of every module and reports what is wrong with
/// them, then finds the operator of each `impl fn`.
pub(crate) fn resolve(program: &mut Program, diagnostics: &mut Diagnostics) {
    find_modules(program, diagnostics);
    for index in 0..program.modules.len() {
        check_imported_names(program, index, diagnostics);
        check_names_in_files(&program.modules[index], diagnostics);
        check_declarations_against_imports(&program.modules[index], diagnostics);
        check_names_against_prelude(program, index, diagnostics);
        bind_implementations(program, index, diagnostics);
    }
    check_cycles(program, diagnostics);
}

/// UNIT-2: finds the module each import names. One that no file declares
/// is reported once for its `use` line.
fn find_modules(program: &mut Program, diagnostics: &mut Diagnostics) {
    // Looked up before the loop below, which changes the modules and so
    // cannot also read them.
    let positions: HashMap<String, usize> = program
        .modules
        .iter()
        .enumerate()
        .map(|(index, module)| (module.path.clone(), index))
        .collect();
    // UNIT-14: "no `use` names" an application.
    let applications: HashSet<usize> = program
        .modules
        .iter()
        .enumerate()
        .filter(|(_, module)| module.application)
        .map(|(index, _)| index)
        .collect();
    // The `use` lines already reported, by where their path is written.
    let mut reported: HashSet<Location> = HashSet::new();
    for module in &mut program.modules {
        for file in &mut module.files {
            for import in &mut file.imports {
                import.module = positions.get(&import.from).copied();
                // An application is imported from as no module is: the
                // names of the line are bound to nothing.
                if import
                    .module
                    .is_some_and(|found| applications.contains(&found))
                {
                    import.module = None;
                    if reported.insert(import.from_at) {
                        diagnostics.emit(
                            error(
                                Category::Module,
                                codes::APPLICATION_IMPORT,
                                import.from_at,
                                format!("`{}` is an application, and is not imported", import.from),
                                "UNIT-14",
                            )
                            .with_note("an application's declarations are private to it"),
                        );
                    }
                    continue;
                }
                // `insert` is true the first time a value is added.
                if import.module.is_none() && reported.insert(import.from_at) {
                    diagnostics.emit(
                        error(
                            Category::Module,
                            codes::IMPORT_UNKNOWN_MODULE,
                            import.from_at,
                            format!("no source file declares module `{}`", import.from),
                            "UNIT-2",
                        )
                        .with_note(
                            "a module is found by its `module` line, in the source roots given",
                        ),
                    );
                }
            }
        }
    }
}

/// UNIT-2, UNIT-3: a selective import names a public declaration of its
/// module.
fn check_imported_names(program: &Program, index: usize, diagnostics: &mut Diagnostics) {
    for file in &program.modules[index].files {
        for import in &file.imports {
            if import.kind != ImportKind::Selective {
                continue;
            }
            // An unknown module has been reported; its names are not.
            let Some(target) = import.module else {
                continue;
            };
            let target = &program.modules[target];
            let mut declared = target.lookup(&import.name).peekable();
            if declared.peek().is_none() {
                diagnostics.emit(error(
                    Category::Name,
                    codes::IMPORT_UNKNOWN_NAME,
                    import.at,
                    format!("module `{}` declares no `{}`", target.path, import.name),
                    "UNIT-2",
                ));
            } else if let Some(private) = declared.find(|declaration| !declaration.is_public())
                // Of a temporal and value pair, one exported is enough:
                // the import binds the public declarations of the name.
                && !target.lookup(&import.name).any(|declaration| declaration.is_public())
            {
                let note = if private.test_only {
                    "it is declared for the module's tests (TST-2)"
                } else {
                    "a function, a type or a constant is private unless marked `export` (UNIT-3)"
                };
                diagnostics.emit(
                    error(
                        Category::Module,
                        codes::IMPORT_PRIVATE,
                        import.at,
                        format!("`{}` is private to module `{}`", import.name, target.path),
                        "UNIT-2",
                    )
                    .with_related(private.at, "declared here")
                    .with_note(note),
                );
            }
        }
    }
}

/// UNIT-12: within a file, a `use` binds no name another `use` has
/// bound.
fn check_names_in_files(module: &Module, diagnostics: &mut Diagnostics) {
    for file in &module.files {
        let mut first: HashMap<&str, Location> = HashMap::new();
        for import in &file.imports {
            match first.get(import.name.as_str()) {
                Some(&earlier) => diagnostics.emit(
                    error(
                        Category::Name,
                        codes::NAME_DUPLICATE,
                        import.at,
                        format!("`{}` is imported twice in this file", import.name),
                        "UNIT-12",
                    )
                    .with_related(earlier, "first imported here"),
                ),
                None => {
                    first.insert(&import.name, import.at);
                }
            }
        }
    }
}

/// UNIT-12: "A declaration takes no name its module imports", in any of
/// the module's files. The declaration is the one reported, wherever the
/// import stands.
fn check_declarations_against_imports(module: &Module, diagnostics: &mut Diagnostics) {
    // Each imported name, with the first place it is imported.
    let mut imported: HashMap<&str, Location> = HashMap::new();
    for file in &module.files {
        for import in &file.imports {
            imported.entry(&import.name).or_insert(import.at);
        }
    }
    for declaration in &module.declarations {
        if !declaration.kind.binds_name() {
            continue;
        }
        let Some(&import) = imported.get(declaration.name.as_str()) else {
            continue;
        };
        let rule = if declaration.test_only {
            "TST-2"
        } else {
            "UNIT-12"
        };
        diagnostics.emit(
            error(
                Category::Name,
                codes::NAME_DUPLICATE,
                declaration.at,
                format!(
                    "`{}` is declared in module `{}`, which imports that name",
                    declaration.name, module.path
                ),
                rule,
            )
            .with_related(import, "imported here"),
        );
    }
}

/// UNIT-12: "outside core's mandatory module neither a declaration nor
/// an import alias takes a prelude name."
fn check_names_against_prelude(program: &Program, index: usize, diagnostics: &mut Diagnostics) {
    check_names_against_result_forms(&program.modules[index], diagnostics);
    let Some(core) = program.core().filter(|core| *core != index) else {
        return;
    };
    let core = &program.modules[core];
    // The public declaration of core that has `name`, when one has.
    let prelude = |name: &str| {
        core.lookup(name)
            .find(|declaration| declaration.is_public())
    };
    let module = &program.modules[index];
    for declaration in &module.declarations {
        if !declaration.kind.binds_name() {
            continue;
        }
        if let Some(bound) = prelude(&declaration.name) {
            diagnostics.emit(
                error(
                    Category::Name,
                    codes::NAME_DUPLICATE,
                    declaration.at,
                    format!(
                        "`{}` is declared in module `{}`, and the prelude binds that name",
                        declaration.name, module.path
                    ),
                    "UNIT-12",
                )
                .with_related(bound.at, "declared here, in core's module"),
            );
        }
    }
    for file in &module.files {
        for import in &file.imports {
            if import.kind != ImportKind::Alias {
                continue;
            }
            if let Some(bound) = prelude(&import.name) {
                diagnostics.emit(
                    error(
                        Category::Name,
                        codes::NAME_DUPLICATE,
                        import.at,
                        format!("the alias `{}` takes a name the prelude binds", import.name),
                        "UNIT-12",
                    )
                    .with_related(bound.at, "declared here, in core's module"),
                );
            }
        }
    }
}

/// ERR-13: "`ok`, `err`, `is_ok`, `is_err`, and `capture` are
/// contextual names ... No declaration, import alias, parameter, or
/// local takes one of them (UNIT-12)". Core's module is held to it as
/// any other is.
fn check_names_against_result_forms(module: &Module, diagnostics: &mut Diagnostics) {
    let mut report = |at: Location, name: &str, what: &str| {
        diagnostics.emit(
            error(
                Category::Name,
                codes::NAME_DUPLICATE,
                at,
                format!("`{name}` is the name of a result form, and nothing else takes it"),
                "ERR-13",
            )
            .with_note(format!(
                "{what} takes none of `{}`",
                RESULT_FORMS.join("`, `")
            )),
        );
    };
    for declaration in &module.declarations {
        if declaration.kind.binds_name() && RESULT_FORMS.contains(&declaration.name.as_str()) {
            report(declaration.at, &declaration.name, "a declaration");
        }
    }
    for file in &module.files {
        for import in &file.imports {
            if import.kind == ImportKind::Alias && RESULT_FORMS.contains(&import.name.as_str()) {
                report(import.at, &import.name, "an import alias");
            }
        }
    }
}

/// GEN-4: a candidate "binds that operator through local declaration,
/// selective import, or the prelude. No matching operator is a
/// declaration error." A native candidate (NATIVE-1) binds its operator
/// the same way.
fn bind_implementations(program: &mut Program, index: usize, diagnostics: &mut Diagnostics) {
    // What is found is gathered first and written back after: the search
    // reads other modules, so it cannot change this one as it goes.
    let mut found: Vec<(usize, OperatorRef)> = Vec::new();
    let mut prelude: Vec<PreludeName> = Vec::new();
    let module = &program.modules[index];
    for (position, implementation) in module.declarations.iter().enumerate() {
        if implementation.kind.candidate().is_none() {
            continue;
        }
        // TST-1: one in a test context has been rejected already.
        if implementation.test_only {
            continue;
        }
        let name = implementation.name.as_str();
        let operator = |module: &Module| {
            module
                .scope
                .get(name)
                .into_iter()
                .flatten()
                .copied()
                .find(|&other| module.declarations[other].kind == DeclarationKind::Operator)
        };

        // A declaration of the module itself.
        if let Some(declaration) = operator(module) {
            found.push((
                position,
                OperatorRef {
                    module: index,
                    declaration,
                },
            ));
            continue;
        }
        if let Some(other) = module.lookup(name).next() {
            diagnostics.emit(
                error(
                    Category::Operator,
                    codes::IMPL_UNKNOWN_OPERATOR,
                    implementation.at,
                    format!("`{name}` is not an operator, so nothing can implement it"),
                    "GEN-4",
                )
                .with_related(
                    other.at,
                    format!("declared here, as a {}", other.kind.describe()),
                ),
            );
            continue;
        }

        // A selective import of the implementation's own file (UNIT-2).
        let import = module.files[implementation.file]
            .imports
            .iter()
            .find(|import| import.name == name);
        if let Some(import) = import {
            // An import that names nothing has been reported.
            let Some(target) = import.module else {
                continue;
            };
            if import.kind == ImportKind::Selective
                && let Some(declaration) = operator(&program.modules[target])
            {
                found.push((
                    position,
                    OperatorRef {
                        module: target,
                        declaration,
                    },
                ));
            } else if import.kind == ImportKind::Alias
                || program.modules[target].lookup(name).next().is_some()
            {
                diagnostics.emit(
                    error(
                        Category::Operator,
                        codes::IMPL_UNKNOWN_OPERATOR,
                        implementation.at,
                        format!("`{name}` is not an operator, so nothing can implement it"),
                        "GEN-4",
                    )
                    .with_related(import.at, "imported here"),
                );
            }
            continue;
        }

        // The prelude: an operator of core's mandatory module (UNIT-11).
        if let Some(core) = program.core()
            && core != index
        {
            if let Some(declaration) = operator(&program.modules[core]) {
                found.push((
                    position,
                    OperatorRef {
                        module: core,
                        declaration,
                    },
                ));
                continue;
            }
            // A name the prelude binds to something else names no
            // operator: "No matching operator is a declaration error."
            let other = program.modules[core]
                .lookup(name)
                .find(|declaration| declaration.is_public());
            if let Some(other) = other {
                diagnostics.emit(
                    error(
                        Category::Operator,
                        codes::IMPL_UNKNOWN_OPERATOR,
                        implementation.at,
                        format!("`{name}` is not an operator, so nothing can implement it"),
                        "GEN-4",
                    )
                    .with_related(
                        other.at,
                        format!(
                            "the prelude binds it: declared here, as a {}",
                            other.kind.describe()
                        ),
                    ),
                );
                continue;
            }
        }

        // Nothing binds it, or the program has no module `core`. See
        // `Module::prelude_names`.
        prelude.push(PreludeName {
            name: name.to_string(),
            at: implementation.at,
        });
        // GEN-4: "No matching operator is a declaration error." With
        // core in the program every operator there is is known.
        if program.core().is_some() {
            diagnostics.emit(error(
                Category::Operator,
                codes::IMPL_UNKNOWN_OPERATOR,
                implementation.at,
                format!("nothing declares an operator `{name}`, so nothing can implement it"),
                "GEN-4",
            ));
        }
    }

    let module = &mut program.modules[index];
    for (position, operator) in found {
        let kind = &mut module.declarations[position].kind;
        *kind = match *kind {
            DeclarationKind::Implementation(role, _) => {
                DeclarationKind::Implementation(role, Some(operator))
            }
            DeclarationKind::NativeCandidate(role, _) => {
                DeclarationKind::NativeCandidate(role, Some(operator))
            }
            other => other,
        };
    }
    module.prelude_names.extend(prelude);
}

/// UNIT-3: "Import cycles are rejected."
///
/// Each group of modules that import one another is reported once, at
/// an import that the order of the files cannot change: the first import
/// into the group in the group's first module by path.
fn check_cycles(program: &Program, diagnostics: &mut Diagnostics) {
    // A graph with a node for each module and an edge for each import.
    // `petgraph` numbers nodes in the order they are added, so node `n`
    // is module `n`.
    let mut graph: DiGraph<(), ()> = DiGraph::new();
    for _ in &program.modules {
        graph.add_node(());
    }
    for (index, module) in program.modules.iter().enumerate() {
        for import in module.files.iter().flat_map(|file| &file.imports) {
            if let Some(target) = import.module {
                // `update_edge` adds the edge once however often the
                // module is imported.
                graph.update_edge(NodeIndex::new(index), NodeIndex::new(target), ());
            }
        }
    }

    // A strongly connected component is a largest set of nodes that all
    // reach one another: here, modules in a cycle of imports. A module
    // in no cycle is a component of its own, unless it imports itself.
    for component in kosaraju_scc(&graph) {
        let mut members: Vec<usize> = component.iter().map(|node| node.index()).collect();
        members.sort_unstable();
        let first = members[0];
        if members.len() == 1 && !graph.contains_edge(NodeIndex::new(first), NodeIndex::new(first))
        {
            continue;
        }
        let module = &program.modules[first];
        let import = module
            .files
            .iter()
            .flat_map(|file| &file.imports)
            .find(|import| {
                import
                    .module
                    .is_some_and(|target| members.contains(&target))
            });
        // The component has an edge out of every member, so the import
        // is there; `let ... else` keeps this from being a panic if not.
        let Some(import) = import else {
            continue;
        };
        let names: Vec<String> = members
            .iter()
            .map(|&member| format!("`{}`", program.modules[member].path))
            .collect();
        let message = match names.as_slice() {
            [only] => format!("module {only} imports itself"),
            // `[rest @ .., last]` splits the list into all but the last,
            // and the last.
            [rest @ .., last] => {
                format!("modules {} and {last} import one another", rest.join(", "))
            }
            [] => continue,
        };
        diagnostics.emit(error(
            Category::Module,
            codes::IMPORT_CYCLE,
            import.from_at,
            message,
            "UNIT-3",
        ));
    }
}
