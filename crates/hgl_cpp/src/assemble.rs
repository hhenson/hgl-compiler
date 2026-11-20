//! Puts the pieces `emit` wrote into the target's files: for each
//! module a header of its declarations and a unit of its definitions
//! and tests, and for each module with a test that can be run an entry
//! unit. Part of the transpiler, the last stage.
//!
//! C++ is compiled a unit at a time and whole, so a piece is written
//! only where it is *clear*: neither it nor any piece it names, through
//! the pieces those name, has a form that is not written. A test that
//! is not clear is listed with the first such form, which
//! `hgl_lower::pieces::blocked` finds as it finds it for the hgraph
//! target.

use std::collections::BTreeSet;

use hgl_analyser::Program;
use hgl_diagnostics::{Diagnostic, Location};
use hgl_lower::pieces::{Found, Reach, Used, blocked};

use crate::emit::{Piece, Written, module_namespace, test_namespace};
use crate::{
    Application, Executable, File, Output, Test, Unit, entry_path, header_path, runtime, unit_path,
};

/// The first line of every file the target writes.
const WRITTEN: &str = "// Written by the hgl compiler for hgraph's C++ API. Do not edit.";

/// What blocks each piece of one module: the first form each needs that
/// is not written, or `None` for one that is clear.
struct Blocks {
    /// For each declaration, in the order declared.
    declarations: Vec<Option<Diagnostic>>,
    /// For each named test, in the order declared.
    tests: Vec<Option<Diagnostic>>,
    /// For an application's run statement.
    application: Option<Diagnostic>,
    /// For each instance of a generic declaration, in the order of
    /// [`Written::instances`].
    instances: Vec<Option<Diagnostic>>,
}

/// The target's output for `program`, whose modules were written as
/// `written`, and the form that blocks each piece of a module other
/// than core's that is blocked by a piece it names: core's pieces are
/// reported where the program reaches them.
pub(crate) fn output(program: &Program, written: &[Written]) -> (Output, Vec<Diagnostic>) {
    let core = program.core();
    // What each piece a declaration or an instance is written as
    // reaches: the declaration's own, and the instance's.
    let pieces = |(module, declaration, instance): &Used| -> Vec<&Reach> {
        let Some(written) = written.get(*module) else {
            return Vec::new();
        };
        let mut found: Vec<&Reach> = written
            .declarations
            .get(*declaration)
            .map(|piece| &piece.reach)
            .into_iter()
            .collect();
        if let Some(suffix) = instance {
            found.extend(
                written
                    .instances
                    .iter()
                    .filter(|(of, named, _)| of == declaration && named == suffix)
                    .map(|(_, _, piece)| &piece.reach),
            );
        }
        found
    };
    let mut found = Found::new();
    let mut blocked = |piece: &Piece| blocked(pieces, &mut found, &piece.reach);
    let blocks: Vec<Blocks> = written
        .iter()
        .map(|module| Blocks {
            declarations: module.declarations.iter().map(&mut blocked).collect(),
            tests: module.tests.iter().map(&mut blocked).collect(),
            application: module.application.as_ref().and_then(&mut blocked),
            instances: module
                .instances
                .iter()
                .map(|(_, _, piece)| blocked(piece))
                .collect(),
        })
        .collect();
    let mut files = Vec::new();
    let mut units = Vec::new();
    // The unit of each module, as a position in `units`.
    let mut unit_of: Vec<Option<usize>> = vec![None; written.len()];
    // For each module's unit, the other modules whose functions what it
    // holds calls: an executable that links it links theirs.
    let mut calls_into: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); written.len()];
    // For each test of each module, where its statements start in the
    // module's unit.
    let mut test_lines: Vec<Vec<Vec<(usize, Location)>>> = written
        .iter()
        .map(|module| vec![Vec::new(); module.tests.len()])
        .collect();
    for (index, module) in written.iter().enumerate() {
        let path = &program.modules[index].path;
        // A type's declaration is in the header alone; each other piece
        // that is clear has its definition in the unit.
        let defined: Vec<&Piece> = module
            .declarations
            .iter()
            .zip(&blocks[index].declarations)
            .chain(
                module
                    .instances
                    .iter()
                    .map(|(_, _, piece)| piece)
                    .zip(&blocks[index].instances),
            )
            .filter(|(piece, blocked)| {
                blocked.is_none() && !(piece.text.is_empty() && piece.declaration.is_empty())
            })
            .map(|(piece, _)| piece)
            .collect();
        // Core's tests are no tests of the program's.
        let tests: Vec<(usize, &Piece)> = module
            .tests
            .iter()
            .enumerate()
            .filter(|(test, _)| Some(index) != core && blocks[index].tests[*test].is_none())
            .collect();
        if defined.is_empty() && tests.is_empty() {
            continue;
        }
        // The headers of the other modules whose functions its pieces
        // call, and of those whose types its tests name.
        let included: BTreeSet<usize> = defined
            .iter()
            .copied()
            .chain(tests.iter().map(|(_, piece)| *piece))
            .flat_map(|piece| &piece.reach.uses)
            .map(|(module, _, _)| *module)
            .chain(
                tests
                    .iter()
                    .flat_map(|(_, piece)| piece.types.iter().copied()),
            )
            .filter(|&used| used != index)
            .collect();
        // The other modules whose types its pieces name: its header
        // includes theirs. Modules import no cycle (UNIT-3,
        // `import.cycle`), so no header includes itself.
        let typed: BTreeSet<usize> = defined
            .iter()
            .flat_map(|piece| &piece.types)
            .copied()
            .filter(|&used| used != index)
            .collect();
        let mut header = format!(
            "{WRITTEN}\n#pragma once\n\n#include <{}>\n",
            runtime::HEADER
        );
        for &used in &typed {
            header.push_str(&format!("#include \"{}.h\"\n", program.modules[used].path));
        }
        let mut unit = format!("{WRITTEN}\n#include \"{path}.h\"\n");
        for &used in &included {
            unit.push_str(&format!("#include \"{}.h\"\n", program.modules[used].path));
        }
        calls_into[index] = included;
        if !defined.is_empty() {
            let namespace = module_namespace(path);
            let declared = namespace.trim_start_matches("::");
            header.push_str(&declarations(declared, &defined));
            unit.push_str(&format!("\nnamespace {declared}\n{{\n"));
            for piece in &defined {
                if !piece.text.is_empty() {
                    unit.push('\n');
                    unit.push_str(&piece.text);
                }
            }
            unit.push_str(&format!("\n}}  // namespace {declared}\n"));
        }
        if !tests.is_empty() {
            let namespace = test_namespace(path);
            let declared = namespace.trim_start_matches("::");
            unit.push_str(&format!("\nnamespace {declared}\n{{\n"));
            for (test, piece) in &tests {
                unit.push('\n');
                // The line the piece starts on, counted from 1.
                let first = unit.matches('\n').count() + 1;
                test_lines[index][*test] = piece
                    .statements
                    .iter()
                    .map(|(line, at)| (first + line, *at))
                    .collect();
                unit.push_str(&piece.text);
            }
            unit.push_str(&format!("\n}}  // namespace {declared}\n"));
        }
        unit_of[index] = Some(units.len());
        units.push(Unit {
            module: path.clone(),
            source: unit_path(path),
            header: header_path(path),
        });
        files.push(File {
            path: header_path(path),
            text: header,
        });
        files.push(File {
            path: unit_path(path),
            text: unit,
        });
    }

    // One executable for each module with a test that can be run,
    // linked from its module's unit and every unit that one calls into,
    // through the units those call into: a unit holds every function of
    // its module that is written, which its tests may not call, and each
    // is linked whole.
    let mut executables = Vec::new();
    let mut tests = Vec::new();
    for (index, module) in written.iter().enumerate() {
        if Some(index) == core {
            continue;
        }
        let declared = &program.modules[index];
        let clear: Vec<(usize, &Piece)> = module
            .tests
            .iter()
            .enumerate()
            .filter(|(test, _)| blocks[index].tests[*test].is_none())
            .collect();
        let executable = (!clear.is_empty()).then_some(executables.len());
        for (position, test) in declared.tests.iter().enumerate() {
            let blocked = blocks[index].tests[position].clone();
            tests.push(Test {
                module: declared.path.clone(),
                name: test.name.clone(),
                integration: test.integration,
                // A duration literal is never negative (LEX-7).
                timeout: test
                    .timeout
                    .map(|micros| std::time::Duration::from_micros(micros.unsigned_abs())),
                executable: executable.filter(|_| blocked.is_none()),
                blocked,
                statements: std::mem::take(&mut test_lines[index][position]),
            });
        }
        if clear.is_empty() {
            continue;
        }
        let mut reached: BTreeSet<usize> = BTreeSet::new();
        let mut next = vec![index];
        while let Some(module) = next.pop() {
            if reached.insert(module) {
                next.extend(&calls_into[module]);
            }
        }
        files.push(File {
            path: entry_path(&declared.path),
            text: entry(program, index, &clear),
        });
        executables.push(Executable {
            module: declared.path.clone(),
            source: entry_path(&declared.path),
            units: reached
                .iter()
                .filter_map(|&module| unit_of[module])
                .collect(),
        });
    }

    // UNIT-14: each application, with what stops it being run.
    let applications = program
        .modules
        .iter()
        .zip(written)
        .zip(&blocks)
        .filter(|((_, written), _)| written.application.is_some())
        .map(|((module, _), blocks)| Application {
            path: module.path.clone(),
            real_time: module.run_body.as_ref().is_some_and(|run| run.real_time),
            blocked: blocks.application.clone(),
        })
        .collect();

    // What blocks each piece of the program's own modules that is
    // blocked by another piece.
    let mut blocking = Vec::new();
    for (index, module) in written.iter().enumerate() {
        if Some(index) == core {
            continue;
        }
        let pieces = module
            .declarations
            .iter()
            .zip(&blocks[index].declarations)
            .chain(module.tests.iter().zip(&blocks[index].tests))
            .chain(module.application.iter().zip([&blocks[index].application]))
            .chain(
                module
                    .instances
                    .iter()
                    .map(|(_, _, piece)| piece)
                    .zip(&blocks[index].instances),
            );
        for (piece, blocked) in pieces {
            if let (None, Some(diagnostic)) = (&piece.reach.broken, blocked) {
                blocking.push(diagnostic.clone());
            }
        }
    }

    let output = Output {
        files,
        units,
        executables,
        tests,
        applications,
    };
    (output, blocking)
}

/// The declarations of `pieces`, in the namespace `namespace`, the
/// types' first: a function may name a type declared after it (UNIT-6).
/// Nothing where none declares anything.
fn declarations(namespace: &str, pieces: &[&Piece]) -> String {
    if pieces.iter().all(|piece| piece.declaration.is_empty()) {
        return String::new();
    }
    let mut text = format!("\nnamespace {namespace}\n{{\n");
    let types = pieces.iter().filter(|piece| piece.is_type);
    let rest = pieces.iter().filter(|piece| !piece.is_type);
    for piece in types.chain(rest) {
        if !piece.declaration.is_empty() {
            text.push('\n');
            text.push_str(&piece.declaration);
        }
    }
    text.push_str(&format!("\n}}  // namespace {namespace}\n"));
    text
}

/// The entry unit of module `index`'s tests: their declarations, a
/// registration of each with the runtime library's test host by its
/// identity (TST-15), and a `main` that runs those its command line
/// names. `clear` is each test that can be run, with its position among
/// the module's tests. The registrations are in the entry unit, and in
/// no module's unit, so that an executable linked with another
/// module's unit runs only its own module's tests.
fn entry(program: &Program, index: usize, clear: &[(usize, &Piece)]) -> String {
    let module = &program.modules[index];
    let tested = test_namespace(&module.path);
    let declared = tested.trim_start_matches("::");
    let mut text = format!("{WRITTEN}\n#include <{}>\n", runtime::HOST_HEADER);
    text.push_str(&format!("\nnamespace {declared}\n{{\n"));
    for (test, _) in clear {
        let name = crate::emit::identifier(&module.tests[*test].name);
        text.push_str(&format!("void {name}();\n"));
    }
    text.push_str(&format!("}}  // namespace {declared}\n"));
    text.push_str("\nnamespace\n{\n// Each test, by its identity, `module.path::name` (TST-15).\n");
    let registration = crate::emit::testing("Registration");
    for (position, (test, _)) in clear.iter().enumerate() {
        let test = &module.tests[*test];
        let identity = crate::emit::string(&format!("{}::{}", module.path, test.name));
        let name = crate::emit::identifier(&test.name);
        text.push_str(&format!(
            "const {registration} test_{position}{{{identity}, &{tested}::{name}}};\n"
        ));
    }
    text.push_str("}  // namespace\n");
    text.push_str(&format!(
        "\nint main(int argc, char **argv)\n{{\n    return {}(argc, argv);\n}}\n",
        crate::emit::testing("main")
    ));
    text
}
