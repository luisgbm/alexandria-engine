"use strict";

(() => {

  const $ = id => document.getElementById(id);

  let deployment = null, catalogRevision = "", selectedRevision = null;

  let csrf = "", doc = null, entries = [], categories = [], featured = null, dirty = false, previewTimer, previewGeneration = 0, bootstrap = "";

  const notice = (text, error = false) => { $("notice").textContent = text; $("notice").classList.toggle("error", error); };

  function loginView() { $("workspace").hidden = true; $("logout").hidden = true; $("login").hidden = false; }

  async function api(path, options = {}) {

    const headers = { ...(options.body instanceof FormData ? {} : {"Content-Type":"application/json"}), ...(csrf ? {"X-CSRF-Token":csrf} : {}), ...options.headers };

    const response = await fetch(path, {...options, headers, credentials:"same-origin", cache:"no-store"});

    const data = await response.json().catch(() => ({}));

    if (!response.ok) {

      if (response.status === 401) { loginView(); notice("Sua sessão expirou. Entre novamente; o texto continua nesta aba.", true); }

      throw new Error(data.error || "Não foi possível concluir a operação.");

    }

    return data;

  }

  function busy(button, action) { button.disabled = true; return Promise.resolve().then(action).catch(error => notice(error.name === "NotAllowedError" ? "Operação cancelada ou passkey indisponível. Você pode tentar novamente." : error.message, true)).finally(() => { button.disabled = false; }); }

  function decode(value) { const raw = atob(value.replace(/-/g,"+").replace(/_/g,"/")); return Uint8Array.from(raw, c => c.charCodeAt(0)); }

  function encode(buffer) { return btoa(String.fromCharCode(...new Uint8Array(buffer))).replace(/\+/g,"-").replace(/\//g,"_").replace(/=+$/g,""); }

  function options(json, registration) {

    const data = json.publicKey || json.public_key;

    if (registration && PublicKeyCredential.parseCreationOptionsFromJSON) return PublicKeyCredential.parseCreationOptionsFromJSON(data);

    if (!registration && PublicKeyCredential.parseRequestOptionsFromJSON) return PublicKeyCredential.parseRequestOptionsFromJSON(data);

    data.challenge = decode(data.challenge);

    if (data.user) data.user.id = decode(data.user.id);

    for (const key of ["allowCredentials","excludeCredentials"]) if (data[key]) data[key] = data[key].map(c => ({...c,id:decode(c.id)}));

    return data;

  }

  function credentialJSON(credential) {

    if (credential.toJSON) return credential.toJSON();

    const response = {clientDataJSON:encode(credential.response.clientDataJSON)};

    for (const key of ["attestationObject","authenticatorData","signature","userHandle"]) if (credential.response[key]) response[key] = encode(credential.response[key]);

    if (credential.response.getTransports) response.transports = credential.response.getTransports();

    return {id:credential.id,rawId:encode(credential.rawId),type:credential.type,response,extensions:credential.getClientExtensionResults()};

  }

  async function register(name, initial = false) {

    if (!window.PublicKeyCredential) throw new Error("Abra este endereço em um navegador com suporte a passkeys.");

    const flow = await api("/api/auth/register/start", {method:"POST", body:JSON.stringify({bootstrap:initial ? (bootstrap || $("bootstrap").value.trim()) : null})});

    const credential = await navigator.credentials.create({publicKey:options(flow.options,true)});

    await api("/api/auth/register/finish",{method:"POST",body:JSON.stringify({id:flow.id,credential:credentialJSON(credential),name})});

    bootstrap = ""; $("bootstrap").value = ""; $("key-dialog").close(); await openApp(); notice("Passkey cadastrada com segurança.");

  }

  $("signin").onclick = () => busy($("signin"), async () => {

    const flow = await api("/api/auth/login/start",{method:"POST",body:"{}"});

    const credential = await navigator.credentials.get({publicKey:options(flow.options,false)});

    await api("/api/auth/login/finish",{method:"POST",body:JSON.stringify({id:flow.id,credential:credentialJSON(credential)})});

    await openApp(); notice("Você entrou com sua passkey.");

  });

  $("enroll").onclick = () => busy($("enroll"), () => register($("first-key-name").value.trim(),true));

  $("add-key").onclick = () => $("key-dialog").showModal();

  $("cancel-key").onclick = () => $("key-dialog").close();

  $("key-form").onsubmit = event => { event.preventDefault(); busy(event.submitter, () => register($("key-name").value.trim())); };

  $("logout").onclick = () => busy($("logout"), async () => {

    if (dirty && !confirm("Há alterações não salvas. Deseja sair mesmo assim?")) return;

    await api("/api/logout",{method:"POST",body:"{}"}); csrf="";doc=null;dirty=false;$("editor").hidden=true;$("empty").hidden=false;$("preview").srcdoc="";loginView();notice("Sessão encerrada.");

  });

  async function openApp() {

    const session = await api("/api/session"); csrf=session.csrf;categories=session.site.categories;

    $("login").hidden=true;$("workspace").hidden=false;$("logout").hidden=false;

    $("key-count").textContent=`${session.credentials} passkey(s) cadastrada(s)`;

    $("key-hint").textContent=session.credentials<2 ? "Cadastre uma segunda passkey antes de depender exclusivamente deste acesso." : "Mantenha pelo menos duas alternativas de acesso e o material de recuperação offline.";

    const keyData=await api("/api/credentials");$("key-list").replaceChildren();

    for (const key of keyData.credentials) {const li=document.createElement("li");li.textContent=key.name;$("key-list").append(li);}

    await loadCategories(); await loadDeployments(); await refreshEntries();await loadAnalytics();

  }

  let analyticsGeneration=0;
  async function loadAnalytics(){
    const generation=++analyticsGeneration;$("analytics-status").textContent="Atualizando estatísticas…";
    try {const data=await api(`/api/analytics?period=${encodeURIComponent($("analytics-period").value)}`);if(generation!==analyticsGeneration)return;
      if(!data.enabled){$("analytics-status").textContent="As estatísticas ainda não foram ativadas neste ambiente.";return;}
      $("analytics-views").textContent=data.views.toLocaleString("pt-BR");$("analytics-visitors").textContent=data.visitor_days.toLocaleString("pt-BR");
      $("analytics-status").textContent=`Coleta iniciada em ${new Date(data.start*1000).toLocaleString("pt-BR")} · ${data.last_ingested ? "Última leitura: "+new Date(data.last_ingested*1000).toLocaleString("pt-BR") : "Aguardando primeira leitura"} · Horário de Brasília`;
      $("analytics-chart").replaceChildren();const max=Math.max(1,...data.series.map(d=>d.views));
      for(const point of data.series){const column=document.createElement("div"),bar=document.createElement("div"),label=document.createElement("span");column.className="analytics-column";bar.className="analytics-bar";bar.style.height=`${point.views ? Math.max(3,point.views/max*120) : 1}px`;bar.title=`${point.date}: ${point.views} visualizações, ${point.visitors} visitantes-dia`;bar.setAttribute("role","img");bar.setAttribute("aria-label",bar.title);label.textContent=point.date.slice(5).split("-").reverse().join("/");column.append(bar,label);$("analytics-chart").append(column);}
      $("analytics-top").replaceChildren();for(const article of data.top_articles){const row=document.createElement("tr"),name=document.createElement("td"),views=document.createElement("td"),visitors=document.createElement("td");name.textContent=article.title;views.textContent=article.views.toLocaleString("pt-BR");visitors.textContent=article.visitors.toLocaleString("pt-BR");row.append(name,views,visitors);$("analytics-top").append(row);}
      if(!data.top_articles.length){const row=document.createElement("tr"),cell=document.createElement("td");cell.colSpan=3;cell.textContent="Ainda não há leituras de artigos neste período.";row.append(cell);$("analytics-top").append(row);}
    } catch(error){if(generation===analyticsGeneration)$("analytics-status").textContent=error.message;}
  }
  $("analytics-period").onchange=loadAnalytics;
  $("analytics-refresh").onclick=()=>busy($("analytics-refresh"),loadAnalytics);
  async function refreshEntries() { entries=(await api("/api/articles")).entries; renderEntries(); }

  const normalize = text => text.toLowerCase().normalize("NFD").replace(/\p{M}/gu,"");

  function renderEntries() {

    const query=normalize($("filter").value), filter=$("status-filter").value;

    const visible=entries.filter(e => normalize(e.title+" "+(e.tags||[]).join(" ")).includes(query) && (filter==="trash" ? e.trashed : !e.trashed) && (filter==="trash" || filter==="all" || (filter==="page" ? e.kind==="page" : filter==="draft" ? e.has_saved_draft : e.status==="published"))).sort((a,b)=>b.date.localeCompare(a.date)||a.id.localeCompare(b.id));

    $("list-count").textContent=`${visible.length} item(ns)`;$("articles").replaceChildren();

    for (const entry of visible) {

      const li=document.createElement("li"),button=document.createElement("button"),small=document.createElement("small");

      button.type="button";button.textContent=entry.title;button.classList.toggle("selected",doc?.metadata.id===entry.id);

      small.textContent=`${entry.kind==="page" ? "Página" : "Artigo"} · ${entry.trashed ? "Lixeira" : entry.has_saved_draft ? "Rascunho" : "Publicado"} · ${entry.date.slice(0,10)}`;

      button.append(small);button.onclick=()=>busy(button,()=>selectEntry(entry.id));li.append(button);$("articles").append(li);

    }

  }

  $("filter").oninput=renderEntries;$("status-filter").onchange=renderEntries;

  function allowSwitch() {return !dirty || confirm("Há alterações não salvas. Deseja abrir outro artigo?");}

  async function selectEntry(id) {if(!allowSwitch())return;setDocument(await api(`/api/articles/${encodeURIComponent(id)}`));}

  function setDocument(data) {

    doc=data;dirty=false;featured=data.metadata.featured_image;const m=data.metadata;$("tags").value=m.tags.join(", ");

    $("editor").hidden=false;$("empty").hidden=true;$("title").value=m.title;$("html").value=data.html;$("permalink").value=m.permalink||"";

    $("permalink").readOnly=data.permalink_locked||data.publication_status==="published";$("path-hint").textContent=$("permalink").readOnly ? "Este endereço já é público e será preservado." : "Escolha o endereço definitivo antes da publicação.";

    $("published-local").value=m.original_published_local.replace(" ","T");$("image-alt").value=m.featured_image_alt||"";$("image-caption").value=m.featured_image_caption||"";

    $("categories").replaceChildren();for(const cat of categories){const option=document.createElement("option");option.value=cat.slug;option.textContent=cat.name;option.selected=m.categories.includes(cat.slug);$("categories").append(option);}

    const generated=m.listings.length>0;$("publish").hidden=!deployment?.enabled;$("publish").disabled=data.trashed;$("html").readOnly=generated;$("save").disabled=data.trashed;$("entry-state").textContent=generated ? "Página com lista automática" : data.publication_status==="published" ? "Conteúdo publicado · edição em rascunho" : "Rascunho privado";

    for(const field of $("editor").querySelectorAll("input,textarea,select"))field.disabled=data.trashed;
    $("insert-image").disabled=data.trashed;$("remove-image").disabled=data.trashed;
    $("trash-entry").textContent=data.trashed ? "Restaurar da lixeira" : "Mover para lixeira";
    $("trash-hint").textContent=data.trashed ? "Este conteúdo está na lixeira editorial. Uma versão já publicada continua disponível." : "Mover para a lixeira não apaga versões, imagens nem páginas publicadas.";
    $("dirty").textContent="";refreshThumb();renderEntries();schedulePreview();loadRevisions(data.metadata.id).catch(e=>notice(e.message,true));

  }

  async function loadDeployments() {

    deployment=await api("/api/deployments");$("deployments").hidden=!deployment.enabled;$("publish").hidden=!deployment.enabled;

    if(!deployment.enabled)return;

    $("active-release").textContent=`Versão ativa: ${deployment.active}`;$("view-staging").href=deployment.public_url+"/";$("release-list").replaceChildren();

    for(const release of deployment.releases){

      const li=document.createElement("li");li.textContent=`${new Date(release.created_at*1000).toLocaleString("pt-BR")} · ${release.id} `;

      if(release.id===deployment.active){li.append(document.createTextNode(" · Ativa"));}

      else {const button=document.createElement("button");button.type="button";button.className="quiet";button.textContent="Restaurar esta versão";button.onclick=()=>busy(button,async()=>{

        if(dirty)throw new Error("Salve ou descarte as alterações antes de restaurar uma versão.");

        if(!confirm(`Restaurar ${release.id} no staging? Os rascunhos serão mantidos.`))return;

        notice("Verificando e restaurando versão de staging…");await api("/api/rollback",{method:"POST",body:JSON.stringify({release:release.id,expected_active:deployment.active})});

        await loadDeployments();await refreshEntries();if(doc)await selectEntry(doc.metadata.id);notice("Versão anterior restaurada no staging.");

      });li.append(button);}$("release-list").append(li);

    }

    const latest=deployment.events[0];$("deployment-result").textContent=latest ? `Última operação: ${latest.action} · ${latest.result}` : "Nenhuma publicação nova.";

  }

  $("publish").onclick=()=>busy($("publish"),async()=>{

    if(!doc)return;if(doc.trashed)throw new Error("Restaure o conteúdo da lixeira antes de publicar.");if(dirty)throw new Error("Salve o rascunho antes de publicar.");

    if(!confirm(`Publicar “${doc.metadata.title}” somente no staging?`))return;

    notice("Gerando e validando o site. A versão atual permanece disponível…");

    await api(`/api/publish/${encodeURIComponent(doc.metadata.id)}`,{method:"POST",body:JSON.stringify({revision:doc.revision,expected_active:deployment.active})});

    await loadDeployments();await refreshEntries();await selectEntry(doc.metadata.id);notice("Publicado no staging. O site original não foi alterado.");

  });

  $("new-article").onclick=()=>busy($("new-article"),async()=>{if(!allowSwitch())return;setDocument(await api("/api/articles",{method:"POST",body:"{}"}));await refreshEntries();notice("Novo rascunho criado.");});

  $("new-page").onclick=()=>busy($("new-page"),async()=>{if(!allowSwitch())return;setDocument(await api("/api/articles",{method:"POST",body:JSON.stringify({kind:"page"})}));await refreshEntries();notice("Nova página criada em rascunho.");});
  async function loadRevisions(id){
    const result=await api(`/api/articles/${encodeURIComponent(id)}/revisions`);if(doc?.metadata.id!==id)return;
    $("revision-list").replaceChildren();for(const revision of result.revisions){
      const li=document.createElement("li"),button=document.createElement("button");button.type="button";button.className="quiet";
      button.textContent=revision.original ? "Versão importada original" : `${revision.date ? new Date(revision.date).toLocaleString("pt-BR") : revision.revision} · ${revision.title}${revision.current ? " · Atual" : ""}`;
      button.onclick=()=>busy(button,async()=>{const previous=await api(`/api/articles/${encodeURIComponent(id)}/revisions/${encodeURIComponent(revision.revision)}`);if(doc?.metadata.id!==id)return;
        selectedRevision={id,from:revision.revision,revision:doc.revision};$("revision-heading").textContent=previous.metadata.title;$("revision-details").textContent="Restaurar cria uma nova versão privada; o endereço publicado é preservado.";$("revision-html").value=previous.html;$("restore-revision").disabled=doc.trashed;$("revision-dialog").showModal();});li.append(button);$("revision-list").append(li);
    }
  }
  $("close-revision").onclick=()=>$("revision-dialog").close();
  $("restore-revision").onclick=()=>busy($("restore-revision"),async()=>{
    if(!selectedRevision)return;if(dirty)throw new Error("Salve ou descarte as alterações antes de restaurar uma versão.");
    if(!confirm("Restaurar esta versão como um novo rascunho privado?"))return;
    const result=await api(`/api/articles/${encodeURIComponent(selectedRevision.id)}/restore`,{method:"POST",body:JSON.stringify(selectedRevision)});
    $("revision-dialog").close();setDocument(result);await refreshEntries();notice("Versão restaurada em um novo rascunho. O site não mudou.");
  });
  $("trash-entry").onclick=()=>busy($("trash-entry"),async()=>{
    if(!doc)return;if(dirty)throw new Error("Salve ou descarte as alterações antes de mover o conteúdo.");
    if(!confirm(doc.trashed ? "Restaurar este conteúdo da lixeira?" : "Mover para a lixeira editorial? Páginas publicadas continuam disponíveis."))return;
    const result=await api(`/api/articles/${encodeURIComponent(doc.metadata.id)}/trash`,{method:"POST",body:JSON.stringify({revision:doc.revision,trashed:!doc.trashed})});
    setDocument(result);await refreshEntries();notice(result.trashed ? "Conteúdo movido para a lixeira. O site não mudou." : "Conteúdo restaurado da lixeira.");
  });
  async function loadCategories(){
    const result=await api("/api/categories");catalogRevision=result.revision;categories=result.categories;$("category-list").replaceChildren();
    for(const category of categories){const li=document.createElement("li"),name=document.createElement("input"),rename=document.createElement("button"),remove=document.createElement("button"),info=document.createElement("small");
      name.value=category.name;name.maxLength=100;name.setAttribute("aria-label",`Nome da categoria ${category.slug}`);info.textContent=` /category/${category.slug}/ · ${category.entries} conteúdo(s) `;
      rename.type=remove.type="button";rename.textContent="Renomear";remove.textContent="Excluir";rename.className=remove.className="quiet";
      rename.onclick=()=>busy(rename,async()=>{await api(`/api/categories/${encodeURIComponent(category.slug)}`,{method:"PUT",body:JSON.stringify({revision:catalogRevision,name:name.value})});await loadCategories();if(doc&&!dirty)setDocument(await api(`/api/articles/${encodeURIComponent(doc.metadata.id)}`));notice("Categoria renomeada no catálogo editorial. Publique um conteúdo para atualizar o staging.");});
      remove.onclick=()=>busy(remove,async()=>{if(!confirm(`Excluir a categoria “${category.name}” se estiver sem uso?`))return;await api(`/api/categories/${encodeURIComponent(category.slug)}`,{method:"DELETE",body:JSON.stringify({revision:catalogRevision})});await loadCategories();notice("Categoria sem uso excluída.");});
      const up=document.createElement("button"),down=document.createElement("button");up.type=down.type="button";up.textContent="↑";down.textContent="↓";up.setAttribute("aria-label",`Subir ${category.name}`);down.setAttribute("aria-label",`Descer ${category.name}`);
      for(const [button,delta] of [[up,-1],[down,1]]){const index=categories.indexOf(category);button.disabled=index+delta<0||index+delta>=categories.length;button.onclick=()=>busy(button,async()=>{const slugs=categories.map(c=>c.slug);[slugs[index],slugs[index+delta]]=[slugs[index+delta],slugs[index]];await api("/api/categories/order",{method:"POST",body:JSON.stringify({revision:catalogRevision,slugs})});await loadCategories();notice("Ordem das categorias salva.");});}
      li.append(name,info,rename,remove,up,down);$("category-list").append(li);
    }
    if(doc){const selected=new Set([...$("categories").selectedOptions].map(o=>o.value));$("categories").replaceChildren();for(const category of categories){const option=document.createElement("option");option.value=category.slug;option.textContent=category.name;option.selected=selected.has(category.slug);$("categories").append(option);}}
  }
  $("create-category").onclick=()=>busy($("create-category"),async()=>{await api("/api/categories",{method:"POST",body:JSON.stringify({revision:catalogRevision,name:$("category-name").value,slug:$("category-slug").value})});$("category-name").value="";$("category-slug").value="";await loadCategories();if(doc&&!dirty)setDocument(await api(`/api/articles/${encodeURIComponent(doc.metadata.id)}`));notice("Categoria criada.");});

  function edit() {if(!doc)return;dirty=true;$("dirty").textContent="Alterações não salvas";schedulePreview();}

  for(const id of ["title","html","permalink","published-local","image-alt","image-caption","categories","tags"])$(id).addEventListener("input",edit);

  function fields() {return {tags:$("tags").value.split(",").map(t=>t.trim()).filter(Boolean),title:$("title").value,html:$("html").value,permalink:$("permalink").value.trim()||null,published_local:$("published-local").value.length===16 ? $("published-local").value+":00" : $("published-local").value,categories:[...$("categories").selectedOptions].map(o=>o.value),featured_image:featured,featured_image_alt:$("image-alt").value,featured_image_caption:$("image-caption").value};}

  $("editor").onsubmit=event=>{event.preventDefault();busy($("save"),async()=>{const result=await api(`/api/articles/${encodeURIComponent(doc.metadata.id)}`,{method:"PUT",body:JSON.stringify({...fields(),revision:doc.revision})});setDocument(result);await refreshEntries();notice("Rascunho salvo. O site não foi alterado.");});};

  function refreshThumb() {$("featured-thumb").hidden=!featured;if(featured)$("featured-thumb").src="/api/media"+featured;else $("featured-thumb").removeAttribute("src");}

  $("file").onchange=async()=>{const file=$("file").files[0];if(!file)return;try{if(file.size>8*1024*1024)throw new Error("Escolha uma imagem de até 8 MB.");notice("Enviando e verificando imagem…");const form=new FormData();form.append("image",file);const result=await api("/api/uploads",{method:"POST",body:form});featured=result.path;refreshThumb();edit();notice("Imagem enviada. Inclua uma descrição e uma legenda.");}catch(error){notice(error.message,true);}finally{$("file").value="";}};

  $("remove-image").onclick=()=>{featured=null;refreshThumb();edit();};

  const escape = value => value.replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;").replace(/"/g,"&quot;");

  $("insert-image").onclick=()=>{if(!featured){notice("Escolha uma imagem primeiro.",true);return;}const html=`\n<figure><img src="${escape(featured)}" alt="${escape($("image-alt").value)}"><figcaption>${escape($("image-caption").value)}</figcaption></figure>\n`;const field=$("html");field.setRangeText(html,field.selectionStart,field.selectionEnd,"end");edit();};

  function schedulePreview() {clearTimeout(previewTimer);previewTimer=setTimeout(renderPreview,650);}

  async function renderPreview() {if(!doc)return;const generation=++previewGeneration;$("preview-status").textContent="Atualizando prévia…";try{const result=await api("/api/preview",{method:"POST",body:JSON.stringify(fields())});if(generation===previewGeneration){$("preview").srcdoc=result.html;$("preview-status").textContent="Prévia isolada · o site não foi alterado";}}catch(error){if(generation===previewGeneration)$("preview-status").textContent=error.message;}}

  $("refresh-preview").onclick=renderPreview;

  $("wide").onclick=()=>{$("preview").classList.remove("phone");$("wide").setAttribute("aria-pressed","true");$("narrow").setAttribute("aria-pressed","false");};

  $("narrow").onclick=()=>{$("preview").classList.add("phone");$("narrow").setAttribute("aria-pressed","true");$("wide").setAttribute("aria-pressed","false");};

  window.addEventListener("beforeunload",event=>{if(dirty){event.preventDefault();event.returnValue="";}});

  (async()=>{const fragment=new URLSearchParams(location.hash.slice(1));bootstrap=fragment.get("setup")||"";if(location.hash)history.replaceState(null,"",location.pathname);const status=await api("/api/status");$("setup").hidden=status.initialized;$("signin").hidden=!status.initialized;if(bootstrap)$("bootstrap").value=bootstrap;try{await openApp();}catch(_){loginView();if(!status.initialized)notice("Cadastre sua primeira passkey para abrir o ambiente editorial.");else notice("");}})().catch(error=>notice(error.message,true));

})();
