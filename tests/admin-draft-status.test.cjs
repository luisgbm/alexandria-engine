const {test}=require('node:test'),assert=require('node:assert/strict'),vm=require('node:vm'),fs=require('node:fs');
const source=fs.readFileSync(require('node:path').join(__dirname,'../admin/admin.js'),'utf8');
test('draft filter excludes identical published revisions and distinguishes pending edits',()=>{
 const make=()=>({value:'',children:[],classList:{toggle(){}},append(...items){this.children.push(...items);},replaceChildren(){this.children=[];}});
 const nodes={};const get=id=>nodes[id]??=make();get('status-filter').value='draft';
 const entries=[{id:'live',title:'Unchanged',status:'published',has_saved_draft:false},{id:'edited',title:'Edited',status:'published',has_saved_draft:true},{id:'new',title:'New draft',status:'draft',has_saved_draft:true}].map(e=>({...e,kind:'post',trashed:false,date:'2026-10-23'}));
 const ctx=vm.createContext({$:get,entries,doc:null,document:{createElement:make}});
 vm.runInContext(source.slice(source.indexOf('  const normalize ='),source.indexOf('  $("filter").oninput')),ctx);vm.runInContext('renderEntries()',ctx);
 assert.equal(get('list-count').textContent,'2 item(ns)');assert.deepEqual(get('articles').children.map(li=>li.children[0].textContent).sort(),['Edited','New draft']);
 get('status-filter').value='published';vm.runInContext('renderEntries()',ctx);
 const labels=get('articles').children.map(li=>li.children[0].children[0].textContent);
 assert.ok(labels.includes('Artigo · Publicado · 2026-10-23'));assert.ok(labels.includes('Artigo · Publicado · alterações em rascunho · 2026-10-23'));
});
