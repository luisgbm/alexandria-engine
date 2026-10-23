const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const source=fs.readFileSync(require('node:path').join(__dirname,'../admin/admin.js'),'utf8');
function ui(initial) {
  const nodes={};
  const node=()=>({disabled:false,textContent:'',children:[],listeners:{},append(...items){this.children.push(...items);},replaceChildren(){this.children=[];},setAttribute(k,v){this[k]=v;},addEventListener(k,fn){this.listeners[k]=fn;},showModal(){this.open=true;},close(){this.open=false;this.listeners.close?.();}});
  const get=id=>nodes[id]??=node();
  const calls=[];let keys=initial;let failure=null;let message='';
  const context=vm.createContext({$:get,document:{createElement:node},Date,encodeURIComponent,
    api:async(path,options)=>{calls.push({path,options});if(options){if(failure)throw Error(failure);keys=keys.filter(k=>!path.includes(k.id));return {ok:true};}return {credentials:keys};},
    refreshAfterSuccess:async(text,refresh)=>{message=text;await refresh();}
  });
  vm.runInContext(source.slice(source.indexOf('  let keyToRemove ='),source.indexOf('  document.addEventListener("click",event=>',source.indexOf('  let keyToRemove ='))),context);
  return {nodes,calls,load:()=>vm.runInContext('loadPasskeys()',context),fail:text=>failure=text,submit:()=>get('remove-key-form').onsubmit({preventDefault(){}}),message:()=>message};
}
const key=(id,name)=>({id,name,created_at:1700000000});
test('last-key protection is visible and opening/cancelling confirmation never removes a key',async()=>{
  const u=ui([key('aaa','Only key')]);await u.load();assert.equal(u.nodes['key-list'].children[0].children[1].disabled,true);
  const v=ui([key('aaa','<Device A>'),key('bbb','Device B')]);await v.load();
  v.nodes['key-list'].children[0].children[1].onclick();
  assert.equal(v.nodes['remove-key-name'].textContent,'<Device A>');assert.equal(v.nodes['remove-key-dialog'].open,true);
  assert.equal(v.calls.filter(c=>c.options).length,0);
  v.nodes['cancel-remove-key'].onclick();await v.submit();assert.equal(v.calls.filter(c=>c.options).length,0);
});
test('confirmation removes only selected key, refreshes last-key guard and shows server errors inline',async()=>{
  const u=ui([key('aaa','Device A'),key('bbb','Device B')]);await u.load();
  u.nodes['key-list'].children[0].children[1].onclick();u.fail('Entre novamente antes de remover uma passkey.');await u.submit();
  assert.match(u.nodes['remove-key-status'].textContent,/Entre novamente/);assert.equal(u.nodes['remove-key-dialog'].open,true);
  assert.equal(u.nodes['confirm-remove-key'].disabled,false);
  u.fail(null);await u.submit();assert.equal(u.nodes['remove-key-dialog'].open,false);
  const sent=u.calls.filter(c=>c.options).at(-1);assert.equal(sent.path,'/api/credentials/aaa/remove');
  assert.deepEqual(JSON.parse(sent.options.body),{confirmation:'REMOVER PASSKEY'});
  assert.equal(u.nodes['key-list'].children.length,1);assert.equal(u.nodes['key-list'].children[0].children[1].disabled,true);
  assert.match(u.message(),/Passkey removida/);
});
