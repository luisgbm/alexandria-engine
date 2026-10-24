const {test}=require('node:test'),assert=require('node:assert/strict'),vm=require('node:vm'),fs=require('node:fs');
const source=fs.readFileSync(require('node:path').join(__dirname,'../admin/admin.js'),'utf8');
test('draft filter excludes identical published revisions and distinguishes pending edits',()=>{
 const make=()=>({value:'',children:[],classList:{toggle(){}},append(...items){this.children.push(...items);},replaceChildren(){this.children=[];}});
 const nodes={};const get=id=>nodes[id]??=make();get('status-filter').value='draft';
 const entries=[{id:'live',title:'Unchanged',status:'published',has_saved_draft:false},{id:'edited',title:'Edited',status:'published',has_saved_draft:true},{id:'new',title:'New draft',status:'draft',has_saved_draft:true}].map(e=>({...e,kind:'post',trashed:false,date:'2026-10-23'}));
 const ctx=vm.createContext({$:get,entries,doc:null,contentSection:"post",document:{createElement:make}});
 vm.runInContext(source.slice(source.indexOf('  const normalize ='),source.indexOf('  $("filter").oninput')),ctx);vm.runInContext('renderEntries()',ctx);
 assert.equal(get('list-count').textContent,'2 item(ns)');assert.deepEqual(get('articles').children.map(li=>li.children[0].textContent).sort(),['Edited','New draft']);
 get('status-filter').value='published';vm.runInContext('renderEntries()',ctx);
 const labels=get('articles').children.map(li=>li.children[0].children[0].textContent);
 assert.ok(labels.includes('Artigo · Publicado · 2026-10-23'));assert.ok(labels.includes('Artigo · Publicado · alterações em rascunho · 2026-10-23'));
});
test('content menu separates pages and articles and exposes trash across both kinds',()=>{
 const make=()=>({value:'',children:[],classList:{toggle(){}},append(...items){this.children.push(...items);},replaceChildren(){this.children=[];}}),nodes={};const get=id=>nodes[id]??=make();get('status-filter').value='all';
 const entries=[{id:'post',kind:'post',trashed:false},{id:'page',kind:'page',trashed:false},{id:'trash-page',kind:'page',trashed:true}].map(e=>({...e,title:e.id,date:'2026-10-23',status:'published',has_saved_draft:false}));
 const ctx=vm.createContext({$:get,entries,doc:null,contentSection:'post',document:{createElement:make}});
 vm.runInContext(source.slice(source.indexOf('  const normalize ='),source.indexOf('  $("filter").oninput')),ctx);
 const visible=()=>get('articles').children.map(li=>li.children[0].textContent);
 vm.runInContext('renderEntries()',ctx);assert.deepEqual(visible(),['post']);
 ctx.contentSection='page';vm.runInContext('renderEntries()',ctx);assert.deepEqual(visible(),['page']);
 ctx.contentSection='trash';vm.runInContext('renderEntries()',ctx);assert.deepEqual(visible(),['trash-page']);assert.equal(get('empty-trash').hidden,false);
});
test('switching content sections protects unsaved edits and clears a previous editor',()=>{
 const nodes={},get=id=>nodes[id]??={hidden:false,value:'all'};let allowed=false,rendered=0;
 const ctx=vm.createContext({$:get,contentSection:'post',doc:{metadata:{id:'post'}},dirty:true,previewTimer:null,previewGeneration:0,clearTimeout(){},allowSwitch:()=>allowed,renderEntries(){rendered++;},document:{querySelectorAll:()=>[]}});
 vm.runInContext(source.slice(source.indexOf('  function selectContentSection('),source.indexOf('  function allowSwitch()')),ctx);
 ctx.selectContentSection('page');assert.equal(ctx.contentSection,'post');assert.equal(ctx.dirty,true);
 allowed=true;ctx.selectContentSection('page');assert.equal(ctx.contentSection,'page');assert.equal(ctx.doc,null);assert.equal(ctx.dirty,false);assert.equal(get('editor').hidden,true);assert.equal(get('new-page').hidden,false);assert.equal(rendered,1);
});
test('category filter combines with title and state and does not filter pages or trash',()=>{
 const make=()=>({value:'',children:[],classList:{toggle(){}},append(...items){this.children.push(...items);},replaceChildren(){this.children=[];}}),nodes={};const get=id=>nodes[id]??=make();get('status-filter').value='all';get('category-filter').value='filosofia';
 const entries=[{id:'one',title:'Texto um',categories:['artigos','filosofia'],status:'published'},{id:'two',title:'Texto dois',categories:['artigos'],status:'published'},{id:'draft',title:'Texto rascunho',categories:['filosofia'],status:'draft',has_saved_draft:true},{id:'page',title:'Página',kind:'page',categories:[],status:'published'},{id:'trash',title:'Lixeira',trashed:true,categories:[],status:'draft'}].map(e=>({kind:'post',trashed:false,date:'2026-10-23',has_saved_draft:false,...e}));
 const ctx=vm.createContext({$:get,entries,doc:null,contentSection:'post',document:{createElement:make}});
 vm.runInContext(source.slice(source.indexOf('  const normalize ='),source.indexOf('  $("filter").oninput')),ctx);
 const visible=()=>get('articles').children.map(li=>li.children[0].textContent).sort();
 vm.runInContext('renderEntries()',ctx);assert.deepEqual(visible(),['Texto rascunho','Texto um']);
 get('status-filter').value='published';get('filter').value='um';vm.runInContext('renderEntries()',ctx);assert.deepEqual(visible(),['Texto um']);
 get('filter').value='';ctx.contentSection='page';vm.runInContext('renderEntries()',ctx);assert.deepEqual(visible(),['Página']);
 ctx.contentSection='trash';vm.runInContext('renderEntries()',ctx);assert.deepEqual(visible(),['Lixeira']);
 get('status-filter').value='all';get('category-filter').value='';ctx.contentSection='post';vm.runInContext('renderEntries()',ctx);assert.equal(visible().length,3);
});
test('category choices refresh names while retaining selection and reset if removed',()=>{
 const filter={value:'filosofia',children:[],replaceChildren(...items){this.children=items;},append(item){this.children.push(item);}};
 const ctx=vm.createContext({$:()=>filter,categories:[{slug:'filosofia',name:'Filosofia'}],document:{createElement:()=>({})},renderEntries(){}});
 vm.runInContext(source.slice(source.indexOf('  function refreshCategoryFilter('),source.indexOf('  async function loadCategories()')),ctx);
 ctx.refreshCategoryFilter();assert.equal(filter.value,'filosofia');assert.deepEqual(filter.children.map(o=>o.textContent),['Todas as categorias','Filosofia']);
 ctx.categories=[{slug:'filosofia',name:'Novo nome'}];ctx.refreshCategoryFilter();assert.equal(filter.value,'filosofia');assert.equal(filter.children[1].textContent,'Novo nome');
 ctx.categories=[];ctx.refreshCategoryFilter();assert.equal(filter.value,'');
});
