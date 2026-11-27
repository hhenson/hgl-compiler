//! EXPR-9 rejects a repeated constant dictionary key or set member at its
//! second occurrence. Runtime expressions continue to use collection equality.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use hgl_diagnostics::{Category, Location};
use hgl_parser::SyntaxNode;

use super::Typer;
use crate::types::{ConstantValue, Type};
use crate::{codes, error};

impl Typer<'_> {
    pub(super) fn check_collection_constants(
        &mut self,
        nodes: impl Iterator<Item = SyntaxNode>,
        collection: &Type,
    ) {
        let (ty, code, message) = match collection {
            Type::Dict { key, .. } => (
                key.as_ref(),
                codes::DICT_DUPLICATE_KEY,
                "this dictionary key is the same constant as an earlier key",
            ),
            Type::Set(element) => (
                element.as_ref(),
                codes::SET_DUPLICATE_MEMBER,
                "this member of the set is the same constant as an earlier member",
            ),
            _ => return,
        };
        let mut known = HashMap::new();
        let mut symbolic: Vec<(ConstantValue, Location)> = Vec::new();
        for node in nodes {
            let Ok(value) = self.read_constant(&node, Some(ty)) else {
                continue;
            };
            let at = self.at(&node);
            let first = if let Some(key) = value.key() {
                match known.entry(key) {
                    Entry::Occupied(entry) => Some(*entry.get()),
                    Entry::Vacant(entry) => {
                        entry.insert(at);
                        None
                    }
                }
            } else if let Some((_, first)) = symbolic
                .iter()
                .find(|(old, _)| old.same(&value) == Some(true))
            {
                Some(*first)
            } else {
                if value.same(&value) == Some(true) {
                    symbolic.push((value, at));
                }
                None
            };
            if let Some(first) = first {
                self.diagnostics.emit(
                    error(Category::Type, code, at, message.to_string(), "EXPR-9")
                        .with_related(first, "first written here"),
                );
            }
        }
    }
}
