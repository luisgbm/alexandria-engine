const {test} = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const source = fs.readFileSync(require('node:path').join(__dirname,'../admin/admin.js'),'utf8');
function dialog(width='480'){
  const escape=source.split('\n').find(line=>line.startsWith('  const escape ='));
  const code=source.split('\n').find(line=>line.startsWith('  function imageCode()'));
  const refresh=source.slice(source.indexOf('  function refreshImageCode()'),source.indexOf('  async function loadLibrary()'));
  const values={'image-path':'/uploads/a.png','image-alt':'A "photo"','image-caption':'Caption <text>&','image-width':width,'image-code':''};
  const nodes=Object.fromEntries(Object.entries(values).map(([id,value])=>[id,{value,setCustomValidity(text){this.validation=text;}}]));
  for(const id of ['copy-image-html','insert-image','html'])nodes[id]={disabled:false,readOnly:false};
  const context=vm.createContext({$:id=>nodes[id],doc:{trashed:false},selectedImage:{deleted:false}});
  vm.runInContext(escape+'\n'+code+'\n'+refresh,context);
  return {nodes,context,snippet:()=>vm.runInContext('imageCode()',context),refresh:()=>vm.runInContext('refreshImageCode()',context)};
}
test('default image HTML is centered, modest, responsive and escapes metadata',()=>{
  assert.match(source,/\$\("image-width"\)\.value="480"/);
  assert.match(fs.readFileSync(require('node:path').join(__dirname,'../admin/index.html'),'utf8'),/id="image-width"[^>]*value="480"/);
  assert.equal(dialog().snippet(),'<figure style="text-align:center;"><img src="/uploads/a.png" alt="A &quot;photo&quot;" width="480" style="max-width:100%;height:auto;"><figcaption>Caption &lt;text&gt;&amp;</figcaption></figure>');
});
test('custom widths update HTML and invalid input disables copying and insertion',()=>{
  const d=dialog('640');d.refresh();assert.match(d.nodes['image-code'].value,/width="640"/);assert.equal(d.nodes['insert-image'].disabled,false);
  for(const width of ['', '0', '-1', '2.5', '2401', 'Infinity', '320" onerror="evil']){
    d.nodes['image-width'].value=width;assert.throws(d.snippet,/largura inteira/);d.refresh();
    assert.equal(d.nodes['copy-image-html'].disabled,true);assert.equal(d.nodes['insert-image'].disabled,true);assert.equal(d.nodes['image-code'].value,'');
  }
  d.nodes['image-width'].value='240';d.refresh();assert.equal(d.nodes['copy-image-html'].disabled,false);assert.equal(d.nodes['image-width'].validation,'');assert.match(d.nodes['image-code'].value,/width="240"/);
  d.context.doc.trashed=true;d.refresh();assert.equal(d.nodes['insert-image'].disabled,true);
});
