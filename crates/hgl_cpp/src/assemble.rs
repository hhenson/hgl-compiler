//! Puts the pieces `emit` wrote into the target's files: for each
//! module a header of its declarations, a types header of the types it
//! declares where it declares one, and a unit of its definitions and
//! tests, for each module with a test that can be run an entry unit, and
//! for each application that can be run one too (UNIT-14). Part of the
//! transpiler, the last stage.
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

use crate::emit::{APPLICATION, Piece, Written, module_namespace, program_names, test_namespace};
use crate::{
    Application, Executable, File, Output, Test, Unit, application_path, entry_path, header_path,
    runtime, types_header_path, unit_path,
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
                blocked.is_none()
                    && !(piece.text.is_empty()
                        && piece.declaration.is_empty()
                        && piece.layout.is_none())
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
        // An application's run statement, where it is clear (UNIT-14).
        let application = module
            .application
            .as_ref()
            .filter(|_| blocks[index].application.is_none());
        if defined.is_empty() && tests.is_empty() && application.is_none() {
            continue;
        }
        // The headers of the other modules whose functions its pieces
        // call, and of those whose enums they name.
        let included: BTreeSet<usize> = defined
            .iter()
            .copied()
            .chain(tests.iter().map(|(_, piece)| *piece))
            .chain(application)
            .flat_map(|piece| {
                piece
                    .reach
                    .uses
                    .iter()
                    .map(|(module, _, _)| *module)
                    .chain(piece.types.iter().map(|(module, _)| *module))
            })
            .filter(|&used| used != index)
            .collect();
        // The module's types are in a header of their own, which its
        // header includes; its other declarations are in its header.
        let (typed, functions): (Vec<&Piece>, Vec<&Piece>) =
            defined.iter().copied().partition(|piece| piece.is_type);
        // The modules whose enums and structs its declarations name: its
        // header includes their types headers, and no other module's
        // header. An instance is written in its declaration's module
        // (GEN-2), and may name a type of a module that imports that one;
        // a types header includes only the types headers of the modules
        // its own types name, which its module imports (UNIT-3: "Import
        // cycles are rejected"), so no two headers include each other.
        let named: BTreeSet<usize> = functions
            .iter()
            .flat_map(|piece| piece.types.iter().map(|(module, _)| *module))
            .filter(|&module| module != index)
            .collect();
        let mut header = format!(
            "{WRITTEN}\n#pragma once\n\n#include <{}>\n",
            runtime::HEADER
        );
        if !typed.is_empty() {
            header.push_str(&format!("#include \"{path}.types.h\"\n"));
            files.push(File {
                path: types_header_path(path),
                text: types_header(program, index, &typed),
            });
        }
        for &module in &named {
            header.push_str(&format!(
                "#include \"{}.types.h\"\n",
                program.modules[module].path
            ));
        }
        let mut unit = format!("{WRITTEN}\n#include \"{path}.h\"\n");
        for &used in &included {
            unit.push_str(&format!("#include \"{}.h\"\n", program.modules[used].path));
        }
        calls_into[index] = included;
        if !defined.is_empty() {
            let namespace = module_namespace(path);
            let declared = namespace.trim_start_matches("::");
            header.push_str(&declarations(declared, &functions));
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
        if let Some(piece) = application {
            let namespace = module_namespace(path);
            let declared = namespace.trim_start_matches("::");
            unit.push_str(&format!(
                "\nnamespace {declared}\n{{\n\n{}\n}}  // namespace {declared}\n",
                piece.text
            ));
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

    // UNIT-14: each application, with what stops it being run, and for
    // one that can be run its entry unit and the units its executable is
    // linked from, as a module's tests' are.
    let mut applications = Vec::new();
    for (index, module) in written.iter().enumerate() {
        if module.application.is_none() {
            continue;
        }
        let declared = &program.modules[index];
        let blocked = blocks[index].application.clone();
        let mut application = Application {
            path: declared.path.clone(),
            real_time: declared.run_body.as_ref().is_some_and(|run| run.real_time),
            blocked,
            source: None,
            units: Vec::new(),
        };
        if application.blocked.is_none() {
            let mut reached: BTreeSet<usize> = BTreeSet::new();
            let mut next = vec![index];
            while let Some(module) = next.pop() {
                if reached.insert(module) {
                    next.extend(&calls_into[module]);
                }
            }
            files.push(File {
                path: application_path(&declared.path),
                text: application_entry(program, index),
            });
            application.source = Some(application_path(&declared.path));
            application.units = reached
                .iter()
                .filter_map(|&module| unit_of[module])
                .collect();
        }
        applications.push(application);
    }

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

    // No macro of a platform's headers takes a name the files write.
    // The lines a unit's block adds move each test's statements in it.
    let names = program_names(program);
    for file in &mut files {
        let (text, added) = undefined(&file.text, &names);
        file.text = text;
        for test in tests
            .iter_mut()
            .filter(|test| unit_path(&test.module) == file.path)
        {
            for (line, _) in &mut test.statements {
                *line += added;
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

/// The types header of module `index`, of the pieces `typed` that
/// declare its types (`emit::Layout`): the types headers of the modules
/// whose types they name; the enums, which a struct's generic parameter
/// may be of, and each struct declared; what each struct inherits from;
/// the structs, each after those it holds, as C++ needs a struct defined
/// before a struct that holds it, though a type may name one declared
/// after it (UNIT-6); and what each struct tells the runtime library.
fn types_header(program: &Program, index: usize, typed: &[&Piece]) -> String {
    let path = &program.modules[index].path;
    let namespace = module_namespace(path);
    let declared = namespace.trim_start_matches("::");
    let mut text = format!(
        "{WRITTEN}\n#pragma once\n\n#include <{}>\n",
        runtime::HEADER
    );
    let named: BTreeSet<usize> = typed
        .iter()
        .flat_map(|piece| piece.types.iter().map(|(module, _)| *module))
        .filter(|&module| module != index)
        .collect();
    for &module in &named {
        text.push_str(&format!(
            "#include \"{}.types.h\"\n",
            program.modules[module].path
        ));
    }
    // The structs, each after those it holds; the order declared
    // otherwise. What a struct holds holds none of it (TYPE-6: a struct
    // that holds itself is not written), so the order is found.
    let structs: Vec<&crate::emit::Layout> = typed
        .iter()
        .filter_map(|piece| piece.layout.as_deref())
        .collect();
    let mut ordered: Vec<&crate::emit::Layout> = Vec::new();
    // `visiting` holds the structs whose dependencies are being placed: one
    // met again there is not followed, so that no order is looked for
    // without end.
    fn visit<'l>(
        layout: &'l crate::emit::Layout,
        structs: &[&'l crate::emit::Layout],
        ordered: &mut Vec<&'l crate::emit::Layout>,
        visiting: &mut Vec<usize>,
    ) {
        if visiting.contains(&layout.declaration)
            || ordered
                .iter()
                .any(|done| done.declaration == layout.declaration)
        {
            return;
        }
        visiting.push(layout.declaration);
        for held in &layout.holds {
            if let Some(first) = structs.iter().find(|other| other.declaration == *held) {
                visit(first, structs, ordered, visiting);
            }
        }
        visiting.pop();
        if !ordered
            .iter()
            .any(|done| done.declaration == layout.declaration)
        {
            ordered.push(layout);
        }
    }
    for layout in &structs {
        visit(layout, &structs, &mut ordered, &mut Vec::new());
    }
    let within =
        |body: String| format!("\nnamespace {declared}\n{{\n{body}\n}}  // namespace {declared}\n");
    // The enums first: a struct's generic parameter may be of one, which
    // its forward declaration names.
    let enums: Vec<String> = typed
        .iter()
        .filter(|piece| piece.layout.is_none() && !piece.declaration.is_empty())
        .map(|piece| piece.declaration.clone())
        .collect();
    let mut first = format!("\n{}", enums.join("\n"));
    if !ordered.is_empty() {
        let forward: String = ordered
            .iter()
            .map(|layout| layout.forward.as_str())
            .collect();
        let deltas: String = ordered
            .iter()
            .map(|layout| layout.delta_forward.as_str())
            .collect();
        if !enums.is_empty() {
            first.push('\n');
        }
        first.push_str(&format!(
            "{forward}namespace d_\n{{\n{deltas}}}  // namespace d_"
        ));
    }
    text.push_str(&within(first));
    if !ordered.is_empty() {
        let parents: String = ordered
            .iter()
            .map(|layout| layout.parents.as_str())
            .collect();
        if !parents.is_empty() {
            text.push('\n');
            text.push_str(&parents);
        }
        let definitions: Vec<String> = ordered
            .iter()
            .map(|layout| layout.definition.clone())
            .collect();
        text.push_str(&within(format!("\n{}", definitions.join("\n"))));
    }
    if !ordered.is_empty() {
        text.push('\n');
        for layout in &ordered {
            text.push_str(&layout.traits);
        }
    }
    text
}

/// `text` with an `#undef` of each of `names` it writes, after its last
/// `#include`, and how many lines that adds: a header included there may
/// define any as a macro, and none is one where the file writes it. A
/// macro already expanded in those headers is not changed by it, and
/// `#undef` of a name no macro has does nothing.
fn undefined(text: &str, names: &BTreeSet<String>) -> (String, usize) {
    let split = text.rfind("#include ").map_or(0, |at| {
        text[at..].find('\n').map_or(text.len(), |end| at + end + 1)
    });
    let (head, body) = text.split_at(split);
    let written: BTreeSet<&str> = identifiers(body)
        .into_iter()
        .filter(|word| names.contains(*word))
        .collect();
    if written.is_empty() {
        return (text.to_string(), 0);
    }
    let mut block =
        String::from("\n// Names this file writes, which no macro of a header above takes.\n");
    for name in written {
        block.push_str(&format!("#undef {name}\n"));
    }
    let added = block.matches('\n').count();
    (format!("{head}{block}{body}"), added)
}

/// The identifiers of C++ text, outside its comments and its string and
/// character literals, in the order written.
fn identifiers(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'/' if bytes.get(at + 1) == Some(&b'/') => {
                while at < bytes.len() && bytes[at] != b'\n' {
                    at += 1;
                }
            }
            quote @ (b'"' | b'\'') => {
                at += 1;
                while at < bytes.len() && bytes[at] != quote {
                    at += if bytes[at] == b'\\' { 2 } else { 1 };
                }
                at += 1;
            }
            byte if byte.is_ascii_alphabetic() || byte == b'_' => {
                let begin = at;
                while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                    at += 1;
                }
                found.push(&text[begin..at]);
            }
            byte if byte.is_ascii_digit() => {
                while at < bytes.len()
                    && (bytes[at].is_ascii_alphanumeric() || matches!(bytes[at], b'_' | b'.'))
                {
                    at += 1;
                }
            }
            _ => at += 1,
        }
    }
    found
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

/// The entry unit of application `index` (UNIT-14): the declaration of the
/// function its run statement is written as, and a `main` that is the
/// runtime library's host of it, which runs it once in the mode its word
/// says (RUN-3) and records what became of the run. It includes the host's
/// light header alone, as a test's entry unit does.
fn application_entry(program: &Program, index: usize) -> String {
    let module = &program.modules[index];
    let namespace = module_namespace(&module.path);
    let declared = namespace.trim_start_matches("::");
    let real_time = module.run_body.as_ref().is_some_and(|run| run.real_time);
    let (word, mode) = if real_time {
        ("run", "in real time")
    } else {
        ("eval", "a simulation")
    };
    let library = runtime::NAMESPACE;
    format!(
        "{WRITTEN}\n#include <{}>\n\nnamespace {declared}\n{{\n\
         ::{library}::application::Output {APPLICATION}();\n}}  // namespace {declared}\n\n\
         // The run statement of `{}` is `{word}`: {mode} (RUN-3).\n\
         int main(int argc, char **argv)\n{{\n    \
         ::{library}::application::main(argc, argv, {real_time}, &{namespace}::{APPLICATION});\n}}\n",
        runtime::APPLICATION_HOST_HEADER,
        module.path,
    )
}
