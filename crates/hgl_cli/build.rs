//! Gives the CLI the same hgraph tag as SDK provisioning and packaging.

fn main() {
    let tag = "../../HGRAPH_TAG";
    println!("cargo:rerun-if-changed={tag}");
    let tag = std::fs::read_to_string(tag).expect("the workspace pins HGRAPH_TAG");
    println!("cargo:rustc-env=HGL_HGRAPH_TAG={}", tag.trim());
}
