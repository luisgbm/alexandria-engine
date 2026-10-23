const {test}=require('node:test');
const assert=require('node:assert/strict');
const vm=require('node:vm');
const fs=require('node:fs');
const source=fs.readFileSync(require('node:path').join(__dirname,'../templates/menu.js'),'utf8');
function menu(){
  function node(){const classes=new Set();return {hidden:true,attrs:{'aria-expanded':'false'},listeners:{},
    classList:{add:value=>classes.add(value),toggle(value,on){if(on)classes.add(value);else classes.delete(value);},contains:value=>classes.has(value)},
    setAttribute(key,value){this.attrs[key]=value;},getAttribute(key){return this.attrs[key];},addEventListener(event,fn){this.listeners[event]=fn;},contains(other){return this===other;},focus(){this.focused=true;}};}
  const toggle=node(),nav=node(),header=node(),document=node();toggle.closest=()=>header;
  const mobile={matches:true,listeners:{},addEventListener(event,fn){this.listeners[event]=fn;}};
  document.querySelector=id=>id==='#menu-toggle'?toggle:nav;
  vm.runInNewContext(source,{document,matchMedia:()=>mobile});
  return {toggle,nav,header,document,mobile};
}
test('mobile toggle opens and closes with correct accessibility state',()=>{
  const m=menu();assert.equal(m.toggle.hidden,false);assert(m.header.classList.contains('menu-ready'));
  m.toggle.listeners.click();assert.equal(m.toggle.attrs['aria-expanded'],'true');assert.equal(m.toggle.attrs['aria-label'],'Fechar menu');assert(m.nav.classList.contains('is-open'));
  m.document.listeners.click({target:m.toggle});assert(m.nav.classList.contains('is-open'));
  m.toggle.listeners.click();assert.equal(m.toggle.attrs['aria-expanded'],'false');assert(!m.nav.classList.contains('is-open'));
});
test('Escape, outside clicks, link navigation and breakpoint changes close the menu',()=>{
  const m=menu();m.toggle.listeners.click();m.document.listeners.keydown({key:'Escape'});
  assert.equal(m.toggle.attrs['aria-expanded'],'false');assert.equal(m.toggle.focused,true);
  m.toggle.listeners.click();m.document.listeners.click({target:{}});assert(!m.nav.classList.contains('is-open'));
  m.toggle.listeners.click();m.nav.listeners.click({target:{closest:()=>({tagName:'A'})}});assert(!m.nav.classList.contains('is-open'));
  m.toggle.listeners.click();m.mobile.matches=false;m.mobile.listeners.change();assert(!m.nav.classList.contains('is-open'));
});
