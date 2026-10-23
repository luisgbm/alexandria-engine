"use strict";
(() => {
  const form = document.querySelector("#search-form");
  const input = document.querySelector("#query");
  const results = document.querySelector("#search-results");
  const status = document.querySelector("#search-status");
  const normalize = value => value.toLowerCase().normalize("NFD").replace(/\p{M}/gu, "");
  function highlight(parent, text, terms) {
    for (const part of text.split(/([\p{L}\p{N}]+)/u)) {
      if (terms.includes(normalize(part))) {
        const mark = document.createElement("mark");
        mark.textContent = part;
        parent.append(mark);
      } else parent.append(document.createTextNode(part));
    }
  }
  let resources;
  let generation = 0;
  async function search(query) {
    const current = ++generation;
    results.replaceChildren();
    const terms = [...new Set(normalize(query).match(/[\p{L}\p{N}]+/gu) || [])];
    if (!terms.length) { status.textContent = "Digite uma palavra para pesquisar."; return; }
    status.textContent = "Pesquisando…";
    try {
      resources ||= Promise.all(["documents", "index"].map(name => fetch(`/search/${name}.json`).then(response => {
        if (!response.ok) throw new Error("Falha ao carregar o índice");
        return response.json();
      }))).catch(error => { resources = undefined; throw error; });
      const [documents, index] = await resources;
      if (generation !== current) return;
      const scores = new Map();
      for (const [id, score] of Object.entries(index[terms[0]] || {})) {
        if (terms.every(term => Object.hasOwn(index[term] || {}, id))) {
          scores.set(Number(id), terms.reduce((sum, term) => sum + index[term][id], 0));
        }
      }
      const ranked = [...scores].sort((a, b) => b[1] - a[1] || a[0] - b[0]);
      status.textContent = `${ranked.length} resultado(s).`;
      for (const [id] of ranked.slice(0, 100)) {
        const doc = documents[id];
        const item = document.createElement("li");
        const link = document.createElement("a");
        link.href = doc.url;
        highlight(link, doc.title, terms);
        const snippet = document.createElement("p");
        const position = normalize(doc.text).indexOf(terms[0]);
        const start = Math.max(0, position - 80);
        highlight(snippet, `${start ? "…" : ""}${doc.text.slice(start, start + 260)}${doc.text.length > start + 260 ? "…" : ""}`, terms);
        item.append(link, snippet);
        results.append(item);
      }
      if (ranked.length > 100) status.textContent += " Mostrando os primeiros 100; refine a pesquisa.";
    } catch (_) {
      if (generation === current) status.textContent = "Não foi possível carregar a pesquisa. Tente novamente.";
    }
  }
  form.addEventListener("submit", event => {
    event.preventDefault();
    const query = input.value.trim();
    history.replaceState(null, "", `?q=${encodeURIComponent(query)}`);
    search(query);
  });
  input.value = new URLSearchParams(location.search).get("q") || "";
  if (input.value) search(input.value);
})();
