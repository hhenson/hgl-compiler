//! Combines the analysed interface with source text and the parser's SDOC
//! layout. All indices used for lookup stay within this stage.

use std::collections::BTreeMap;

use hgl_analyser::{
    DeclarationKind, Detail, GenericKind, GenericParameter, ModuleFile, Pack, Program, Signature,
};
use hgl_diagnostics::Location;
use hgl_parser::ast::AstNode;
use hgl_parser::documentation::{Comment, attached};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};
use hgl_source::{FileId, SourceMap};

use crate::model::{
    Bounds, Declaration, Documentation, Entry, Generic, Interface, Model, Module, Parameter, Part,
    ResultType, SCHEMA_VERSION, Section, Source,
};

/// An analysed declaration's module and declaration indices.
type Id = (usize, usize);

/// Builds the model in module, part and source order (UNIT-1, SDOC-4).
pub(crate) fn build<'a>(
    program: &'a Program,
    sources: &SourceMap,
    path: &impl Fn(FileId) -> String,
    private: bool,
) -> Model<'a> {
    let mut candidates = BTreeMap::<Id, Vec<Id>>::new();
    for (module, data) in program.modules.iter().enumerate() {
        for (index, declaration) in data.declarations.iter().enumerate() {
            if let Some((_, Some(operator))) = declaration.kind.candidate() {
                candidates
                    .entry((operator.module, operator.declaration))
                    .or_default()
                    .push((module, index));
            }
        }
    }
    let reader = Reader {
        program,
        sources,
        path,
    };
    Model {
        schema_version: SCHEMA_VERSION,
        modules: program
            .modules
            .iter()
            .enumerate()
            .map(|(module, data)| {
                let mut declarations = Vec::new();
                for (index, declaration) in data.declarations.iter().enumerate() {
                    if matches!(declaration.kind.candidate(), Some((_, Some(_))))
                        || (!private && !declaration.is_public())
                    {
                        continue;
                    }
                    let id = (module, index);
                    let mut item = reader.declaration(id);
                    if let Some(implementations) = candidates.get(&id) {
                        item.candidates = implementations
                            .iter()
                            .map(|&implementation| reader.declaration(implementation))
                            .collect();
                    }
                    declarations.push(((declaration.file, declaration.at.span.start), item));
                }
                if private {
                    for test in &data.tests {
                        let part = &data.files[test.file];
                        declarations.push((
                            (test.file, test.at.span.start),
                            Declaration {
                                kind: "test",
                                name: &test.name,
                                canonical_name: format!("{}::{}", data.path, test.name),
                                role: None,
                                exported: false,
                                source: reader.source(&data.path, part, test.at),
                                documentation: documentation(&test.syntax),
                                interface: Interface::default(),
                                members: Vec::new(),
                                candidates: Vec::new(),
                            },
                        ));
                    }
                }
                declarations.sort_by_key(|(position, _)| *position);
                Module {
                    name: &data.path,
                    application: data.application,
                    parts: data
                        .files
                        .iter()
                        .map(|part| reader.part(&data.path, part))
                        .collect(),
                    declarations: declarations.into_iter().map(|(_, item)| item).collect(),
                }
            })
            .collect(),
    }
}

/// The input used to read interfaces, their source positions and comments.
struct Reader<'a, 'path, F> {
    program: &'a Program,
    sources: &'path SourceMap,
    path: &'path F,
}

impl<'a, F: Fn(FileId) -> String> Reader<'a, '_, F> {
    fn source(&self, module: &'a str, part: &'a ModuleFile, at: Location) -> Source<'a> {
        let position = self.sources.file(at.file).line_col(at.span.start);
        Source {
            module,
            part: part.part.as_deref(),
            file: (self.path)(at.file),
            line: position.line,
            column: position.column,
        }
    }

    fn part(&self, module: &'a str, part: &'a ModuleFile) -> Part<'a> {
        let header = part
            .syntax
            .module_decl()
            .map(|head| (head.syntax().clone(), head.path()))
            .or_else(|| {
                part.syntax
                    .application_decl()
                    .map(|head| (head.syntax().clone(), head.path()))
            });
        let span = header
            .as_ref()
            .and_then(|(_, path)| path.as_ref())
            .map_or_else(|| part.syntax.syntax().span(), |path| path.syntax().span());
        Part {
            name: part.part.as_deref(),
            source: self.source(module, part, Location::new(part.file, span)),
            documentation: header.and_then(|(syntax, _)| documentation(&syntax)),
        }
    }

    fn declaration(&self, (module, index): Id) -> Declaration<'a> {
        let data = &self.program.modules[module];
        let declaration = &data.declarations[index];
        let part = &data.files[declaration.file];
        let syntax = &declaration.syntax;
        // GEN-4: a candidate's canonical name belongs to the operator,
        // whereas its file and text belong to the implementing module.
        let owner = declaration
            .kind
            .candidate()
            .and_then(|(_, operator)| operator)
            .map_or(data, |operator| &self.program.modules[operator.module]);
        let canonical_name = format!("{}::{}", owner.path, declaration.name);
        let mut interface = Interface {
            requires: child(syntax, SyntaxKind::RequiresClause).map(|clause| written(&clause)),
            properties: children(syntax, SyntaxKind::OperatorProperties)
                .map(|clause| written(&clause))
                .collect(),
            ..Interface::default()
        };
        let mut members = Vec::new();
        let container = MemberContainer {
            reader: self,
            module: &data.path,
            part,
            syntax,
            canonical_name: &canonical_name,
            exported: declaration.is_public(),
        };
        match data.details.get(index) {
            Some(Detail::Callable(signature)) => callable(syntax, signature, &mut interface),
            Some(Detail::Struct(shape)) => {
                interface.generic_parameters = generics(syntax, &shape.generics);
                interface.parents = shape.parents.iter().map(ToString::to_string).collect();
                interface.abstract_struct = syntax
                    .children_with_tokens()
                    .any(|item| item.kind() == SyntaxKind::KwAbstract);
                for field in &shape.fields {
                    let ty = field.ty.as_ref().or_else(|| {
                        // A field that only overrides a parent's default
                        // still has its analysed member type (TYPE-4).
                        shape
                            .members
                            .iter()
                            .find(|member| member.name == field.name)
                            .map(|member| &member.ty)
                    });
                    members.push(container.member(
                        "field",
                        SyntaxKind::StructMember,
                        &field.name,
                        field.at,
                        ty.map(ToString::to_string),
                        None,
                    ));
                }
            }
            Some(Detail::Enum(shape)) => {
                for member in &shape.members {
                    members.push(container.member(
                        "enumMember",
                        SyntaxKind::EnumMember,
                        &member.name,
                        member.at,
                        None,
                        member.value.map(|value| value.to_string()),
                    ));
                }
            }
            Some(Detail::Native(shape)) => {
                for property in &shape.properties {
                    members.push(container.member(
                        "nativeProperty",
                        SyntaxKind::NativeProperty,
                        &property.name,
                        property.at,
                        Some(property.ty.to_string()),
                        None,
                    ));
                }
            }
            Some(Detail::Constant(ty)) => {
                interface.ty = Some(ty.to_string());
                interface.value = default(syntax);
            }
            Some(Detail::Plain) | None => {}
        }
        Declaration {
            kind: kind(declaration.kind),
            name: &declaration.name,
            canonical_name,
            role: declaration.kind.role().map(|role| role.as_str()),
            exported: declaration.is_public(),
            source: self.source(&data.path, part, declaration.at),
            documentation: documentation(syntax),
            interface,
            members,
            candidates: Vec::new(),
        }
    }
}

/// Where a struct field, enum member or native property is documented.
struct MemberContainer<'a, 'b, 'path, F> {
    reader: &'b Reader<'a, 'path, F>,
    module: &'a str,
    part: &'a ModuleFile,
    syntax: &'b SyntaxNode,
    canonical_name: &'b str,
    exported: bool,
}

impl<'a, F: Fn(FileId) -> String> MemberContainer<'a, '_, '_, F> {
    fn member(
        &self,
        kind: &'static str,
        syntax_kind: SyntaxKind,
        name: &'a str,
        at: Location,
        ty: Option<String>,
        value: Option<String>,
    ) -> Declaration<'a> {
        let syntax = children(self.syntax, syntax_kind).find(|node| encloses(node, at));
        Declaration {
            kind,
            name,
            canonical_name: format!("{}.{name}", self.canonical_name),
            role: None,
            exported: self.exported,
            source: self.reader.source(self.module, self.part, at),
            documentation: syntax.as_ref().and_then(documentation),
            interface: Interface {
                ty,
                default: syntax.as_ref().and_then(default),
                value,
                ..Interface::default()
            },
            members: Vec::new(),
            candidates: Vec::new(),
        }
    }
}

fn kind(declaration: DeclarationKind) -> &'static str {
    match declaration {
        DeclarationKind::Function(_) => "function",
        DeclarationKind::NativeFunction(_) => "nativeFunction",
        DeclarationKind::NativeImplementation(_) => "nativeImplementation",
        DeclarationKind::Operator => "operator",
        DeclarationKind::Implementation(..) => "candidate",
        DeclarationKind::NativeCandidate(..) => "nativeCandidate",
        DeclarationKind::NativeCandidateImplementation(_) => "nativeCandidateImplementation",
        DeclarationKind::Struct => "struct",
        DeclarationKind::Enum => "enum",
        DeclarationKind::NativeType => "nativeType",
        DeclarationKind::Constant => "constant",
    }
}

fn callable<'a>(syntax: &SyntaxNode, signature: &'a Signature, interface: &mut Interface<'a>) {
    interface.generic_parameters = generics(syntax, &signature.generics);
    for (kind, output) in [
        (
            SyntaxKind::ConfigParamList,
            &mut interface.configuration_parameters,
        ),
        (SyntaxKind::ParamList, &mut interface.parameters),
    ] {
        let list = child(syntax, kind);
        let written_parameters: Vec<_> = list
            .iter()
            .flat_map(|list| children(list, SyntaxKind::Param))
            .collect();
        let configuration = kind == SyntaxKind::ConfigParamList;
        for parameter in signature
            .parameters
            .iter()
            .filter(|parameter| parameter.configuration == configuration)
        {
            let node = written_parameters
                .iter()
                .find(|node| encloses(node, parameter.at));
            output.push(Parameter {
                name: &parameter.name,
                ty: parameter.ty.to_string(),
                written: node.map(written),
                constant: parameter.constant,
                var: parameter.var,
                reference: parameter.reference,
                pack: parameter.pack.map(|pack| match pack {
                    Pack::Named => "named",
                    Pack::Positional => "positional",
                }),
                bounds: parameter.pack.map(|_| Bounds {
                    minimum: parameter.pack_bounds.0,
                    maximum: parameter.pack_bounds.1,
                }),
                default: node.and_then(default),
            });
        }
    }
    interface.result = signature.result.as_ref().map(|ty| ResultType {
        ty: ty.to_string(),
        written: child(syntax, SyntaxKind::ReturnType).map(|node| written(&node)),
        reference: signature.reference,
    });
}

fn generics<'a>(syntax: &SyntaxNode, parameters: &'a [GenericParameter]) -> Vec<Generic<'a>> {
    let list = child(syntax, SyntaxKind::GenericParams);
    let nodes: Vec<_> = list
        .iter()
        .flat_map(|list| children(list, SyntaxKind::GenericParam))
        .collect();
    parameters
        .iter()
        .map(|parameter| {
            let node = nodes.iter().find(|node| encloses(node, parameter.at));
            let (kind, ty, bounds) = match &parameter.kind {
                GenericKind::Plain => ("plain", None, None),
                GenericKind::Type => ("type", None, None),
                GenericKind::Pack { minimum, maximum } => (
                    "pack",
                    None,
                    Some(Bounds {
                        minimum: *minimum,
                        maximum: *maximum,
                    }),
                ),
                GenericKind::Value(ty) => ("value", Some(ty.to_string()), None),
            };
            Generic {
                name: &parameter.name,
                kind,
                ty,
                bounds,
                written: node.map(written),
                default: node.and_then(default),
            }
        })
        .collect()
}

fn documentation(syntax: &SyntaxNode) -> Option<Documentation> {
    let token = attached(syntax)?;
    let comment = Comment::read(&token);
    Some(Documentation {
        text: comment.text(),
        summary: comment.summary(),
        sections: comment
            .sections()
            .map(|section| Section {
                heading: section.heading(),
                text: section.text(),
                entries: section
                    .entries()
                    // SDOC-3 does not give a spelling for struct field keys.
                    .filter(|_| {
                        syntax.kind() != SyntaxKind::StructDecl
                            || section.heading() != Some("Args:")
                    })
                    .map(|entry| Entry {
                        key: entry.name.to_string(),
                        text: entry.text(),
                    })
                    .collect(),
            })
            .collect(),
    })
}

fn encloses(node: &SyntaxNode, at: Location) -> bool {
    let span = node.span();
    span.start <= at.span.start && at.span.end <= span.end
}

fn children(parent: &SyntaxNode, kind: SyntaxKind) -> impl Iterator<Item = SyntaxNode> {
    parent.children().filter(move |item| item.kind() == kind)
}

fn child(parent: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxNode> {
    children(parent, kind).next()
}

/// The child after the assignment, where a declaration permits a default.
fn default(node: &SyntaxNode) -> Option<String> {
    node.children_with_tokens()
        .skip_while(|item| item.kind() != SyntaxKind::Eq)
        .find_map(|item| item.into_node())
        .map(|node| written(&node))
}

/// Authored annotations and expressions kept intact except for LEX-15 line breaks.
fn written(node: &SyntaxNode) -> String {
    node.text()
        .to_string()
        .trim()
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}
