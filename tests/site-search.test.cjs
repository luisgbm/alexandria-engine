const {test} = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const source = fs.readFileSync(require('node:path').join(__dirname, '../templates/search.js'), 'utf8');
class Node {
  constructor(tag='div') { this.tag=tag;this.children=[];this.listeners={};this.value='';this.hidden=false;this.text=''; }
  addEventListener(event,fn){this.listeners[event]=fn;}
  append(...children){this.children.push(...children);}
  replaceChildren(...children){this.children=children;this.text='';}
  set textContent(text){this.text=text;this.children=[];}
  get textContent(){return this.text+this.children.map(n=>typeof n==='string'?n:n.textContent).join('');}
  focus(){this.focused=true;}
}
const documents=[{url:'/artigos/espirito/',title:'Espírito e ciência',text:'Estudo do espírito e da ciência.'}];
const index={espirito:{0:6},ciencia:{0:4}};
function page(href='https://example.test/artigos/original/?utm=keep#ref',deferred=false){
  let url=new URL(href);const nodes={};
  for(const id of ['search-form','query','search-results','search-status','search-panel','page-content','close-search'])nodes[id]=new Node();
  nodes['search-panel'].hidden=true;
  const pending=[],calls=[];
  const location={};for(const key of ['href','pathname','search','hash'])Object.defineProperty(location,key,{get:()=>url[key]});
  const context=vm.createContext({URL,URLSearchParams,location,
    history:{replaceState(_state,_title,path){url=new URL(path,url);}},
    document:{querySelector:id=>nodes[id.slice(1)],createElement:tag=>new Node(tag),createTextNode:text=>{const n=new Node('text');n.textContent=text;return n;}},
    fetch:path=>{calls.push(path);const response={ok:true,json:async()=>path.includes('documents')?documents:index};return deferred?new Promise(resolve=>pending.push(()=>resolve(response))):Promise.resolve(response);}
  });
  vm.runInContext(source,context);
  return {nodes,calls,pending,url:()=>url,submit(value){nodes.query.value=value;nodes['search-form'].listeners.submit({preventDefault(){}});}};
}
const settled=()=>new Promise(resolve=>setImmediate(resolve));

test('header search stays on the article, normalizes accents and loads the index only when needed',async()=>{
  const p=page();assert.equal(p.calls.length,0);
  p.submit('espírito ciência');await settled();
  assert.equal(p.url().pathname,'/artigos/original/');assert.equal(p.url().searchParams.get('utm'),'keep');assert.equal(p.url().hash,'#ref');
  assert.equal(p.nodes['search-panel'].hidden,false);assert.equal(p.nodes['page-content'].hidden,true);
  assert.equal(p.nodes['search-results'].children.length,1);assert.equal(p.nodes['search-status'].textContent,'1 resultado(s).');
  const link=p.nodes['search-results'].children[0].children[0];assert.equal(link.href,'/artigos/espirito/');assert.equal(link.textContent,'Espírito e ciência');assert.equal(link.children.filter(n=>n.tag==='mark').length,2);
  p.submit('missing');await settled();assert.equal(p.nodes['search-status'].textContent,'0 resultado(s).');assert.equal(p.calls.length,2);
  p.nodes['close-search'].listeners.click();assert.equal(p.nodes['page-content'].hidden,false);assert.equal(p.nodes['search-panel'].hidden,true);assert.equal(p.url().searchParams.has('q'),false);assert.equal(p.nodes.query.focused,true);
});

test('old search URLs use the homepage with inline results',async()=>{
  const p=page('https://example.test/search/?q=espirito');await settled();
  assert.equal(p.url().pathname,'/');assert.equal(p.url().searchParams.get('q'),'espirito');assert.equal(p.nodes['search-results'].children.length,1);
});

test('clearing a pending query restores the page and rejects its late results',async()=>{
  const p=page(undefined,true);p.submit('espirito');assert.equal(p.pending.length,2);
  p.nodes.query.value='';p.nodes.query.listeners.input();p.pending.forEach(resolve=>resolve());await settled();
  assert.equal(p.nodes['page-content'].hidden,false);assert.equal(p.nodes['search-panel'].hidden,true);assert.equal(p.nodes['search-results'].children.length,0);assert.equal(p.nodes['search-status'].textContent,'');
});
