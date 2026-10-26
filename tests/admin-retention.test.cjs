const {test}=require('node:test'),assert=require('node:assert/strict'),vm=require('node:vm'),fs=require('node:fs');
const source=fs.readFileSync(require('node:path').join(__dirname,'../admin/admin.js'),'utf8');
test('publications show local disk usage and never offer archived rollback targets',async()=>{
 const nodes={};const make=()=>({children:[],textContent:'',replaceChildren(){this.children=[];},append(...items){this.children.push(...items);}});const get=id=>nodes[id]??=make();
 const data={enabled:true,active:'current',public_url:'https://stage.example.org',events:[],storage:{retained_count:2,archived_count:1,release_bytes:1024**3,snapshot_bytes:1024**3,total_bytes:2*1024**3,keep_previous:5},releases:[{id:'current',created_at:1,retained:true,release_bytes:1024**3,snapshot_bytes:0},{id:'previous',created_at:1,retained:true},{id:'archived',created_at:1,retained:false}]};
 const ctx=vm.createContext({$:get,document:{createElement:make,createTextNode:text=>({textContent:text})},api:async()=>data});
 vm.runInContext(source.slice(source.indexOf('  function storageSize('),source.indexOf('  $("publish").onclick')),ctx);
 await ctx.loadDeployments();assert.equal(get('release-list').children.length,2);assert.equal(get('release-list').children[1].children[0].textContent,'Restaurar esta versão');
 assert.match(get('publication-storage').textContent,/2 versões locais/);assert.match(get('publication-storage').textContent,/Total: 2 GiB/);assert.match(get('retention-policy').textContent,/1 registros antigos/);
 assert.ok(get('release-list').children.every(item=>!item.textContent.includes('archived')));
});
