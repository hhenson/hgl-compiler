//! What a piece of a program reaches, and what blocks it. Part of
//! lowering: a transpiler writes a program a piece at a time, each
//! declaration, instance, test and run statement, records in each the
//! first form it does not write and what the piece names, and asks
//! [`blocked`] whether a test or an application reaches a form left out.

use std::collections::{BTreeSet, HashMap, HashSet};

use hgl_diagnostics::Diagnostic;

/// What a piece names: a declaration, as the position of its module in
/// the program and its position in that module, and for a declaration
/// that is written by instance, the suffix of the instance named.
pub type Used = (usize, usize, Option<String>);

/// What one piece of a program reaches: the first form in it that the
/// target does not write yet, and the declarations it names, through
/// which it reaches theirs (`crate::blocked`).
#[derive(Default)]
pub struct Reach {
    /// The first form in it that the target does not write yet. A piece
    /// with one is left out of the file.
    pub broken: Option<Diagnostic>,
    /// The functions and the types it names.
    pub uses: BTreeSet<Used>,
}

/// What [`blocked`] has found a function or an instance needs, by what
/// names it: the first form it reaches that the target did not write,
/// or none. Each is found once, however many pieces reach it: a chain of
/// as many instances as one call makes (`hgl_analyser::INSTANCES`) is
/// looked through once, and not once for each of its links.
pub type Found = HashMap<Used, Option<Diagnostic>>;

/// The first form `test` needs that the target did not write: one of
/// its own, or of a function it reaches through the functions it
/// names. `pieces` gives what each piece a function or an instance is
/// written as reaches, by what names it ([`pieces_of`]).
///
/// The functions are looked through depth first, in the order each
/// piece names them, each once: a function may be reached twice, or
/// reach itself. What is found of each is kept in `found` where it does
/// not hang on the path it was reached by: all that a look that finds
/// nothing passed, and where no function reached one on its own path
/// again, what each on the path to the form found reaches.
pub fn blocked<'p>(
    pieces: impl Fn(&Used) -> Vec<&'p Reach>,
    found: &mut Found,
    test: &Reach,
) -> Option<Diagnostic> {
    if let Some(diagnostic) = &test.broken {
        return Some(diagnostic.clone());
    }
    // Each function on the path from `test`, with what it names, and how
    // many of those have been looked through.
    let mut path: Vec<(Option<Used>, Vec<Used>, usize)> =
        vec![(None, test.uses.iter().cloned().collect(), 0)];
    let mut seen: HashSet<Used> = HashSet::new();
    let mut finished: Vec<Used> = Vec::new();
    // True once a function named one on its own path again.
    let mut again = false;
    let result = loop {
        let Some((_, uses, next)) = path.last_mut() else {
            break None;
        };
        let Some(used) = uses.get(*next).cloned() else {
            if let Some((Some(done), _, _)) = path.pop() {
                finished.push(done);
            }
            continue;
        };
        *next += 1;
        if let Some(known) = found.get(&used) {
            match known {
                Some(diagnostic) => break Some(diagnostic.clone()),
                None => continue,
            }
        }
        if !seen.insert(used.clone()) {
            again |= path.iter().any(|(on, _, _)| on.as_ref() == Some(&used));
            continue;
        }
        let pieces = pieces(&used);
        // The declaration's piece is looked at before the instance's,
        // and what the instance names before what the declaration does.
        if let Some(diagnostic) = pieces.iter().find_map(|piece| piece.broken.clone()) {
            path.push((Some(used), Vec::new(), 0));
            break Some(diagnostic);
        }
        let names: Vec<Used> = pieces
            .iter()
            .rev()
            .flat_map(|piece| piece.uses.iter().cloned())
            .collect();
        path.push((Some(used), names, 0));
    };
    // What finds nothing passed nothing that does: each it passed finds
    // nothing, however it is reached.
    if result.is_none() {
        for used in seen {
            found.insert(used, None);
        }
    } else if !again {
        for used in finished {
            found.insert(used, None);
        }
        for (on, _, _) in path {
            if let Some(on) = on {
                found.insert(on, result.clone());
            }
        }
    }
    result
}
