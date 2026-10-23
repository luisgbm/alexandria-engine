const {test} = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const source = fs.readFileSync(require('node:path').join(__dirname, '../admin/admin.js'), 'utf8');
const handler = source.slice(source.indexOf('  $("insert-more").onclick='), source.indexOf('  async function loadHomepage'));

test('More button inserts at the cursor, marks the draft dirty and focuses the HTML editor', () => {
  const button = {}, field = {value:'<p>Before</p><p>After</p>', selectionStart:13, selectionEnd:13,
    focus(){this.focused=true;}, setRangeText(text,start,end,mode){
      assert.equal(mode,'end'); this.value=this.value.slice(0,start)+text+this.value.slice(end);
      this.selectionStart=this.selectionEnd=start+text.length;
    }};
  let edited=0;
  const context=vm.createContext({$:id=>id==='html'?field:button,doc:{trashed:false},edit(){edited++;},notice(){}});
  vm.runInContext(handler,context);button.onclick();
  assert.equal(field.value,'<p>Before</p><!--more--><p>After</p>');
  assert.equal(field.selectionStart,24);assert.equal(edited,1);assert.equal(field.focused,true);
  field.readOnly=true;button.onclick();assert.equal(edited,1);
  field.readOnly=false;context.doc.trashed=true;button.onclick();assert.equal(edited,1);
});
