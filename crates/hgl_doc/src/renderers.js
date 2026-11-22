// SDOC-7: use local classic scripts to render formulas and diagrams on file://.
// Keep the source visible until the renderer produces a replacement.
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

  const math = document.querySelectorAll("code.formula-inline, code[data-renderer=math]");
  const maxMathSize = 20;
  const mathDimensions = ["height", "depth", "width", "lspace", "voffset"];
  if (math.length) {
    load("vendor/katex/katex.min.js").then(() => {
      for (const source of math) {
        const displayMode = source.dataset.renderer === "math";
        const rendered = document.createElement(displayMode ? "div" : "span");
        rendered.className = displayMode ? "formula-block" : "formula";
        try {
          katex.render(source.textContent, rendered, {
            displayMode,
            output: "mathml",
            trust: false,
            strict: "error",
            maxExpand: 1000,
            maxSize: maxMathSize,
            throwOnError: true,
          });
          // KaTeX's cap can leave signed offsets and spacing outside
          // the bound. Check their converted lengths before publishing.
          const oversized = [...rendered.querySelectorAll("mspace, mpadded")].some(element =>
            mathDimensions.some(name => {
              const size = element.getAttribute(name);
              return size?.endsWith("em") && Math.abs(Number.parseFloat(size)) > maxMathSize;
            })
          );
          if (oversized) {
            continue;
          }
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

  const diagrams = document.querySelectorAll("code[data-renderer=mermaid]");
  if (diagrams.length) {
    load("vendor/mermaid/mermaid.min.js").then(async () => {
      mermaid.initialize({
        startOnLoad: false,
        securityLevel: "strict",
        suppressErrorRendering: true,
        theme: matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "default",
        htmlLabels: false,
        journey: {textPlacement: "tspan"},
        secure: [...new Set([
          ...mermaid.mermaidAPI.defaultConfig.secure,
          "htmlLabels", "textPlacement", "themeCSS", "themeVariables", "fontFamily", "securityLevel",
        ])],
      });
      const template = document.createElement("template");
      for (const [index, source] of diagrams.entries()) {
        try {
          const { svg } = await mermaid.render("diagram-" + index, source.textContent);
          // Mermaid serializes a browser SVG fragment. Parse it as such:
          // its xlink attributes need no explicit XML namespace declaration.
          template.innerHTML = svg;
          const tree = template.content.firstElementChild;
          if (tree?.localName !== "svg" || tree.querySelector("foreignObject")) {
            continue;
          }
          for (const link of tree.querySelectorAll("a")) {
            // Diagram links never leave the offline reference.
            link.removeAttribute("href");
            link.removeAttributeNS("http://www.w3.org/1999/xlink", "href");
          }
          const rendered = document.createElement("div");
          rendered.className = "diagram";
          rendered.append(tree);
          source.parentElement.replaceWith(rendered);
        } catch {
          // The original diagram remains readable.
        }
      }
    }).catch(() => {});
  }
})();
