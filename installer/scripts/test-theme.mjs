import assert from 'node:assert/strict';
import {readFileSync,mkdirSync,writeFileSync} from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
import postcss from 'postcss';
import * as vue from 'vue';
import {Resvg} from '@resvg/resvg-js';
const read=name=>readFileSync(new URL('../'+name,import.meta.url),'utf8');
const source=read('src/theme.ts'), nativeSource=read('src/theme-window.ts');
function fixture(code=source,{saved=null,dark=false,storageError=false}={}){
 const values=new Map(saved===null?[]:[['k11c-installer.theme',saved]]),listeners=new Set(),calls=[];
 const media={matches:dark,addEventListener:(event,fn)=>listeners.add(fn),removeEventListener:(event,fn)=>listeners.delete(fn)};
 const document={documentElement:{dataset:{},style:{}}};
 const window={matchMedia:()=>media,localStorage:{getItem:key=>{if(storageError)throw Error('storage');return values.get(key)??null;},setItem:(key,value)=>{if(storageError)throw Error('storage');values.set(key,value);}}};
 const exports={};const context={exports,require:name=>{assert.equal(name,'vue');return vue;},window,document,console:{warn(){}}};
 const js=ts.transpileModule(code,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
 vm.runInNewContext(js,context);
 return {api:exports,document,values,listeners,calls,media,change(dark){media.matches=dark;for(const listener of listeners)listener();}};
}
const flush=()=>new Promise(resolve=>setImmediate(resolve));
async function check(code=source){
 for(const dark of [false,true]){
  const f=fixture(code,{dark});const a=f.api;
  assert.equal(a.themePreference.value,'system');assert.equal(a.resolvedTheme.value,dark?'dark':'light');
  assert.equal(f.document.documentElement.dataset.theme,a.resolvedTheme.value);
  assert.equal(f.document.documentElement.style.colorScheme,a.resolvedTheme.value);
  const stop=a.startThemeSync(async(...args)=>f.calls.push(args));await flush();
  assert.equal(f.listeners.size,1);assert.equal(f.calls.length,1);
  f.change(!dark);await flush();assert.equal(a.resolvedTheme.value,dark?'light':'dark');assert.equal(f.calls.length,2);
  for(const selection of ['light','dark']){
   assert.equal(a.setTheme(selection),true);await flush();const count=f.calls.length;
   assert.equal(f.values.get(a.themeKey),selection);assert.equal(f.document.documentElement.dataset.theme,selection);
   f.change(selection!=='dark');await flush();assert.equal(a.resolvedTheme.value,selection);assert.equal(f.calls.length,count,'Explicit mode ignores OS changes');
   const again=fixture(code,{saved:selection,dark:selection!=='dark'});assert.equal(again.api.resolvedTheme.value,selection);
  }
  const prior=f.calls.length;assert.equal(a.setTheme('invalid'),false);await flush();assert.equal(f.calls.length,prior);assert.equal(a.themePreference.value,'dark');
  a.setTheme('system');f.change(false);await flush();assert.equal(a.resolvedTheme.value,'light');f.change(true);await flush();assert.equal(a.resolvedTheme.value,'dark');
  stop();assert.equal(f.listeners.size,0);const after=f.calls.length;f.change(false);await flush();assert.equal(f.calls.length,after);
 }
 assert.equal(fixture(code,{saved:'obsolete',dark:true}).api.themePreference.value,'system');
 const blocked=fixture(code,{storageError:true,dark:true});assert.equal(blocked.api.setTheme('light'),true);assert.equal(blocked.api.resolvedTheme.value,'light');
 const q=fixture(code);let release;const events=[];let first=true;
 q.api.startThemeSync(async(preference)=>{events.push(preference);if(first){first=false;await new Promise(r=>release=r);throw Error('native unavailable');}});
 await flush();q.api.setTheme('dark');q.api.setTheme('light');await flush();assert.deepEqual(events,['system'],'Native changes serialize');release();await flush();await flush();assert.deepEqual(events,['system','dark','light'],'Failed native change does not block later choices');
}
async function checkNative(code=nativeSource){
 for(const native of [false,true]){
  const calls=[];const exports={};const mock={setTheme:async value=>calls.push(['theme',value]),setBackgroundColor:async value=>calls.push(['background',value])};
  vm.runInNewContext(ts.transpileModule(code,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,require:name=>name.endsWith('/core')?{isTauri:()=>native}:{getCurrentWindow:()=>mock}});
  for(const [preference,resolved]of [['dark','dark'],['light','light'],['system','dark'],['system','light']])await exports.syncWindowTheme(preference,resolved);
  assert.deepEqual(calls,native?[['theme','dark'],['background','#202020'],['theme','light'],['background','#f3f3f3'],['theme',null],['background','#202020'],['theme',null],['background','#f3f3f3']]:[]);
 }
}
function wiring(){
 const main=read('src/main.ts');assert.match(main,/startThemeSync\(syncWindowTheme\)/);assert.ok(main.indexOf("import './theme.css'")<main.indexOf("import './style.css'"));
 const config=JSON.parse(read('src-tauri/tauri.conf.json'));assert.equal(config.app.windows[0].theme,undefined);
 const permissions=JSON.parse(read('src-tauri/capabilities/main.json')).permissions;
 for(const permission of ['core:window:allow-set-theme','core:window:allow-set-background-color'])assert.ok(permissions.includes(permission));
 const css=postcss.parse(read('src/theme.css'));const palettes=[];
 css.walkRules(rule=>{const values={};rule.walkDecls(d=>values[d.prop]=d.value);palettes.push(values);});
 function luminance(hex){const rgb=hex.slice(1).match(/../g).map(n=>parseInt(n,16)/255).map(c=>c<=.04045?c/12.92:((c+.055)/1.055)**2.4);return rgb[0]*.2126+rgb[1]*.7152+rgb[2]*.0722;}
 for(const p of palettes)for(const [text,bg]of [['--text','--surface'],['--text-secondary','--canvas'],['--text-secondary','--surface'],['--on-accent','--accent'],['--accent','--surface'],['--success','--success-surface'],['--warning','--warning-surface'],['--danger','--danger-surface']]){
  const x=luminance(p[text]),y=luminance(p[bg]);assert.ok((Math.max(x,y)+.05)/(Math.min(x,y)+.05)>=4.5,`${text}/${bg} normal text contrast`);
 }
 for(const file of ['src/style.css','src/wizard.css','src/typography.css'])postcss.parse(read(file)).walkDecls(d=>{if(d.prop==='font-size'&&d.value.endsWith('px'))assert.ok(parseFloat(d.value)>=12,`${file} caption floor`);});
}
function checkIcon(){
 const ico=readFileSync(new URL('../src-tauri/icons/icon.ico',import.meta.url));const sizes=[16,20,24,32,40,48,64,128,256];
 assert.equal(ico.readUInt16LE(2),1);assert.equal(ico.readUInt16LE(4),sizes.length);
 const svg=read('public/app-icon.svg');let end=6+16*sizes.length;
 for(let i=0;i<sizes.length;i++){
  const at=6+i*16,n=sizes[i],length=ico.readUInt32LE(at+8),offset=ico.readUInt32LE(at+12);assert.equal(offset,end);assert.equal(ico[at]||256,n);assert.equal(ico[at+1]||256,n);
  const image=new Resvg(svg,{fitTo:{mode:'width',value:n},font:{loadSystemFonts:false}}).render();
  assert.deepEqual(ico.subarray(offset,offset+length),image.asPng(),'ICO matches UI artwork at each scale');assert.equal(image.pixels[3],0,'Transparent exterior');assert.equal(image.pixels[(Math.floor(n/2)*n+Math.floor(n/2))*4+3],255,'Visible center');end+=length;
 }
 assert.equal(end,ico.length);assert.match(read('src/App.vue'),/src="\/app-icon.svg"/);assert.match(read('index.html'),/href="\/app-icon.svg"/);
}
await check();await checkNative();wiring();checkIcon();const report=[];
if(process.argv.includes('--mutation'))for(const [name,file,before,after]of [
 ['saved theme ignored','state','if (isTheme(saved)) return saved;','if (false) return saved;'],
 ['invalid theme accepted','state','if (!isTheme(value)) return false;',''],
 ['OS theme reversed','state',"media?.matches ? 'dark' : 'light'","media?.matches ? 'light' : 'dark'"],
 ['preference not saved','state','window.localStorage.setItem(themeKey, value);',''],
 ['DOM theme disconnected','state','document.documentElement.dataset.theme = resolvedTheme.value;',''],
 ['native controls wrong color scheme','state','document.documentElement.style.colorScheme = resolvedTheme.value;',"document.documentElement.style.colorScheme = 'dark';"],
 ['system listener removed','state',"media?.addEventListener('change', systemChanged);",''],
 ['manual mode causes native OS updates','state',"if (themePreference.value === 'system') applyTheme();",'applyTheme();'],
 ['native serialization removed','state','pending = pending.then(() => sync(preference, resolved))','pending = sync(preference, resolved)'],
 ['listener cleanup removed','state',"media?.removeEventListener('change', systemChanged);",''],
 ['system mode forced dark','native',"preference === 'system' ? null : preference","preference === 'system' ? 'dark' : preference"],
 ['title bar disconnected','native',"await window.setTheme(preference === 'system' ? null : preference);",''],
 ['window background reversed','native',"resolved === 'dark' ? '#202020' : '#f3f3f3'","resolved === 'dark' ? '#f3f3f3' : '#202020'"],
 ]){
 const original=file==='state'?source:nativeSource;assert.equal(original.split(before).length-1,1,name);
 try{await(file==='state'?check:checkNative)(original.replace(before,after));}catch(error){if(error.code!=='ERR_ASSERTION')throw error;report.push({name,killed:true});continue;}throw Error('SURVIVED '+name);
}
assert.equal(read('src/theme.ts'),source);assert.equal(read('src/theme-window.ts'),nativeSource);
mkdirSync('test-results',{recursive:true});writeFileSync(process.argv.includes('--mutation')?'test-results/theme-mutations.json':'test-results/theme-verification.json',JSON.stringify({passed:true,production_modules:true,mutations:report,source_unchanged:true},null,2));
console.log('THEME_ICON_PASS mutations='+report.length+'; state, native title bar contract, contrast, typography, 9 ICO sizes');
