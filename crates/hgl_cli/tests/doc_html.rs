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
        for href in hrefs(html) {
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
    assert!(api.contains("$<code>shared</code>$"));
    assert!(api.contains("class=\"language-math\">a^2+b^2=c^2"));
    assert!(api.contains("class=\"language-mermaid\">graph LR; A --&gt; B"));
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
