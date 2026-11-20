//! Consume a prebuilt C ABI or build that ABI once against the installed SDK.
use std::{
    env,
    hash::{DefaultHasher, Hash, Hasher},
    path::PathBuf,
    process::Command,
};
fn run(c: &mut Command) {
    let status = c.status().expect("launch CMake");
    assert!(status.success(), "CMake failed: {status}");
}
fn main() {
    let mut configuration = DefaultHasher::new();
    for key in [
        "HGL_HGRAPH_BRIDGE",
        "HGL_HGRAPH_SDK",
        "CMAKE_PREFIX_PATH",
        "CXX",
        "CXXFLAGS",
        "CMAKE_GENERATOR",
    ] {
        println!("cargo::rerun-if-env-changed={key}");
        env::var_os(key).hash(&mut configuration);
    }
    for f in ["build.rs", "CMakeLists.txt", "bridge.cpp"] {
        println!("cargo::rerun-if-changed={f}");
    }
    let dir = if let Some(bin) = env::var_os("HGL_HGRAPH_BRIDGE") {
        PathBuf::from(bin)
            .canonicalize()
            .expect("HGL_HGRAPH_BRIDGE must name a library directory")
    } else {
        // CMake retains both its initial compiler flags and resolved dependency paths.
        // A changed SDK or compiler configuration needs its own CMake cache.
        let out = PathBuf::from(env::var_os("OUT_DIR").unwrap())
            .join(format!("bridge-{:016x}", configuration.finish()));
        let mut config = Command::new("cmake");
        config
            .args(["-S", ".", "-B"])
            .arg(&out)
            .arg("-DCMAKE_BUILD_TYPE=Release")
            .arg("-DHGRAPH_USE_PYARROW_ARROW=OFF");
        if let Some(sdk) = env::var_os("HGL_HGRAPH_SDK") {
            let sdk = PathBuf::from(sdk)
                .canonicalize()
                .expect("installed HGraph SDK");
            let package = sdk.join("lib/cmake/hgraph");
            config.arg(format!("-Dhgraph_DIR={}", package.display()));
            for d in ["include/hgraph", "lib"] {
                println!("cargo::rerun-if-changed={}", sdk.join(d).display());
            }
        }
        run(&mut config);
        run(Command::new("cmake").arg("--build").arg(&out).args([
            "--config",
            "Release",
            "--parallel",
        ]));
        out
    };
    println!("cargo::rustc-link-search=native={}", dir.display());
    println!("cargo::rustc-link-lib=dylib=hgl_bridge");
    println!("cargo::metadata=libdir={}", dir.display());
}
