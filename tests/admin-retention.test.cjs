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
test('publication deletion uses Yes/No, captures expected active version, and prevents duplicate requests',async()=>{
 const nodes={};const get=id=>nodes[id]??={disabled:false,showModal(){this.open=true;},close(){this.open=false;}};let calls=0,resolveApi,refreshed=0,payload;
 const ctx=vm.createContext({$:get,dirty:false,deployment:{active:'shown'},notice(){},withPublishProgress:async(_,action)=>action(),api:async(path,options)=>{calls++;assert.equal(path,'/api/deployments/delete');payload=JSON.parse(options.body);return new Promise(resolve=>resolveApi=resolve);},refreshAfterSuccess:async()=>{refreshed++;},loadDeployments(){}});
 vm.runInContext(source.slice(source.indexOf('  let publicationDeleteTarget='),source.indexOf('  $("publish").onclick')),ctx);
 ctx.openPublicationDelete({id:'older'});get('cancel-delete-publication').onclick();assert.equal(calls,0);assert.equal(get('delete-publication-dialog').open,false);
 ctx.openPublicationDelete({id:'older'});ctx.deployment.active='changed';
 const pending=get('delete-publication-form').onsubmit({preventDefault(){}});await get('delete-publication-form').onsubmit({preventDefault(){}});
 assert.equal(calls,1);assert.deepEqual(payload,{release:'older',expected_active:'shown'});resolveApi({});await pending;
 assert.equal(refreshed,1);assert.equal(get('delete-publication-dialog').open,false);
 ctx.openPublicationDelete({id:'older'});ctx.api=async()=>{throw Error('Backup required');};await get('delete-publication-form').onsubmit({preventDefault(){}});
 assert.equal(get('delete-publication-dialog').open,true);assert.equal(get('delete-publication-status').textContent,'Backup required');assert.equal(get('confirm-delete-publication').disabled,false);
});
