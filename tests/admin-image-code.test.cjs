const {test} = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const source = fs.readFileSync(require('node:path').join(__dirname,'../admin/admin.js'),'utf8');
test('image HTML defaults to a centered figure and escapes image metadata',()=>{
  const escape=source.split('\n').find(line=>line.startsWith('  const escape ='));
  const code=source.split('\n').find(line=>line.startsWith('  function imageCode()'));
  const values={'image-path':'/uploads/a.png','image-alt':'A "photo"','image-caption':'Caption <text>&'};
  const context=vm.createContext({$:id=>({value:values[id]})});
  vm.runInContext(escape+'\n'+code+'\nresult=imageCode();',context);
  assert.equal(context.result,'<figure style="text-align:center;"><img src="/uploads/a.png" alt="A &quot;photo&quot;"><figcaption>Caption &lt;text&gt;&amp;</figcaption></figure>');
});
