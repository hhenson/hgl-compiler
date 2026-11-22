//! Files and declarations: the units-and-names, functions-and-phases,
//! types-and-values, generics-and-operators, native-boundaries and
//! tests-and-eval chapters.

// Brings every kind into scope by its bare name, so that the grammar reads
// `self.at(KwFn)` and not `self.at(SyntaxKind::KwFn)`.
use crate::kind::SyntaxKind::*;
use crate::parser::{Bracket, Context, Parser};

/// The declaration that starts at the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Item {
    Module,
    Use,
    Function,
    Struct,
    Enum,
    Operator,
    Instantiate,
    NativeType,
    NativeFunction,
    Constant,
    Test,
    TestContext,
    /// An application's run statement: `run call` or `eval call`.
    Run,
    /// A statement that sets an application's store: `inject ...`, or
    /// a call.
    Store,
    /// `export` before something that is no declaration.
    StrayExport,
}

impl Parser<'_> {
    // ---- Roots ----

    /// ```text
    /// source_file = [ NL ], ( module_decl | application_decl ),
    ///               { NL, top_level_item }, [ NL ], EOF ;
    /// ```
    pub(crate) fn source_file(&mut self) {
        self.start(SourceFile);
        // UNIT-1: a file names one module, before anything else.
        // UNIT-14: "An application is one file, headed `application
        // name`, where a module's files are headed `module name`."
        if self.at(KwModule) {
            let errors = self.errors;
            self.module_decl();
            self.end_of_item(errors, false);
        } else if self.at_application() {
            let errors = self.errors;
            self.application_decl();
            self.end_of_item(errors, false);
        } else {
            self.expected_rule("`module` or `application`", "UNIT-1");
        }
        self.items(false);
        self.finish();
    }

    /// Declarations with no `module` line before them.
    pub(crate) fn declarations(&mut self) {
        self.start(Fragment);
        self.items(false);
        self.finish();
    }

    /// Statements with no block around them.
    pub(crate) fn statements(&mut self) {
        self.start(Fragment);
        loop {
            self.lines(
                "a statement",
                "LEX-6",
                |_| false,
                |parser| parser.statement(),
            );
            if self.at(Eof) {
                break;
            }
            // `lines` stops at a `}`; here no block is open for it to close.
            self.begin_entry();
            self.expected("a statement");
            self.leaf(Error);
        }
        self.finish();
    }

    // ---- Items ----

    /// `{ NL, top_level_item }`, and the items of a test context when
    /// `in_test`:
    ///
    /// ```text
    /// top_level_item = use_decl | declaration ;
    /// declaration = function_decl | struct_decl | enum_decl | operator_decl
    ///             | instantiate_decl | native_type_decl | native_function_decl
    ///             | test_decl | test_context ;
    /// test_item = function_decl | struct_decl | enum_decl | test_decl ;
    /// ```
    fn items(&mut self, in_test: bool) {
        loop {
            if self.at(Eof) || (in_test && self.at(RBrace)) {
                break;
            }
            // The line breaks before the item have separated it from the
            // last one and are not counted against its first token.
            self.begin_entry();
            let Some(item) = self.item_kind() else {
                self.expected("a declaration");
                self.skip_to_item(in_test);
                continue;
            };
            match item {
                // UNIT-1: one `module` line, and `source_file` has taken it.
                Item::Module => self.error(
                    "a file has one `module` line, and it comes first",
                    "UNIT-1",
                    None,
                ),
                // RUN-1: a run statement is an item of a file. Whether
                // the file is an application's is the analyser's to say
                // (`run.position`).
                Item::Run if !in_test => {}
                // UNIT-14: so is a statement that sets the run's store.
                Item::Store if !in_test => {}
                Item::Function | Item::Struct | Item::Enum | Item::Test | Item::StrayExport => {}
                // TST-1: an unnamed test context holds helper functions,
                // types and named tests. The item is parsed all the same.
                _ if in_test => self.error(
                    "a test context holds functions, structs, enums and named tests",
                    "TST-1",
                    None,
                ),
                _ => {}
            }
            let errors = self.errors;
            self.item(item);
            self.end_of_item(errors, in_test);
        }
    }

    /// Checks what follows an item. `errors` is the error count from
    /// before the item.
    fn end_of_item(&mut self, errors: usize, in_test: bool) {
        if self.errors != errors {
            // The item is broken; what is left of its line follows from
            // that and is not reported again.
            self.skip_line(false);
        } else if !(self.at(Eof) || self.at_line_start() || (in_test && self.at(RBrace))) {
            // LEX-5: a line break separates declarations.
            self.expected_rule("a line break", "LEX-5");
        }
    }

    /// Says which declaration starts at the cursor, from the tokens there.
    /// `None` when none does.
    ///
    /// Every declaration is recognised here, before any of it is parsed,
    /// so that its node can be opened knowing its kind.
    fn item_kind(&self) -> Option<Item> {
        match self.current() {
            KwModule => return Some(Item::Module),
            KwUse => return Some(Item::Use),
            // TST-17: `@timeout(...)` stands before an integration test.
            // LEX-10: `timeout` is a name with that meaning only here.
            At if self.nth_word(1, "timeout") => return Some(Item::Test),
            // ERR-10: any other annotation stands before a function.
            At => return Some(Item::Function),
            // RUN-1: `eval` or `run` where an item starts begins a run
            // statement. Both are reserved words (LEX-10).
            KwEval | KwRun => return Some(Item::Run),
            // UNIT-14: `store_statement = inject_decl | IDENTIFIER,
            // call_suffix`, what an application sets its run's store
            // with. No declaration begins with either.
            KwInject => return Some(Item::Store),
            Ident if self.nth(1) == LParen => return Some(Item::Store),
            _ => {}
        }
        // `export` may come first; the declaration is named by what
        // follows it.
        let exported = self.at(KwExport);
        let n = usize::from(exported);
        let item = match self.nth(n) {
            KwFn | KwImpl => Item::Function,
            KwAbstract | KwStruct => Item::Struct,
            KwEnum => Item::Enum,
            KwOperator => Item::Operator,
            KwInstantiate => Item::Instantiate,
            KwTest if self.nth(n + 1) == LBrace => Item::TestContext,
            KwTest => Item::Test,
            // LEX-10, TST-17: `integration_test` is a name that takes
            // its meaning "before a test's name".
            Ident if self.nth_word(n, "integration_test") && self.nth(n + 1) == Ident => Item::Test,
            // LEX-9, LEX-10: `const` and `native` are names that take a
            // meaning from where they stand. Before `fn` or `type` at the
            // start of a declaration, they are modifiers.
            Ident if self.nth_word(n, "const") && self.nth(n + 1) == KwFn => Item::Function,
            // UNIT-13: before a name, `const` begins a constant.
            Ident if self.nth_word(n, "const") && self.nth(n + 1) == Ident => Item::Constant,
            Ident if self.nth_word(n, "native") && self.nth_word(n + 1, "type") => Item::NativeType,
            // `native impl` begins a native candidate of an operator:
            // `impl` is a reserved word, so nothing else follows `native`
            // with it.
            Ident
                if self.nth_word(n, "native")
                    && (self.nth(n + 1) == KwFn
                        || self.nth(n + 1) == KwImpl
                        || self.nth_word(n + 1, "const") && self.nth(n + 2) == KwFn) =>
            {
                Item::NativeFunction
            }
            _ if exported => Item::StrayExport,
            _ => return None,
        };
        Some(item)
    }

    /// Parses the declaration `item_kind` found at the cursor.
    fn item(&mut self, item: Item) {
        match item {
            Item::Module => self.module_decl(),
            Item::Use => self.use_decl(),
            Item::Function => self.function_decl(false),
            Item::Struct => self.struct_decl(),
            Item::Enum => self.enum_decl(),
            Item::Operator => self.operator_decl(),
            Item::Instantiate => self.instantiate_decl(),
            Item::NativeType => self.native_type_decl(),
            Item::NativeFunction => self.native_function_decl(),
            Item::Constant => self.constant_decl(),
            Item::Test => self.test_decl(),
            Item::TestContext => self.test_context(),
            Item::Run => self.run_statement(),
            Item::Store => self.store_statement(),
            Item::StrayExport => {
                self.leaf(Error);
                self.expected("a declaration");
            }
        }
    }

    /// Steps over tokens, into one `Error` node, up to the next line that
    /// starts a declaration. Takes at least one token.
    fn skip_to_item(&mut self, in_test: bool) {
        let mut depth = 0usize;
        self.start(Error);
        loop {
            match self.current() {
                LBrace => depth += 1,
                RBrace => depth = depth.saturating_sub(1),
                _ => {}
            }
            self.bump();
            if self.at(Eof) {
                break;
            }
            if depth == 0 {
                // The `}` that closes the enclosing test context.
                if in_test && self.at(RBrace) {
                    break;
                }
                if self.at_line_start() && self.item_kind().is_some() {
                    break;
                }
            }
        }
        self.finish();
    }

    /// Takes an `export` that stands before a declaration the grammar
    /// gives no exported form, and reports it.
    fn no_export(&mut self) {
        if self.at(KwExport) {
            self.bump();
            self.expected(
                "`fn`, `const fn`, `const`, `struct`, `enum`, `operator` or `native type`",
            );
        }
    }

    /// ```text
    /// constant_decl = [ "export" ], "const", IDENTIFIER, ":", value_type,
    ///     "=", const_expression ;
    /// ```
    ///
    /// UNIT-13: "A constant, `const name: T = expression`, names one
    /// ordinary value of the type T."
    fn constant_decl(&mut self) {
        self.start(ConstantDecl);
        self.eat(KwExport);
        self.expect_word("const");
        self.expect_name("a constant's name");
        if self.eat(Colon) {
            self.type_();
        } else {
            self.expected_rule("`:`", "UNIT-13");
        }
        // The value is on the declaration's line, as a field's default
        // is: an `=` on a later line begins nothing (LEX-5).
        if self.at_assign() && !self.nl() {
            self.eat_assign();
            self.expression();
        } else {
            self.expected_rule("`=`", "UNIT-13");
        }
        self.finish();
    }

    // ---- Units and names ----

    /// `module_decl = "module", module_path, [ "part", IDENTIFIER ] ;`
    fn module_decl(&mut self) {
        self.start(ModuleDecl);
        self.bump();
        self.module_path();
        if self.eat(KwPart) {
            self.expect_name("a part name");
        }
        self.finish();
    }

    /// True at `application` followed by a name. LEX-10: `application`
    /// is a name that takes its meaning "at the head of a file".
    fn at_application(&self) -> bool {
        self.nth_word(0, "application") && self.nth(1) == Ident
    }

    /// `application_decl = "application", module_path ;`
    fn application_decl(&mut self) {
        self.start(ApplicationDecl);
        self.bump();
        self.module_path();
        self.finish();
    }

    /// ```text
    /// run_statement = run_expression | eval_expression ;
    /// ```
    ///
    /// The cursor is on `run` or `eval`.
    fn run_statement(&mut self) {
        self.start(RunStmt);
        let kind = if self.at(KwEval) { EvalExpr } else { RunExpr };
        self.run_form(kind);
        self.finish();
    }

    /// `store_statement = inject_decl | IDENTIFIER, call_suffix ;`
    ///
    /// UNIT-14: "Between its declarations and its run statement it may
    /// inject `run_config`, giving the run's configuration its defaults
    /// (RUN-6), and set the run's store: `inject global_state`, and then
    /// calls of `put` on it." Which names and which calls is the
    /// analyser's to say (`application.statement`).
    fn store_statement(&mut self) {
        self.start(StoreStmt);
        if self.at(KwInject) {
            self.inject_decl();
        } else {
            // `item_kind` has seen a name and its `(`: a call, and no
            // more of an expression than that.
            self.named_call_stmt();
        }
        self.finish();
    }

    /// `module_path = IDENTIFIER, { ".", IDENTIFIER } ;`
    fn module_path(&mut self) {
        self.start(ModulePath);
        self.expect_name("a module name");
        while self.eat(Dot) {
            self.expect_name("a module name");
        }
        self.finish();
    }

    /// ```text
    /// use_decl = "use", module_path, ( "::", import_set | "as", IDENTIFIER ) ;
    /// import_set = "{", import_name, { ",", import_name }, [ "," ], "}" ;
    /// ```
    fn use_decl(&mut self) {
        self.start(UseDecl);
        self.bump();
        self.module_path();
        if self.eat(ColonColon) {
            if self.at(LBrace) && !self.cut() {
                self.start(ImportSet);
                self.bump();
                // LEX-5: line breaks do not count inside an import set.
                self.nested(Context::PLAIN, |parser| {
                    if parser.at(RBrace) {
                        parser.expected("a name");
                    }
                    parser.list(RBrace, "a name", true, |parser| {
                        parser.eat(Ident);
                    });
                });
                self.finish();
            } else {
                self.expected("`{`");
            }
        } else if self.eat(KwAs) {
            self.expect_name("a name");
        } else {
            self.expected("`::` or `as`");
        }
        self.finish();
    }

    // ---- Functions ----

    /// ```text
    /// function_decl = { warning_annotation, NL },
    ///     [ "export" | "impl" ], [ "const" ], "fn",
    ///     callable_name, [ generic_parameters ], function_signature,
    ///     [ requires_clause ], function_body ;
    /// function_body = "=>", expression | block ;
    /// ```
    fn function_decl(&mut self, test_helper: bool) {
        self.start(FnDecl);
        // ERR-10: `@suppress_warning("...")` stands on the lines before a
        // named function.
        let mut annotated = false;
        while self.at(At) {
            annotated = true;
            self.warning_annotation();
            if self.at_line_start() {
                self.begin_entry();
            } else if !self.at(Eof) {
                self.expected_rule("a line break", "ERR-10");
            }
        }
        if annotated && self.item_kind() != Some(Item::Function) {
            self.expected_rule("a function declaration", "ERR-10");
            self.finish();
            return;
        }

        if test_helper && matches!(self.current(), KwExport | KwImpl) {
            self.error(
                "a test helper must be a private `fn` or `const fn`",
                "TST-1",
                None,
            );
        }
        if self.eat(KwExport) {
            if self.at(KwImpl) && !self.cut() {
                // The grammar has `export` or `impl`, and no form with
                // both: a candidate is reached through its operator
                // (UNIT-3).
                self.expected("`fn` or `const`");
                self.bump();
            }
        } else {
            self.eat(KwImpl);
        }
        // FN-5: `const` is a modifier here because `fn` follows it. As the
        // function's name, after `fn`, it is an ordinary identifier.
        // GEN-4: `impl const fn` is a value candidate of its operator.
        if self.at_word("const") && self.nth(1) == KwFn && !self.cut() {
            self.bump();
        }
        self.expect(KwFn);
        self.expect_name("a function name");
        self.callable_header();
        // LEX-5: `=>` cannot begin a declaration, so it may start a line.
        if self.at(FatArrow) && !self.cut() {
            self.arrow_body();
        } else if self.at(LBrace) {
            self.body_block();
        } else {
            self.expected("`=>` or `{`");
        }
        self.finish();
    }

    /// `warning_annotation = "@", "suppress_warning", "(", STRING_LITERAL, ")" ;`
    fn warning_annotation(&mut self) {
        self.start(WarningAnnotation);
        self.bump();
        // LEX-10: `suppress_warning` is a name with a meaning only here.
        if self.at_word("suppress_warning") && !self.cut() {
            self.bump();
        } else {
            self.expected_rule("`suppress_warning`", "ERR-10");
        }
        if self.at(LParen) && !self.cut() {
            // LEX-27: an annotation's argument is a call's.
            self.attached(Bracket::Call);
            self.bump();
            self.nested(Context::PLAIN, |parser| {
                parser.expect(Str);
                parser.close(RParen);
            });
        } else {
            self.expected("`(`");
        }
        self.finish();
    }

    /// The part of a declaration shared by functions, operators and native
    /// functions: `[ generic_parameters ], [ configuration_parameters ], function_signature,
    /// [ requires_clause ]`.
    fn callable_header(&mut self) {
        if self.at(Lt) && !self.nl() {
            self.generic_params();
        }
        // FN-18: a declaration puts its configuration parameters before the parentheses.
        if self.at(LBrace) && !self.cut() {
            self.start(ConfigParamList);
            self.bump();
            self.nested(Context::LIST, |parser| {
                if parser.at(RBrace) {
                    parser.expected_rule("a configuration parameter", "FN-18");
                }
                parser.list(RBrace, "a configuration parameter", true, |parser| {
                    parser.parameter();
                });
            });
            self.finish();
        }
        self.signature();
        // A `requires` clause may stand on its own line.
        if self.at(KwRequires) && !self.nl() {
            self.requires_clause();
        }
    }

    /// ```text
    /// function_signature = "(", [ parameters ], ")", [ "->", [ "ref" ], type ] ;
    /// parameters = parameter, { ",", parameter }, [ "," ] ;
    /// ```
    fn signature(&mut self) {
        // The parameter list is required, so one line break before it is
        // ignored: the declaration is incomplete without it.
        if self.at(LParen) && !self.cut() {
            // LEX-27: `fn f(x)` and `fn f<T>(x)`, with nothing before the
            // `(`.
            self.attached(Bracket::Parameters);
            self.start(ParamList);
            self.bump();
            self.nested(Context::LIST, |parser| {
                parser.list(RParen, "a parameter", true, |parser| parser.parameter());
            });
            self.finish();
        } else {
            self.expected("`(`");
        }
        // LEX-5: a result arrow may start a line.
        if self.at(Arrow) && !self.nl() {
            self.start(ReturnType);
            self.bump();
            // FN-16: "and its result, `-> ref f64`". The mark is a mode
            // of the result, no part of its type; which functions may
            // take it is for the analyser.
            self.eat(KwRef);
            self.type_();
            self.finish();
        }
    }

    /// `"->", type`: the result of an anonymous function or of a
    /// constraint call, which takes no `ref` (FN-16 gives the mark to a
    /// declared function's result alone).
    pub(crate) fn return_type(&mut self) {
        self.start(ReturnType);
        self.bump();
        self.type_();
        self.finish();
    }

    /// `"=>", expression`
    ///
    /// FN-10: the body is the whole expression, up to the enclosing
    /// delimiter, a comma at the level of the enclosing list, or a line
    /// break that counts. That is where `expression` stops in any case.
    pub(crate) fn arrow_body(&mut self) {
        self.start(ArrowBody);
        self.bump();
        self.expression();
        self.finish();
    }

    /// ```text
    /// parameter = [ "ref" ], IDENTIFIER, ":", type, [ "=", const_expression ]
    ///           | "const", IDENTIFIER, ":", value_type, [ "=", const_expression ]
    ///           | "var", IDENTIFIER, ":", value_type
    ///           | [ "const" ], IDENTIFIER, ":", pack, [ "=", const_expression ]
    ///           | IDENTIFIER, ":", function_type ;
    /// pack = "...", type, [ cardinality ]
    ///      | "...", "{", type, "}", [ cardinality ] ;
    /// ```
    ///
    /// One permissive form is parsed: an optional `const`, a name, a type
    /// or pack, an optional default. The grammar admits `const` and a
    /// default on a pack so that FN-8 rejects them by rule; that and
    /// which other combinations mean something is for the analyser.
    ///
    /// FN-15: a `var` parameter is `"var", IDENTIFIER, ":", value_type`,
    /// and no more: it is no pack and has no default, so neither is
    /// read after one, and what is written there is not expected.
    ///
    /// FN-16: a `ref` parameter is `"ref"`, a name, a type and an
    /// optional default: "A `ref` parameter is neither `const` nor a
    /// pack", so no `const` and no `...` is read after the mark. Which
    /// functions may take it is for the analyser.
    fn parameter(&mut self) {
        let var = self.at(KwVar);
        let reference = self.at(KwRef);
        let constant = self.at_word("const") && self.nth(1) == Ident;
        if !self.at(Ident) && !var && !reference {
            return;
        }
        self.start(Param);
        if var || reference {
            self.bump();
            if !self.at(Ident) {
                self.expected("a parameter's name");
                self.finish();
                return;
            }
        } else if constant {
            // FN-5, LEX-9: `const` is a modifier only before the
            // parameter's name. In `const: i64` it is the name.
            self.bump();
        }
        self.bump();
        if self.eat(Colon) {
            if !var
                && !reference
                && !constant
                && (self.at(KwFn) || self.at_word("const") && self.nth(1) == KwFn)
            {
                self.function_type();
                self.finish();
                return;
            }
            if !var && !reference && self.eat(Ellipsis) {
                // FN-8: a pack takes any number of arguments. `...T` is a
                // positional pack; `...{T}` a named one.
                if self.at(LBrace) {
                    self.start(PackType);
                    self.bump();
                    self.nested(Context::PLAIN, |parser| {
                        parser.type_();
                        parser.close(RBrace);
                    });
                    self.finish();
                } else {
                    self.type_();
                }
                // GEN-10: a cardinality follows the pack it bounds.
                if self.at(LBrace) {
                    self.cardinality();
                }
            } else {
                self.type_();
            }
        } else {
            self.expected("`:`");
        }
        if !var && self.at_assign() {
            self.eat_assign();
            self.expression();
        }
        self.finish();
    }

    /// FN-17: `[ "const" ], "fn", "(", [ function_type_parameters ],
    /// ")", [ "->", [ "ref" ], type ]`. Fixed inputs may name a position.
    /// Named packs in this template await GEN-17.
    fn function_type(&mut self) {
        self.start(FunctionType);
        if self.at_word("const") {
            self.bump();
        }
        self.expect(KwFn);
        if self.at(LParen) {
            self.attached(Bracket::Parameters);
            self.start(ParamList);
            self.bump();
            self.nested(Context::LIST, |parser| {
                parser.list(RParen, "a function type parameter", true, |parser| {
                    parser.start(FunctionTypeParam);
                    if parser.at(Ident) && parser.nth(1) == Colon {
                        parser.bump();
                        parser.bump();
                    }
                    parser.type_();
                    parser.finish();
                });
            });
            self.finish();
        } else {
            self.expected("`(`");
        }
        if self.at(Arrow) && !self.nl() {
            self.start(ReturnType);
            self.bump();
            self.eat(KwRef);
            self.type_();
            self.finish();
        }
        self.finish();
    }

    // ---- Structures and enumerations ----

    /// ```text
    /// struct_decl = [ "export" ], [ "abstract" ], "struct", IDENTIFIER,
    ///     [ generic_parameters ], [ ":", named_type, { ",", named_type } ],
    ///     [ requires_clause ],
    ///     "{", [ NL ], [ struct_member, { NL, struct_member }, [ NL ] ], "}" ;
    /// ```
    fn struct_decl(&mut self) {
        self.start(StructDecl);
        self.eat(KwExport);
        self.eat(KwAbstract);
        self.expect(KwStruct);
        self.expect_name("a struct name");
        if self.at(Lt) && !self.nl() {
            self.generic_params();
        }
        if self.at(Colon) && !self.nl() {
            self.start(StructBases);
            self.bump();
            loop {
                if self.at(Ident) && !self.cut() {
                    self.named_type();
                } else {
                    self.expected("a type name");
                    break;
                }
                if !self.eat(Comma) {
                    break;
                }
            }
            self.finish();
        }
        if self.at(KwRequires) && !self.nl() {
            self.requires_clause();
        }
        if self.at(LBrace) {
            self.header_brace();
            self.bump();
            self.nested(Context::BLOCK, |parser| {
                // LEX-5: one member to a line.
                parser.lines(
                    "a field",
                    "LEX-5",
                    |_| false,
                    |parser| parser.struct_member(),
                );
                parser.close_brace();
            });
        } else {
            self.expected("`{`");
        }
        self.finish();
    }

    /// ```text
    /// struct_member = member_name, ":", type, [ "=", const_expression ]
    ///               | member_name, "=", const_expression ;
    /// ```
    fn struct_member(&mut self) {
        // LEX-9: `time` may name a field.
        if !self.at_member_name() {
            return;
        }
        self.start(StructMember);
        self.bump();
        if self.eat(Colon) {
            self.type_();
            if self.at_assign() && !self.nl() {
                self.eat_assign();
                self.expression();
            }
        } else if self.at_assign() && !self.nl() {
            self.eat_assign();
            self.expression();
        } else {
            self.expected("`:` or `=`");
        }
        self.finish();
    }

    /// ```text
    /// enum_decl = [ "export" ], "enum", IDENTIFIER,
    ///     "{", [ NL ], [ enum_member, { ",", [ NL ], enum_member }, [ "," ] ], [ NL ], "}" ;
    /// enum_member = member_name, [ "=", const_expression ] ;
    /// ```
    fn enum_decl(&mut self) {
        self.start(EnumDecl);
        self.eat(KwExport);
        self.expect(KwEnum);
        self.expect_name("an enum name");
        if self.at(LBrace) {
            self.header_brace();
            self.bump();
            // Line breaks count here: the grammar admits one after `{`,
            // after a comma and before `}`, and the list helper asks for
            // them in just those places.
            self.nested(Context::BLOCK, |parser| {
                parser.list(RBrace, "an enum member", true, |parser| {
                    // LEX-9: a member may take a type's reserved name,
                    // as the `error` of `/{zero: .error}` does.
                    if !parser.at_member_name() {
                        return;
                    }
                    parser.start(EnumMember);
                    parser.bump();
                    if parser.at_assign() && !parser.nl() {
                        parser.eat_assign();
                        parser.expression();
                    }
                    parser.finish();
                });
            });
        } else {
            self.expected("`{`");
        }
        self.finish();
    }

    // ---- Operators ----

    /// ```text
    /// operator_decl = [ "export" ], "operator", callable_name, [ generic_parameters ],
    ///     function_signature, [ requires_clause ], { operator_properties } ;
    /// ```
    fn operator_decl(&mut self) {
        self.start(OperatorDecl);
        // UNIT-3 and GEN-4: `export` belongs to the operator declaration.
        self.eat(KwExport);
        self.expect(KwOperator);
        self.expect_name("an operator name");
        self.callable_header();
        // LEX-10: `properties` is a name with a meaning only here. Like
        // `requires`, a properties clause may stand on its own line.
        // LEX-5: a `properties` clause continues its operator declaration.
        while self.at_word("properties") && !self.cut() {
            self.operator_properties();
        }
        self.finish();
    }

    /// ```text
    /// operator_properties = "properties", "<", value_type, { ",", value_type }, ">",
    ///     "{", operator_property, { ",", operator_property }, [ "," ], "}" ;
    /// ```
    fn operator_properties(&mut self) {
        self.start(OperatorProperties);
        self.bump();
        if self.at(Lt) && !self.nl() {
            self.attached(Bracket::Generics);
            self.angle_list("a type", false, false, |parser| {
                parser.type_();
                true
            });
        } else {
            self.expected("`<`");
        }
        if self.at(LBrace) && !self.cut() {
            self.bump();
            // LEX-5: line breaks do not count inside a property list.
            self.nested(Context::PLAIN, |parser| {
                if parser.at(RBrace) {
                    parser.expected("a property");
                }
                parser.list(RBrace, "a property", true, |parser| {
                    parser.operator_property()
                });
            });
        } else {
            self.expected("`{`");
        }
        self.finish();
    }

    /// `operator_property = "associative" | "commutative" | "identity", "=", const_expression ;`
    fn operator_property(&mut self) {
        if !self.at(Ident) {
            return;
        }
        let known = ["associative", "commutative", "identity"].contains(&self.text(0));
        if !known {
            self.expected("`associative`, `commutative` or `identity`");
        }
        self.start(OperatorProperty);
        let identity = self.at_word("identity");
        self.bump();
        if identity {
            if self.at_assign() {
                self.eat_assign();
                self.expression();
            } else {
                self.expected("`=`");
            }
        } else if self.at_assign() {
            // Only `identity` has a value; the others are bare markers.
            // The value is read all the same, so that the list goes on.
            self.expected("`,` or `}`");
            self.eat_assign();
            self.expression();
        }
        self.finish();
    }

    /// ```text
    /// instantiate_decl = "instantiate", instantiation, { ",", instantiation }, [ "," ] ;
    /// instantiation = callable_name, "<", materialization_argument,
    ///     { ",", materialization_argument }, [ "," ], ">" ;
    /// ```
    fn instantiate_decl(&mut self) {
        self.start(InstantiateDecl);
        self.no_export();
        self.expect(KwInstantiate);
        loop {
            self.start(Instantiation);
            self.expect_name("a function name");
            if self.at(Lt) && !self.nl() {
                self.attached(Bracket::Generics);
                self.start(GenericArgs);
                self.angle_list("a type or a value", true, false, |parser| {
                    parser.materialization_argument()
                });
                self.finish();
            } else {
                self.expected("`<`");
            }
            self.finish();
            if !self.eat(Comma) {
                break;
            }
            // A comma may end the list. It goes on only when another
            // instantiation follows.
            if !(self.at(Ident) && self.nth(1) == Lt) || self.cut() {
                break;
            }
        }
        self.finish();
    }

    // ---- Native boundaries ----

    /// ```text
    /// native_type_decl = [ "export" ], "native", "type", IDENTIFIER,
    ///     [ native_type_body ] ;
    /// native_type_body = "{", [ NL ],
    ///     [ native_property, { NL, native_property }, [ NL ] ], "}" ;
    /// native_property = member_name, ":", value_type ;
    /// ```
    ///
    /// NATIVE-6: "A native type may declare read-only properties, each a
    /// name and an ordinary value type, written as a struct's typed
    /// fields are."
    fn native_type_decl(&mut self) {
        self.start(NativeTypeDecl);
        self.eat(KwExport);
        self.expect_word("native");
        self.expect_word("type");
        self.expect_name("a type name");
        // The body is on the declaration's line, as a struct's is: a `{`
        // on a later line begins another statement (LEX-5).
        if self.at(LBrace) && !self.nl() {
            self.bump();
            self.nested(Context::BLOCK, |parser| {
                // LEX-5: one property to a line.
                parser.lines(
                    "a property",
                    "LEX-5",
                    |_| false,
                    |parser| parser.native_property(),
                );
                parser.close_brace();
            });
        }
        self.finish();
    }

    /// `native_property = member_name, ":", value_type ;`
    fn native_property(&mut self) {
        // LEX-9: any word names a member. What is no name is left for
        // the caller's line discipline, as a struct's member is.
        if !self.at_member_name() {
            return;
        }
        self.start(NativeProperty);
        self.bump();
        if self.eat(Colon) {
            self.type_();
        } else {
            self.expected("`:`");
        }
        self.finish();
    }

    /// ```text
    /// native_function_decl = [ "export" ], "native", [ "const" ], "fn",
    ///     callable_name, [ generic_parameters ], function_signature,
    ///     [ requires_clause ]
    ///   | "native", [ "impl" ], [ "const" ], "fn", callable_name,
    ///     [ generic_parameters ], function_signature, [ requires_clause ],
    ///     [ native_contract_body ] ;
    /// ```
    ///
    /// NATIVE-1: "A native function that is no candidate is private
    /// unless exported, as a function is (UNIT-3). `export` is written
    /// on its declaration, the form with no body, and nowhere else."
    fn native_function_decl(&mut self) {
        self.start(NativeFnDecl);
        let exported = self.eat(KwExport);
        self.expect_word("native");
        // NATIVE-1: `native impl` declares a candidate of an operator.
        if self.at(KwImpl) && !self.cut() {
            if exported {
                // The grammar has `export` or `impl`, and no form with
                // both: a candidate is reached through its operator
                // (UNIT-3).
                self.expected("`fn` or `const`");
            }
            self.bump();
        }
        if self.at_word("const") && !self.cut() {
            self.bump();
        }
        self.expect(KwFn);
        self.expect_name("a function name");
        self.callable_header();
        // NATIVE-3: no body is a declaration; `{}` and `{ when; }` are
        // implementations. Nothing but the body can begin with `{` here,
        // so one on the next line is a misplaced body (LEX-5).
        if self.at(LBrace) {
            if exported {
                // The exported form is the declaration, which has no
                // body: the part that implements it does not repeat
                // `export`.
                self.expected("a line break");
            }
            self.header_brace();
            self.native_contract_body();
        }
        self.finish();
    }

    /// ```text
    /// native_contract_body = "{", [ NL ], { ( inject_decl, [ ";" ] | native_hook ), [ NL ] }, "}" ;
    /// native_hook = ( "start" | "when" | "stop" ), ";" ;
    /// ```
    fn native_contract_body(&mut self) {
        self.start(NativeContractBody);
        self.bump();
        self.nested(Context::BLOCK, |parser| {
            loop {
                match parser.current() {
                    RBrace | Eof => break,
                    KwInject => {
                        parser.begin_entry();
                        parser.inject_decl();
                        parser.eat(Semicolon);
                    }
                    // LEX-4: the one place a semicolon belongs.
                    KwStart | KwWhen | KwStop if parser.nth(1) == Semicolon && !parser.nl_at(1) => {
                        parser.start(NativeHook);
                        parser.bump();
                        parser.bump();
                        parser.finish();
                    }
                    _ => {
                        // NATIVE-3: a contract body states what the native
                        // code does; it holds no statements.
                        parser.begin_entry();
                        parser.expected_rule("`inject`, `start;`, `when;` or `stop;`", "NATIVE-3");
                        parser.skip_line(true);
                    }
                }
            }
            parser.close_brace();
        });
        self.finish();
    }

    // ---- Tests ----

    /// ```text
    /// test_decl = "test", IDENTIFIER, test_block
    ///           | [ timeout_annotation, NL ], "integration_test", IDENTIFIER, test_block ;
    /// ```
    fn test_decl(&mut self) {
        self.start(TestDecl);
        // TST-17: `@timeout(5s)` stands on the line before an
        // integration test, and before nothing else.
        if self.at(At) {
            self.timeout_annotation();
            if self.at_line_start() {
                self.begin_entry();
            } else if !self.at(Eof) {
                self.expected_rule("a line break", "TST-17");
            }
            if !(self.nth_word(0, "integration_test") && self.nth(1) == Ident) {
                self.expected_rule("an integration test", "TST-17");
                self.finish();
                return;
            }
        }
        self.no_export();
        // `item_kind` has seen which of the two words this is.
        if self.nth_word(0, "integration_test") {
            self.bump();
        } else {
            self.expect(KwTest);
        }
        self.expect_name("a test name");
        if self.at(LBrace) {
            self.header_brace();
            self.block_with(Self::test_block_contents);
        } else {
            self.expected("`{`");
        }
        self.finish();
    }

    /// TST-1: a named test starts with its helper functions, before any
    /// statements. Each helper declaration is followed by a line break.
    fn test_block_contents(&mut self) {
        while self.at_test_helper() {
            self.begin_entry();
            let errors = self.errors;
            self.function_decl(true);
            if self.errors != errors {
                self.skip_line(false);
            } else if !self.at_line_start() {
                self.expected_rule("a line break after a test helper", "TST-1");
            }
        }
        self.block_statements(|parser| {
            if parser.at_test_helper() {
                parser.error(
                    "a test helper must precede the test's statements",
                    "TST-1",
                    None,
                );
                // Read the whole misplaced declaration for recovery, so
                // its body braces cannot close the test's block.
                parser.start(Error);
                parser.function_decl(true);
                parser.finish();
            } else {
                parser.statement();
            }
        });
    }

    /// A named function or an attached warning annotation. A named
    /// function has a name where an anonymous one's parameters begin.
    fn at_test_helper(&self) -> bool {
        if self.at(At) && self.nth_word(1, "suppress_warning") {
            return true;
        }
        // Recognize forbidden modifiers too, to diagnose them on a
        // declaration while recovering its body as a unit.
        let visibility = usize::from(matches!(self.current(), KwExport | KwImpl));
        let modifier = visibility + usize::from(self.nth_word(visibility, "const"));
        self.nth(modifier) == KwFn && self.nth(modifier + 1) == Ident
    }

    /// `timeout_annotation = "@", "timeout", "(", TEMPORAL_LITERAL, ")" ;`
    ///
    /// TST-17: "The annotation's literal is a duration".
    fn timeout_annotation(&mut self) {
        self.start(TimeoutAnnotation);
        self.bump();
        // `item_kind` has seen the word.
        self.bump();
        if self.at(LParen) && !self.cut() {
            // LEX-27: an annotation's argument is a call's.
            self.attached(Bracket::Call);
            self.bump();
            self.nested(Context::PLAIN, |parser| {
                if parser.at(Duration) {
                    parser.bump();
                } else {
                    parser.expected_rule("a duration", "TST-17");
                }
                parser.close(RParen);
            });
        } else {
            self.expected("`(`");
        }
        self.finish();
    }

    /// `test_context = "test", "{", [ NL ], [ test_item, { NL, test_item }, [ NL ] ], "}" ;`
    fn test_context(&mut self) {
        self.start(TestContext);
        self.no_export();
        self.expect(KwTest);
        // A context inside a context is reported (TST-1) and parsed, so
        // this function can call itself; `enter` bounds that.
        if self.at(LBrace) && self.enter() {
            self.header_brace();
            self.bump();
            self.nested(Context::BLOCK, |parser| {
                parser.items(true);
                parser.close_brace();
            });
            self.leave();
        }
        self.finish();
    }
}
