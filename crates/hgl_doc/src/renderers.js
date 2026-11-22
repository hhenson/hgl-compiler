// Use local classic scripts so every renderer works on file://. Source stays
// visible until its renderer has produced a replacement successfully.
(() => {
  const base = new URL(".", document.currentScript.src);
  function load(path) {
    return new Promise((resolve, reject) => {
      const script = document.createElement("script");
      script.src = new URL(path, base).href;
      script.onload = resolve;
      script.onerror = reject;
      document.head.append(script);
    });
  }

  const math = document.querySelectorAll("code.formula-inline, code.language-math");
  if (math.length) {
    load("vendor/katex/katex.min.js").then(() => {
      for (const source of math) {
        const displayMode = source.classList.contains("language-math");
        const rendered = document.createElement(displayMode ? "div" : "span");
        rendered.className = displayMode ? "formula-block" : "formula";
        try {
          katex.render(source.textContent, rendered, {
            displayMode,
            output: "mathml",
            trust: false,
            throwOnError: true,
          });
          if (displayMode) {
            source.parentElement.replaceWith(rendered);
          } else {
            const before = source.previousSibling;
            const after = source.nextSibling;
            if (before?.nodeType === Node.TEXT_NODE && before.data.endsWith("$")) {
              before.data = before.data.slice(0, -1);
            }
            if (after?.nodeType === Node.TEXT_NODE && after.data.startsWith("$")) {
              after.data = after.data.slice(1);
            }
            source.replaceWith(rendered);
          }
        } catch {
          // The original formula remains readable.
        }
      }
    }).catch(() => {});
  }

  const diagrams = document.querySelectorAll("code.language-mermaid");
  if (diagrams.length) {
    load("vendor/mermaid/mermaid.min.js").then(async () => {
      mermaid.initialize({
        startOnLoad: false,
        securityLevel: "strict",
        suppressErrorRendering: true,
        theme: matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "default",
        flowchart: { htmlLabels: false },
      });
      const parser = new DOMParser();
      for (const [index, source] of diagrams.entries()) {
        try {
          const { svg } = await mermaid.render("diagram-" + index, source.textContent);
          const tree = parser.parseFromString(svg, "image/svg+xml").documentElement;
          if (tree.localName === "svg") {
            const rendered = document.createElement("div");
            rendered.className = "diagram";
            rendered.append(document.importNode(tree, true));
            source.parentElement.replaceWith(rendered);
          }
        } catch {
          // The original diagram remains readable.
        }
      }
    }).catch(() => {});
  }
})();
