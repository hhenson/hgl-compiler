//! Read and run the debugger launch plan used by the release smoke check.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;
use serde_json::Value;

use crate::Result;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Launch {
    executable: PathBuf,
    arguments: Vec<String>,
    environment: BTreeMap<String, String>,
    working_directory: PathBuf,
}

fn load(description: &Path) -> Result<Launch> {
    Ok(serde_json::from_slice(&fs::read(description)?)?)
}

/// Print one string in the launch plan's nested JSON.
pub(crate) fn field(description: &Path, keys: &[String]) -> Result<()> {
    let document: Value = serde_json::from_slice(&fs::read(description)?)?;
    let selected = keys.iter().try_fold(&document, |value, key| {
        value
            .get(key)
            .ok_or_else(|| format!("the launch description has no {key} field"))
    })?;
    println!(
        "{}",
        selected
            .as_str()
            .ok_or("the launch field is not a string")?
    );
    Ok(())
}

/// Print the command for the platform debugger, one argument per line.
pub(crate) fn command(description: &Path) -> Result<()> {
    let launch = load(description)?;
    println!("{}", launch.executable.display());
    for argument in launch.arguments {
        println!("{argument}");
    }
    Ok(())
}

/// Check that the described fixture runs normally and publishes twice.
pub(crate) fn run(description: &Path) -> Result<()> {
    let launch = load(description)?;
    let ran = Command::new(launch.executable)
        .args(launch.arguments)
        .envs(launch.environment)
        .current_dir(launch.working_directory)
        .stdin(Stdio::null())
        .output()?;
    let publications = String::from_utf8(ran.stdout)?
        .lines()
        .filter(|line| line.starts_with("published "))
        .count();
    if !ran.status.success() || publications != 2 {
        return Err(format!(
            "the described run ended {} with {publications} publications:\n{}",
            ran.status
                .code()
                .map_or_else(|| ran.status.to_string(), |code| code.to_string()),
            String::from_utf8_lossy(&ran.stderr),
        )
        .into());
    }
    println!("the described run published {publications} times");
    Ok(())
}
