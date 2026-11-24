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
        let links = hrefs(html).filter(|href| {
            !href.split_once(':').is_some_and(|(scheme, _)| {
                scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
            })
        });
        for href in links.chain(scripts) {
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
fn documentation_site_links_visible_declarations_and_web_destinations() {
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
    assert!(api.contains(
        "<a href=\"https://example.invalid/external\">External label</a> (<code>https://example.invalid/external</code>)"
    ));
    assert!(api.contains("External picture (<code>https://example.invalid/picture.png</code>)"));
    assert!(!api.contains("<img"));
    assert!(
        api.contains("<code>private_marker</code>")
            && !api.contains("Private implementation detail.")
    );
    assert!(api.contains("<th scope=\"col\">Default</th>"));
    assert!(api.contains("class=\"keyword\">assert"));
    assert!(api.contains("<h4><strong>Further reading</strong></h4>"));
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
    assert!(hrefs(&api).any(|href| href == "#decl-private_marker"));
    assert_eq!(
        hrefs(&api)
            .filter(|href| href.contains("mod-guide.types.html#"))
            .count(),
        4,
        "a declaration private to another module remains outside this comment's scope"
    );
    internal_link_targets_exist(&private, &snapshot(&private));
}

/// SDOC-4: separate table columns preserve modes (FN-1, FN-15 and FN-16)
/// on names and keep both FN-8 pack marks with their types.
#[test]
fn documentation_site_shows_parameter_modes_and_packs() {
    let root = tempdir().unwrap();
    let source = root.path().join("parameters.hgl");
    fs::write(
        &source,
        concat!(
            "module parameters\n",
            "export const fn modes{const offset: i64 = 1}(const count: i64, var values: list<i64>) -> i64 => count + offset\n",
            "export const fn packs<...Names>(items: ...bool, fields: ...{Names}) -> i64 => 0\n",
            "export fn observe(ref input: i64) { when {} }\n",
        ),
    )
    .unwrap();
    let site = write_site(root.path(), &source, &[]);
    let page = fs::read_to_string(site.join("modules/mod-parameters.html")).unwrap();
    for (name, ty, default) in [
        ("const offset", "i64", "<code>1</code>"),
        ("const count", "i64", "—"),
        ("var values", "list&lt;i64&gt;", "—"),
        ("ref input", "i64", "—"),
        ("items", "...bool", "—"),
        ("fields", "...{Names}", "—"),
    ] {
        assert!(
            page.contains(&format!(
                "<th scope=\"row\"><code>{name}</code></th><td><code>{ty}</code></td><td>{default}</td>"
            )),
            "no table row for {name}: {ty}"
        );
    }
}

/// SDOC-7: Markdown destinations are visible, and only HTTP(S) link
/// labels carry external anchors.
#[test]
fn documentation_site_shows_markdown_destinations_as_text() {
    let root = tempdir().unwrap();
    let source = root.path().join("destinations.hgl");
    fs::write(
        &source,
        r#"module destinations

/** [**Web**](HTTPS://example.invalid/page?x=1&y=2)

[`home`](http://example.invalid/home)

[Relative](../guide)
[Fragment](#fragment)
[Other](custom:inert)
[Script](javascript:inert)
[Data](data:text/plain,inert)
[Quoted](<https://example.invalid/"q">)

[![Picture](https://example.invalid/picture.png)](https://example.invalid/gallery)
*/
export const fn home() -> i64 => 1
"#,
    )
    .unwrap();
    let site = write_site(root.path(), &source, &[]);
    let page = fs::read_to_string(site.join("modules/mod-destinations.html")).unwrap();
    assert!(page.contains(concat!(
        "<a href=\"HTTPS://example.invalid/page?x=1&amp;y=2\"><strong>Web</strong></a> ",
        "(<code>HTTPS://example.invalid/page?x=1&amp;y=2</code>)",
    )));
    assert!(page.contains(concat!(
        "<a href=\"http://example.invalid/home\"><code>home</code></a> ",
        "(<code>http://example.invalid/home</code>)",
    )));
    assert!(page.contains(concat!(
        "<a href=\"https://example.invalid/gallery\">Picture</a> ",
        "(<code>https://example.invalid/picture.png</code>) ",
        "(<code>https://example.invalid/gallery</code>)",
    )));
    assert!(page.contains("(<code>https://example.invalid/&quot;q&quot;</code>)"));
    assert!(!page.contains("<img"));
    for (label, destination) in [
        ("Relative", "../guide"),
        ("Fragment", "#fragment"),
        ("Other", "custom:inert"),
        ("Script", "javascript:inert"),
        ("Data", "data:text/plain,inert"),
    ] {
        assert!(page.contains(&format!("{label} (<code>{destination}</code>)")));
        assert!(!hrefs(&page).any(|href| href == destination));
    }
}

/// SDOC-4 and SDOC-7: initializer blocks stay in signatures, and the
/// parameter table and section headings use the extracted Markdown.
#[test]
fn site_renders_initializer_headers_and_entry_markdown() {
    let root = tempdir().unwrap();
    let source = root.path().join("extraction.hgl");
    fs::write(&source, include_str!("fixtures/doc_extraction.hgl")).unwrap();
    let site = write_site(root.path(), &source, &["--private"]);
    let page = fs::read_to_string(site.join("modules/mod-extraction_check.html")).unwrap();
    for signature in [
        "export const answer: i64 = { 42 }",
        "count: i64 = { 42 }",
        "export const fn closed_header() -&gt; i64",
        "test named_header",
    ] {
        assert!(page.contains(&format!(
            "<pre class=\"signature\"><code>{signature}</code></pre>"
        )));
    }
    assert!(page.contains("<p>First paragraph.</p>"));
    assert!(page.contains("<th>Key</th><th>Value</th>"));
    assert!(page.contains("<li>Item"));
    assert_eq!(page.matches("data-example=\"shown\"").count(), 2);
    assert!(!page.contains("data-example=\"runnable\""));
    assert!(page.contains("<p>Last paragraph.</p>"));
    for heading in ["Options", "Rules", "Status", "Notes for display"] {
        assert!(page.contains(&format!("<h4>{heading}</h4>")));
    }
    assert!(page.contains("<p>A custom section.</p>"));
}

/// SDOC-3 and SDOC-7: generated titles and Markdown use the containing
/// module, part, article, or section to set their heading levels.
#[test]
fn documentation_headings_follow_their_containing_outline() {
    let root = tempdir().unwrap();
    let source = root.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(
        source.join("main.hgl"),
        r#"/** # Module overview */
module outline

/**
# Public overview

Public details
--------------

Notes:
    # Public note

    ~~~text
    # Literal source
    ~~~
*/
export operator echo<T>(value: T) -> T

/**
# Candidate overview

Type Args:
    T: The input type.

Args:
    value: The input value.

        # Argument detail

Returns:
    # Return detail

    More detail
    -----------

Examples:
    ~~~hgl
    assert true
    ~~~
*/
impl const fn echo<T>(value: T) -> T => value

export struct Packet {
    /**
    Member prose.

    **Member note**:
        # Member detail
    */
    value: i64
}
"#,
    )
    .unwrap();
    fs::write(
        source.join("extra.hgl"),
        "/** # Part overview */\nmodule outline part details\n",
    )
    .unwrap();
    let site = write_site(root.path(), &source, &[]);
    let page = fs::read_to_string(site.join("modules/mod-outline.html")).unwrap();
    for (level, label) in [
        (2, "Module overview"),
        (2, "Part details"),
        (3, "Part overview"),
        (4, "Public overview"),
        (5, "Public details"),
        (5, "Public note"),
        (4, "Candidates"),
        (5, "echo"),
        (6, "Candidate overview"),
        (6, "Type arguments"),
        (6, "Arguments"),
        (6, "Returns"),
        (6, "Examples"),
        (4, "Members"),
        (5, "value"),
        (6, "<strong>Member note</strong>"),
    ] {
        let heading = format!("<h{level}>{label}</h{level}>");
        assert!(page.contains(&heading), "Missing {heading}");
    }
    for (level, label) in [
        (7, "Argument detail"),
        (7, "Return detail"),
        (8, "More detail"),
        (7, "Member detail"),
    ] {
        let heading = format!("<div role=\"heading\" aria-level=\"{level}\">{label}</div>");
        assert!(page.contains(&heading), "Missing {heading}");
    }
    assert_eq!(page.matches("<h1>").count(), 1);
    assert!(page.contains("<code class=\"language-text\"># Literal source\n</code>"));
    assert!(page.contains("data-example=\"runnable\""));
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

/// SDOC-8 and SDOC-10: private declarations and unrelated additions leave
/// public links intact. Names repeated on a page still have separate anchors.
#[test]
fn declaration_anchors_survive_private_and_unrelated_declarations() {
    let root = tempdir().unwrap();
    let source = root.path().join("source.hgl");
    let declarations = r#"
struct Hidden { shared: i64 }
/** The field is `Shown.shared` and the operator is `choose`. */
export struct Shown { shared: i64 }
export operator choose<T>(value: T) -> i64
impl const fn choose(value: i64) -> i64 => value
impl const fn choose(value: str) -> i64 => 0
"#;
    fs::write(&source, format!("module anchors\n{declarations}")).unwrap();
    let public = write_site(root.path(), &source, &[]);
    write_site(root.path(), &source, &["--out", "private", "--private"]);
    fs::write(
        &source,
        format!("module anchors\nexport const earlier: i64 = 1\n{declarations}"),
    )
    .unwrap();
    write_site(root.path(), &source, &["--out", "expanded"]);
    for (site, private) in [
        (public, false),
        (root.path().join("private"), true),
        (root.path().join("expanded"), false),
    ] {
        let page = fs::read_to_string(site.join("modules/mod-anchors.html")).unwrap();
        for anchor in ["Shown", "shared", "choose", "choose-2", "choose-3"] {
            assert_eq!(page.matches(&format!("id=\"decl-{anchor}\"")).count(), 1);
        }
        assert_eq!(page.contains("id=\"decl-shared-2\""), private);
        assert!(page.contains(r##"<a href="#decl-shared"><code>Shown.shared</code></a>"##));
        assert!(page.contains(r##"<a href="#decl-choose"><code>choose</code></a>"##));
        let search = fs::read_to_string(site.join("search-index.js")).unwrap();
        assert!(search.contains(r#""href":"modules/mod-anchors.html#decl-Shown""#));
        assert!(search.contains(r#""href":"modules/mod-anchors.html#decl-choose""#));
        internal_link_targets_exist(&site, &snapshot(&site));
    }
}

/// SDOC-8: paths for long module names stay distinct even on a
/// case-insensitive filesystem and do not depend on the other modules.
#[test]
fn long_module_paths_survive_unrelated_modules() {
    let root = tempdir().unwrap();
    let source = root.path().join("source");
    fs::create_dir(&source).unwrap();
    let prefix = format!("zz.{}", "long_module_part_for_reference.".repeat(8));
    let lower = format!("{prefix}values");
    let upper = format!("{prefix}Values");
    for (file, module) in [("lower.hgl", &lower), ("upper.hgl", &upper)] {
        fs::write(
            source.join(file),
            format!("module {module}\nexport const answer: i64 = 13\n"),
        )
        .unwrap();
    }
    fs::write(
        source.join("reference.hgl"),
        format!(
            "/** See `lower::answer` and `upper::answer`. */\n\
             module reference\nuse {lower} as lower\nuse {upper} as upper\n"
        ),
    )
    .unwrap();
    let original = write_site(root.path(), &source, &[]);
    fs::write(source.join("earlier.hgl"), "module a.first\n").unwrap();
    write_site(root.path(), &source, &["--out", "expanded"]);
    let references = |site: &Path| {
        let page = fs::read_to_string(site.join("modules/mod-reference.html")).unwrap();
        hrefs(&page)
            .filter_map(|link| link.strip_suffix("#decl-answer").map(str::to_owned))
            .collect::<Vec<_>>()
    };
    let paths = references(&original);
    let [lower_path, upper_path] = paths.as_slice() else {
        panic!("Expected two cross-module destinations, found {paths:?}");
    };
    assert_ne!(
        lower_path.to_ascii_lowercase(),
        upper_path.to_ascii_lowercase()
    );
    let expanded = root.path().join("expanded");
    assert_eq!(references(&expanded), paths);
    for (path, module) in [(lower_path, lower), (upper_path, upper)] {
        assert!(path.len() <= 205, "{path}");
        let contents = fs::read_to_string(expanded.join("modules").join(path)).unwrap();
        assert!(contents.contains(&format!("<h1>{module}</h1>")));
    }
    internal_link_targets_exist(&expanded, &snapshot(&expanded));
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

#[cfg(unix)]
#[test]
fn an_output_site_can_be_rebuilt_under_a_read_only_parent() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempdir().unwrap();
    let source = root.path().join("source");
    input(&source, "\n");
    let parent = root.path().join("read-only");
    let site = parent.join("site");
    fs::create_dir_all(&site).unwrap();
    let permissions = fs::metadata(&parent).unwrap().permissions();
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o555)).unwrap();

    let generate = || {
        Command::new(env!("CARGO_BIN_EXE_hgl"))
            .current_dir(root.path())
            .args(["doc", "source", "--out", "read-only/site"])
            .output()
            .unwrap()
    };
    let first = generate();
    fs::write(site.join("CNAME"), "docs.example.org").unwrap();
    fs::remove_file(source.join("api.hgl")).unwrap();
    let rebuild = generate();
    fs::set_permissions(&parent, permissions).unwrap();

    for output in [first, rebuild] {
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(site.join("index.html").is_file());
    assert!(!site.join("modules/mod-guide.api.html").exists());
    assert_eq!(
        fs::read_to_string(site.join("CNAME")).unwrap(),
        "docs.example.org"
    );
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
fn a_legacy_site_reuses_its_marker() {
    let root = tempdir().unwrap();
    let source = root.path().join("source");
    input(&source, "\n");
    let site = root.path().join("hgl-doc");
    fs::create_dir(&site).unwrap();
    let marker = site.join(".hgl-doc");
    let version = "hgl doc HTML site version 1\n";
    fs::write(&marker, version).unwrap();

    write_site(root.path(), &source, &[]);
    assert!(site.join("index.html").is_file());
    assert_eq!(fs::read_to_string(marker).unwrap(), version);
}

#[test]
fn concurrent_first_writes_and_rebuilds_leave_one_complete_site() {
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
    let site = root.path().join("hgl-doc");
    fs::create_dir(&site).unwrap();
    write_site(root.path(), &left, &["--out", "expected-left"]);
    write_site(root.path(), &right, &["--out", "expected-right"]);
    let expected_left = snapshot(&root.path().join("expected-left"));
    let expected_right = snapshot(&root.path().join("expected-right"));

    let publish = |source: &Path| {
        Command::new(env!("CARGO_BIN_EXE_hgl"))
            .current_dir(root.path())
            .arg("doc")
            .arg(source)
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap()
    };
    // The first pair both target the empty directory. The second pair
    // both replace a site installed by a previous run.
    for _ in 0..2 {
        for child in [publish(&left), publish(&right)] {
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let actual = snapshot(&site);
        assert!(actual == expected_left || actual == expected_right);
    }
}
