import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import vm from 'node:vm';
import ts from 'typescript';
import { ref } from 'vue';
import { parse } from '@vue/compiler-sfc';
import { parse as parseTemplate } from '@vue/compiler-dom';
process.chdir(path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..'));
const source=readFileSync('src/i18n.ts','utf8');
const catalog=readFileSync('src/messages.ts','utf8');
const transpile=code=>ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const catalogContext={exports:{}};vm.runInNewContext(transpile(catalog),catalogContext);
const rows=catalogContext.exports.messages;
const keys=new Set(rows.map(row=>row[0]));
assert.equal(rows.length,keys.size,'Unique translation keys');
const placeholders=value=>[...value.matchAll(/\{(\w+)\}/g)].map(m=>m[1]).sort().join(',');
for(const row of rows){
  assert.equal(row.length,5);
  for(const cell of row){assert.ok(cell.trim(),row[0]);assert.equal(placeholders(cell),placeholders(row[0]),row[0]);assert.ok(!cell.includes('\ufffd'),'Valid UTF-8');}
  for(const cell of row.slice(1))assert.doesNotMatch(cell,/[가-힣]/,row[0]);
  assert.doesNotMatch(row[2],/[这为与个来时将启关设传输连备动选确认储载错请检验执删写读显语复览网软盘块号断无进应权驱镜览后]/,`Traditional Chinese only: ${row[2]}`);
}
// Check every Korean source key, including dynamic status/error dictionaries.
function checkStrings(code){const tree=ts.createSourceFile('keys.ts',code,ts.ScriptTarget.Latest,true);function visit(node){if(ts.isStringLiteral(node)&&/[가-힣]/.test(node.text))assert.ok(keys.has(node.text),`Missing translation: ${node.text}`);ts.forEachChild(node,visit);}visit(tree);}
for(const name of ['App','InstallWizard','RestoreSummary']){
  const {descriptor}=parse(readFileSync(`src/${name}.vue`,'utf8'));
  checkStrings(descriptor.scriptSetup.content);
  function visit(node){
    if(node.type===2)assert.doesNotMatch(node.content,/[가-힣]/,'Static UI text must use translation');
    if(node.type===5)checkStrings(node.content.content);
    for(const p of node.props??[]){if(p.type===6&&p.value)assert.doesNotMatch(p.value.content,/[가-힣]/,'Static UI attribute');if(p.type===7&&p.exp)checkStrings(p.exp.content);}
    for(const c of node.children??[])visit(c);
  }
  visit(parseTemplate(descriptor.template.content));
}
function load(code=source,{saved=null,preferred=['en-US'],blocked=false}={}){
  const storage=new Map(saved===null?[]:[['k11c-installer.language',saved]]);
  const document={documentElement:{lang:''}};
  const context={exports:{},require:name=>name==='vue'?{ref}:name==='./messages'?{messages:rows}:assert.fail(name),navigator:{languages:preferred,language:preferred[0]??''},document,window:{localStorage:{getItem:key=>{if(blocked)throw Error('denied');return storage.get(key)??null;},setItem:(key,value)=>{if(blocked)throw Error('denied');storage.set(key,value);}}}};
  vm.runInNewContext(transpile(code),context);
  return {api:context.exports,storage,document};
}
function suite(code=source){
  const {api,storage,document}=load(code,{saved:'es',preferred:['ko-KR']});
  assert.equal(api.locale.value,'es','Saved preference wins');
  assert.equal(document.documentElement.lang,'es');
  assert.equal(Array.from(api.languages,l=>l.id).join(','),'en,ko,zh-TW,es,ja');
  assert.equal(api.languages[2].name,'繁體中文');
  assert.equal(api.t('설정'),'Ajustes');
  for(const [tag,expected] of [['en-GB','en'],['ko-KR','ko'],['zh-TW','zh-TW'],['zh-HK','zh-TW'],['zh-CN','zh-TW'],['zh_Hant_HK','zh-TW'],['es-MX','es'],['ja-JP','ja'],['de-DE','en']])assert.equal(load(code,{saved:'invalid',preferred:[tag]}).api.locale.value,expected,tag);
  assert.equal(load(code,{preferred:['fr','ja-JP']}).api.locale.value,'ja');
  assert.equal(load(code,{preferred:[]}).api.locale.value,'en');
  for(const [id,label] of [['en','Settings'],['ko','설정'],['zh-TW','設定'],['es','Ajustes'],['ja','設定']]){
    assert.equal(api.setLocale(id),true);assert.equal(api.locale.value,id);assert.equal(api.t('설정'),label);assert.equal(document.documentElement.lang,id);assert.equal(storage.get(api.languageKey),id);
    assert.equal(load(code,{saved:storage.get(api.languageKey),preferred:['en']}).api.locale.value,id,'Reopen restores preference');
    assert.equal(api.t('{operation} 확인',{operation:'K11C'}).includes('K11C'),true);
    assert.equal(api.number(1234.5,1),new Intl.NumberFormat(id,{minimumFractionDigits:1,maximumFractionDigits:1}).format(1234.5));
    assert.equal(api.dateTime(1790775000),new Date(1790775000000).toLocaleString(id));
  }
  assert.equal(api.setLocale('zh-CN'),false);assert.equal(api.locale.value,'ja','No Simplified Chinese locale');
  assert.equal(api.setLocale('<script>'),false);assert.equal(api.locale.value,'ja');
  assert.equal(api.t('USB_READ'),'USB_READ');assert.equal(api.t(null),'');assert.equal(api.t(undefined),'');
  api.setLocale('en');assert.equal(api.t('{operation} 확인',{operation:'<b>K11C</b>'}),'Confirm: <b>K11C</b>');
  assert.equal(api.t('{operation} 확인'),'Confirm: {operation}');
  const denied=load(code,{blocked:true,preferred:['ko']});assert.equal(denied.api.locale.value,'ko');assert.equal(denied.api.setLocale('es'),true);assert.equal(denied.api.t('설정'),'Ajustes');
}
suite();
const mutations=[
  ['saved preference ignored','if (isLocale(saved)) return saved;',''],
  ['locale guard removed','if (!isLocale(value)) return false;',''],
  ['locale guard reversed','if (!isLocale(value)) return false;','if (isLocale(value)) return false;'],
  ['reactivity removed','locale.value = value;',''],
  ['persistence removed','window.localStorage.setItem(languageKey, value);',''],
  ['document language removed','document.documentElement.lang = locale.value;',''],
  ['Chinese locale misrouted',"base === 'zh' ? 'zh-TW' : base","base === 'zh' ? 'en' : base"],
  ['wrong translation column','es: 3','es: 1'],
  ['parameter interpolation removed','String(parameters[name])','token'],
  ['unknown language fallback changed',"return 'en';","return 'ko';"],
];
const report=[];
if(process.argv.includes('--mutation'))for(const[name,before,after]of mutations){
  assert.equal(source.split(before).length-1,1,name);
  try{suite(source.replace(before,after));}catch(error){assert.equal(error.code,'ERR_ASSERTION',name);report.push({name,killed:true});continue;}
  assert.fail(`Survived: ${name}`);
}
assert.equal(readFileSync('src/i18n.ts','utf8'),source);assert.equal(readFileSync('src/messages.ts','utf8'),catalog);
mkdirSync('test-results',{recursive:true});
writeFileSync(process.argv.includes('--mutation')?'test-results/i18n-mutations.json':'test-results/i18n-verification.json',JSON.stringify({passed:true,locales:5,chinese:'Traditional only',messages:rows.length,mutations:report},null,2));
console.log(`I18N_PASS messages=${rows.length} locales=5 Traditional-only mutations=${report.length}`);
