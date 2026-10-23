const {test}=require('node:test');const assert=require('node:assert/strict');const vm=require('node:vm');const fs=require('node:fs');
const source=fs.readFileSync(require('node:path').join(__dirname,'../admin/admin.js'),'utf8');
test('publishing modal remains open until resolution and closes on success or failure',async()=>{
  let resolve;const modal={open:false,showModal(){this.open=true;},close(){this.open=false;}};
  const nodes={'publish-progress':modal,notice:{hidden:false},'publish-progress-message':{}};
  const ctx=vm.createContext({$:id=>nodes[id],noticeTimer:1,clearTimeout(){},Error});
  vm.runInContext(source.slice(source.indexOf('  async function withPublishProgress('),source.indexOf('  $("publish-progress").addEventListener')),ctx);
  const pending=new Promise(r=>resolve=r);ctx.action=()=>pending;
  const running=vm.runInContext('withPublishProgress("Building",action)',ctx);assert.equal(modal.open,true);assert.equal(nodes.notice.hidden,true);
  assert.equal(nodes['publish-progress-message'].textContent,'Building');resolve('complete');assert.equal(await running,'complete');assert.equal(modal.open,false);
  ctx.action=async()=>{throw Error('Validation failed');};await assert.rejects(vm.runInContext('withPublishProgress("Building",action)',ctx),/Validation failed/);assert.equal(modal.open,false);
});
test('mobile defaults to seven days, desktop to thirty; menu selection state and resizing preserve user-selected period',()=>{
  for(const mobile of [true,false]){
    const nodes={};const get=id=>nodes[id]??={value:'',attrs:{},classList:{toggle(k,v){this[k]=v;}},setAttribute(k,v){this.attrs[k]=v;},getAttribute(k){return this.attrs[k];}};
    const events={},media=[];const ctx=vm.createContext({$:get,document:{addEventListener(k,fn){events[k]=fn;}},window:{matchMedia(q){return {matches:q.includes('max-width')&&mobile,addEventListener(k,fn){media.push(fn);}};}}});
    vm.runInContext(source.slice(source.indexOf('  function setAdminMenu('),source.indexOf('  function decode(value)')),ctx);
    assert.equal(get('analytics-period').value,mobile?'week':'month');get('admin-menu-toggle').onclick();assert.equal(get('admin-menu-toggle').attrs['aria-expanded'],'true');
    events.keydown({key:'Escape'});assert.equal(get('admin-menu-toggle').attrs['aria-expanded'],'false');
    get('analytics-period').value='year';media[0]();assert.equal(get('analytics-period').value,'year');
  }
});
