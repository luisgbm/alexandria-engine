"use strict";

(() => {

  const $ = id => document.getElementById(id);

  let sidebarRevision="", sidebarDirty=false;
  let deployment = null, catalogRevision = "", selectedRevision = null;

  let environment = "staging", environmentLabel = "Ambiente de testes";
  let csrf = "", doc = null, entries = [], categories = [], dirty = false, previewTimer, previewGeneration = 0, bootstrap = "";

  let noticeTimer;
  const notice = (text, error = false) => {
    clearTimeout(noticeTimer);
    const status = $("notice");
    status.textContent = text;
    status.hidden = !text;
    status.classList.toggle("error", error);
    const local=$("image-notice");if($("image-dialog").open){local.textContent=text;local.hidden=!text;local.classList.toggle("error",error);}
    // Keep errors and in-progress operations visible; dismiss completed feedback.
    if(text && !error && !text.endsWith("…")) noticeTimer=setTimeout(()=>{
      status.hidden=true;
      if(local.textContent===text)local.hidden=true;
    },5000);
  };

  // Mutation success is authoritative; a later refresh failure must not suggest
  // that saving, publishing or rollback failed and encourage a duplicate action.
  async function refreshAfterSuccess(message, refresh) {
    notice(message);
    try { await refresh(); }
    catch (_) { notice(`${message} Não foi possível atualizar os painéis. Recarregue a página para conferir.`, true); }
  }

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

  function busy(button, action) { const label = button.textContent; button.disabled = true; if(button.id === "save")button.textContent = "Salvando…"; return Promise.resolve().then(action).catch(error => notice(error.name === "NotAllowedError" ? "Operação cancelada ou passkey indisponível. Você pode tentar novamente." : error.message, true)).finally(() => { button.disabled = false; if(button.id === "save")button.textContent = label; }); }

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

    if ((dirty||sidebarDirty) && !confirm("Há alterações não salvas. Deseja sair mesmo assim?")) return;

    await api("/api/logout",{method:"POST",body:"{}"}); csrf="";doc=null;dirty=false;$("editor").hidden=true;$("empty").hidden=false;$("preview").srcdoc="";loginView();notice("Sessão encerrada.");

  });

  const pages = {"/":"statistics","/content/":"content","/images/":"images","/appearance/":"appearance","/categories/":"categories","/publications/":"publications","/security/":"security"};
  function identity(data) {
    environment = data.environment; environmentLabel = data.environment_label;
    $("environment-badge").textContent = environmentLabel;
    $("publish").textContent = environment === "production" ? "Publicar" : "Publicar no staging";
    $("publish-sidebar").textContent = environment === "production" ? "Publicar barra lateral" : "Publicar barra lateral no staging";
  }
  async function showPage(path, push = false) {
    const name = pages[path] || "statistics";
    if(push) history.pushState(null,"",path);
    for(const panel of document.querySelectorAll("[data-page]")) panel.hidden=panel.dataset.page!==name;
    for(const link of document.querySelectorAll(".admin-nav a")) {if(link.pathname===path)link.setAttribute("aria-current","page");else link.removeAttribute("aria-current");}
    document.title = `${document.querySelector('.admin-nav a[aria-current]')?.textContent || "Administração"} · Alexandria`;
    if(name==="statistics")await loadAnalytics();
    if(name==="content"){await loadCategories();await loadDeployments();await refreshEntries();}
    if(name==="categories")await loadCategories();
    if(name==="publications")await loadDeployments();
    if(name==="images")await loadLibrary();
    if(name==="appearance"){await loadDeployments();if(!sidebarDirty)await loadHomepage();}
    if(name==="security")await loadPasskeys();
  }
  let keyToRemove = null;
  async function loadPasskeys() {
    const keys=await api("/api/credentials");$("key-list").replaceChildren();
    const last=keys.credentials.length<2;
    $("key-count").textContent=`${keys.credentials.length} passkey(s) cadastrada(s)`;
    $("key-hint").textContent=last ? "A última passkey não pode ser removida. Cadastre outra primeiro." : "Mantenha duas alternativas de acesso e o material de recuperação offline.";
    for(const key of keys.credentials){
      const li=document.createElement("li"),label=document.createElement("span"),button=document.createElement("button");
      label.textContent=`${key.name} · ${new Date(key.created_at*1000).toLocaleString("pt-BR")}`;
      button.type="button";button.className="quiet";button.textContent="Remover passkey";button.disabled=last;
      button.setAttribute("aria-label",`Remover passkey: ${key.name}`);
      if(last)button.title="Cadastre outra passkey antes de remover esta.";
      button.onclick=()=>{keyToRemove=key;$("remove-key-name").textContent=key.name;$("remove-key-status").textContent="";$("confirm-remove-key").disabled=false;$("remove-key-dialog").showModal();};
      li.append(label,button);$("key-list").append(li);
    }
  }
  $("cancel-remove-key").onclick=()=>$("remove-key-dialog").close();
  $("remove-key-dialog").addEventListener("close",()=>{keyToRemove=null;});
  $("remove-key-form").onsubmit=async event=>{
    event.preventDefault();if(!keyToRemove)return;
    const button=$("confirm-remove-key");button.disabled=true;$("cancel-remove-key").disabled=true;$("remove-key-status").textContent="Removendo passkey…";
    try {
      await api(`/api/credentials/${encodeURIComponent(keyToRemove.id)}/remove`,{method:"POST",body:JSON.stringify({confirmation:"REMOVER PASSKEY"})});
      $("remove-key-dialog").close();
      await refreshAfterSuccess("Passkey removida. Outras sessões foram encerradas. Remova também a cópia do seu gerenciador de senhas, se desejar.",loadPasskeys);
    } catch(error) { $("remove-key-status").textContent=error.message; }
    finally { button.disabled=false;$("cancel-remove-key").disabled=false; }
  };
  document.addEventListener("click",event=>{const link=event.target.closest("a[data-nav]");if(!link || event.ctrlKey || event.metaKey || event.shiftKey || event.button!==0)return;event.preventDefault();showPage(link.pathname,true).catch(e=>notice(e.message,true));});
  window.addEventListener("popstate",()=>{if(csrf)showPage(location.pathname).catch(e=>notice(e.message,true));});
  async function openApp() {
    const session = await api("/api/session"); csrf=session.csrf;categories=session.site.categories;identity(session);
    $("login").hidden=true;$("workspace").hidden=false;$("logout").hidden=false;
    await showPage(location.pathname);
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
    $("empty-trash").hidden=filter!=="trash";$("empty-trash").disabled=!entries.some(e=>e.trashed);

    const visible=entries.filter(e => normalize(e.title).includes(query) && (filter==="trash" ? e.trashed : !e.trashed) && (filter==="trash" || filter==="all" || (filter==="page" ? e.kind==="page" : filter==="draft" ? e.has_saved_draft : e.status==="published"))).sort((a,b)=>b.date.localeCompare(a.date)||a.id.localeCompare(b.id));

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

    if(doc?.metadata.id!==data.metadata.id)autoPermalink=true;
    clearTimeout(permalinkTimer);++permalinkGeneration;
    doc=data;dirty=false;const m=data.metadata;

    $("editor").hidden=false;$("empty").hidden=true;$("title").value=m.title;$("html").value=editableHtml(data);$("permalink").value=m.permalink||"";

    $("permalink").readOnly=data.permalink_locked||data.publication_status==="published";$("path-hint").textContent=$("permalink").readOnly ? "Este endereço já é público e será preservado." : "Gerado pelo título; endereços repetidos recebem um sufixo. Você também pode editar manualmente.";

    $("published-local").value=m.original_published_local.replace(" ","T");

    $("categories").replaceChildren();for(const cat of categories){const option=document.createElement("option");option.value=cat.slug;option.textContent=cat.name;option.selected=m.categories.includes(cat.slug);$("categories").append(option);}

    const generated=m.listings.length>0;$("publish").hidden=!deployment?.enabled;$("publish").disabled=data.trashed;$("html").readOnly=generated;$("save").disabled=data.trashed;$("entry-state").textContent=generated ? "Página com lista automática" : data.publication_status==="published" ? "Conteúdo publicado · edição em rascunho" : "Rascunho privado";

    for(const field of $("editor").querySelectorAll("input,textarea,select"))field.disabled=data.trashed;

    $("insert-more").disabled=data.trashed||generated;
    $("trash-entry").textContent=data.trashed ? "Restaurar da lixeira" : "Mover para lixeira";
    $("trash-hint").textContent=data.trashed ? "Este conteúdo está na lixeira editorial. Uma versão já publicada continua disponível." : "Mover para a lixeira não apaga versões, imagens nem páginas publicadas.";
    $("dirty").textContent="";renderEntries();schedulePreview();loadRevisions(data.metadata.id).catch(e=>notice(e.message,true));

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

        if(!confirm(`Restaurar ${release.id} em ${environmentLabel}? Os rascunhos serão mantidos.`))return;

        notice(`Verificando e restaurando versão em ${environmentLabel}…`);await api("/api/rollback",{method:"POST",body:JSON.stringify({release:release.id,expected_active:deployment.active})});

        await refreshAfterSuccess(`Versão anterior restaurada em ${environmentLabel}.`, async()=>{await loadDeployments();await refreshEntries();if(doc)await selectEntry(doc.metadata.id);});

      });li.append(button);}$("release-list").append(li);

    }

    const latest=deployment.events[0];$("deployment-result").textContent=latest ? `Última operação: ${latest.action} · ${latest.result}` : "Nenhuma publicação nova.";

  }

  $("publish").onclick=()=>busy($("publish"),async()=>{

    if(!doc)return;if(doc.trashed)throw new Error("Restaure o conteúdo da lixeira antes de publicar.");if(dirty)throw new Error("Salve o rascunho antes de publicar.");

    if(!confirm(`Publicar “${doc.metadata.title}” em ${environmentLabel}?`))return;

    notice("Gerando e validando o site. A versão atual permanece disponível…");

    await api(`/api/publish/${encodeURIComponent(doc.metadata.id)}`,{method:"POST",body:JSON.stringify({revision:doc.revision,expected_active:deployment.active})});

    await refreshAfterSuccess(`Publicado em ${environmentLabel}.`, async()=>{await loadDeployments();await refreshEntries();await selectEntry(doc.metadata.id);});

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
  let trashRevision="";
  $("empty-trash").onclick=()=>busy($("empty-trash"),async()=>{
    if(dirty)throw new Error("Salve ou descarte as alterações antes de esvaziar a lixeira.");
    const data=await api("/api/trash");trashRevision=data.revision;
    $("empty-trash-items").replaceChildren();for(const item of data.items){const li=document.createElement("li");li.textContent=item.title;$("empty-trash-items").append(li);}
    $("empty-trash-confirmation").value="";$("confirm-empty-trash").disabled=true;
    $("empty-trash-status").textContent=`${data.items.length} item(ns) na lixeira inteira, incluindo itens fora da busca atual.`;
    if(!data.items.length){await refreshEntries();notice("A lixeira já está vazia.");return;}
    $("empty-trash-dialog").showModal();
  });
  $("cancel-empty-trash").onclick=()=>$("empty-trash-dialog").close();
  $("empty-trash-confirmation").oninput=()=>{$("confirm-empty-trash").disabled=$("empty-trash-confirmation").value!=="ESVAZIAR LIXEIRA";};
  $("empty-trash-form").onsubmit=async event=>{
    event.preventDefault();const button=$("confirm-empty-trash");button.disabled=true;$("cancel-empty-trash").disabled=true;
    try{
      const result=await api("/api/trash",{method:"POST",body:JSON.stringify({revision:trashRevision,confirmation:$("empty-trash-confirmation").value})});
      $("empty-trash-dialog").close();
      if(doc?.trashed){doc=null;dirty=false;$("editor").hidden=true;$("empty").hidden=false;$("preview").srcdoc="";clearTimeout(previewTimer);previewGeneration++;}
      await refreshAfterSuccess(`${result.deleted} item(ns) excluído(s) da lixeira. Páginas publicadas e imagens foram preservadas.${result.cleanup_pending ? " A limpeza de arquivos privados precisa de revisão pelo operador." : ""}`,refreshEntries);
    }catch(error){$("empty-trash-status").textContent=error.message;}
    finally{button.disabled=$("empty-trash-confirmation").value!=="ESVAZIAR LIXEIRA";$("cancel-empty-trash").disabled=false;}
  };

  async function loadCategories(){
    const result=await api("/api/categories");catalogRevision=result.revision;categories=result.categories;$("category-list").replaceChildren();
    for(const category of categories){const li=document.createElement("li"),name=document.createElement("input"),rename=document.createElement("button"),remove=document.createElement("button"),info=document.createElement("small");
      name.value=category.name;name.maxLength=100;name.setAttribute("aria-label",`Nome da categoria ${category.slug}`);info.textContent=` /category/${category.slug}/ · ${category.entries} conteúdo(s) `;
      rename.type=remove.type="button";rename.textContent="Renomear";remove.textContent="Excluir";rename.className=remove.className="quiet";
      rename.onclick=()=>busy(rename,async()=>{await api(`/api/categories/${encodeURIComponent(category.slug)}`,{method:"PUT",body:JSON.stringify({revision:catalogRevision,name:name.value})});await loadCategories();if(doc&&!dirty)setDocument(await api(`/api/articles/${encodeURIComponent(doc.metadata.id)}`));notice("Categoria renomeada no catálogo editorial. Publique um conteúdo para atualizar o site deste ambiente.");});
      remove.onclick=()=>busy(remove,async()=>{if(!confirm(`Excluir a categoria “${category.name}” se estiver sem uso?`))return;await api(`/api/categories/${encodeURIComponent(category.slug)}`,{method:"DELETE",body:JSON.stringify({revision:catalogRevision})});await loadCategories();notice("Categoria sem uso excluída.");});
      const up=document.createElement("button"),down=document.createElement("button");up.type=down.type="button";up.textContent="↑";down.textContent="↓";up.setAttribute("aria-label",`Subir ${category.name}`);down.setAttribute("aria-label",`Descer ${category.name}`);
      for(const [button,delta] of [[up,-1],[down,1]]){const index=categories.indexOf(category);button.disabled=index+delta<0||index+delta>=categories.length;button.onclick=()=>busy(button,async()=>{const slugs=categories.map(c=>c.slug);[slugs[index],slugs[index+delta]]=[slugs[index+delta],slugs[index]];await api("/api/categories/order",{method:"POST",body:JSON.stringify({revision:catalogRevision,slugs})});await loadCategories();notice("Ordem das categorias salva.");});}
      li.append(name,info,rename,remove,up,down);$("category-list").append(li);
    }
    if(doc){const selected=new Set([...$("categories").selectedOptions].map(o=>o.value));$("categories").replaceChildren();for(const category of categories){const option=document.createElement("option");option.value=category.slug;option.textContent=category.name;option.selected=selected.has(category.slug);$("categories").append(option);}}
  }
  $("create-category").onclick=()=>busy($("create-category"),async()=>{await api("/api/categories",{method:"POST",body:JSON.stringify({revision:catalogRevision,name:$("category-name").value,slug:$("category-slug").value})});$("category-name").value="";$("category-slug").value="";await loadCategories();if(doc&&!dirty)setDocument(await api(`/api/articles/${encodeURIComponent(doc.metadata.id)}`));notice("Categoria criada.");});

  $("insert-more").onclick=()=>{
    if(!doc||doc.trashed||$("html").readOnly)return;
    const field=$("html");field.setRangeText("<!--more-->",field.selectionStart,field.selectionEnd,"end");field.focus();edit();
    notice("Corte da prévia inserido. Salve o rascunho para manter a alteração.");
  };
  async function loadHomepage(){
    $("sidebar-html").disabled=true;$("save-sidebar").disabled=true;
    try{const data=await api("/api/homepage");sidebarRevision=data.revision;$("sidebar-html").value=data.html;sidebarDirty=false;$("sidebar-state").textContent="Rascunho salvo. Os artigos recentes são gerados automaticamente.";}
    finally{$("sidebar-html").disabled=false;$("save-sidebar").disabled=false;}
    $("publish-sidebar").hidden=!deployment?.enabled;$("view-homepage").hidden=!deployment?.enabled;
    if(deployment?.enabled)$("view-homepage").href=deployment.public_url+"/";
  }
  $("sidebar-html").addEventListener("input",()=>{sidebarDirty=true;$("sidebar-state").textContent="Alterações não salvas";});
  $("sidebar-form").onsubmit=event=>{event.preventDefault();busy($("save-sidebar"),async()=>{
    $("sidebar-html").disabled=true;
    try{const data=await api("/api/homepage",{method:"PUT",body:JSON.stringify({revision:sidebarRevision,html:$("sidebar-html").value})});sidebarRevision=data.revision;$("sidebar-html").value=data.html;sidebarDirty=false;$("sidebar-state").textContent="Barra lateral salva como rascunho. Publique para atualizar a página inicial.";notice("Barra lateral salva. O site não foi alterado.");}
    finally{$("sidebar-html").disabled=false;}
  });};
  $("publish-sidebar").onclick=()=>busy($("publish-sidebar"),async()=>{
    if(sidebarDirty)throw new Error("Salve o rascunho da barra lateral antes de publicar.");
    notice("Publicando barra lateral…");
    await api("/api/homepage/publish",{method:"POST",body:JSON.stringify({revision:sidebarRevision,expected_active:deployment.active})});
    $("sidebar-state").textContent="Barra lateral publicada. Confira a página inicial.";
    await refreshAfterSuccess("Barra lateral publicada. Rascunhos de artigos foram preservados.",loadDeployments);
  });

  function edit() {if(!doc)return;dirty=true;$("dirty").textContent="Alterações não salvas";schedulePreview();}

  for(const id of ["title","html","permalink","published-local","categories"])$(id).addEventListener("input",edit);


  let autoPermalink=true, permalinkTimer, permalinkGeneration=0;
  function titlePermalink(title,kind){
    const slug=title.toLowerCase().normalize("NFD").replace(/[\u0300-\u036f]/g,"").split(/[^a-z0-9]+/).filter(Boolean).join("-").slice(0,200).replace(/-+$/,"")||"sem-titulo";
    return `${kind==="page" ? "/" : "/artigos/"}${slug}/`;
  }
  $("title").addEventListener("input",()=>{
    clearTimeout(permalinkTimer);const generation=++permalinkGeneration;
    if(!doc || !autoPermalink || $("permalink").readOnly)return;
    const id=doc.metadata.id,title=$("title").value;
    $("permalink").value=titlePermalink(title,doc.metadata.kind);
    $("path-hint").textContent="Verificando disponibilidade do endereço…";
    permalinkTimer=setTimeout(async()=>{
      try{
        const result=await api(`/api/articles/${encodeURIComponent(id)}/permalink`,{method:"POST",body:JSON.stringify({title,current:$("permalink").value})});
        if(generation!==permalinkGeneration || doc?.metadata.id!==id || !autoPermalink)return;
        $("permalink").value=result.permalink;$("path-hint").textContent="Endereço disponível. A disponibilidade será verificada novamente ao salvar.";
        schedulePreview();
      }catch(error){if(generation===permalinkGeneration)$("path-hint").textContent=`Não foi possível verificar agora. O endereço será verificado ao salvar. ${error.message}`;}
    },300);
  });
  $("permalink").addEventListener("input",()=>{
    autoPermalink=false;clearTimeout(permalinkTimer);++permalinkGeneration;
    $("path-hint").textContent="Endereço personalizado; será verificado ao salvar.";
  });

  function fields() {return {auto_permalink:autoPermalink&&!$("permalink").readOnly,title:$("title").value,html:$("html").value,permalink:$("permalink").value.trim()||null,published_local:$("published-local").value.length===16 ? $("published-local").value+":00" : $("published-local").value,categories:[...$("categories").selectedOptions].map(o=>o.value),featured_image:null,featured_image_alt:"",featured_image_caption:""};}

  $("editor").onsubmit=event=>{event.preventDefault();busy($("save"),async()=>{notice("Salvando rascunho…");const result=await api(`/api/articles/${encodeURIComponent(doc.metadata.id)}`,{method:"PUT",body:JSON.stringify({...fields(),revision:doc.revision})});setDocument(result);await refreshAfterSuccess("Rascunho salvo. O site não foi alterado.",refreshEntries);});};

  const escape = value => value.replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;").replace(/"/g,"&quot;");
  function editableHtml(data){
    const m=data.metadata, wrapper=new DOMParser().parseFromString(data.html,"text/html");
    // Preserve old featured images as explicit HTML, without inserting duplicates.
    if(m.featured_image && ![...wrapper.querySelectorAll("img")].some(i=>i.getAttribute("src")===m.featured_image)){
      return `<figure><img src="${escape(m.featured_image)}" alt="${escape(m.featured_image_alt||"")}"><figcaption>${escape(m.featured_image_caption||"")}</figcaption></figure>\n${data.html}`;
    }
    return data.html;
  }
  let libraryRevision="", imageOffset=0, libraryImages=[], selectedImage=null, imageGeneration=0, searchTimer;
  function imageCode(){const width=Number($("image-width").value);if(!Number.isInteger(width)||width<1||width>2400)throw new Error("Escolha uma largura inteira entre 1 e 2400 px.");return `<figure style="text-align:center;"><img src="${escape($("image-path").value)}" alt="${escape($("image-alt").value)}" width="${width}" style="max-width:100%;height:auto;"><figcaption>${escape($("image-caption").value)}</figcaption></figure>`;}
  function refreshImageCode(){
    let invalid=false;
    try{$("image-code").value=imageCode();$("image-width").setCustomValidity("");}
    catch(error){invalid=true;$("image-code").value="";$("image-width").setCustomValidity(error.message);}
    $("copy-image-html").disabled=invalid;
    $("insert-image").disabled=invalid||!doc||doc.trashed||$("html").readOnly||selectedImage?.deleted;
  }
  async function loadLibrary(){
    const generation=++imageGeneration;$("image-grid").replaceChildren();$("image-count").textContent="Carregando imagens…";$("images-prev").disabled=$("images-next").disabled=true;
    const query=new URLSearchParams({q:$("image-search").value,offset:String(imageOffset),deleted:String($("image-filter").value==="deleted")});
    const data=await api(`/api/library?${query}`);if(generation!==imageGeneration)return;
    libraryRevision=data.revision;libraryImages=data.images;$("image-grid").replaceChildren();
    $("image-count").textContent=`${data.total} imagem(ns) · ${data.total ? imageOffset+1 : 0}–${Math.min(imageOffset+data.images.length,data.total)}`;
    $("images-prev").disabled=imageOffset===0;$("images-next").disabled=imageOffset+data.limit>=data.total;
    for(const image of data.images){const button=document.createElement("button"),img=document.createElement("img"),label=document.createElement("span");button.type="button";button.className="image-card";img.src="/api/media"+image.path;img.alt=image.alt;img.loading="lazy";label.textContent=image.name||decodeURIComponent(image.path.split("/").pop());button.append(img,label);button.onclick=()=>selectImage(image);$("image-grid").append(button);}
    return data;
  }
  function selectImage(image){delete $("remove-image").dataset.confirmPath;$("image-notice").hidden=true;selectedImage=image;$("library-preview").src="/api/media"+image.path;$("library-preview").alt=image.alt;$("image-path").value=image.path;$("image-alt").value=image.alt;$("image-caption").value=image.caption;$("image-width").value="480";refreshImageCode();$("remove-image").textContent=image.deleted ? "Restaurar na biblioteca" : "Excluir da biblioteca";$("image-dialog").showModal();}
  $("close-image").onclick=()=>$("image-dialog").close();
  for(const id of ["image-alt","image-caption","image-width"])$(id).oninput=refreshImageCode;
  async function copyImage(value){if(!navigator.clipboard){notice("Selecione e copie o campo acima.");return;}await navigator.clipboard.writeText(value);notice("Copiado.");}
  $("copy-image-link").onclick=()=>busy($("copy-image-link"),()=>copyImage($("image-path").value));
  $("copy-image-html").onclick=()=>busy($("copy-image-html"),()=>copyImage(imageCode()));
  $("image-form").onsubmit=event=>{event.preventDefault();busy($("save-image"),async()=>{await api("/api/library",{method:"PUT",body:JSON.stringify({path:selectedImage.path,revision:libraryRevision,alt:$("image-alt").value,caption:$("image-caption").value,deleted:selectedImage.deleted})});$("image-dialog").close();await refreshAfterSuccess("Imagem salva na biblioteca.",loadLibrary);});};
  $("remove-image").onclick=()=>busy($("remove-image"),async()=>{if(!selectedImage.deleted && $("remove-image").dataset.confirmPath!==selectedImage.path){$("remove-image").dataset.confirmPath=selectedImage.path;$("remove-image").textContent="Confirmar exclusão da biblioteca";notice("Clique novamente para excluir da biblioteca. Os arquivos usados em artigos e versões anteriores serão preservados.");return;}await api("/api/library",{method:"PUT",body:JSON.stringify({path:selectedImage.path,revision:libraryRevision,alt:$("image-alt").value,caption:$("image-caption").value,deleted:!selectedImage.deleted})});$("image-dialog").close();await refreshAfterSuccess("Biblioteca atualizada.",loadLibrary);});
  $("insert-image").onclick=()=>{if(!doc||doc.trashed||$("html").readOnly)return;const code="\n"+imageCode()+"\n";const field=$("html");field.setRangeText(code,field.selectionStart,field.selectionEnd,"end");edit();$("image-dialog").close();showPage("/content/",true).catch(e=>notice(e.message,true));notice("Imagem inserida no HTML. Salve o rascunho para manter a alteração.");};
  $("image-search").oninput=()=>{clearTimeout(searchTimer);imageOffset=0;searchTimer=setTimeout(()=>loadLibrary().catch(e=>notice(e.message,true)),300);};
  $("image-filter").onchange=()=>{imageOffset=0;loadLibrary().catch(e=>notice(e.message,true));};
  $("images-prev").onclick=()=>{imageOffset=Math.max(0,imageOffset-48);loadLibrary().catch(e=>notice(e.message,true));};
  $("images-next").onclick=()=>{imageOffset+=48;loadLibrary().catch(e=>notice(e.message,true));};
  $("file").onchange=async()=>{const file=$("file").files[0];if(!file)return;$("file").disabled=true;$("image-search").disabled=true;$("image-filter").disabled=true;$("images-prev").disabled=$("images-next").disabled=true;clearTimeout(searchTimer);let uploaded=false;try{if(file.size>8*1024*1024)throw new Error("Escolha uma imagem de até 8 MB.");notice("Enviando e verificando imagem…");const form=new FormData();form.append("image",file);const result=await api("/api/uploads",{method:"POST",body:form});uploaded=true;clearTimeout(searchTimer);imageOffset=0;$("image-filter").value="active";$("image-search").value="";const refreshed=await loadLibrary();const image=refreshed?.images.find(i=>i.path===result.path);if(image)selectImage(image);notice("Imagem enviada à biblioteca. Edite a descrição e a legenda.");}catch(error){if(!uploaded)await loadLibrary().catch(()=>{});notice(uploaded ? "Imagem enviada, mas a biblioteca não pôde ser atualizada. Reabra a página de imagens para conferir." : error.message,true);}finally{$("file").value="";$("file").disabled=false;$("image-search").disabled=false;$("image-filter").disabled=false;}};

  function schedulePreview() {clearTimeout(previewTimer);previewTimer=setTimeout(renderPreview,650);}

  async function renderPreview() {if(!doc)return;const generation=++previewGeneration;$("preview-status").textContent="Atualizando prévia…";try{const result=await api("/api/preview",{method:"POST",body:JSON.stringify(fields())});if(generation===previewGeneration){$("preview").srcdoc=result.html;$("preview-status").textContent="Prévia isolada · o site não foi alterado";}}catch(error){if(generation===previewGeneration)$("preview-status").textContent=error.message;}}

  $("refresh-preview").onclick=renderPreview;

  $("wide").onclick=()=>{$("preview").classList.remove("phone");$("wide").setAttribute("aria-pressed","true");$("narrow").setAttribute("aria-pressed","false");};

  $("narrow").onclick=()=>{$("preview").classList.add("phone");$("narrow").setAttribute("aria-pressed","true");$("wide").setAttribute("aria-pressed","false");};

  window.addEventListener("beforeunload",event=>{if(dirty||sidebarDirty){event.preventDefault();event.returnValue="";}});

  (async()=>{const fragment=new URLSearchParams(location.hash.slice(1));bootstrap=fragment.get("setup")||"";if(location.hash)history.replaceState(null,"",location.pathname);const status=await api("/api/status");identity(status);$("setup").hidden=status.initialized;$("signin").hidden=!status.initialized;if(bootstrap)$("bootstrap").value=bootstrap;try{await openApp();}catch(_){loginView();if(!status.initialized)notice("Cadastre sua primeira passkey para abrir o ambiente editorial.");else notice("");}})().catch(error=>notice(error.message,true));

})();
