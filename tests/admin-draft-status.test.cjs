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
