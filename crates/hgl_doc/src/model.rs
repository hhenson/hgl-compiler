//! The documentation model. Its serialized fields are the versioned JSON API.

use hgl_diagnostics::Location;
use serde::Serialize;

/// The current version of the API documentation model.
pub(crate) const SCHEMA_VERSION: u32 = 1;

/// A complete program's documentation.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Model<'a> {
    pub(crate) schema_version: u32,
    pub(crate) modules: Vec<Module<'a>>,
}

/// One analysed module or application in the program.
#[derive(Serialize)]
pub(crate) struct Module<'a> {
    pub(crate) name: &'a str,
    pub(crate) application: bool,
    pub(crate) parts: Vec<Part<'a>>,
    pub(crate) declarations: Vec<Declaration<'a>>,
}

/// A source file contributing to a module, with its header comment.
#[derive(Serialize)]
pub(crate) struct Part<'a> {
    pub(crate) name: Option<&'a str>,
    pub(crate) source: Source<'a>,
    pub(crate) documentation: Option<Documentation>,
}

/// A declaration, member, candidate or named test, preserving its own source
/// and text independently of entries that share the same canonical name.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Declaration<'a> {
    pub(crate) kind: &'static str,
    pub(crate) name: &'a str,
    pub(crate) canonical_name: String,
    pub(crate) role: Option<&'static str>,
    pub(crate) exported: bool,
    #[serde(skip)]
    pub(crate) signature: String,
    pub(crate) source: Source<'a>,
    pub(crate) documentation: Option<Documentation>,
    #[serde(flatten)]
    pub(crate) interface: Interface<'a>,
    pub(crate) members: Vec<Declaration<'a>>,
    pub(crate) candidates: Vec<Declaration<'a>>,
}

/// The written and analysed parts of an interface. Inapplicable fields are
/// empty or null, as defined in the schema.
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Interface<'a> {
    pub(crate) generic_parameters: Vec<Generic<'a>>,
    pub(crate) configuration_parameters: Vec<Parameter<'a>>,
    pub(crate) parameters: Vec<Parameter<'a>>,
    pub(crate) result: Option<ResultType>,
    pub(crate) requires: Option<String>,
    pub(crate) properties: Vec<String>,
    pub(crate) parents: Vec<String>,
    #[serde(rename = "type")]
    pub(crate) ty: Option<String>,
    pub(crate) default: Option<String>,
    pub(crate) value: Option<String>,
    pub(crate) abstract_struct: bool,
}

/// A generic type, type pack or constant parameter.
#[derive(Serialize)]
pub(crate) struct Generic<'a> {
    pub(crate) name: &'a str,
    pub(crate) kind: &'static str,
    #[serde(rename = "type")]
    pub(crate) ty: Option<String>,
    pub(crate) bounds: Option<Bounds>,
    pub(crate) written: Option<String>,
    pub(crate) default: Option<String>,
}

/// A configuration, fixed or variadic parameter.
#[derive(Serialize)]
pub(crate) struct Parameter<'a> {
    pub(crate) name: &'a str,
    #[serde(rename = "type")]
    pub(crate) ty: String,
    pub(crate) written: Option<String>,
    pub(crate) constant: bool,
    pub(crate) var: bool,
    pub(crate) reference: bool,
    pub(crate) pack: Option<&'static str>,
    pub(crate) bounds: Option<Bounds>,
    pub(crate) default: Option<String>,
}

/// The lower and optional upper bound of a pack.
#[derive(Serialize)]
pub(crate) struct Bounds {
    pub(crate) minimum: u64,
    pub(crate) maximum: Option<u64>,
}

/// The type and mode of a callable's result.
#[derive(Serialize)]
pub(crate) struct ResultType {
    #[serde(rename = "type")]
    pub(crate) ty: String,
    pub(crate) written: Option<String>,
    pub(crate) reference: bool,
}

/// A position in a module part. Lines and columns are one-based.
#[derive(Serialize)]
pub(crate) struct Source<'a> {
    pub(crate) module: &'a str,
    pub(crate) part: Option<&'a str>,
    pub(crate) file: String,
    pub(crate) line: u32,
    pub(crate) column: u32,
    #[serde(skip)]
    pub(crate) at: Location,
}

/// A comment's normalized text and its parsed SDOC layout.
#[derive(Serialize)]
pub(crate) struct Documentation {
    pub(crate) text: String,
    pub(crate) summary: String,
    pub(crate) sections: Vec<Section>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) examples: Vec<Example>,
}

/// A CommonMark HGL fence and whether it is a test (SDOC-9).
#[derive(Serialize)]
pub(crate) struct Example {
    pub(crate) code: String,
    pub(crate) position: Option<usize>,
    pub(crate) runnable: bool,
}

/// A recognized SDOC heading, authored heading or unheaded prose.
#[derive(Serialize)]
pub(crate) struct Section {
    pub(crate) heading: Option<String>,
    pub(crate) recognized: bool,
    pub(crate) text: String,
    pub(crate) entries: Vec<Entry>,
    #[serde(skip)]
    pub(crate) prose: String,
}

/// One key and description as written under its heading.
#[derive(Serialize)]
pub(crate) struct Entry {
    pub(crate) key: String,
    pub(crate) text: String,
}
