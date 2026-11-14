//! Resolves the names used inside declarations (UNIT-10).
//!
//! > A name resolves to the one binding in scope for it: an earlier local
//! > or iteration binding of an enclosing block; a parameter of an
//! > enclosing function; a generic parameter of an enclosing declaration;
//! > a module declaration; a selective import; or the prelude.
//!
//! This pass walks every signature, body, type and constraint. Where a
//! name is bound (a `let`, a parameter) it adds the name to the scope,
//! and rejects it when the name is already there (UNIT-12). Where a name
//! is used it looks the name up, innermost scope first, and records what
//! it found.
//!
//! Scopes nest as the source does. A *frame* holds the bindings of one
//! level: a declaration (its generic parameters and parameters), a block,
//! the pattern of a `for`, an anonymous function. The frames in use form
//! a stack; a lookup reads it from the top.
//!
//! What a name *is* does not matter here: a type, a function and a local
//! are found the same way. Whether the thing found fits where it is used
//! is for the passes that know types.

use std::collections::HashMap;

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};
use hgl_source::FileId;

use crate::program::{
    Binding, Decision, DeclarationKind, ImportKind, LocalKind, Module, NameUse, PreludeName,
    Program,
};
use crate::{INJECTABLES, codes, error};

/// Resolves the names of every module, filling in [`Module::uses`] and
/// [`Module::decisions`] and adding to [`Module::prelude_names`].
pub(crate) fn resolve(program: &mut Program, diagnostics: &mut Diagnostics) {
    for index in 0..program.modules.len() {
        // A module is made from at least one file.
        let Some(first) = program.modules[index].files.first() else {
            continue;
        };
        // The resolver reads the whole program and so cannot change it.
        // What it finds is taken out when it is done, and written back.
        let mut resolver = Resolver {
            program,
            module: &program.modules[index],
            index,
            file: 0,
            file_id: first.file,
            in_test: false,
            frames: Vec::new(),
            hidden: Vec::new(),
            diagnostics,
            uses: Vec::new(),
            prelude: Vec::new(),
            decisions: Vec::new(),
        };
        resolver.module();
        let (mut uses, prelude, decisions) = (resolver.uses, resolver.prelude, resolver.decisions);

        let module = &mut program.modules[index];
        // File order, then source order.
        let position: HashMap<FileId, usize> = module
            .files
            .iter()
            .enumerate()
            .map(|(position, file)| (file.file, position))
            .collect();
        uses.sort_by_key(|used| (position[&used.at.file], used.at.span.start));
        module.uses = uses;
        module.decisions = decisions;
        for name in prelude {
            if !module
                .prelude_names
                .iter()
                .any(|known| known.name == name.name)
            {
                module.prelude_names.push(name);
            }
        }
    }
}

/// One level of scope.
#[derive(Default)]
struct Frame {
    /// True for the frame of an anonymous function. Looking a name up
    /// past it is a capture (FN-14).
    lambda: bool,
    /// The bindings of this level.
    bound: HashMap<String, (Location, LocalKind)>,
    /// Names this block declares further down. One used before then is
    /// used before its declaration (UNIT-6).
    later: HashMap<String, Location>,
}

/// A name a default may not read (UNIT-7): a parameter of the same
/// declaration, or a field of the same struct.
struct Hidden {
    name: String,
    at: Location,
    what: &'static str,
}

struct Resolver<'a> {
    program: &'a Program,
    /// The module being resolved, and its position in the program.
    module: &'a Module,
    index: usize,
    /// The file being resolved: its position in the module, and itself.
    file: usize,
    file_id: FileId,
    /// True inside a test or a test helper, where the module's test
    /// scope is visible (TST-1).
    in_test: bool,
    frames: Vec<Frame>,
    hidden: Vec<Hidden>,
    diagnostics: &'a mut Diagnostics,
    uses: Vec<NameUse>,
    prelude: Vec<PreludeName>,
    decisions: Vec<Decision>,
}

// ---- Reading the tree ----

/// The first child node of `node` of kind `kind`.
fn child(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxNode> {
    node.children().find(|child| child.kind() == kind)
}

/// The identifier tokens directly inside `node`, in order.
fn identifiers(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.kind() == SyntaxKind::Ident)
}

/// The name a parameter or generic parameter declares: the last
/// identifier before its `:`. In `const n: i64` that is `n`, and in
/// `const: i64` it is `const` (FN-5).
fn declared_name(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .take_while(|token| token.kind() != SyntaxKind::Colon)
        .filter(|token| token.kind() == SyntaxKind::Ident)
        .last()
}

/// The child nodes of a parameter or struct member, split at its `=`:
/// its type, and its default.
fn split_at_default(node: &SyntaxNode) -> (Vec<SyntaxNode>, Vec<SyntaxNode>) {
    let mut before = Vec::new();
    let mut after = Vec::new();
    let mut seen = false;
    for element in node.children_with_tokens() {
        if element.kind() == SyntaxKind::Eq {
            seen = true;
        } else if let Some(node) = element.into_node() {
            if seen {
                after.push(node);
            } else {
                before.push(node);
            }
        }
    }
    (before, after)
}

impl Resolver<'_> {
    // ---- Reporting ----

    fn at(&self, token: &SyntaxToken) -> Location {
        Location::new(self.file_id, token.span())
    }

    /// Records a use that resolved, and gives the binding back.
    fn used(&mut self, token: &SyntaxToken, binding: Binding) -> Option<Binding> {
        self.uses.push(NameUse {
            name: token.text().to_string(),
            at: self.at(token),
            binding: binding.clone(),
        });
        Some(binding)
    }

    // ---- The module ----

    /// Points the resolver at a file of the module.
    fn enter(&mut self, file: usize, in_test: bool) {
        self.file = file;
        self.file_id = self.module.files[file].file;
        self.in_test = in_test;
        self.frames.clear();
        self.hidden.clear();
    }

    fn module(&mut self) {
        // `self.module` is a reference that outlives `self`'s borrow,
        // so the loops below may call `&mut self` methods.
        let module = self.module;
        for declaration in &module.declarations {
            self.enter(declaration.file, declaration.test_only);
            self.declaration(&declaration.syntax);
        }
        for test in &module.tests {
            self.enter(test.file, true);
            self.children(&test.syntax);
        }
        for (file, node) in &module.instantiations {
            self.enter(*file, false);
            self.children(node);
        }
    }

    fn declaration(&mut self, node: &SyntaxNode) {
        match node.kind() {
            SyntaxKind::FnDecl | SyntaxKind::OperatorDecl | SyntaxKind::NativeFnDecl => {
                self.callable(node);
            }
            SyntaxKind::StructDecl => self.structure(node),
            // An enum's values and a native type name nothing of their
            // own; what they use is in the module's scope.
            _ => self.children(node),
        }
    }

    /// A function, an operator or a native function.
    fn callable(&mut self, node: &SyntaxNode) {
        self.frames.push(Frame::default());
        // GEN-1: "Generic scope covers the whole declaration."
        if let Some(generics) = child(node, SyntaxKind::GenericParams) {
            self.generic_params(&generics);
        }
        let parameters: Vec<SyntaxNode> = child(node, SyntaxKind::ParamList)
            .into_iter()
            .flat_map(|list| list.children())
            .filter(|child| child.kind() == SyntaxKind::Param)
            .collect();
        // The types first: they see the generic parameters.
        for parameter in &parameters {
            for part in split_at_default(parameter).0 {
                self.walk(&part);
            }
        }
        // UNIT-7: a default is resolved "excluding the callable's formal
        // arguments".
        self.hidden = parameters
            .iter()
            .filter_map(declared_name)
            .map(|name| Hidden {
                name: name.text().to_string(),
                at: self.at(&name),
                what: "a parameter of the same declaration",
            })
            .collect();
        for parameter in &parameters {
            for part in split_at_default(parameter).1 {
                self.walk(&part);
            }
        }
        self.hidden.clear();
        for parameter in &parameters {
            if let Some(name) = declared_name(parameter) {
                self.bind(&name, LocalKind::Parameter);
            }
        }
        // The result type, the constraints, the body.
        for part in node.children() {
            match part.kind() {
                SyntaxKind::GenericParams | SyntaxKind::ParamList => {}
                // A native contract's `inject` names stand in the
                // declaration's own scope: it has no block.
                SyntaxKind::NativeContractBody => self.children(&part),
                _ => self.walk(&part),
            }
        }
        self.frames.pop();
    }

    fn generic_params(&mut self, generics: &SyntaxNode) {
        let parameters: Vec<SyntaxNode> = generics
            .children()
            .filter(|parameter| parameter.kind() == SyntaxKind::GenericParam)
            .collect();
        // GEN-1: "Generic scope covers the whole declaration", the
        // generic parameters themselves included. Every name is bound
        // before any type is read, so that in `<const N: T, T>` the
        // type of `N` finds the `T` written after it.
        for parameter in &parameters {
            if let Some(name) = declared_name(parameter) {
                self.bind(&name, LocalKind::GenericParameter);
            }
        }
        // The type of `const N: i64`.
        for parameter in &parameters {
            self.children(parameter);
        }
    }

    fn structure(&mut self, node: &SyntaxNode) {
        self.frames.push(Frame::default());
        if let Some(generics) = child(node, SyntaxKind::GenericParams) {
            self.generic_params(&generics);
        }
        // UNIT-7: a default is resolved "excluding ... the struct's
        // instance fields". A field's name is the first token of its
        // member; `time` may be one (LEX-9) and is no name to look up.
        let fields: Vec<Hidden> = node
            .children()
            .filter(|member| member.kind() == SyntaxKind::StructMember)
            .filter_map(|member| identifiers(&member).next())
            .map(|name| Hidden {
                name: name.text().to_string(),
                at: self.at(&name),
                what: "a field of the same struct",
            })
            .collect();
        for part in node.children() {
            match part.kind() {
                SyntaxKind::GenericParams => {}
                SyntaxKind::StructMember => {
                    let (types, defaults) = split_at_default(&part);
                    // `name = value` has no `:`; everything is after `=`.
                    for part in types {
                        self.walk(&part);
                    }
                    self.hidden = fields
                        .iter()
                        .map(|field| Hidden {
                            name: field.name.clone(),
                            at: field.at,
                            what: field.what,
                        })
                        .collect();
                    for part in defaults {
                        self.walk(&part);
                    }
                    self.hidden.clear();
                }
                _ => self.walk(&part),
            }
        }
        self.frames.pop();
    }

    // ---- Walking ----

    /// Walks the child nodes of `node`.
    fn children(&mut self, node: &SyntaxNode) {
        for part in node.children() {
            self.walk(&part);
        }
    }

    /// Walks one node: binds what it declares, looks up what it uses.
    fn walk(&mut self, node: &SyntaxNode) {
        match node.kind() {
            SyntaxKind::Block => self.block(node),
            SyntaxKind::LocalDecl | SyntaxKind::StateDecl | SyntaxKind::CacheDecl => {
                // The initialiser first: `let x = x` does not read
                // itself (UNIT-6: a local is visible after it is
                // declared).
                self.children(node);
                let kind = match node.first_token().map(|token| token.kind()) {
                    Some(SyntaxKind::KwVar) => LocalKind::Var,
                    Some(SyntaxKind::KwState) => LocalKind::State,
                    Some(SyntaxKind::KwCache) => LocalKind::Cache,
                    _ => LocalKind::Let,
                };
                if let Some(name) = identifiers(node).next() {
                    self.bind(&name, kind);
                }
            }
            SyntaxKind::InjectDecl => {
                for name in identifiers(node) {
                    // TIME-8: "`inject` names no other."
                    if !INJECTABLES.contains(&name.text()) {
                        self.diagnostics.emit(
                            error(
                                Category::Injectable,
                                codes::INJECT_UNKNOWN_CAPABILITY,
                                self.at(&name),
                                format!("`{}` is not a capability", name.text()),
                                "TIME-8",
                            )
                            .with_note(format!(
                                "`inject` names one of `{}`",
                                INJECTABLES.join("`, `")
                            )),
                        );
                    }
                    // It is bound all the same, so that its uses find it.
                    self.bind(&name, LocalKind::Inject);
                }
            }
            SyntaxKind::ForStmt => self.for_statement(node),
            SyntaxKind::FnExpr => self.function_expression(node),
            SyntaxKind::ConstraintEach => self.each_constraint(node),
            SyntaxKind::NameExpr
            | SyntaxKind::NamedType
            | SyntaxKind::TypedConstructor
            | SyntaxKind::ConstraintCall
            | SyntaxKind::Instantiation => {
                self.name_of(node);
                self.children(node);
            }
            SyntaxKind::UndecidedConstructor => {
                // GEN-14: "directly following a name resolved to a
                // generic type in an expression, `<` opens generic
                // arguments rather than a comparison."
                let binding = self.name_of(node);
                self.decisions.push(Decision {
                    at: Location::new(self.file_id, node.span()),
                    constructor: binding.map(|binding| self.is_generic_type(&binding)),
                });
                // The names inside are the same names under either
                // reading, so they are resolved as they stand.
                self.children(node);
            }
            SyntaxKind::BinaryExpr => {
                self.children(node);
                self.check_application(node);
            }
            // The two names of `alias::name` are read by the node that
            // holds them.
            SyntaxKind::QualifiedName => {}
            _ => self.children(node),
        }
    }

    /// A block: "A block's scope is the scope around it plus its own
    /// bindings" (UNIT-12).
    ///
    /// "A `switch` case has no scope of its own: its statements bind in
    /// the block around the `switch`." So what a case declares is noted
    /// here, with the block's own statements.
    fn block(&mut self, node: &SyntaxNode) {
        let mut frame = Frame::default();
        self.declared_later(node, &mut frame);
        self.frames.push(frame);
        self.children(node);
        self.frames.pop();
    }

    /// Notes the names the statements of `node` declare in its block:
    /// those of its `let`, `var`, `state`, `cache` and `inject`
    /// statements, and of the same under the cases of a `switch`.
    fn declared_later(&self, node: &SyntaxNode, frame: &mut Frame) {
        for statement in node.children() {
            let names: Vec<SyntaxToken> = match statement.kind() {
                SyntaxKind::LocalDecl | SyntaxKind::StateDecl | SyntaxKind::CacheDecl => {
                    identifiers(&statement).take(1).collect()
                }
                SyntaxKind::InjectDecl => identifiers(&statement).collect(),
                SyntaxKind::SwitchStmt | SyntaxKind::SwitchCase | SyntaxKind::SwitchDefault => {
                    self.declared_later(&statement, frame);
                    Vec::new()
                }
                _ => Vec::new(),
            };
            for name in names {
                let at = self.at(&name);
                frame.later.entry(name.text().to_string()).or_insert(at);
            }
        }
    }

    /// `for a, b in items { ... }`: the pattern's names are bound for the
    /// body, not for `items`.
    fn for_statement(&mut self, node: &SyntaxNode) {
        // `in` is a name with a meaning only here (LEX-10): the last
        // identifier directly inside the statement, when it is `in`.
        let mut names: Vec<SyntaxToken> = identifiers(node).collect();
        if names.last().is_some_and(|token| token.text() == "in") {
            names.pop();
        }
        for part in node.children() {
            if part.kind() != SyntaxKind::Block {
                self.walk(&part);
            }
        }
        self.frames.push(Frame::default());
        for name in &names {
            self.bind(name, LocalKind::Iteration);
        }
        if let Some(body) = child(node, SyntaxKind::Block) {
            self.walk(&body);
        }
        self.frames.pop();
    }

    /// `fn(x) => ...`: "An anonymous function's scope is the scope around
    /// it plus its parameters" (FN-14).
    fn function_expression(&mut self, node: &SyntaxNode) {
        self.frames.push(Frame {
            lambda: true,
            ..Frame::default()
        });
        for part in node.children() {
            if part.kind() != SyntaxKind::ParamList {
                self.walk(&part);
                continue;
            }
            for parameter in part.children() {
                // The type of `x: i64`, then the name.
                self.children(&parameter);
                if let Some(name) = identifiers(&parameter).next() {
                    self.bind(&name, LocalKind::Parameter);
                }
            }
        }
        self.frames.pop();
    }

    /// `each T in types(Ts) { numeric(T) }`: `T` is bound inside the
    /// braces.
    fn each_constraint(&mut self, node: &SyntaxNode) {
        // The identifiers directly inside are `each`, the name, `in`.
        let name = identifiers(node).nth(1);
        let mut parts = node.children();
        // What is iterated is outside the scope of the name.
        if let Some(operand) = parts.next() {
            self.walk(&operand);
        }
        self.frames.push(Frame::default());
        if let Some(name) = name {
            self.bind(&name, LocalKind::GenericParameter);
        }
        for part in parts {
            self.walk(&part);
        }
        self.frames.pop();
    }

    // ---- Binding ----

    /// Adds a binding to the innermost frame, unless the name is already
    /// in scope.
    ///
    /// UNIT-12: "No binding takes a name already in scope: that is a
    /// duplicate definition. A local or iteration binding, parameter, or
    /// generic parameter takes no name bound there by an enclosing block
    /// or function, a module declaration, a `use` in its file, or the
    /// prelude."
    fn bind(&mut self, token: &SyntaxToken, kind: LocalKind) {
        let name = token.text();
        let at = self.at(token);
        let innermost = self.frames.len().saturating_sub(1);
        if let Some(frame) = self.frames.last_mut() {
            frame.later.remove(name);
        }

        // A binding of an enclosing block, function or declaration.
        let enclosing = self
            .frames
            .iter()
            .enumerate()
            .rev()
            .find_map(|(depth, frame)| frame.bound.get(name).map(|found| (depth, *found)));
        if let Some((depth, (first, first_kind))) = enclosing {
            // UNIT-5: "Duplicate ... parameter names ... or local
            // bindings in one scope are rejected."
            let rule = if depth == innermost {
                "UNIT-5"
            } else {
                "UNIT-12"
            };
            self.diagnostics.emit(
                error(
                    Category::Name,
                    codes::NAME_DUPLICATE,
                    at,
                    format!("`{name}` is already in scope"),
                    rule,
                )
                .with_related(
                    first,
                    format!("{} of that name is here", first_kind.describe()),
                ),
            );
            // The first keeps the name.
            return;
        }

        // A `use` of the file, or a declaration of the module.
        let import = self.module.files[self.file]
            .imports
            .iter()
            .find(|import| import.name == name);
        let declaration = self
            .module
            .lookup(name)
            .find(|declaration| self.in_test || !declaration.test_only);
        let clash = match (import, declaration) {
            (Some(import), _) => Some((import.at, "imported here".to_string())),
            (None, Some(declaration)) => Some((
                declaration.at,
                format!("declared here, as a {}", declaration.kind.describe()),
            )),
            (None, None) => None,
        };
        if let Some((first, words)) = clash {
            self.diagnostics.emit(
                error(
                    Category::Name,
                    codes::NAME_DUPLICATE,
                    at,
                    format!("`{name}` is already in scope"),
                    "UNIT-12",
                )
                .with_related(first, words)
                .with_note(format!(
                    "{} takes no name of its module or of its file's imports",
                    kind.describe()
                )),
            );
        }
        // It is bound all the same, so that its uses find it.
        if let Some(frame) = self.frames.last_mut() {
            frame.bound.insert(name.to_string(), (at, kind));
        }
    }

    // ---- Looking up ----

    /// Resolves the name a node uses: its `alias::name`, or its first
    /// identifier.
    fn name_of(&mut self, node: &SyntaxNode) -> Option<Binding> {
        if let Some(qualified) = child(node, SyntaxKind::QualifiedName) {
            let mut names = identifiers(&qualified);
            let alias = names.next()?;
            let name = names.next()?;
            return self.qualified(&alias, &name);
        }
        let name = identifiers(node).next()?;
        self.lookup(&name)
    }

    /// UNIT-10: resolves a name to the one binding in scope for it.
    fn lookup(&mut self, token: &SyntaxToken) -> Option<Binding> {
        let name = token.text();
        let at = self.at(token);

        // A local, a parameter or a generic parameter, innermost first.
        let mut captured = false;
        let mut found = None;
        for frame in self.frames.iter().rev() {
            if let Some(&(bound, kind)) = frame.bound.get(name) {
                found = Some((bound, kind, captured));
                break;
            }
            // Past the frame of an anonymous function, a binding is one
            // of the function around it.
            captured |= frame.lambda;
        }
        if let Some((bound, kind, captured)) = found {
            // FN-14: "From the functions and blocks around it, its body
            // uses only parameters, generic parameters, `let` locals,
            // and iteration bindings."
            let usable = matches!(
                kind,
                LocalKind::Parameter
                    | LocalKind::GenericParameter
                    | LocalKind::Let
                    | LocalKind::Iteration
            );
            if captured && !usable {
                self.diagnostics.emit(
                    error(
                        Category::Name,
                        codes::NAME_CAPTURED,
                        at,
                        format!(
                            "an anonymous function cannot use `{name}`, {} of the function around it",
                            kind.describe()
                        ),
                        "FN-14",
                    )
                    .with_related(bound, "declared here"),
                );
            }
            return self.used(token, Binding::Local { at: bound, kind });
        }

        // A `use` of the file (UNIT-2: imports bind in their own file).
        let import = self.module.files[self.file]
            .imports
            .iter()
            .find(|import| import.name == name);
        if let Some(import) = import {
            if import.kind == ImportKind::Alias {
                self.diagnostics.emit(
                    error(
                        Category::Name,
                        codes::ALIAS_UNQUALIFIED,
                        at,
                        format!("`{name}` is a module alias and names nothing by itself"),
                        "UNIT-2",
                    )
                    .with_related(import.at, "the alias is declared here")
                    .with_note(format!("write `{name}::` and the name of a declaration")),
                );
                return None;
            }
            // An import that names nothing has been reported where it
            // stands.
            let target = import.module?;
            let declarations = self.public(target, name);
            if declarations.is_empty() {
                return None;
            }
            return self.used(
                token,
                Binding::Declarations {
                    module: target,
                    declarations,
                },
            );
        }

        // A declaration of the module.
        if let Some(bound) = self.module.scope.get(name) {
            let visible: Vec<usize> = bound
                .iter()
                .copied()
                .filter(|&index| self.in_test || !self.module.declarations[index].test_only)
                .collect();
            if visible.is_empty() {
                // TST-2: "Production cannot refer to test-only
                // declarations."
                let helper = &self.module.declarations[bound[0]];
                self.diagnostics.emit(
                    error(
                        Category::Name,
                        codes::NAME_TEST_ONLY,
                        at,
                        format!("`{name}` is declared for tests and cannot be used here"),
                        "TST-2",
                    )
                    .with_related(helper.at, "declared here, in a test context"),
                );
            }
            let declarations = if visible.is_empty() {
                bound.clone()
            } else {
                visible
            };
            return self.used(
                token,
                Binding::Declarations {
                    module: self.index,
                    declarations,
                },
            );
        }

        // UNIT-6: "Ordinary block locals become visible after
        // declaration."
        let later = self
            .frames
            .iter()
            .rev()
            .find_map(|frame| frame.later.get(name).copied());
        if let Some(later) = later {
            self.diagnostics.emit(
                error(
                    Category::Name,
                    codes::NAME_USED_BEFORE_DECLARATION,
                    at,
                    format!("`{name}` is used before it is declared"),
                    "UNIT-6",
                )
                .with_related(later, "declared here"),
            );
            return None;
        }

        // UNIT-7: what a default may not read.
        if let Some(hidden) = self.hidden.iter().find(|hidden| hidden.name == name) {
            self.diagnostics.emit(
                error(
                    Category::Name,
                    codes::DEFAULT_FORMAL_ARGUMENT,
                    at,
                    format!("a default cannot read `{name}`, {}", hidden.what),
                    "UNIT-7",
                )
                .with_related(hidden.at, "declared here"),
            );
            return None;
        }

        // UNIT-11: the prelude binds it, or nothing does. See
        // `Module::prelude_names`.
        if !self.prelude.iter().any(|known| known.name == name) {
            self.prelude.push(PreludeName {
                name: name.to_string(),
                at,
            });
        }
        None
    }

    /// UNIT-2: "an alias qualifies a public interface with
    /// `alias::name`."
    fn qualified(&mut self, alias: &SyntaxToken, name: &SyntaxToken) -> Option<Binding> {
        let import = self.module.files[self.file]
            .imports
            .iter()
            .find(|import| import.name == alias.text() && import.kind == ImportKind::Alias);
        let Some(import) = import else {
            self.diagnostics.emit(
                error(
                    Category::Name,
                    codes::ALIAS_UNKNOWN,
                    self.at(alias),
                    format!("`{}` is not a module alias of this file", alias.text()),
                    "UNIT-2",
                )
                .with_note(format!(
                    "declare one with `use <module> as {}`",
                    alias.text()
                )),
            );
            return None;
        };
        // An alias of a module that no file declares has been reported.
        let target = import.module?;
        let module = &self.program.modules[target];
        if module.lookup(name.text()).next().is_none() {
            self.diagnostics.emit(error(
                Category::Name,
                codes::IMPORT_UNKNOWN_NAME,
                self.at(name),
                format!("module `{}` declares no `{}`", module.path, name.text()),
                "UNIT-2",
            ));
            return None;
        }
        let declarations = self.public(target, name.text());
        if declarations.is_empty() {
            self.diagnostics.emit(
                error(
                    Category::Module,
                    codes::IMPORT_PRIVATE,
                    self.at(name),
                    format!("`{}` is private to module `{}`", name.text(), module.path),
                    "UNIT-2",
                )
                .with_note("a function or type is private unless marked `export` (UNIT-3)"),
            );
            return None;
        }
        self.used(
            name,
            Binding::Declarations {
                module: target,
                declarations,
            },
        )
    }

    /// The positions of the public declarations of `name` in module
    /// `target`.
    fn public(&self, target: usize, name: &str) -> Vec<usize> {
        let module = &self.program.modules[target];
        module
            .scope
            .get(name)
            .into_iter()
            .flatten()
            .copied()
            .filter(|&index| module.declarations[index].is_public())
            .collect()
    }

    // ---- Generic types (GEN-14) ----

    /// True when a binding is a generic type: a struct with generic
    /// parameters.
    fn is_generic_type(&self, binding: &Binding) -> bool {
        let Binding::Declarations {
            module,
            declarations,
        } = binding
        else {
            return false;
        };
        declarations.iter().any(|&index| {
            let declaration = &self.program.modules[*module].declarations[index];
            declaration.kind == DeclarationKind::Struct
                && child(&declaration.syntax, SyntaxKind::GenericParams).is_some()
        })
    }

    /// GEN-14: "Ordinary `<` and `>` do not order type names", and
    /// "Reject an invalid application without comparison fallback." A
    /// generic type's name before a `<` that the parser read as a
    /// comparison is an application its arguments do not complete.
    fn check_application(&mut self, node: &SyntaxNode) {
        let less_than = node
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .any(|token| token.kind() == SyntaxKind::Lt);
        if !less_than {
            return;
        }
        let Some(left) = node.children().next() else {
            return;
        };
        if left.kind() != SyntaxKind::NameExpr {
            return;
        }
        // The name the operand resolved by: of `alias::name`, the one
        // after the `::`.
        let name = match child(&left, SyntaxKind::QualifiedName) {
            Some(qualified) => identifiers(&qualified).nth(1),
            None => identifiers(&left).next(),
        };
        let Some(name) = name else {
            return;
        };
        // The name was resolved when the operand was walked.
        let at = self.at(&name);
        let generic = self
            .uses
            .iter()
            .rev()
            .find(|used| used.at == at)
            .is_some_and(|used| self.is_generic_type(&used.binding));
        if generic {
            self.diagnostics.emit(
                error(
                    Category::Type,
                    codes::GENERIC_INVALID_APPLICATION,
                    at,
                    format!(
                        "`{}` is a generic type: `<` after it opens its arguments, and these \
                         are not arguments",
                        name.text()
                    ),
                    "GEN-14",
                )
                .with_note(format!(
                    "a generic type is applied as `{}<...>(...)`",
                    name.text()
                )),
            );
        }
    }
}
