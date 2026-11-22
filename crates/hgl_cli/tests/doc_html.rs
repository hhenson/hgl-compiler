//! The HTML API site through the driver, including SDOC-7, SDOC-8 and SDOC-10.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::tempdir;

const API: &str = include_str!("fixtures/doc_html/source/api.hgl");

const TYPES: &str = include_str!("fixtures/doc_html/source/types.hgl");

fn input(directory: &Path, line_end: &str) {
    fs::create_dir_all(directory).unwrap();
    for (name, source) in [("api.hgl", API), ("types.hgl", TYPES)] {
        fs::write(directory.join(name), source.replace('\n', line_end)).unwrap();
    }
}

fn write_site(directory: &Path, source: &Path, extra: &[&str]) -> PathBuf {
    let output = Command::new(env!("CARGO_BIN_EXE_hgl"))
        .current_dir(directory)
        .arg("doc")
        .arg(source)
        .args(extra)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    directory.join("hgl-doc")
}

fn snapshot(directory: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn collect(root: &Path, directory: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    fs::read(&path).unwrap(),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    collect(directory, directory, &mut files);
    files
}

fn hrefs(html: &str) -> impl Iterator<Item = &str> {
    html.split("href=\"")
        .skip(1)
        .filter_map(|tail| tail.split_once('"').map(|(href, _)| href))
}

fn internal_link_targets_exist(directory: &Path, files: &BTreeMap<PathBuf, Vec<u8>>) {
    for (filename, bytes) in files {
        if filename
            .extension()
            .is_none_or(|extension| extension != "html")
        {
            continue;
        }
        let page = directory.join(filename);
        let html = std::str::from_utf8(bytes).unwrap();
        let scripts = html
            .split("src=\"")
            .skip(1)
            .filter_map(|tail| tail.split_once('"').map(|(src, _)| src));
        for href in hrefs(html).chain(scripts) {
            let (target, anchor) = href
                .split_once('#')
                .map_or((href, None), |(path, id)| (path, Some(id)));
            let target = if target.is_empty() {
                page.clone()
            } else {
                page.parent().unwrap().join(target)
            };
            assert!(
                target.is_file(),
                "{filename:?} links to a missing file: {href}"
            );
            if let Some(anchor) = anchor.filter(|anchor| !anchor.is_empty()) {
                let contents = fs::read_to_string(target).unwrap();
                assert!(
                    contents.contains(&format!("id=\"{anchor}\"")),
                    "{filename:?} links to a missing anchor: {href}"
                );
            }
        }
    }
}

#[test]
fn documentation_site_links_only_visible_declarations_and_local_files() {
    let root = tempdir().unwrap();
    let source = root.path().join("source");
    input(&source, "\n");
    let site = write_site(root.path(), &source, &[]);
    let files = snapshot(&site);
    let index = fs::read_to_string(site.join("index.html")).unwrap();
    assert!(index.contains("guide.api") && index.contains("guide.types"));
    let api = fs::read_to_string(site.join("modules/mod-guide.api.html")).unwrap();
    assert!(api.contains("increment"));
    assert!(api.contains("The integer candidate.") && api.contains("The text candidate."));
    assert!(api.contains("<table") && api.contains("The options are evaluated once."));
    assert_eq!(api.matches("The input value.").count(), 1);
    assert!(api.contains("&lt;script&gt;") && !api.contains("<script>alert"));
    assert!(api.contains("External label") && api.contains("External picture"));
    assert!(!api.contains("example.invalid"));
    assert!(
        api.contains("<code>private_marker</code>")
            && !api.contains("Private implementation detail.")
    );
    assert!(api.contains("<th scope=\"col\">Default</th>"));
    assert!(api.contains("class=\"keyword\">assert"));
    assert!(api.contains("<p><strong>Further reading</strong>:"));
    assert!(api.contains("$<code class=\"formula-inline\">shared</code>$"));
    assert!(api.contains("class=\"language-math\">a^2+b^2=c^2"));
    assert!(api.contains("class=\"language-mermaid\">graph LR; A --&gt; B"));
    assert!(api.contains("class=\"language-math\">\\badHglCommand{b}"));
    assert!(api.contains("class=\"language-mermaid\">not a diagram"));
    assert_eq!(
        hrefs(&api)
            .filter(|href| href.contains("mod-guide.types.html#"))
            .count(),
        4
    );
    assert!(api.contains("<code>value</code>") && api.contains("<code>hidden_elsewhere</code>"));
    internal_link_targets_exist(&site, &files);

    let private = root.path().join("private");
    write_site(
        root.path(),
        &source,
        &["--format", "html", "--out", "private", "--private"],
    );
    let api = fs::read_to_string(private.join("modules/mod-guide.api.html")).unwrap();
    assert!(api.contains("Private implementation detail."));
    assert!(hrefs(&api).any(|href| href.starts_with("#decl-private_marker-")));
    assert_eq!(
        hrefs(&api)
            .filter(|href| href.contains("mod-guide.types.html#"))
            .count(),
        4,
        "a declaration private to another module remains outside this comment's scope"
    );
    internal_link_targets_exist(&private, &snapshot(&private));
}

#[test]
fn site_bytes_do_not_depend_on_source_location_or_line_endings() {
    let lf = tempdir().unwrap();
    let crlf = tempdir().unwrap();
    let first_source = lf.path().join("source");
    let second_source = crlf.path().join("source");
    input(&first_source, "\n");
    input(&second_source, "\r\n");
    let first = write_site(lf.path(), &first_source, &[]);
    let second = write_site(crlf.path(), &second_source, &[]);
    assert_eq!(snapshot(&first), snapshot(&second));
}

#[test]
fn documentation_site_matches_the_golden_pages() {
    let root = tempdir().unwrap();
    let source = root.path().join("source");
    input(&source, "\n");
    let site = write_site(root.path(), &source, &[]);
    for (path, expected) in [
        (
            "index.html",
            include_bytes!("fixtures/doc_html/golden/index.html").as_slice(),
        ),
        (
            "modules/mod-guide.api.html",
            include_bytes!("fixtures/doc_html/golden/mod-guide.api.html").as_slice(),
        ),
        (
            "modules/mod-guide.types.html",
            include_bytes!("fixtures/doc_html/golden/mod-guide.types.html").as_slice(),
        ),
    ] {
        assert_eq!(fs::read(site.join(path)).unwrap(), expected, "{path}");
    }
}

#[test]
fn modules_that_differ_in_case_have_distinct_paths() {
    let root = tempdir().unwrap();
    let source = root.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("upper.hgl"), "module guide.Types\n").unwrap();
    fs::write(source.join("lower.hgl"), "module guide.types\n").unwrap();
    let site = write_site(root.path(), &source, &[]);
    let index = fs::read_to_string(site.join("index.html")).unwrap();
    let upper = hrefs(&index)
        .find(|path| path.ends_with(".~54ypes.html"))
        .unwrap();
    let lower = "modules/mod-guide.types.html";
    assert_ne!(upper.to_ascii_lowercase(), lower);
    assert!(
        fs::read_to_string(site.join(upper))
            .unwrap()
            .contains("<h1>guide.Types</h1>")
    );
    assert!(
        fs::read_to_string(site.join(lower))
            .unwrap()
            .contains("<h1>guide.types</h1>")
    );
}

#[test]
fn the_offline_search_index_matches_the_pages_and_visibility() {
    let root = tempdir().unwrap();
    let source = root.path().join("source");
    input(&source, "\n");
    let public = write_site(root.path(), &source, &[]);
    write_site(root.path(), &source, &["--out", "private", "--private"]);
    for (site, private) in [(public, false), (root.path().join("private"), true)] {
        let index = fs::read_to_string(site.join("search-index.js")).unwrap();
        let payload = index
            .strip_prefix("const hglDeclarations = ")
            .and_then(|content| content.strip_suffix(";\n"))
            .unwrap();
        let entries: Vec<serde_json::Value> = serde_json::from_str(payload).unwrap();
        let mut has_private = false;
        let mut operators = 0;
        for entry in entries {
            let name = entry["canonicalName"].as_str().unwrap();
            has_private |= name == "guide.api::private_marker";
            operators += usize::from(name == "guide.api::choose");
            let href = entry["href"].as_str().unwrap();
            let (file, anchor) = href.split_once('#').unwrap();
            assert!(file.starts_with("modules/") && !file.contains(':'));
            let page = fs::read_to_string(site.join(file)).unwrap();
            assert!(page.contains(&format!("id=\"{anchor}\"")), "{href}");
        }
        assert_eq!(has_private, private);
        assert_eq!(operators, 1, "candidates use the operator's search result");
    }
}

#[test]
fn the_site_copies_the_renderer_bundles_and_their_licenses() {
    let root = tempdir().unwrap();
    let source = root.path().join("source");
    input(&source, "\n");
    let site = write_site(root.path(), &source, &[]);
    for (file, expected) in [
        (
            "vendor/katex/katex.min.js",
            include_bytes!("../../hgl_doc/vendor/katex/katex.min.js").as_slice(),
        ),
        (
            "vendor/katex/LICENSE",
            include_bytes!("../../hgl_doc/vendor/katex/LICENSE").as_slice(),
        ),
        (
            "vendor/mermaid/mermaid.min.js",
            include_bytes!("../../hgl_doc/vendor/mermaid/mermaid.min.js").as_slice(),
        ),
        (
            "vendor/mermaid/LICENSE",
            include_bytes!("../../hgl_doc/vendor/mermaid/LICENSE").as_slice(),
        ),
        (
            "vendor/mermaid/THIRD-PARTY-NOTICES.txt",
            include_bytes!("../../hgl_doc/vendor/mermaid/THIRD-PARTY-NOTICES.txt").as_slice(),
        ),
    ] {
        assert_eq!(fs::read(site.join(file)).unwrap(), expected, "{file}");
    }
}

#[test]
fn renderer_assets_follow_the_pages_that_use_them() {
    let root = tempdir().unwrap();
    let source = root.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("core.hgl"), "module core\n").unwrap();
    for (output, comment, math, mermaid) in [
        (
            "plain",
            "Ordinary text.\n\n~~~mermaid ignored\nA --> B\n~~~",
            false,
            false,
        ),
        ("math", "A formula $`x^2`$.", true, false),
        (
            "diagram",
            "~~~mermaid\nflowchart LR; A --> B\n~~~",
            false,
            true,
        ),
    ] {
        fs::write(
            source.join("assets.hgl"),
            format!("module guide.assets\n/**\n{comment}\n*/\nexport const marker: i64 = 1\n"),
        )
        .unwrap();
        write_site(root.path(), &source, &["--out", output]);
        let site = root.path().join(output);
        let page = fs::read_to_string(site.join("modules/mod-guide.assets.html")).unwrap();
        assert_eq!(
            page.contains("../renderers.js"),
            math || mermaid,
            "{output}"
        );
        assert_eq!(
            site.join("renderers.js").is_file(),
            math || mermaid,
            "{output}"
        );
        assert_eq!(
            site.join("vendor/katex/katex.min.js").is_file(),
            math,
            "{output}"
        );
        assert_eq!(
            site.join("vendor/mermaid/mermaid.min.js").is_file(),
            mermaid,
            "{output}"
        );
        assert_eq!(
            site.join("vendor/mermaid/THIRD-PARTY-NOTICES.txt")
                .is_file(),
            mermaid,
            "{output}"
        );
    }
}

#[test]
fn rejected_source_does_not_create_the_site() {
    let root = tempdir().unwrap();
    let source = root.path().join("bad.hgl");
    fs::write(
        &source,
        "module guide.bad\nexport const fail: i64 = unknown\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_hgl"))
        .current_dir(root.path())
        .args(["doc", "bad.hgl"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!root.path().join("hgl-doc").exists());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("name.unknown")
    );
}

#[test]
fn a_nonempty_unmarked_directory_is_left_alone() {
    let root = tempdir().unwrap();
    let source = root.path().join("source");
    input(&source, "\n");
    fs::write(root.path().join("index.html"), "my index").unwrap();
    fs::write(root.path().join("site.css"), "my CSS").unwrap();
    let before = snapshot(root.path());
    let output = Command::new(env!("CARGO_BIN_EXE_hgl"))
        .current_dir(root.path())
        .args(["doc", "source", "--out", "."])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("neither empty nor an hgl doc site"));
    assert_eq!(snapshot(root.path()), before);
}

#[test]
fn rebuilding_an_owned_site_removes_private_pages_and_stale_assets() {
    let root = tempdir().unwrap();
    let source = root.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("core.hgl"), "module core\n").unwrap();
    fs::write(
        source.join("stable.hgl"),
        "module guide.stable\nexport const visible: i64 = 1\nconst hidden: i64 = 2\n",
    )
    .unwrap();
    let removed = source.join("removed.hgl");
    fs::write(
        &removed,
        "module guide.removed\n/**\n~~~mermaid\nflowchart LR; A --> B\n~~~\n*/\nexport const gone: i64 = 3\n",
    )
    .unwrap();

    let site = write_site(root.path(), &source, &["--private"]);
    let stable = site.join("modules/mod-guide.stable.html");
    let removed_page = site.join("modules/mod-guide.removed.html");
    let renderer = site.join("vendor/mermaid/mermaid.min.js");
    assert!(fs::read_to_string(&stable).unwrap().contains("hidden"));
    assert!(removed_page.is_file());
    assert!(renderer.is_file());

    fs::write(site.join("CNAME"), "docs.example.org").unwrap();
    fs::write(site.join("modules/notes.txt"), "local notes").unwrap();
    fs::remove_file(removed).unwrap();
    write_site(root.path(), &source, &[]);

    assert!(!fs::read_to_string(stable).unwrap().contains("hidden"));
    assert!(
        !fs::read_to_string(site.join("search-index.js"))
            .unwrap()
            .contains("hidden")
    );
    assert!(!removed_page.exists());
    assert!(!renderer.exists());
    assert!(!site.join("renderers.js").exists());
    assert!(!site.join("vendor/mermaid/THIRD-PARTY-NOTICES.txt").exists());
    assert_eq!(
        fs::read_to_string(site.join("CNAME")).unwrap(),
        "docs.example.org"
    );
    assert_eq!(
        fs::read_to_string(site.join("modules/notes.txt")).unwrap(),
        "local notes"
    );
}

#[test]
fn an_unrelated_file_in_an_owned_site_cannot_be_overwritten() {
    let root = tempdir().unwrap();
    let source = root.path().join("source");
    input(&source, "\n");
    let site = write_site(root.path(), &source, &[]);
    fs::write(
        site.join("modules/mod-guide.extra.html"),
        "a hand-written page",
    )
    .unwrap();
    fs::write(
        source.join("extra.hgl"),
        "module guide.extra\nexport const extra: i64 = 3\n",
    )
    .unwrap();
    let before = snapshot(&site);
    let output = Command::new(env!("CARGO_BIN_EXE_hgl"))
        .current_dir(root.path())
        .args(["doc", "source"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot replace unrelated"));
    assert_eq!(snapshot(&site), before);

    fs::remove_file(site.join("modules/mod-guide.extra.html")).unwrap();
    write_site(root.path(), &source, &[]);
    let page = fs::read_to_string(site.join("modules/mod-guide.extra.html")).unwrap();
    assert!(page.contains("extra"));
}

#[test]
fn a_site_manifest_cannot_claim_an_unrelated_file() {
    let root = tempdir().unwrap();
    let source = root.path().join("source");
    input(&source, "\n");
    let site = write_site(root.path(), &source, &[]);
    fs::write(site.join("notes.txt"), "local notes").unwrap();
    fs::write(site.join(".hgl-doc-files"), "notes.txt\n").unwrap();
    let before = snapshot(&site);
    let output = Command::new(env!("CARGO_BIN_EXE_hgl"))
        .current_dir(root.path())
        .args(["doc", "source"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid file name"));
    assert_eq!(snapshot(&site), before);
}

#[test]
fn concurrent_site_rebuilds_leave_one_complete_site() {
    let root = tempdir().unwrap();
    for (file, module) in [("left.hgl", "guide.left"), ("right.hgl", "guide.right")] {
        fs::write(
            root.path().join(file),
            format!("module {module}\nexport const value: i64 = 1\n"),
        )
        .unwrap();
    }
    let left = root.path().join("left.hgl");
    let right = root.path().join("right.hgl");
    let site = write_site(root.path(), &left, &[]);
    fs::create_dir(root.path().join("expected-left")).unwrap();
    write_site(root.path(), &left, &["--out", "expected-left"]);
    write_site(root.path(), &right, &["--out", "expected-right"]);

    let rebuild = |source: &Path| {
        Command::new(env!("CARGO_BIN_EXE_hgl"))
            .current_dir(root.path())
            .arg("doc")
            .arg(source)
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap()
    };
    let left_build = rebuild(&left);
    let right_build = rebuild(&right);
    for child in [left_build, right_build] {
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let actual = snapshot(&site);
    let expected_left = snapshot(&root.path().join("expected-left"));
    let expected_right = snapshot(&root.path().join("expected-right"));
    assert!(actual == expected_left || actual == expected_right);
}
