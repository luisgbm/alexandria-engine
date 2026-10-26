"use strict";
(() => {
  const form = document.querySelector("#search-form");
  const input = document.querySelector("#query");
  const results = document.querySelector("#search-results");
  const status = document.querySelector("#search-status");
  const panel = document.querySelector("#search-panel");
  const content = document.querySelector("#page-content");
  const close = document.querySelector("#close-search");
  function updateUrl(query) {
    const url = new URL(location.href);
    if (url.pathname === "/search/") url.pathname = "/";
    if (query) url.searchParams.set("q",query); else url.searchParams.delete("q");
    history.replaceState(null,"",url.pathname+url.search+url.hash);
  }
  function clearSearch(focus = false) {
    ++generation;input.value="";results.replaceChildren();status.textContent="";
    panel.hidden=true;content.hidden=false;updateUrl("");
    if(focus)input.focus();
  }
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
  const parts = new Map();
  function json(path) {
    if (!parts.has(path)) parts.set(path, fetch(path,{cache:"no-cache"}).then(response=>{
      if(!response.ok)throw new Error("Falha ao carregar o índice");
      return response.json();
    }).catch(error=>{parts.delete(path);throw error;}));
    return parts.get(path);
  }
  let generation = 0;
  async function search(query) {
    const current = ++generation;
    results.replaceChildren();
    const terms = [...new Set(normalize(query).match(/[\p{L}\p{N}]+/gu) || [])];
    if (!terms.length) { clearSearch(); return; }
    panel.hidden=false;content.hidden=true;
    status.textContent = "Pesquisando…";
    try {
      resources ||= Promise.all(["documents", "index"].map(name => fetch(`/search/${name}.json`,{cache:"no-cache"}).then(response => {
        if (!response.ok) throw new Error("Falha ao carregar o índice");
        return response.json();
      }))).catch(error => { resources = undefined; throw error; });
      const [documents, description] = await resources;
      if (generation !== current) return;
      const sharded=description.schema_version===2;
      let index=description;
      if(sharded){
        index={};
        const available=new Set(description.shards);
        const keys=[...new Set(terms.map(term=>Array.from(term).slice(0,2).map(c=>c.codePointAt(0).toString(16)).join("-")))];
        const loaded=await Promise.all(keys.filter(key=>available.has(key)).map(key=>json(`/search/terms-${key}.json`)));
        for(const part of loaded)Object.assign(index,part);
        if(generation!==current)return;
      }
      const positions=new Map(documents.map((doc,i)=>[doc.id,i]));
      const scores = new Map();
      for (const [id, score] of Object.entries(index[terms[0]] || {})) {
        if (terms.every(term => Object.hasOwn(index[term] || {}, id))) {
          const position=sharded?positions.get(id):Number(id);
          if(position!==undefined)scores.set(position, terms.reduce((sum, term) => sum + index[term][id], 0));
        }
      }
      const ranked = [...scores].sort((a, b) => b[1] - a[1] || a[0] - b[0]);
      status.textContent = `${ranked.length} resultado(s).`;
      const selected=ranked.slice(0,100).map(([id])=>documents[id]);
      const detailed=await Promise.all(selected.map(doc=>{
        if(!sharded)return doc;
        if(!/^[A-Za-z0-9_-]+$/.test(doc.id))throw new Error("Documento inválido");
        if(description.text_shards===16){
          let hash=0;for(const c of doc.id)hash=(Math.imul(hash,31)+c.charCodeAt(0))>>>0;
          return json(`/search/text-group-${(hash%16).toString(16)}.json`).then(group=>{
            if(!Object.hasOwn(group,doc.id))throw new Error("Documento não encontrado");
            return group[doc.id];
          });
        }
        return json(`/search/text-${doc.id}.json`);
      }));
      if(generation!==current)return;
      for (const doc of detailed) {
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
    updateUrl(query);
    search(query);
  });
  close.addEventListener("click",()=>clearSearch(true));
  input.addEventListener("input",()=>{if(!input.value.trim())clearSearch();});
  input.addEventListener("keydown",event=>{if(event.key==="Escape"){event.preventDefault();clearSearch(true);}});
  if(location.pathname==="/search/") updateUrl(new URLSearchParams(location.search).get("q")||"");
  input.value = new URLSearchParams(location.search).get("q") || "";
  if (input.value) search(input.value);
})();
