//! Keep path auditing independent of a file's spelling or ELF compression.

use super::*;

#[test]
fn a_forbidden_include_flag_is_detected() -> Result<()> {
    let root = tempfile::tempdir()?;
    let bundle = root.path().canonicalize()?;
    let build = bundle.join("forbidden-build");
    fs::write(
        bundle.join("arguments.txt"),
        format!("-I{}/x\n", build.display()),
    )?;
    let error = verify_paths(&bundle, &[build]).unwrap_err().to_string();
    assert!(error.contains("build path in arguments.txt:"), "{error}");
    Ok(())
}

#[test]
fn a_home_component_in_a_license_url_is_allowed() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let root = scratch.path().canonicalize()?;
    fs::write(
        root.join("LICENSE.txt"),
        "https://example.com/crates/home/license\n",
    )?;
    verify_paths(&root, &[])
}

#[test]
fn paths_after_a_rejected_match_are_checked() -> Result<()> {
    for (file, contents, diagnostic) in [
        (
            "katex.txt",
            "see x/home/a:/home/sample/build\n",
            "build path in katex.txt: /home/sample/build",
        ),
        (
            "simdjson.h",
            "// x/Users/a,/Users/sample/build\n",
            "build path in simdjson.h: /Users/sample/build",
        ),
        (
            "hgraphConfig.cmake",
            "set(X \"https://example.com/a:/opt/sample/build\")\n",
            "external path in hgraphConfig.cmake",
        ),
        (
            "hgl-sdk.txt",
            "x/home/a:/home/sample/build\n",
            "build path in hgl-sdk.txt: /home/sample/build",
        ),
    ] {
        let scratch = tempfile::tempdir()?;
        let bundle = scratch.path().canonicalize()?;
        fs::write(bundle.join(file), contents)?;
        let error = verify_paths(&bundle, &[]).expect_err(file).to_string();
        assert!(error.contains(diagnostic), "{error}");
    }
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn a_forbidden_path_in_legacy_gnu_compressed_debug_is_detected() -> Result<()> {
    let available = |name| {
        Command::new(name)
            .arg("--version")
            .output()
            .is_ok_and(|result| result.status.success())
    };
    if !available("cc") || !available("objcopy") {
        return Ok(());
    }
    let scratch = tempfile::tempdir()?;
    let root = scratch.path().canonicalize()?;
    let build = root.join("forbidden-build");
    let bundle = root.join("bundle");
    fs::create_dir(&build)?;
    fs::create_dir(&bundle)?;
    fs::write(
        build.join("sample.c"),
        "int twice(int value) { return value + value; }\n",
    )?;
    let library = bundle.join("library.so");
    output(
        Command::new("cc")
            .args(["-g", "-gdwarf-4", "-fPIC", "-shared", "sample.c", "-o"])
            .arg(&library)
            .current_dir(&build),
    )?;
    output(
        Command::new("objcopy")
            .arg("--compress-debug-sections=zlib-gnu")
            .arg(&library),
    )?;
    // Retain only legacy names so uncompressed sections cannot trigger decoding.
    output(
        Command::new("objcopy")
            .arg("--remove-section=.debug_*")
            .arg(&library),
    )?;
    let bytes = fs::read(&library)?;
    let contains = |bit: &[u8]| bytes.windows(bit.len()).any(|window| window == bit);
    assert!(contains(b".zdebug_info"));
    assert!(!contains(b".debug_"));
    assert!(!contains(build.as_os_str().as_encoded_bytes()));
    let error = verify_paths(&bundle, &[build]).unwrap_err().to_string();
    assert!(error.contains("build path in library.so:"), "{error}");
    Ok(())
}
