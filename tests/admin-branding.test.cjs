const {test}=require('node:test'),assert=require('node:assert/strict'),vm=require('node:vm'),fs=require('node:fs');
const source=fs.readFileSync(require('node:path').join(__dirname,'../admin/admin.js'),'utf8');
test('login and signed-in navigation use configured publication title as text',()=>{
 const nodes={};const get=id=>nodes[id]??={textContent:''};
 const document={title:'',querySelector:()=>({textContent:'Segurança'})};
 const ctx=vm.createContext({$:get,document,siteTitle:'Alexandria',environment:'staging',environmentLabel:''});
 vm.runInContext(source.slice(source.indexOf('  function updateAdminTitle('),source.indexOf('  async function showPage(')),ctx);
 ctx.identity({site_title:'Arquivo & Memória',environment:'staging',environment_label:'Testes'});
 assert.equal(get('site-name').textContent,'Arquivo & Memória');assert.equal(document.title,'Segurança · Arquivo & Memória');
 ctx.identity({site:{title:'Outro <arquivo>'},environment:'production',environment_label:'Produção'});
 assert.equal(get('site-name').textContent,'Outro <arquivo>');assert.equal(document.title,'Segurança · Outro <arquivo>');
});
