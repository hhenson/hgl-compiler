//! Prepare, audit and reuse the compiler's platform release bundles.

use std::fs;

use serde::Deserialize;

use crate::Result;

mod audit;
mod cache;
mod licenses;
mod reuse;

pub(crate) use audit::{verify, verify_debug};
pub(crate) use cache::{copy_native, native_key, save_native};
pub(crate) use licenses::licenses;
pub(crate) use reuse::find_verified_run;

const NATIVE_LIBRARIES: [&str; 3] = ["fmt", "spdlog", "simdjson"];

/// Read the version of the workspace whose release is being built.
pub(crate) fn version() -> Result<String> {
    #[derive(Deserialize)]
    struct Manifest {
        workspace: Workspace,
    }
    #[derive(Deserialize)]
    struct Workspace {
        package: Package,
    }
    #[derive(Deserialize)]
    struct Package {
        version: String,
    }

    let manifest: Manifest = toml::from_str(&fs::read_to_string("Cargo.toml")?)?;
    Ok(manifest.workspace.package.version)
}
