const {test}=require('node:test'),assert=require('node:assert/strict'),vm=require('node:vm'),fs=require('node:fs');
const source=fs.readFileSync(require('node:path').join(__dirname,'../admin/admin.js'),'utf8');
const make=()=>({children:[],textContent:'',hidden:true,attrs:{},append(...items){this.children.push(...items);},replaceChildren(...items){this.children=items;},setAttribute(k,v){this.attrs[k]=v;},set innerHTML(_){throw Error('Unsafe HTML rendering');}});
test('image menu resets pagination, retains search and requests the correct trash view',async()=>{
 const nodes={};const get=id=>nodes[id]??=make();get('image-search').value='cat';
 const buttons=['active','deleted'].map(section=>({...make(),dataset:{imageSection:section}}));let url;
 const ctx=vm.createContext({$:get,document:{querySelectorAll:()=>buttons,createElement:make},imageSection:'active',imageOffset:96,imageGeneration:0,URLSearchParams,api:async path=>{url=path;return {revision:'fresh',images:[],total:0,limit:48};}});
 vm.runInContext(source.slice(source.indexOf('  function setImageSection('),source.indexOf('  for(const button of document.querySelectorAll("[data-image-section]"))button.onclick')),ctx);
 vm.runInContext(source.slice(source.indexOf('  async function loadLibrary('),source.indexOf('  function selectImage(')),ctx);
 ctx.setImageSection('deleted');await ctx.loadLibrary();assert.equal(ctx.imageOffset,0);assert.equal(buttons[1].attrs['aria-pressed'],'true');
 assert.match(url,/deleted=true/);assert.match(url,/q=cat/);
 ctx.setImageSection('active');await ctx.loadLibrary();assert.match(url,/deleted=false/);assert.equal(buttons[0].attrs['aria-pressed'],'true');
});
test('blocked deletion names articles/pages and contexts as text and clears stale references',()=>{
 const nodes={};const get=id=>nodes[id]??=make();const ctx=vm.createContext({$:get,document:{createElement:make}});
 vm.runInContext(source.slice(source.indexOf('  function showImageReferences('),source.indexOf('  let purgeTarget=')),ctx);
 ctx.showImageReferences([{kind:'post',title:'<img onerror=evil()>',permalink:'/artigos/exemplo/',contexts:['Rascunho salvo','Publicação atual']},{kind:'page',title:'Página antiga',contexts:['Histórico editorial']}]);
 assert.equal(get('purge-image-usage').hidden,false);const rows=get('purge-image-references').children;
 assert.equal(rows[0].children[0].textContent,'📝 Artigo: <img onerror=evil()>');assert.match(rows[0].children[1].textContent,/Rascunho salvo, Publicação atual/);
 assert.equal(rows[1].children[0].textContent,'📄 Página: Página antiga');ctx.showImageReferences([]);assert.equal(get('purge-image-usage').hidden,true);assert.equal(get('purge-image-references').children.length,0);
});
test('API failures preserve structured image reference details',async()=>{
 const refs=[{title:'An article'}];const ctx=vm.createContext({csrf:'token',FormData:class{},fetch:async()=>({ok:false,status:409,json:async()=>({error:'In use',references:refs})}),Error});
 vm.runInContext(source.slice(source.indexOf('  async function api('),source.indexOf('  function busy(')),ctx);
 await assert.rejects(ctx.api('/api/library/purge'),error=>error.message==='In use'&&error.references===refs);
});
const html=fs.readFileSync(require('node:path').join(__dirname,'../admin/index.html'),'utf8');
test('permanent deletion dialogs use yes/no without typed confirmations',()=>{
 assert.doesNotMatch(html,/id="(?:empty-trash|purge-image)-confirmation"/);
 for(const id of ['cancel-purge-image','cancel-empty-trash'])assert.match(html,new RegExp(`id="${id}"[^>]*>Não, cancelar`));
 for(const id of ['confirm-purge-image','confirm-empty-trash'])assert.match(html,new RegExp(`id="${id}"[^>]*>Sim,`));
});
test('image deletion waits for Yes, No only closes and history retention is explained',async()=>{
 const nodes={};const get=id=>nodes[id]??={...make(),disabled:false,showModal(){this.open=true;},close(){this.open=false;}};let calls=0,message;
 const ctx=vm.createContext({$:get,selectedImage:{path:'/uploads/test.png',name:'Test'},libraryRevision:'revision',imageOffset:0,showImageReferences(){},api:async(_,options)=>{calls++;assert.equal(JSON.parse(options.body).confirmation,'EXCLUIR IMAGEM');return {retained_for_history:true};},refreshAfterSuccess:async text=>{message=text;},loadLibrary(){}});
 vm.runInContext(source.slice(source.indexOf('  let purgeTarget='),source.indexOf('  $("insert-image").onclick')),ctx);
 get('purge-image').onclick();assert.equal(calls,0);assert.equal(get('confirm-purge-image').disabled,false);
 get('cancel-purge-image').onclick();assert.equal(calls,0);assert.equal(get('purge-image-dialog').open,false);
 get('purge-image').onclick();await get('purge-image-form').onsubmit({preventDefault(){}});
 assert.equal(calls,1);assert.match(message,/históricas/);assert.equal(get('purge-image-dialog').open,false);
});
test('article trash No leaves content intact and Yes submits the reviewed revision',async()=>{
 const nodes={};const get=id=>nodes[id]??={...make(),disabled:false,showModal(){this.open=true;},close(){this.open=false;}};let writes=0;
 const ctx=vm.createContext({$:get,dirty:false,doc:null,document:{createElement:make},busy:async(_,fn)=>fn(),api:async(_,options)=>{if(!options)return {revision:'reviewed',items:[{title:'Test'}]};writes++;assert.equal(JSON.parse(options.body).revision,'reviewed');assert.equal(JSON.parse(options.body).confirmation,'ESVAZIAR LIXEIRA');return {deleted:1};},refreshEntries(){},refreshAfterSuccess:async()=>{},notice(){}});
 vm.runInContext(source.slice(source.indexOf('  let trashRevision='),source.indexOf('  function refreshCategoryFilter(')),ctx);
 await get('empty-trash').onclick();get('cancel-empty-trash').onclick();assert.equal(writes,0);
 await get('empty-trash').onclick();await get('empty-trash-form').onsubmit({preventDefault(){}});assert.equal(writes,1);assert.equal(get('empty-trash-dialog').open,false);
});
