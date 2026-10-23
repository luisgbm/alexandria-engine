const {test} = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const source = fs.readFileSync(require('node:path').join(__dirname, '../admin/admin.js'), 'utf8');
const block = source.slice(source.indexOf('  let autoPermalink=true'), source.indexOf('  function fields()'));

function editor() {
  const nodes = Object.fromEntries(['title', 'permalink', 'path-hint'].map(id => [id, {
    value: '', readOnly: false, handlers: {}, addEventListener(event, fn) { this.handlers[event] = fn; }
  }]));
  let timer, request;
  const context = vm.createContext({
    $: id => nodes[id], doc: {metadata: {id: 'draft-1', kind: 'post'}},
    clearTimeout: () => {timer = null;}, setTimeout: fn => {timer = fn; return 1;},
    schedulePreview() {}, api: () => new Promise(resolve => {request = resolve;})
  });
  vm.runInContext(block, context);
  return {nodes, context, type(title) {nodes.title.value = title; nodes.title.handlers.input();},
    check() {return timer();}, respond(path) {request({permalink: path});}};
}

test('typing generates a normalized URL immediately and applies the server collision suffix', async () => {
  const e = editor(); e.type('Téste de Artigo!');
  assert.equal(e.nodes.permalink.value, '/artigos/teste-de-artigo/');
  const pending = e.check(); e.respond('/artigos/teste-de-artigo-2bac3d/'); await pending;
  assert.equal(e.nodes.permalink.value, '/artigos/teste-de-artigo-2bac3d/');
});

test('a late response cannot overwrite a newer title or a manual address', async () => {
  const e = editor(); e.type('First'); const first = e.check();
  e.type('Second'); e.respond('/artigos/first-abcdef/'); await first;
  assert.equal(e.nodes.permalink.value, '/artigos/second/');
  const second = e.check(); e.nodes.permalink.value = '/custom/'; e.nodes.permalink.handlers.input();
  e.respond('/artigos/second-abcdef/'); await second;
  e.type('Third'); assert.equal(e.nodes.permalink.value, '/custom/');
});

test('published addresses stay unchanged and pages use a root URL', () => {
  const e = editor(); e.nodes.permalink.readOnly = true; e.nodes.permalink.value = '/existing/';
  e.type('New title'); assert.equal(e.nodes.permalink.value, '/existing/');
  e.nodes.permalink.readOnly = false; e.context.doc.metadata.kind = 'page';
  e.type('Minha Página'); assert.equal(e.nodes.permalink.value, '/minha-pagina/');
});
