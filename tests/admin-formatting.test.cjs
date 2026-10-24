const {test}=require('node:test'),assert=require('node:assert/strict'),vm=require('node:vm'),fs=require('node:fs');
const source=fs.readFileSync(require('node:path').join(__dirname,'../admin/admin.js'),'utf8');
const code=source.slice(source.indexOf('  function formatHtml('),source.indexOf('  for(const button of document.querySelectorAll("[data-html-format]"))button.onclick'));
function field(value,start=0,end=value.length){return {value,selectionStart:start,selectionEnd:end,focus(){},setRangeText(text,a,b){this.value=this.value.slice(0,a)+text+this.value.slice(b);}};}
test('formatting wraps selection, preserves surrounding HTML and builds list items',()=>{
 const ctx=vm.createContext({});vm.runInContext(code,ctx);
 for(const tag of ['p','strong','em','u']){const f=field('before text after',7,11);ctx.formatHtml(f,tag);assert.equal(f.value,`before <${tag}>text</${tag}> after`);}
 const f=field('One\nTwo');ctx.formatHtml(f,'ul');assert.equal(f.value,'<ul>\n  <li>One</li>\n  <li>Two</li>\n</ul>');
 const ordered=field('One');ctx.formatHtml(ordered,'ol');assert.match(ordered.value,/<ol>/);
});
test('links escape attributes, reject active URLs and cancel without editing',()=>{
 const ctx=vm.createContext({});vm.runInContext(code,ctx);
 const f=field('Link');ctx.formatHtml(f,'a','https://example.com/?a=1&b="two"');assert.equal(f.value,'<a href="https://example.com/?a=1&amp;b=&quot;two&quot;">Link</a>');
 for(const url of ['javascript:alert(1)','//evil.example','data:text/html,test'])assert.throws(()=>ctx.formatHtml(field('Link'),'a',url));
 const cancelled=field('Keep');assert.equal(ctx.formatHtml(cancelled,'a',null),false);assert.equal(cancelled.value,'Keep');
});
