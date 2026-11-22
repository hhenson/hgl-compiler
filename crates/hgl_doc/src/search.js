// Both scripts are classic scripts so the search index loads from file://.
(() => {
  const search = document.querySelector("#declaration-search");
  const query = search.querySelector("#declaration-query");
  const results = search.querySelector("#search-results");
  const status = search.querySelector('[role="status"]');

  const entries = hglDeclarations.map(entry => ({
    ...entry,
    key: entry.canonicalName.toLowerCase(),
  }));
  search.hidden = false;
  query.addEventListener("input", () => {
    const name = query.value.trim().toLowerCase();
    if (!name) {
      results.replaceChildren();
      status.textContent = "";
      return;
    }

    const found = entries.filter(entry => entry.key.includes(name));
    status.textContent = found.length === 1 ? "1 declaration" : found.length + " declarations";
    const links = document.createDocumentFragment();
    for (const entry of found) {
      const item = document.createElement("li");
      const link = document.createElement("a");
      link.href = entry.href;
      link.textContent = entry.canonicalName;
      item.append(link);
      links.append(item);
    }
    results.replaceChildren(links);
  });
})();
