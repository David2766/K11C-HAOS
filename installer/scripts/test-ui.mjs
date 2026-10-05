import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, mkdirSync, mkdtempSync } from 'node:fs';
import path from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { parse, compileScript } from '@vue/compiler-sfc';
import ts from 'typescript';
import postcss from 'postcss';
import { createRenderer, nextTick } from 'vue';
globalThis.Document=class Document {};
globalThis.ShadowRoot=class ShadowRoot {};

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
process.chdir(root);
mkdirSync('test-results', { recursive: true });
const out = mkdtempSync(path.join(root, 'test-results/ui-'));
const sources = Object.fromEntries(['App', 'InstallWizard', 'RestoreSummary'].map(name => [name, readFileSync(`src/${name}.vue`, 'utf8')]));
const settingsCss = readFileSync('src/typography.css', 'utf8');
// Compile the production SFCs, not a duplicate state machine. Only the IPC boundary
// is substituted. Vue renders slots, directives and event handlers in the test host.
const fixture = path.join(out, 'ipc.mjs');
writeFileSync(fixture, `export const native=false; export const calls=[]; export const replies={};
export async function call(command,args){calls.push({command,args}); const r=replies[command]; return typeof r==='function'?await r():r??{ok:false,error:{code:'DEVICE_GONE',detail:'Test IPC boundary'}};}
`);
const ipc = await import(pathToFileURL(fixture));
for (const name of ['messages','i18n','theme']) {
  let js=ts.transpileModule(readFileSync(`src/${name}.ts`,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ESNext}}).outputText;
  js=js.replace("from './messages'", "from './messages.mjs'");
  writeFileSync(path.join(out,`${name}.mjs`),js);
}
const i18nUrl=pathToFileURL(path.join(out,'i18n.mjs')).href;
const i18n=await import(i18nUrl);
const themeUrl=pathToFileURL(path.join(out,'theme.mjs')).href;
const theme=await import(themeUrl);
let serial = 0;
async function compile(name, source, wizardUrl, summarySource=sources.RestoreSummary) {
  const { descriptor, errors } = parse(source, { filename: `${name}.vue` });
  assert.equal(errors.length, 0);
  const compiled = compileScript(descriptor, { id: name, inlineTemplate: true, templateOptions: { compilerOptions: { hoistStatic: false } } });
  let js = ts.transpileModule(compiled.content, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
  js = js.replace(/import ['"]\.\/wizard\.css['"];?/, '')
    .replace(/from ['"]\.\/api['"]/, `from ${JSON.stringify(pathToFileURL(fixture).href)}`)
    .replace(/from ['"]\.\/i18n['"]/, `from ${JSON.stringify(i18nUrl)}`)
    .replace(/from ['"]\.\/theme['"]/, `from ${JSON.stringify(themeUrl)}`);
  if (wizardUrl) js = js.replace(/from ['"]\.\/InstallWizard\.vue['"]/, `from ${JSON.stringify(wizardUrl)}`);
  if(name!=='RestoreSummary'){
    const summary=await compile('RestoreSummary',summarySource);
    js=js.replace(/from ['"]\.\/RestoreSummary\.vue['"]/,`from ${JSON.stringify(summary.url)}`);
  }
  const output = path.join(out, `${name}-${serial++}.mjs`);
  writeFileSync(output, js);
  return { url: pathToFileURL(output).href, component: (await import(pathToFileURL(output))).default };
}
function element(type, text = '') {
  return { type, text, props: {}, children: [], parent: null, style: {}, get options(){return this.children.filter(n=>n.type==='option');}, getRootNode(){return {activeElement:null};}, addEventListener() {}, removeEventListener() {} };
}
const renderer = createRenderer({
  createElement: type => element(type), createText: text => element('#text', text), createComment: text => element('#comment', text),
  insert(child, parent, anchor = null) { if(child===anchor)return; if (child.parent) {const old=child.parent.children.indexOf(child);if(old>=0)child.parent.children.splice(old,1);} child.parent = parent; const i = anchor ? parent.children.indexOf(anchor) : -1; i < 0 ? parent.children.push(child) : parent.children.splice(i, 0, child); },
  remove(child) { if (child.parent) {const index=child.parent.children.indexOf(child);if(index>=0)child.parent.children.splice(index, 1);} child.parent = null; },
  setText(node, text) { node.text = text; }, setElementText(node, text) { for(const child of node.children)child.parent=null;node.children = []; node.text = text; },
  parentNode: node => node.parent, nextSibling: node => node.parent?.children[node.parent.children.indexOf(node) + 1] ?? null,
  patchProp(node, key, old, value) { node.props[key] = value;if(key==='value'){node.value=value;node._value=value;} if (key === 'style' && value && typeof value === 'object') Object.assign(node.style, value); },
  setScopeId() {},
});
const nodes = node => [node, ...node.children.flatMap(nodes)];
const visible = node => node.style.display !== 'none' && (!node.parent || visible(node.parent));
const text = node => node.type === '#comment' ? '' : node.text + node.children.map(text).join('');
const disabled = node => node.props.disabled === true || node.props.disabled === '';
function find(root, type, label) { const found = nodes(root).filter(n => visible(n) && n.type === type && (text(n) === label || n.props['aria-label'] === label)); assert.equal(found.length, 1, `${type} ${label}: ${found.length}`); return found[0]; }
async function settle() { await nextTick(); await new Promise(resolve => setImmediate(resolve)); await nextTick(); }
async function click(node, raw = false) { if (!raw) assert.equal(disabled(node), false, `Disabled: ${text(node)}`); await node.props.onClick?.({}); await settle(); }
function stage(root, index) {
  const slides = nodes(root).filter(n => visible(n) && n.props['data-step'] !== undefined);
  assert.equal(slides.length, 1, 'Exactly one visible slide');
  assert.equal(slides[0].props['data-step'], index);
  const active = nodes(root).filter(n => visible(n) && n.props['aria-current'] === 'step');
  assert.equal(active.length, 1);
  assert.equal(text(active[0]).slice(0, 2), String(index + 1).padStart(2, '0'));
  const wizard=nodes(root).find(n=>visible(n)&&n.props['aria-label']==='설치·복구 단계');
  const primaries=nodes(wizard).filter(n=>visible(n)&&n.type==='button'&&String(n.props.class).split(' ').includes('primary'));
  assert.equal(primaries.length,1,'One primary action per wizard step');
  assert.equal(primaries[0],primary(root),'Primary action belongs to the footer');
  assert.equal(primaries[0].parent.type,'footer');
}
function primary(root){const found=nodes(root).filter(n=>visible(n)&&n.type==='button'&&Object.hasOwn(n.props,'data-wizard-primary'));assert.equal(found.length,1);return found[0];}
async function chooseBackupTask(root,mode){
  const back=nodes(root).find(n=>visible(n)&&Object.hasOwn(n.props,'data-backup-back'));
  if(back)await click(back);
  await click(nodes(root).find(n=>visible(n)&&n.props['data-backup-mode']===mode));
  if(mode==='restore')await click(nodes(root).find(n=>visible(n)&&n.props['data-restore-source']==='backup'));
}
function futureDisabled(root, name) {
  const buttons = nodes(root).filter(n => visible(n) && n.props['data-storage-action'] === name);
  assert.equal(buttons.length, 1, name);
  assert.equal(disabled(buttons[0]), true, `${name} disabled`);
}
const device = (mode, id = 'K11C-TEST') => ({ instance_id: id, vid: 0x2207, pid: 0x350a, binding: true, mode, location: 'TEST-PORT' });
const observation=(id='K11C-TEST')=>({ready:true,instance_id:id,location:'TEST-PORT',bytes:32000000000,sectors:62500000,os:'HAOS',kinds:['FULL','BOOT','HAOS']});
async function runSuite(overrides = {}) {
  i18n.setLocale('ko');
  ipc.calls.length = 0;
  for (const key of Object.keys(ipc.replies)) delete ipc.replies[key];
  const wizard = await compile('InstallWizard', overrides.InstallWizard ?? sources.InstallWizard);
  const app = await compile('App', overrides.App ?? sources.App, wizard.url);
  const host = element('root'); const instance = renderer.createApp(app.component); instance.mount(host); await settle();
  try {
    stage(host,0);assert.equal(text(primary(host)),'다음: 연결');await click(primary(host));
    stage(host,1);
    assert.equal(text(primary(host)),'장치 준비 후 다음');
    assert.equal(disabled(primary(host)), true);
    await click(primary(host),true);stage(host,1);
    await click(find(host, 'button', '이전')); stage(host,0);
    assert.equal(disabled(find(host,'button','이전')),true);await click(find(host,'button','이전'),true);stage(host,0);await click(primary(host));stage(host,1);
    await click(find(host, 'button', '03전체 백업')); stage(host,2);
    assert.equal(disabled(primary(host)), true);
    await click(primary(host), true);stage(host,2);
    assert.equal(ipc.calls.length, 0, 'No backup IPC without a device');
    let releaseFinish;
    ipc.replies.image_releases=()=>new Promise(resolve=>{releaseFinish=resolve;});
    await click(find(host, 'button', '04파일 선택')); stage(host,3);
    assert.equal(ipc.calls.filter(c=>c.command==='image_releases').length,1,'First official slide entry auto-loads');
    assert.match(text(host),/공식 버전 목록을 불러오는 중/);
    await click(find(host,'button','버전 목록 새로고침'),true);
    assert.equal(ipc.calls.filter(c=>c.command==='image_releases').length,1,'Loading cannot duplicate metadata requests');
    assert.equal(disabled(primary(host)),true);
    await click(primary(host),true);
    assert.equal(disabled(nodes(host).find(n => n.props.id === 'haos-version')), true);
    await click(find(host, 'button', '내 PC에서 선택다운로드한 HAOS 이미지 사용'));
    assert.equal(text(primary(host)),'파일 선택 후 다음');
    assert.equal(disabled(primary(host)),false);
    await click(find(host, 'button', '05확인·실행')); stage(host,4);
    futureDisabled(host, 'install');
    assert.match(text(host), /연결 안 됨/);
    assert.match(text(host), /백업 없음/);
    assert.match(text(host), /이미지 선택 안 됨/);
    assert.doesNotMatch(text(host), /백업 저장·검증 완료|설치 완료/);
    assert.deepEqual(ipc.calls.map(c=>c.command),['image_releases'],'Only metadata is fetched automatically');
    releaseFinish({ok:false,error:{code:'IMAGE_NETWORK',detail:'offline'}});await settle();
    await click(find(host, 'button', '이전')); stage(host,3);
    assert.equal(disabled(primary(host)),false); // selection survives navigation
    await click(find(host, 'button', '설정'));
    assert.equal(nodes(host).filter(n => visible(n) && n.props['data-step'] !== undefined).length, 0);
    await click(find(host, 'button', '설치·복구')); stage(host,3);
    await click(find(host,'button','공식 다운로드Home Assistant OS 버전 선택'));
    assert.match(text(host),/다운로드 서버에 연결하지 못했습니다/);
    assert.equal(ipc.calls.filter(c=>c.command==='image_releases').length,1,'Failure does not auto-retry on re-entry');
    await click(find(host,'button','다시 시도'));
    assert.equal(ipc.calls.filter(c=>c.command==='image_releases').length,2,'Explicit retry');
    await click(find(host,'button','설정'));
    releaseFinish({ok:true,data:[{version:'99.7',filename:'haos_generic-aarch64-99.7.img.xz',size:400000000,sha256:'a'.repeat(64)},{version:'99.6',filename:'older.img.xz',size:1,sha256:null}]});await settle();
    await click(find(host,'button','설치·복구'));stage(host,3);
    await click(find(host,'button','02연결'));
    await click(find(host,'button','04파일 선택'));
    assert.equal(ipc.calls.filter(c=>c.command==='image_releases').length,2,'Successful list is cached across tab and step re-entry');
    assert.doesNotMatch(text(host),/다운로드 서버에 연결하지 못했습니다/);
    await click(find(host,'button','버전 목록 새로고침'));
    assert.equal(ipc.calls.filter(c=>c.command==='image_releases').length,3,'Manual refresh is available');
    assert.equal(disabled(primary(host)),true,'Stale list cannot download during refresh');
    await click(find(host,'button','버전 목록 새로고침'),true);
    assert.equal(ipc.calls.filter(c=>c.command==='image_releases').length,3,'Refresh remains deduplicated');
    releaseFinish({ok:true,data:[{version:'99.7',filename:'haos_generic-aarch64-99.7.img.xz',size:400000000,sha256:'a'.repeat(64)},{version:'99.6',filename:'older.img.xz',size:1,sha256:null}]});await settle();
    assert.equal(disabled(primary(host)),false);
    const select=nodes(host).find(n=>n.props.id==='haos-version');
    select.props.onChange({target:{value:'99.6'}});await settle();
    assert.equal(disabled(primary(host)),true);
    const beforeDownload=ipc.calls.length;await click(primary(host),true);assert.equal(ipc.calls.length,beforeDownload);
    select.props.onChange({target:{value:'99.7'}});await settle();
    let imageFinish;
    ipc.replies.image_download=()=>new Promise(resolve=>{imageFinish=resolve;});
    const downloading=primary(host).props.onClick({});await settle();stage(host,3);
    for(const label of ['이전','05확인·실행','설정']) assert.equal(disabled(find(host,'button',label)),true);
    assert.equal(disabled(primary(host)),true);
    const busyCalls=ipc.calls.length;await click(primary(host),true);assert.equal(ipc.calls.length,busyCalls);
    assert.doesNotMatch(text(host),/이미지 준비 완료/);
    imageFinish({ok:true,data:{filename:'haos_generic-aarch64-99.7.img.xz',path:'C:/images/verified.img',bytes:1000000000,sha256:'b'.repeat(64),source_sha256:'a'.repeat(64),verification:'github_sha256'}});await downloading;await settle();
    stage(host,4);assert.match(text(host),/haos_generic-aarch64-99.7.img.xz/);futureDisabled(host,'install');
    assert.equal(ipc.calls.some(c=>c.command==='storage_plan'||c.command==='storage_execute'),false,'Image preparation never starts installation');
    await click(find(host,'button','04파일 선택'));
    assert.match(text(host),/이미지 준비 완료/);assert.match(text(host),/공식 SHA-256 · 파티션/);
    const reusedCalls=ipc.calls.length;await click(primary(host));stage(host,4);assert.equal(ipc.calls.length,reusedCalls,'Ready image advances without downloading again');
    await click(find(host,'button','04파일 선택'));
    nodes(host).find(n=>n.props.id==='haos-version').props.onChange({target:{value:'99.6'}});await settle();
    assert.doesNotMatch(text(host),/이미지 준비 완료/);assert.equal(disabled(primary(host)),true,'Changing version invalidates old image');
    nodes(host).find(n=>n.props.id==='haos-version').props.onChange({target:{value:'99.7'}});await settle();
    ipc.replies.image_download={ok:false,data:{filename:'UNVERIFIED.img'},error:{code:'IMAGE_HASH',detail:'checksum mismatch'}};
    await click(primary(host));stage(host,3);assert.doesNotMatch(text(host),/이미지 준비 완료/);assert.match(text(host),/이미지의 SHA-256이 일치하지 않습니다/);
    await click(find(host,'button','05확인·실행'));assert.match(text(host),/이미지 선택 안 됨/);
    await click(find(host,'button','04파일 선택'));
    await click(find(host,'button','내 PC에서 선택다운로드한 HAOS 이미지 사용'));
    ipc.replies.image_select={ok:true,data:{filename:'local.img',path:'C:/images/local.img',bytes:2000000000,verification:'local_structure'}};
    await click(primary(host));stage(host,4);
    await click(find(host,'button','04파일 선택'));assert.match(text(host),/공식 해시 대조 안 됨/);
    ipc.replies.image_select={ok:true,data:null};await click(find(host,'button','파일 변경'));stage(host,3);assert.doesNotMatch(text(host),/이미지 준비 완료/);
    await click(primary(host));stage(host,3); // Picker cancelled without an existing image.
    ipc.replies.image_select={ok:true,data:{filename:'local.img',bytes:2000000000,verification:'local_structure'}};
    await click(primary(host));stage(host,4);await click(find(host,'button','04파일 선택'));
    await click(find(host,'button','공식 다운로드Home Assistant OS 버전 선택'));
    assert.doesNotMatch(text(host),/이미지 준비 완료/,'Changing source invalidates the old image');
    ipc.replies.image_download=()=>new Promise(resolve=>{imageFinish=resolve;});
    const cancelling=primary(host).props.onClick({});await settle();
    ipc.replies.image_cancel={ok:true,data:{requested:true}};
    await click(find(host,'button','취소'));assert.equal(disabled(find(host,'button','취소')),true);
    imageFinish({ok:false,error:{code:'IMAGE_CANCELLED',detail:'cancelled'}});await cancelling;await settle();stage(host,3);assert.match(text(host),/이미지 준비를 취소했습니다/);
    assert.deepEqual(ipc.calls.filter(c=>c.command==='image_download').map(c=>c.args),[{version:'99.7'},{version:'99.7'},{version:'99.7'}]);
    await click(find(host, 'button', '02연결')); stage(host,1);

    ipc.replies.preflight = { ok: true, data: { driver: { installed: true }, devices: [device('Maskrom')] } };
    await click(find(host, 'button', '연결 다시 확인'));
    await click(primary(host));
    assert.equal(disabled(find(host, 'button', '준비 시작')), true);
    await click(find(host, 'button', '준비 시작'), true);
    assert.equal(ipc.calls.some(c => c.command === 'prepare_device'), false);
    // Directly invoke a disabled step to ensure the handler itself honors locks.
    assert.equal(disabled(find(host, 'button', '03전체 백업')), true);
    await click(find(host, 'button', '03전체 백업'), true); stage(host,1);
    await click(find(host, 'button', '취소'));stage(host,1);

    const preflight=rows=>({ok:true,data:{driver:{installed:true},devices:rows}});
    const prepared={ok:true,data:{device:device('Loader'),observation:observation()}};
    // Descriptor mode alone is neither success nor failure. Require matching
    // positive readiness and the same physical port after the preparation call.
    for(const outcome of ['upload-failed','restricted','untrusted','scan-failed','wrong-port','unready','wrong-observation','ready-maskrom','ready']){
      ipc.replies.preflight=preflight([]);await click(find(host,'button','연결 다시 확인'));
      ipc.replies.preflight=preflight([device('Maskrom')]);await click(find(host,'button','연결 다시 확인'));
      await click(primary(host));
      nodes(host).find(n=>n.type==='input'&&n.props.type==='checkbox').props['onUpdate:modelValue'](true);await settle();
      const mode=outcome==='ready'?'Loader':'Maskrom';
      ipc.replies.prepare_device=outcome==='upload-failed'?{ok:false,error:{code:'USB_UPLOAD'}}:{...prepared,data:{device:device(mode),observation:{...observation(),ready:outcome!=='unready',instance_id:outcome==='wrong-observation'?'OTHER': 'K11C-TEST'}}};
      if(outcome==='restricted'||outcome==='untrusted')ipc.replies.prepare_device={ok:false,error:{code:outcome==='restricted'?'LOADER_READ_RESTRICTED':'USB_READ_UNTRUSTED'}};
      ipc.replies.preflight=outcome==='scan-failed'?{ok:false,error:{code:'DEVICE_GONE'}}:preflight([{...device(mode),location:outcome==='wrong-port'?'WRONG-PORT':'TEST-PORT'}]);
      await click(find(host,'button','준비 시작'));stage(host,outcome.startsWith('ready')?2:1);
      if(!outcome.startsWith('ready'))assert.doesNotMatch(text(host),/장치 준비 완료/);
      if(outcome==='restricted')assert.match(text(host),/전체 읽기를 제한합니다/);
      if(outcome==='untrusted')assert.match(text(host),/실제 디스크 내용인지 확인할 수 없는/);
      if(outcome==='ready-maskrom')assert.equal(disabled(primary(host)),false,'Ready Maskrom descriptor permits backup');
      await click(find(host,'button','02연결'));
    }
    await click(find(host,'button','02연결'));

    ipc.replies.preflight.data.devices = [device('Loader')];
    await click(find(host, 'button', '연결 다시 확인'));
    ipc.replies.inspect_device={ok:false,error:{code:'USB_READ'}};
    await click(primary(host));stage(host,1);
    assert.equal(text(primary(host)),'장치 확인 후 다음');
    ipc.replies.inspect_device = { ok: true, data: observation() };
    const uploads=ipc.calls.filter(c=>c.command==='prepare_device').length;
    await click(primary(host));stage(host,2);
    assert.equal(ipc.calls.filter(c=>c.command==='prepare_device').length,uploads,'Loader connection does not re-upload');
    for(const code of ['BACKUP_VERIFY','LOADER_READ_RESTRICTED','USB_READ_UNTRUSTED']){
      ipc.replies.backup_device={ok:false,error:{code}};
      await click(primary(host));stage(host,2);assert.doesNotMatch(text(host),/백업 저장·검증 완료/);
    }
    let finish;
    ipc.replies.backup_device = () => new Promise(resolve => { finish = resolve; });
    const pending = primary(host).props.onClick({}); await settle();stage(host,2);
    assert.match(text(host),/eMMC 읽기·압축 중/,'Backup compression is visible while the request runs');
    for (const label of ['이전', '04파일 선택', '설정']) assert.equal(disabled(find(host, 'button', label)), true, `Busy locks ${label}`);
    assert.equal(disabled(primary(host)),true);
    await click(find(host, 'button', '04파일 선택'), true); stage(host,2);
    const backupCalls=ipc.calls.length;await click(primary(host), true); stage(host,2);assert.equal(ipc.calls.length,backupCalls);
    await click(find(host, 'button', '설정'), true); stage(host,2);
    finish({ ok: true, data: { id:'FULL-260930-120000.k11cbackup',kind:'FULL',verified:true,path: 'C:/backup/test', gpt: { healthy: true } } }); await pending; await settle();
    stage(host,3);
    await click(find(host,'button','03전체 백업'));assert.match(text(host), /백업 저장·검증 완료/);
    const reusedBackupCalls=ipc.calls.length;await click(primary(host));stage(host,3);assert.equal(ipc.calls.length,reusedBackupCalls,'Same-device verified backup is reused');
    await click(find(host, 'button', '05확인·실행')); stage(host,4);
    assert.match(text(host), /eMMC · 29.8 GiB/);
    assert.match(text(host), /C:\/backup\/test/);
    futureDisabled(host, 'install');
    ipc.replies.preflight.data.devices=[{...device('Loader'),location:'NEW-PORT'}];
    await click(find(host,'button','연결 다시 확인'));assert.match(text(host),/백업 없음/,'Same ID on another port invalidates backup');
    ipc.replies.preflight.data.devices = [device('Loader', 'OTHER-DEVICE')];
    await click(find(host, 'button', '연결 다시 확인'));
    assert.doesNotMatch(text(host), /C:\/backup\/test/);
    assert.match(text(host), /백업 없음/);
    assert.deepEqual(ipc.calls.filter(c => c.command === 'backup_device').map(c => c.args), Array.from({length:4},()=>({instanceId:'K11C-TEST',kind:'FULL'})));
    await click(find(host,'button','고급 기능'));
    for(const label of ['저장장치 정보','U-Boot만 업데이트','고급 백업·복원','GPT 검사·복구'])find(host,'h3',label);
    await chooseBackupTask(host,'restore');
    for(const name of ['uboot','gpt-check','restore','gpt-repair'])futureDisabled(host,name);
    const beforeAdvanced=ipc.calls.length;
    for(const name of ['restore','gpt-repair'])await click(nodes(host).find(n=>visible(n)&&n.props['data-storage-action']===name),true);
    assert.equal(ipc.calls.length,beforeAdvanced,'Unready advanced actions cannot invoke IPC');
    ipc.replies.inspect_device={ok:true,data:observation('OTHER-DEVICE')};
    await click(find(host,'button','정보 확인'));
    const readyAdvancedCalls=ipc.calls.length;
    for(const name of ['restore','gpt-repair'])await click(nodes(host).find(n=>visible(n)&&n.props['data-storage-action']===name),true);
    assert.equal(ipc.calls.length,readyAdvancedCalls,'Ready device still requires repairability / selected backup');
    ipc.replies.gpt_check={ok:false,error:{code:'USB_READ_UNTRUSTED'}};
    await click(find(host,'button','GPT 검사'));
    futureDisabled(host,'gpt-repair');assert.doesNotMatch(text(host),/GPT 복구 가능/);
    ipc.replies.backup_device={ok:true,data:{path:'C:/backup/advanced',gpt:{healthy:true}}};
    await chooseBackupTask(host,'backup');
    await click(find(host,'button','백업 시작'));
    assert.equal(ipc.calls.at(-1).command,'backup_device');
    assert.equal(ipc.calls.at(-1).args.instanceId,'OTHER-DEVICE');
    assert.match(text(host),/백업 저장·검증 완료/);assert.match(text(host),/C:\/backup\/advanced/);
    await click(find(host,'button','설치·복구'));stage(host,0);
    assert.doesNotMatch(text(host),/C:\/backup\/advanced/,'Completed advanced backup is not carried into the wizard');
    await click(find(host,'button','고급 기능'));
    await click(find(host,'button','정보 확인'));
    const planData=(operation)=>({plan_id:'c'.repeat(64),operation,device:device('Loader','OTHER-DEVICE'),identity:{sectors:62500000},backup_path:'C:/backup/auto',ranges:[{label:'U-Boot',lba:64,bytes:9687040}],erases_user_data:operation==='install'});
    ipc.replies.storage_plan=()=>({ok:true,data:planData('uboot')});
    ipc.replies.boot_prepare={ok:true,data:{sha256:'b'.repeat(64),revision:'r24',source:'github'}};
    await click(find(host,'button','U-Boot 업데이트'));
    assert.equal(ipc.calls.at(-1).command,'storage_plan');
    assert.deepEqual(ipc.calls.at(-1).args,{instanceId:'OTHER-DEVICE',location:'TEST-PORT',directRestore:false,operation:'uboot',source:'b'.repeat(64),recovery:''});
    assert.match(text(host),/U-Boot 업데이트 확인/);assert.match(text(host),/C:\/backup\/auto/);
    futureDisabled(host,'execute');
    const beforeConfirmation=ipc.calls.length;await click(find(host,'button','기록 시작'),true);assert.equal(ipc.calls.length,beforeConfirmation);
    const input=nodes(host).find(n=>n.props.id==='write-confirm');input.props.onInput({target:{value:'wrong'}});await settle();futureDisabled(host,'execute');
    await click(find(host,'button','취소'));assert.equal(ipc.calls.some(c=>c.command==='storage_execute'),false);
    await click(find(host,'button','U-Boot 업데이트'));
    nodes(host).find(n=>n.props.id==='write-confirm').props.onInput({target:{value:'ok'}});await settle();
    let writeFinish;ipc.replies.storage_execute=()=>new Promise(resolve=>{writeFinish=resolve;});
    const writing=find(host,'button','기록 시작').props.onClick({});await settle();
    assert.equal(disabled(find(host,'button','설치·복구')),true);
    assert.doesNotMatch(text(host),/U-Boot 업데이트 완료/);
    writeFinish({ok:false,error:{code:'WRITE_VERIFY',detail:'bad readback'}});await writing;await settle();
    assert.doesNotMatch(text(host),/U-Boot 업데이트 완료/);assert.match(text(host),/읽기 대조에 실패/);
    await click(find(host,'button','U-Boot 업데이트'));
    nodes(host).find(n=>n.props.id==='write-confirm').props.onInput({target:{value:'ok'}});await settle();
    ipc.replies.storage_execute={ok:true,data:{verified:true,operation:'uboot',backup_path:'backup',journal:'journal'}};
    await click(find(host,'button','기록 시작'));assert.match(text(host),/U-Boot 업데이트 완료/);
    assert.doesNotMatch(text(host),/C:\/backup\/advanced/);
    futureDisabled(host,'uboot');
    await click(find(host,'button','확인'));
    assert.doesNotMatch(text(host),/U-Boot 업데이트 완료/);
    await click(find(host,'button','정보 확인'));
    assert.deepEqual(ipc.calls.filter(c=>c.command==='storage_execute').map(c=>c.args),[{planId:'c'.repeat(64),confirmed:true},{planId:'c'.repeat(64),confirmed:true}]);
    // A fresh preview must be invalidated by a port change, even if instance ID
    // happens to remain the same. Polling can deliver precisely this transition.
    await click(find(host,'button','U-Boot 업데이트'));
    ipc.replies.preflight.data.devices=[{...device('Loader','OTHER-DEVICE'),location:'NEW-PORT'}];
    await click(find(host,'button','연결 다시 확인'));assert.doesNotMatch(text(host),/U-Boot 업데이트 확인/);
    ipc.replies.preflight.data.devices=[device('Loader','OTHER-DEVICE')];await click(find(host,'button','연결 다시 확인'));
    await click(find(host,'button','정보 확인'));
    ipc.replies.gpt_check={ok:true,data:{gpt:{healthy:false},repairable:false}};
    await click(find(host,'button','GPT 검사'));futureDisabled(host,'gpt-repair');
    ipc.replies.gpt_check={ok:true,data:{gpt:{healthy:false},repairable:true}};await click(find(host,'button','GPT 검사'));
    ipc.replies.storage_plan={ok:true,data:planData('gpt-repair')};await click(find(host,'button','GPT 복구'));assert.equal(ipc.calls.at(-1).args.operation,'gpt-repair');await click(find(host,'button','취소'));
    await chooseBackupTask(host,'restore');
    ipc.replies.backup_catalog={ok:true,data:[{id:'k11c-backup',kind:'BOOT',legacy:true,identity:{sectors:62500000},restorable:true,device:device('Loader','OTHER-DEVICE')}]};await click(find(host,'button','목록 새로고침'));
    nodes(host).find(n=>n.props.name==='restore-backup-choice'&&n.props.value==='k11c-backup').props.onChange();await settle();
    ipc.replies.storage_plan={ok:true,data:planData('restore')};await click(find(host,'button','백업 복원'));assert.equal(ipc.calls.at(-1).args.source,'k11c-backup');await click(find(host,'button','취소'));
    await click(find(host,'button','설치·복구'));await click(find(host,'button','04파일 선택'));
    await click(find(host,'button','내 PC에서 선택다운로드한 HAOS 이미지 사용'));
    ipc.replies.image_select={ok:true,data:{filename:'haos.img',sha256:'d'.repeat(64),bytes:1000000000}};await click(primary(host));stage(host,4);
    await click(find(host,'button','03전체 백업'));
    ipc.replies.backup_device={ok:true,data:{id:'FULL-260930-120100.k11cbackup',kind:'FULL',verified:true,path:'C:/backup/full',gpt:{healthy:true}}};await click(primary(host));
    await click(find(host,'button','05확인·실행'));assert.equal(disabled(find(host,'button','설치 준비')),false);
    ipc.replies.storage_plan={ok:true,data:planData('install')};await click(find(host,'button','설치 준비'));
    assert.equal(ipc.calls.at(-1).args.source,'d'.repeat(64));assert.match(text(host),/모든 데이터가 삭제됩니다/);futureDisabled(host,'execute');
    await click(find(host,'button','취소'));
    ipc.replies.preflight=preflight([]);await click(find(host,'button','연결 다시 확인'));
    await click(find(host,'button','고급 기능'));
    await chooseBackupTask(host,'backup');
    const missingDeviceCalls=ipc.calls.length;await click(find(host,'button','백업 시작'),true);
    assert.equal(ipc.calls.length,missingDeviceCalls,'Advanced backup handler also rejects a missing device');
    // HA maintenance is independent of USB, and requires its own target preview.
    const setField=async(id,value)=>{const n=nodes(host).find(n=>n.props.id===id);assert.ok(n,id);n.props['onUpdate:modelValue'](value);await settle();};
    await setField('ha-token','test-token-not-a-real-credential');
    ipc.replies.connectivity_plan={ok:true,data:{operation:'reinstall',target:{hostname:'test-k11c',haos:'18.3',kernel:'6.18.52-haos'},candidate:{app_version:'0.5.6'}}};
    await click(find(host,'button','새로 설치 준비'));assert.match(text(host),/설치할 앱 0.5.6/);
    assert.equal(disabled(find(host,'button','새로 설치')),true);
    const beforeApp=ipc.calls.length;await click(find(host,'button','새로 설치'),true);assert.equal(ipc.calls.length,beforeApp);
    await setField('ha-address','http://another-k11c.local:8123');assert.doesNotMatch(text(host),/설치할 앱 0.5.6/);assert.equal(ipc.calls.at(-1).command,'connectivity_forget');
    await click(find(host,'button','새로 설치 준비'));await setField('app-confirm','K11C');
    let appFinish;ipc.replies.connectivity_execute=()=>new Promise(resolve=>appFinish=resolve);
    const appWriting=find(host,'button','새로 설치').props.onClick({});await settle();assert.equal(disabled(find(host,'button','설치·복구')),true);
    appFinish({ok:false,error:{code:'HA_VERIFY',detail:'not started'}});await appWriting;await settle();assert.doesNotMatch(text(host),/새로 설치 완료/);
    await setField('ha-token','another-test-token');await click(find(host,'button','새로 설치 준비'));await setField('app-confirm','K11C');
    ipc.replies.connectivity_execute={ok:true,data:{operation:'reinstall',backup:'backup123',reboot_recommended:true}};
    await click(find(host,'button','새로 설치'));assert.match(text(host),/새로 설치 완료/);assert.match(text(host),/backup123/);
    assert.equal(ipc.calls.filter(c=>c.command==='connectivity_execute').length,2);
    assert.equal(nodes(host).find(n=>n.props.id==='ha-token').value,'');
    assert.equal(nodes(host).find(n=>n.props.id==='ha-address').value,'http://homeassistant.local:8123');
    const completedCalls=ipc.calls.length;
    await click(find(host,'button','확인'));
    assert.doesNotMatch(text(host),/새로 설치 완료|backup123/);
    assert.equal(ipc.calls.length,completedCalls,'Dismissal never deletes or changes saved files');
    await click(find(host,'button','설치·복구'));stage(host,0);
  } finally { instance.unmount(); }
}

async function runLanguageSuite(overrides={}) {
  i18n.setLocale('en');ipc.calls.length=0;for(const key of Object.keys(ipc.replies))delete ipc.replies[key];
  const wizard=await compile('InstallWizard',overrides.InstallWizard??sources.InstallWizard);
  const app=await compile('App',overrides.App??sources.App,wizard.url);
  const host=element('root');const instance=renderer.createApp(app.component);instance.mount(host);await settle();
  const locales=['en','ko','zh-TW','es','ja'];
  const language=()=>nodes(host).find(n=>n.props.id==='display-language');
  const field=id=>nodes(host).find(n=>n.props.id===id);
  const uiText=node=>node.type==='#comment'||node.props.id==='display-language'?'':node.text+node.children.filter(visible).map(uiText).join('');
  const step=()=>nodes(host).find(n=>visible(n)&&n.props['data-step']!==undefined)?.props['data-step'];
  const switchTo=async id=>{
    const before=JSON.stringify(ipc.calls);const currentStep=step();
    assert.equal(disabled(language()),false);await language().props.onChange({target:{value:id}});await settle();
    assert.equal(i18n.locale.value,id);assert.equal(language().props.value,id);
    assert.equal(JSON.stringify(ipc.calls),before,'Language change has no IPC side effects');
    assert.equal(step(),currentStep,'Language change retains current step');
    const picker=field('display-theme');
    assert.deepEqual(picker.children.filter(n=>n.type==='option').map(n=>n.props.value),['system','light','dark']);
    for(const value of ['dark','light','system']){
      const uiBefore=uiText(host);const callsBefore=JSON.stringify(ipc.calls);
      await picker.props.onChange({target:{value}});await settle();
      assert.equal(theme.themePreference.value,value,'Theme picker is connected');
      assert.equal(picker.props.value,value);
      assert.equal(uiText(host),uiBefore,'Theme retains prepared image, confirmation and busy UI');
      assert.equal(step(),currentStep,'Theme retains wizard stage');
      assert.equal(JSON.stringify(ipc.calls),callsBefore,'Theme selection performs no operation IPC');
    }
    if(id!=='ko')assert.doesNotMatch(uiText(host),/[가-힣]/,'No untranslated Korean UI outside language names');
  };
  const label=key=>i18n.t(key);
  const nav=async key=>click(find(host,'button',label(key)));
  const goto=async(index,key)=>click(find(host,'button',String(index+1).padStart(2,'0')+label(key)));
  try {
    assert.deepEqual(language().children.filter(n=>n.type==='option').map(n=>n.props.value),locales,'Exactly five languages, Traditional only');
    assert.equal(text(language().children.find(n=>n.props.value==='zh-TW')),'繁體中文');
    await click(primary(host));
    for(const id of locales){
      await switchTo(id);assert.equal(text(primary(host)),label('장치 준비 후 다음'));
      assert.equal(language().props['aria-label'],label('언어 선택'));assert.equal(disabled(primary(host)),true);
      ipc.replies.preflight={ok:false,error:{code:'USB_READ',detail:'RAW_USB: timeout @ port TEST-PORT'}};
      await click(find(host,'button',label('연결 다시 확인')));
      assert.ok(uiText(host).includes(label('장치를 읽지 못했습니다. 케이블과 전원을 확인하세요.')));assert.ok(uiText(host).includes('RAW_USB: timeout @ port TEST-PORT'));
    }
    ipc.replies.preflight={ok:true,data:{driver:{installed:true,version:'1.0'},devices:[device('Loader')]}};
    await click(find(host,'button',label('연결 다시 확인')));
    ipc.replies.inspect_device={ok:true,data:observation()};await click(primary(host));
    ipc.replies.image_releases={ok:true,data:[{version:'99.7',filename:'haos-test.img.xz',size:400000000,sha256:'a'.repeat(64)}]};
    ipc.replies.backup_device={ok:true,data:{id:'FULL-260930-120000.k11cbackup',kind:'FULL',verified:true,path:'C:/backup/full',gpt:{healthy:true}}};await click(primary(host));
    await goto(3,'파일 선택');assert.equal(field('haos-version').props.value,'99.7');
    for(const id of locales){await switchTo(id);assert.equal(field('haos-version').props.value,'99.7');assert.equal(text(primary(host)),label('다운로드 후 다음'));}
    await click(find(host,'button',label('내 PC에서 선택')+label('다운로드한 HAOS 이미지 사용')));
    for(const id of locales){
      await switchTo(id);ipc.replies.image_select={ok:true,data:null};await click(primary(host));
      assert.deepEqual(ipc.calls.at(-1),{command:'image_select',args:{locale:id}},'Native picker receives current locale only');assert.equal(step(),3);
    }
    ipc.replies.image_select={ok:true,data:{filename:'official-haos.img',path:'C:/images/official-haos.img',bytes:1000000000,sha256:'b'.repeat(64),source_sha256:'a'.repeat(64),verification:'official_os_factory_data',uboot:'r24',connectivity:'0.5.6',kernel:'6.18.52-haos'}};
    await click(primary(host));assert.equal(step(),4);
    for(const id of locales){
      await switchTo(id);assert.ok(uiText(host).includes('official-haos.img'));assert.ok(uiText(host).includes('6.18.52-haos'));assert.equal(disabled(primary(host)),false);
      assert.ok(uiText(host).includes(label('기존 OS·HA 설정·사용자 데이터가 삭제됩니다.')));
    }
    const plan={plan_id:'c'.repeat(64),operation:'install',device:device('Loader'),identity:{sectors:62500000},backup_path:'C:/backup/auto',ranges:[{label:'HAOS partitions',lba:34816,bytes:536870912}],erases_user_data:true};
    ipc.replies.storage_plan={ok:true,data:plan};await click(primary(host));
    const execute=()=>nodes(host).find(n=>visible(n)&&n.props['data-storage-action']==='execute');
    assert.equal(disabled(execute()),true);
    field('write-confirm').props.onInput({target:{value:'ok'}});await settle();
    for(const id of locales){
      await switchTo(id);assert.equal(field('write-confirm').props.value,'ok');assert.equal(disabled(execute()),false);
      assert.equal(text(find(host,'h2',label('{operation} 확인').replace('{operation}',label('HAOS 설치')))),i18n.t('{operation} 확인',{operation:label('HAOS 설치')}));
      assert.ok(uiText(host).includes(label('모든 데이터가 삭제됩니다. 확인하셨다면 아래에 ok를 입력해주세요.')));assert.ok(uiText(host).includes('C:/backup/auto'));
    }
    let writeFinish;ipc.replies.storage_execute=()=>new Promise(resolve=>writeFinish=resolve);
    const pending=execute().props.onClick({});await settle();
    for(const id of locales){await switchTo(id);assert.ok(uiText(host).includes(label('이미지·기록 범위 확인 중')));assert.equal(disabled(primary(host)),true);assert.equal(disabled(find(host,'button',label('고급 기능'))),true);}
    writeFinish({ok:true,data:{operation:'install',verified:true,backup_path:'C:/backup/auto',journal:'C:/journal'}});await pending;await settle();
    assert.equal(step(),0,'Verified installation resets the wizard immediately');
    assert.doesNotMatch(uiText(host),/official-haos\.img|6\.18\.52-haos/,'Prepared image and its selected components are cleared');
    assert.deepEqual(ipc.calls.filter(c=>c.command==='storage_execute'),[{command:'storage_execute',args:{planId:plan.plan_id,confirmed:true}}]);
    for(const id of locales){await switchTo(id);find(host,'h3',i18n.t('{operation} 완료',{operation:label('HAOS 설치')}));}
    // The previous job selected a local image: reset child-local state too.
    await goto(3,'파일 선택');
    const officialSource=find(host,'button',label('공식 다운로드')+label('Home Assistant OS 버전 선택'));
    assert.equal(officialSource.props['aria-pressed'],true,'New work defaults to official source');
    await goto(4,'확인·실행');
    assert.equal(disabled(primary(host)),true,'Completed image cannot start another installation');
    await nav('고급 기능');
    for(const id of locales){
      await switchTo(id);for(const key of ['U-Boot만 업데이트','고급 백업·복원','GPT 검사·복구','Connectivity 관리'])find(host,'h3',label(key));
    }
    field('ha-token').props['onUpdate:modelValue']('TEST-TOKEN');await settle();
    ipc.replies.connectivity_plan={ok:true,data:{operation:'reinstall',target:{hostname:'test-k11c',haos:'18.3',kernel:'6.18.52-haos'},candidate:{app_version:'0.5.6'}}};
    await nav('새로 설치 준비');field('app-confirm').props['onUpdate:modelValue']('K11C');await settle();
    for(const id of locales){
      await switchTo(id);assert.equal(field('app-confirm').props['onUpdate:modelValue'] instanceof Function,true);
      assert.equal(disabled(find(host,'button',label('새로 설치'))),false);assert.ok(uiText(host).includes('test-k11c'));assert.ok(uiText(host).includes(label('기존 Connectivity 설정·데이터를 삭제합니다. 복구용 백업은 HA에 남습니다. Wi-Fi 연결이 끊길 수 있으므로 유선 LAN을 사용하세요.')));
    }
    await nav('취소');assert.equal(ipc.calls.filter(c=>c.command==='connectivity_execute').length,0);
    await nav('설정');for(const id of locales){
      await switchTo(id);find(host,'h3',label('USB 드라이버'));
      const refresh = find(host,'button',label('상태 다시 확인'));
      const card = refresh.parent;
      assert.ok(String(card.props.class).split(' ').includes('driver-settings'), 'Spacing scope belongs to the actual driver card');
      const siblings = card.children.filter(n=>!n.type.startsWith('#'));
      assert.equal(siblings[siblings.indexOf(refresh)-1].type,'details', 'Disclosure and refresh remain adjacent');
      const rules=[];
      postcss.parse(overrides.settingsCss??settingsCss).walkRules('.driver-settings > details + .secondary', rule=>rules.push(rule));
      assert.equal(rules.length,1,'Driver spacing applies with details both closed and open');
      assert.equal(rules[0].parent.type,'root','Spacing is not restricted to a media query');
      const margins=rules[0].nodes.filter(n=>n.type==='decl'&&n.prop==='margin-top');
      assert.equal(margins.length,1);
      assert.equal(margins[0].value,'20px','Refresh has a positive 20px gap');
      assert.ok(readFileSync('src/main.ts','utf8').includes("import './typography.css'"), 'Production entry loads spacing stylesheet');
    }
    ipc.replies.history={ok:true,data:[{time:1790775000,operation:'backup',result:{ok:true,data:{path:'C:/backup/raw'}}}]};
    await nav('작업 기록');for(const id of locales){await switchTo(id);assert.ok(uiText(host).includes(i18n.dateTime(1790775000)));assert.ok(uiText(host).includes(label('백업')));assert.ok(uiText(host).includes('C:/backup/raw'));}
  }finally{instance.unmount();i18n.setLocale('ko');}
}

async function runArchiveSuite(overrides={}){
  i18n.setLocale('ko');ipc.calls.length=0;for(const key of Object.keys(ipc.replies))delete ipc.replies[key];
  const wizard=await compile('InstallWizard',overrides.InstallWizard??sources.InstallWizard);
  const app=await compile('App',overrides.App??sources.App,wizard.url);
  const host=element('root'),instance=renderer.createApp(app.component);instance.mount(host);await settle();
  const field=id=>nodes(host).find(n=>n.props.id===id);
  const task=mode=>nodes(host).find(n=>visible(n)&&n.props['data-wizard-mode']===mode);
  const full={id:'FULL-260930-120000.k11cbackup',path:'C:/portable/backup/FULL-260930-120000.k11cbackup',kind:'FULL',verified:true,restorable:true,identity:{sectors:62500000},bytes:32000000000,stored_bytes:1000000,gpt:{healthy:true}};
  try{
    stage(host,0);await click(task('restore'));assert.match(String(task('restore').props.class),/selected/);await click(primary(host));stage(host,1);
    ipc.replies.preflight={ok:true,data:{driver:{installed:true},devices:[device('Loader')]}};
    ipc.replies.inspect_device={ok:true,data:{...observation(),os:'other',kinds:['FULL']}};
    await click(find(host,'button','연결 다시 확인'));await click(primary(host));stage(host,2);
    ipc.replies.backup_device={ok:true,data:full};await click(primary(host));stage(host,3);
    assert.equal(ipc.calls.at(-1).args?.kind,'FULL');assert.equal(ipc.calls.some(c=>c.command.startsWith('image_')),false,'Restore never resolves new OS/components');
    ipc.replies.backup_select={ok:true,data:null};await click(primary(host));stage(host,3);
    ipc.replies.backup_select={ok:true,data:{...full,id:'BOOT-260930-115900.k11cbackup',kind:'BOOT'}};await click(primary(host));stage(host,3);
    assert.match(text(host),/여기서는 전체 백업만 복원/);
    const imported={...full,id:'raw-'+'a'.repeat(64),path:'D:/android-full.img'};
    ipc.replies.backup_select={ok:true,data:imported};await click(primary(host));stage(host,4);
    assert.match(text(host),/D:\/android-full.img/);assert.match(text(host),/C:\/portable\/backup\/FULL-/);
    assert.equal(ipc.calls.filter(c=>c.command==='backup_device').length,1);
    ipc.replies.storage_plan={ok:true,data:{plan_id:'d'.repeat(64),operation:'restore-archive',device:device('Loader'),identity:{sectors:62500000},erases_user_data:true,backup_path:full.path,ranges:[{label:'Full eMMC user area',lba:0,bytes:32000000000}]}};
    await click(primary(host));assert.deepEqual(ipc.calls.at(-1).args,{instanceId:'K11C-TEST',location:'TEST-PORT',directRestore:false,operation:'restore-archive',source:imported.id,recovery:full.id});
    field('write-confirm').props.onInput({target:{value:'K11C'}});await settle();futureDisabled(host,'execute');
    field('write-confirm').props.onInput({target:{value:'ok'}});await settle();
    ipc.replies.storage_execute={ok:true,data:{operation:'restore-archive',verified:true,backup_path:full.path,journal:'journal'}};
    await click(find(host,'button','기록 시작'));stage(host,0);assert.match(text(host),/백업 복원 완료/);assert.doesNotMatch(text(host),/D:\/android-full.img/);
    const before=ipc.calls.length;await click(find(host,'button','확인'));assert.equal(ipc.calls.length,before);assert.doesNotMatch(text(host),/FULL-260930/);
    await click(find(host,'button','고급 기능'));await click(find(host,'button','정보 확인'));
    await chooseBackupTask(host,'backup');
    assert.deepEqual(field('backup-kind').options.map(n=>n.props.value),['FULL']);
    const grid=nodes(host).find(n=>String(n.props.class)==='advanced-grid');assert.deepEqual(grid.children.filter(n=>n.type==='section').map(n=>n.props['data-advanced']??'storage'),['backup','uboot','connectivity','storage','gpt']);
    ipc.replies.inspect_device={ok:true,data:observation()};await click(find(host,'button','정보 확인'));
    assert.deepEqual(field('backup-kind').options.map(n=>n.props.value),['FULL','BOOT','HAOS']);
    field('backup-kind').props['onUpdate:modelValue']('HAOS');await settle();await click(find(host,'button','백업 시작'));assert.equal(ipc.calls.at(-1).args.kind,'HAOS');
  }finally{instance.unmount();}
}

async function runAdvancedReadinessSuite(overrides={}){
  i18n.setLocale('ko');ipc.calls.length=0;for(const key of Object.keys(ipc.replies))delete ipc.replies[key];
  const wizard=await compile('InstallWizard',sources.InstallWizard);
  const app=await compile('App',overrides.App??sources.App,wizard.url);
  const host=element('root'),instance=renderer.createApp(app.component);instance.mount(host);await settle();
  const field=id=>nodes(host).find(n=>visible(n)&&n.props.id===id);
  const connect=()=>nodes(host).find(n=>visible(n)&&Object.hasOwn(n.props,'data-advanced-connect'));
  const start=()=>find(host,'button','백업 시작');
  const setDevices=async rows=>{ipc.replies.preflight={ok:true,data:{driver:{installed:true},devices:rows}};await click(find(host,'button','연결 다시 확인'));};
  const unready=()=>{assert.equal(!!field('backup-kind'),false,'No misleading default scope before inspection');assert.equal(disabled(start()),true);assert.ok(connect(),'Advanced has its own preparation action');assert.match(text(host),/저장장치 확인 필요/);assert.doesNotMatch(text(host),/HAOS가 아닌 디스크는/);};
  try{
    ipc.replies.backup_catalog={ok:true,data:[]};
    await click(find(host,'button','고급 기능'));await chooseBackupTask(host,'backup');unready();
    assert.equal(disabled(connect()),true);const emptyCalls=ipc.calls.length;await click(connect(),true);assert.equal(ipc.calls.length,emptyCalls);assert.equal(nodes(host).some(n=>n.props.role==='dialog'),false);
    await setDevices([{...device('Maskrom'),binding:false}]);unready();assert.equal(disabled(connect()),true);
    const unboundCalls=ipc.calls.length;await click(connect(),true);assert.equal(ipc.calls.length,unboundCalls);assert.equal(nodes(host).some(n=>n.props.role==='dialog'),false,'Unbound device cannot open preparation');
    // Hot attachment on the same screen: USB enumeration alone is not storage readiness.
    await setDevices([device('Maskrom')]);unready();assert.equal(disabled(connect()),false);
    const before=ipc.calls.length;await click(connect());assert.equal(ipc.calls.length,before,'MASKROM confirmation precedes preparation');
    assert.equal(disabled(find(host,'button','준비 시작')),true);await click(find(host,'button','준비 시작'),true);assert.equal(ipc.calls.length,before);
    await click(find(host,'button','취소'));unready();
    await click(connect());nodes(host).find(n=>n.type==='input'&&n.props.type==='checkbox').props['onUpdate:modelValue'](true);await settle();
    ipc.replies.prepare_device={ok:false,error:{code:'LOADER_READ_RESTRICTED'}};await click(find(host,'button','준비 시작'));unready();
    const failedCalls=ipc.calls.length;await settle();assert.equal(ipc.calls.length,failedCalls,'No preparation retry loop');
    await click(connect());nodes(host).find(n=>n.type==='input'&&n.props.type==='checkbox').props['onUpdate:modelValue'](true);await settle();
    let finish;ipc.replies.prepare_device=()=>new Promise(resolve=>finish=resolve);
    const oldConnect=connect(),working=find(host,'button','준비 시작').props.onClick({});await settle();const busyCalls=ipc.calls.length;
    await click(oldConnect,true);assert.equal(ipc.calls.length,busyCalls,'Busy preparation cannot duplicate commands');
    finish({ok:true,data:{device:device('Maskrom'),observation:observation()}});await working;await settle();
    assert.deepEqual(field('backup-kind')?.options.map(n=>n.props.value),['FULL','BOOT','HAOS']);assert.equal(disabled(start()),false);assert.equal(!!connect(),false);
    assert.doesNotMatch(text(host),/HAOS가 아닌 디스크는/);
    assert.deepEqual(ipc.calls.filter(c=>c.command==='prepare_device').at(-1).args,{instanceId:'K11C-TEST',location:'TEST-PORT',confirmedK11c:true});
    assert.equal(ipc.calls.some(c=>['backup_device','storage_plan','storage_execute'].includes(c.command)),false,'Preparation cannot start backup/install');
    await click(find(host,'button','설치·복구'));stage(host,0);await click(find(host,'button','고급 기능'));
    // Disconnect invalidates readiness; an already-running Loader only needs inspection.
    await setDevices([]);unready();await setDevices([device('Loader')]);unready();
    const prepareCount=ipc.calls.filter(c=>c.command==='prepare_device').length;
    ipc.replies.inspect_device={ok:false,error:{code:'USB_READ'}};await click(connect());unready();
    ipc.replies.inspect_device={ok:true,data:{...observation(),location:'WRONG-PORT'}};await click(connect());unready();
    ipc.replies.inspect_device={ok:true,data:{...observation(),os:'other'}};await click(connect());
    assert.deepEqual(field('backup-kind')?.options.map(n=>n.props.value),['FULL']);assert.equal(disabled(start()),false);assert.match(text(host),/HAOS가 아닌 디스크는/);
    assert.equal(ipc.calls.filter(c=>c.command==='prepare_device').length,prepareCount,'Ready Loader never requires upload');
    await click(find(host,'button','설치·복구'));stage(host,0);await click(find(host,'button','고급 기능'));
    // Re-entering Advanced after attachment uses the same preparation path.
    await setDevices([]);await setDevices([device('Loader')]);ipc.replies.inspect_device={ok:true,data:observation()};
    await click(find(host,'button','설치·복구'));await click(find(host,'button','고급 기능'));await click(connect());
    assert.deepEqual(field('backup-kind')?.options.map(n=>n.props.value),['FULL','BOOT','HAOS']);
    for(const kind of ['FULL','BOOT','HAOS']){
      field('backup-kind').props['onUpdate:modelValue'](kind);await settle();ipc.replies.backup_device={ok:false,error:{code:'BACKUP_VERIFY'}};
      await click(start());assert.equal(ipc.calls.at(-1).command,'backup_device');assert.equal(ipc.calls.at(-1).args.kind,kind);
    }
  }finally{instance.unmount();}
}

async function runBackupTaskSuite(overrides={}){
  i18n.setLocale('ko');ipc.calls.length=0;for(const key of Object.keys(ipc.replies))delete ipc.replies[key];
  const wizard=await compile('InstallWizard',overrides.InstallWizard??sources.InstallWizard,undefined,overrides.RestoreSummary);
  const app=await compile('App',overrides.App??sources.App,wizard.url,overrides.RestoreSummary);
  const host=element('root'),instance=renderer.createApp(app.component);instance.mount(host);await settle();
  const field=id=>nodes(host).find(n=>visible(n)&&n.props.id===id);
  const pane=()=>nodes(host).filter(n=>visible(n)&&n.props['data-backup-pane']);
  const back=()=>nodes(host).find(n=>visible(n)&&Object.hasOwn(n.props,'data-backup-back'));
  const summary=(root=host)=>nodes(root).find(n=>visible(n)&&n.props['data-restore-kind']);
  const restore=()=>nodes(host).find(n=>visible(n)&&n.props['data-storage-action']==='restore');
  const full={id:'BOOT-misleading.img',path:'D:/BOOT-misleading.img',kind:'FULL',restorable:true,identity:{sectors:62500000},bytes:32000000000};
  const setRestore=async row=>{ipc.replies.backup_select={ok:true,data:row};await click(find(host,'button','백업 파일 가져오기'));};
  try{
    ipc.replies.preflight={ok:true,data:{driver:{installed:true},devices:[device('Loader')]}};
    ipc.replies.inspect_device={ok:true,data:observation()};ipc.replies.backup_catalog={ok:true,data:[]};
    await click(find(host,'button','연결 다시 확인'));await click(find(host,'button','고급 기능'));await click(find(host,'button','정보 확인'));
    assert.equal(pane().length,0,'Initial advanced panel has choices only');assert.equal(field('backup-kind'),undefined);assert.equal(field('restore-backup'),undefined);
    const choices=nodes(host).filter(n=>visible(n)&&n.props['data-backup-mode']);assert.equal(choices.length,2);
    const initialCalls=ipc.calls.length;await click(choices.find(n=>n.props['data-backup-mode']==='backup'));
    assert.equal(ipc.calls.length,initialCalls,'Task choice cannot start a backup/write');assert.deepEqual(pane().map(n=>n.props['data-backup-pane']),['backup']);assert.equal(field('restore-backup'),undefined);
    const staleBackup=find(host,'button','백업 시작');let finish;
    ipc.replies.backup_device=()=>new Promise(resolve=>finish=resolve);
    const working=staleBackup.props.onClick({});await settle();assert.equal(disabled(back()),true);
    await click(back(),true);assert.deepEqual(pane().map(n=>n.props['data-backup-pane']),['backup'],'Busy task cannot change');
    finish({ok:false,error:{code:'BACKUP_VERIFY',detail:'fixture'}});await working;await settle();
    ipc.replies.backup_device={ok:false,error:{code:'BACKUP_VERIFY',detail:'unexpected stale action'}};
    const previewRows=[full,{...full,id:'FULL-disguised-boot.k11cbackup',kind:'BOOT',legacy:true},{...full,id:'FULL-disguised-haos.k11cbackup',kind:'HAOS'},{id:'FULL-broken.k11cbackup',restorable:false},{...full,id:'FULL-unknown.k11cbackup',kind:'unknown'}];
    ipc.replies.backup_catalog={ok:true,data:previewRows};
    const catalogs=ipc.calls.filter(c=>c.command==='backup_catalog').length;
    await chooseBackupTask(host,'restore');assert.equal(field('backup-kind'),undefined);
    assert.equal(ipc.calls.filter(c=>c.command==='backup_catalog').length,catalogs+1,'Entering Restore refreshes the list automatically');
    const choice=id=>nodes(host).find(n=>visible(n)&&n.props.name==='restore-backup-choice'&&n.props.value===id);
    assert.equal(!!summary(),false,'No file is preselected');assert.equal(disabled(restore()),true);
    const scopeNotes=['전체 디스크 · OS와 데이터 포함','부팅 영역만 · OS와 데이터 제외','HAOS 파티션 · 별도 부팅 펌웨어 제외','형식 확인 불가 · 복원할 수 없음','형식 확인 불가 · 복원할 수 없음'];
    for(const locale of ['en','ko','zh-TW','es','ja']){
      i18n.setLocale(locale);await settle();
      for(const [i,row]of previewRows.entries()){
        const label=nodes(host).find(n=>visible(n)&&n.props['data-backup-id']===row.id);assert.ok(label);
        const badge=nodes(label).find(n=>n.props.class==='backup-badge');assert.ok(badge,'Every row has a badge before selection');assert.equal(text(badge),i<3?row.kind:i18n.t('미확인'));
        assert.ok(text(label).includes(i18n.t(scopeNotes[i])),'Unselected scope note is localized');
      }
    }
    i18n.setLocale('ko');await settle();
    for(const row of previewRows.slice(3)){assert.equal(disabled(choice(row.id)),true);choice(row.id).props.onChange();await settle();assert.equal(!!summary(),false,'Invalid rows cannot be selected even through a stale event');}
    const beforeChoice=ipc.calls.length;choice(previewRows[1].id).props.onChange();await settle();assert.equal(ipc.calls.length,beforeChoice,'A row selection is local only');assert.equal(summary()?.props['data-restore-kind'],'BOOT');
    let importFinish;ipc.replies.backup_select=()=>new Promise(resolve=>importFinish=resolve);
    const importing=find(host,'button','백업 파일 가져오기').props.onClick({});await settle();assert.equal(disabled(choice(full.id)),true);choice(full.id).props.onChange();await settle();assert.equal(!!summary(),false,'Busy import locks row selection');importFinish({ok:true,data:null});await importing;await settle();
    const wrongBackupCalls=ipc.calls.length;await click(staleBackup,true);assert.equal(ipc.calls.length,wrongBackupCalls,'Stale backup handler cannot run in restore mode');
    await setRestore(full);assert.equal(summary()?.props['data-restore-kind'],'FULL');assert.match(text(summary()),/FULL · 전체 디스크/);assert.match(text(summary()),/현재 디스크의 모든 데이터가 교체/);assert.equal(disabled(restore()),false);
    assert.equal(ipc.calls.some(c=>c.command==='storage_plan'||c.command==='storage_execute'),false,'Selection is not write authorization');
    for(const [kind,expected] of [['BOOT','OS·앱·설정·사용자 데이터는 복원하지 않습니다.'],['HAOS','별도 부팅 펌웨어와 GPT는 유지합니다.']]){
      await setRestore({...full,id:'FULL-misleading.k11cbackup',kind});assert.equal(summary().props['data-restore-kind'],kind);assert.ok(text(summary()).includes(expected));assert.match(text(summary()),/동일한 HAOS 파티션 구성/);assert.doesNotMatch(text(summary()),/현재 디스크의 모든 데이터가 교체/);
    }
    await setRestore({...full,kind:'BOOT',legacy:true});
    assert.equal(summary().props['data-restore-kind'],'BOOT','Legacy uses reported BOOT kind');
    // Live language switching must retain selection, operation and scope.
    for(const id of ['en','ko','zh-TW','es','ja']){
      const calls=ipc.calls.length;i18n.setLocale(id);await settle();assert.equal(ipc.calls.length,calls);
      assert.ok(text(summary()).includes(i18n.t('BOOT · 부팅 펌웨어와 GPT')));assert.ok(text(summary()).includes(i18n.t('부팅 펌웨어와 GPT만 교체합니다. OS·앱·설정·사용자 데이터는 복원하지 않습니다.')));
    }
    i18n.setLocale('ko');await settle();
    await setRestore({...full,kind:'UNKNOWN'});assert.equal(summary().props['data-restore-kind'],'unknown');assert.match(text(summary()),/다른 파일을 선택/);assert.equal(disabled(restore()),true);
    const unknownCalls=ipc.calls.length;await click(restore(),true);assert.equal(ipc.calls.length,unknownCalls);
    await setRestore({...full,identity:{sectors:62500001}});assert.equal(disabled(restore()),true);assert.match(text(host),/백업의 원본 eMMC 용량과 현재 장치의 용량이 다릅니다/);
    await setRestore(null);assert.equal(summary(),undefined,'Cancel clears stale scope');assert.equal(disabled(restore()),true);
    ipc.replies.backup_select={ok:false,error:{code:'BACKUP_FORMAT',detail:'invalid'}};await click(find(host,'button','백업 파일 가져오기'));assert.equal(summary(),undefined);
    await setRestore(full);const staleRestore=restore();
    // Same target/selection stays valid underneath the write confirmation.
    ipc.replies.storage_plan={ok:true,data:{plan_id:'e'.repeat(64),operation:'restore-archive',device:device('Loader'),identity:full.identity,erases_user_data:true,backup_path:null,ranges:[]}};
    await click(restore());const dialog=nodes(host).find(n=>n.props.role==='dialog');assert.ok(dialog);assert.equal(summary(dialog)?.props['data-restore-kind'],'FULL','Write confirmation repeats parsed scope');
    assert.equal(ipc.calls.at(-1).args.directRestore,true,'Advanced restore explicitly bypasses new recovery creation');
    assert.equal(ipc.calls.at(-1).args.recovery,'');assert.match(text(dialog),/현재 장치를 추가로 백업하지 않고 복원합니다/);
    assert.doesNotMatch(text(dialog),/복구용 백업/);
    assert.equal(disabled(choice(previewRows[1].id)),true);choice(previewRows[1].id).props.onChange();await settle();assert.equal(summary(dialog)?.props['data-restore-kind'],'FULL','Pending write plan locks row selection');
    await click(back(),true);assert.deepEqual(pane().map(n=>n.props['data-backup-pane']),['restore']);assert.equal(summary()?.props['data-restore-kind'],'FULL');assert.ok(text(host).includes(full.path));
    await click(find(host,'button','취소'));
    await click(back());assert.equal(pane().length,0);await chooseBackupTask(host,'backup');
    const wrongRestoreCalls=ipc.calls.length;await click(staleRestore,true);assert.equal(ipc.calls.length,wrongRestoreCalls);
    await chooseBackupTask(host,'restore');assert.equal(summary(),undefined,'Returning to chooser clears selected backup');assert.equal(disabled(restore()),true);
    await chooseBackupTask(host,'backup');ipc.replies.backup_device={ok:true,data:{...full,verified:true,stored_bytes:100,gpt:{healthy:true}}};await click(find(host,'button','백업 시작'));await click(find(host,'button','확인'));assert.equal(pane().length,0,'Completion resets the task chooser');
    // The reset emptied the in-memory rows. First restore entry must repopulate
    // them, including RAW imports, without a manual refresh or a second backup.
    const raw={...full,id:'raw-'+'a'.repeat(64),display_name:'android-full.img'};
    ipc.replies.backup_catalog={ok:true,data:[raw]};await chooseBackupTask(host,'restore');
    assert.ok(choice(raw.id));assert.match(text(field('restore-backup')),/android-full.img/);
    await click(back());let finishCatalog;
    ipc.replies.backup_catalog=()=>new Promise(resolve=>finishCatalog=resolve);
    await click(nodes(host).find(n=>visible(n)&&n.props['data-backup-mode']==='restore'));
    const loading=nodes(host).find(n=>visible(n)&&n.props['data-restore-source']==='backup').props.onClick({});await settle();
    assert.match(text(host),/백업 목록을 불러오는 중/);assert.equal(disabled(find(host,'button','목록 새로고침')),true);
    const during=ipc.calls.length;find(host,'button','목록 새로고침').props.onClick({});await settle();assert.equal(ipc.calls.length,during,'Concurrent catalog loads are deduplicated');
    finishCatalog({ok:true,data:[raw]});await loading;await settle();assert.ok(choice(raw.id));
    // The installation wizard must expose scope both for rejected partial files
    // and full files advanced automatically to review.
    await click(find(host,'button','설치·복구'));await click(nodes(host).find(n=>visible(n)&&n.props['data-wizard-mode']==='restore'));await click(find(host,'button','04파일 선택'));
    ipc.replies.backup_select={ok:true,data:{...full,kind:'BOOT'}};await click(primary(host));stage(host,3);assert.equal(summary()?.props['data-restore-kind'],'BOOT');assert.match(text(host),/여기서는 전체 백업만 복원/);
    ipc.replies.backup_select={ok:true,data:full};await click(primary(host));stage(host,4);assert.equal(summary()?.props['data-restore-kind'],'FULL');
  }finally{instance.unmount();i18n.setLocale('ko');}
}

async function runFactorySuite(overrides={}){
  i18n.setLocale('ko');ipc.calls.length=0;for(const key of Object.keys(ipc.replies))delete ipc.replies[key];
  const wizard=await compile('InstallWizard',sources.InstallWizard);
  const app=await compile('App',overrides.App??sources.App,wizard.url);
  const host=element('root'),instance=renderer.createApp(app.component);instance.mount(host);await settle();
  const by=(key,value)=>nodes(host).find(n=>visible(n)&&n.props[key]===value);
  const file=()=>nodes(host).find(n=>visible(n)&&Object.hasOwn(n.props,'data-factory-select'));
  const source=kind=>by('data-restore-source',kind);
  const plan=()=>by('data-storage-action','factory');
  const factory={id:'f'.repeat(64),filename:'K11C-linux.img',path:'D:/K11C-linux.img',bytes:2400000000,format:'RKFW',os:'Linux',tool:'Rockchip upgrade_tool'};
  const write={plan_id:'c'.repeat(64),operation:'factory',device:device('Loader'),identity:{sectors:62500000},backup_path:null,ranges:[{label:'rootfs',lba:34816,bytes:2400000000}],erases_user_data:true};
  try{
    ipc.replies.backup_catalog={ok:true,data:[]};ipc.replies.preflight={ok:true,data:{driver:{installed:true},devices:[device('Loader')]}};
    await click(find(host,'button','연결 다시 확인'));await click(find(host,'button','고급 기능'));
    await click(by('data-backup-mode','restore'));
    assert.ok(source('manufacturer'));assert.ok(source('backup'));assert.equal(file(),undefined);assert.equal(by('data-storage-action','restore'),undefined,'No mixed restore controls');
    const initial=ipc.calls.length;await click(source('manufacturer'));assert.equal(ipc.calls.length,initial,'Source choice cannot flash or start a tool');
    assert.equal(disabled(plan()),true);ipc.replies.factory_select={ok:true,data:factory};await click(file());assert.equal(ipc.calls.at(-1).command,'factory_select');assert.equal(disabled(plan()),true,'A selected image is not device readiness');
    const beforeUnready=ipc.calls.length;await click(plan(),true);assert.equal(ipc.calls.length,beforeUnready,'Stale plan handler checks readiness');
    ipc.replies.preflight={ok:true,data:{driver:{installed:true},devices:[device('Loader')]}};ipc.replies.inspect_device={ok:true,data:observation()};
    await click(find(host,'button','연결 다시 확인'));await click(find(host,'button','정보 확인'));assert.equal(disabled(plan()),false);
    for(const locale of ['en','ko','zh-TW','es','ja']){i18n.setLocale(locale);await settle();assert.ok(text(host).includes(i18n.t('제조사 이미지')));assert.ok(text(host).includes(factory.filename));assert.equal(disabled(plan()),false);}i18n.setLocale('ko');await settle();
    ipc.replies.factory_select={ok:false,error:{code:'FACTORY_FORMAT',detail:'single partition'}};await click(file());assert.match(text(host),/지원하지 않는 이미지입니다/);assert.equal(disabled(plan()),true);assert.doesNotMatch(text(host),/K11C-linux.img/);
    ipc.replies.factory_select={ok:true,data:null};await click(file());assert.equal(disabled(plan()),true,'Cancelled selection clears previous input');
    ipc.replies.factory_select={ok:true,data:factory};await click(file());const staleFile=file(),stalePlan=plan();
    ipc.replies.factory_plan={ok:true,data:write};await click(plan());assert.deepEqual(ipc.calls.at(-1),{command:'factory_plan',args:{instanceId:'K11C-TEST',location:'TEST-PORT',source:factory.id}});
    assert.ok(by('role','dialog'));assert.match(text(by('role','dialog')),/모든 데이터가 삭제/);assert.doesNotMatch(text(by('role','dialog')),/백업 없음/);assert.equal(disabled(by('data-storage-action','execute')),true);
    let confirm=by('id','write-confirm');confirm.props.onInput({target:{value:'K11C'}});await settle();assert.equal(disabled(by('data-storage-action','execute')),true);
    await click(find(host,'button','취소'));assert.equal(ipc.calls.some(c=>c.command==='factory_execute'),false);
    await click(nodes(host).find(n=>visible(n)&&Object.hasOwn(n.props,'data-restore-source-back')));assert.ok(source('backup'));await click(source('backup'));assert.equal(file(),undefined);
    const beforeWrong=ipc.calls.length;await click(staleFile,true);await click(stalePlan,true);assert.equal(ipc.calls.length,beforeWrong,'Stale manufacturer controls cannot run in backup mode');
    await click(nodes(host).find(n=>visible(n)&&Object.hasOwn(n.props,'data-restore-source-back')));await click(source('manufacturer'));assert.equal(disabled(plan()),true,'Changing source invalidates image and preview');await click(file());await click(plan());
    confirm=by('id','write-confirm');confirm.props.onInput({target:{value:'ok'}});await settle();assert.equal(disabled(by('data-storage-action','execute')),false);
    ipc.replies.factory_execute={ok:true,data:{verified:true,operation:'factory',journal:'factory-journal'}};await click(by('data-storage-action','execute'));
    assert.deepEqual(ipc.calls.at(-1),{command:'factory_execute',args:{planId:write.plan_id,confirmed:true}});assert.equal(ipc.calls.some(c=>c.command==='storage_execute'||c.command==='backup_device'),false,'Manufacturer advanced restore creates no redundant backup');
    assert.doesNotMatch(text(host),/K11C-linux.img/);assert.ok(by('data-backup-mode','restore'),'Completion resets source chooser');
    await click(find(host,'button','확인'));await click(by('data-backup-mode','restore'));await click(source('manufacturer'));await click(find(host,'button','정보 확인'));
    let finish;ipc.replies.factory_select=()=>new Promise(resolve=>finish=resolve);const pending=file().props.onClick({});await settle();
    const back=nodes(host).find(n=>visible(n)&&Object.hasOwn(n.props,'data-restore-source-back'));assert.equal(disabled(back),true);await click(back,true);assert.ok(file(),'Busy source cannot change');finish({ok:true,data:factory});await pending;await settle();assert.ok(file());
    // RAW manufacturer images use the same confirmed factory dispatcher.
    ipc.replies.factory_plan={ok:true,data:{...write,operation:'factory-raw'}};await click(plan());by('id','write-confirm').props.onInput({target:{value:'ok'}});await settle();ipc.replies.factory_execute={ok:true,data:{verified:true,operation:'factory-raw'}};await click(by('data-storage-action','execute'));assert.equal(ipc.calls.at(-1).command,'factory_execute');
  }finally{instance.unmount();i18n.setLocale('ko');}
}

await runFactorySuite();
await runAdvancedReadinessSuite();
await runBackupTaskSuite();
await runArchiveSuite();
await runSuite();
await runLanguageSuite();
console.log('UI_LANGUAGES_PASS: five locales, live switching, picker IPC, prepared image and confirmation preserved, busy locks, raw diagnostics');
console.log('UI_PASS: production wizard + App, one slide, IPC guards, navigation, busy locks, real result state');
const mutations = [
  ['wrong slide branch', 'InstallWizard', 'v-if="current===0"', 'v-if="true"'],
  ['busy handler removed', 'InstallWizard', 'if (locked ||', 'if ('],
  ['busy handler reversed', 'InstallWizard', 'if (locked ||', 'if (!locked ||'],
  ['lower bound removed', 'InstallWizard', ' || next < 0', ''],
  ['primary readiness removed', 'InstallWizard', ':disabled="primaryDisabled"', ':disabled="false"'],
  ['duplicate primary action', 'InstallWizard', '<div class="wizard-heading">', '<div class="wizard-heading"><button class="primary">duplicate</button>'],
  ['download digest guard removed', 'InstallWizard', '!!selectedRelease.value?.sha256', '!!selectedRelease.value'],
  ['stale image retained', 'App', "error.value=null;preparedImage.value=null;notice.value='';", "error.value=null;notice.value='';"],
  ['failed image accepted', 'App', 'else if(accept(r)&&r.data)', 'else if(r.data)'],
  ['image success does not advance', 'App', 'preparedImage.value=r.data;wizardStep.value=4;', 'preparedImage.value=r.data;'],
  ['image source reset removed', 'InstallWizard', "source.value=next;emit('clear-image');", 'source.value=next;'],
  ['image version reset removed', 'InstallWizard', "if(source.value==='official')emit('clear-image');", ''],
  ['install readiness removed', 'InstallWizard', "||(props.mode==='install'?!props.preparedImage:!restoreReady.value)", ''],
  ['active step reversed', 'InstallWizard', "current === index ? 'step'", "current !== index ? 'step'"],
  ['busy prop disconnected', 'App', ':busy="busy||confirmOpen||!!writePlan"', ':busy="false"'],
  ['backup device guard removed', 'App', 'async function backup(){if(!canRead.value)return false;', 'async function backup(){'],
  ['backup compression status removed','App',"'read-compress':'eMMC 읽기·압축 중',",''],
  ['failed backup advances', 'App', "if((backupResult.value?.verified&&backupResult.value?.kind==='FULL')||await backup())wizardStep.value=3;", 'await backup();wizardStep.value=3;'],
  ['backup success does not advance', 'App', "if((backupResult.value?.verified&&backupResult.value?.kind==='FULL')||await backup())wizardStep.value=3;", "if((backupResult.value?.verified&&backupResult.value?.kind==='FULL')||await backup()){}"],
  ['backup reuse removed', 'App', "(backupResult.value?.verified&&backupResult.value?.kind==='FULL')||await backup()", 'await backup()'],
  ['failed Loader inspection advances', 'App', 'if(await inspect())wizardStep.value=2;', 'await inspect();wizardStep.value=2;'],
  ['Loader success does not advance', 'App', 'if(await inspect())wizardStep.value=2;', 'await inspect();'],
  ['readiness gate removed', 'App', 'canInspect.value&&readyObservation.value', 'canInspect.value'],
  ['upload success does not advance', 'App', "if(ready&&canRead.value&&tab.value==='prepare')wizardStep.value=2;", 'if(ready&&canRead.value){}'],
  ['post-upload port check removed', 'App', '&&device.value?.location===location&&matchesObservation', '&&matchesObservation'],
  ['descriptor incorrectly treated as readiness', 'App', "if(ready&&canRead.value&&tab.value==='prepare')wizardStep.value=2;", 'if(ready&&canRead.value&&device.value?.mode==="Loader")wizardStep.value=2;'],
  ['positive readiness removed','App','value?.ready===true&&value.instance_id===id','!!value&&value.instance_id===id'],
  ['positive readiness reversed','App','value?.ready===true','value?.ready===false'],
  ['preparation confirmation removed', 'App', 'if(!canPrepare.value||!confirmed.value)return;', 'if(!canPrepare.value)return;'],
  ['sidebar busy guard removed', 'App', 'async function navigate(id:string){if(busy.value||writePlan.value)return;', 'async function navigate(id:string){'],
  ['stale backup retained', 'App', 'observation.value=null;backupResult.value=null;', 'observation.value=null;'],
  ['automatic release loading removed','InstallWizard',"if(visible)emit('enter-official');",'if(visible){}'],
  ['automatic entry guard reversed','InstallWizard',"if(visible)emit('enter-official');","if(!visible)emit('enter-official');"],
  ['release cache guard removed','App',"if(releaseState.value==='idle')void loadReleases();",'void loadReleases();'],
  ['release loading guard removed','InstallWizard','&&!props.releasesLoading&&!!selectedRelease','&&!!selectedRelease'],
  ['confirmation removed','App',"&&writeConfirm.value==='ok'",''],
  ['confirmation reversed','App',"&&writeConfirm.value==='ok'","&&writeConfirm.value!=='ok'"],
  ['write handler guard removed','App','if(!canExecute.value)return;',''],
  ['stale port plan retained','App',"writePlan.value=null;writeConfirm.value='';gptResult.value=null;writeResult.value=null;",'gptResult.value=null;writeResult.value=null;'],
  ['GPT repair guard removed','App',"if(operation==='gpt-repair'&&!gptResult.value?.repairable)return;",''],
  ['restore guard removed','App',"if(['restore','restore-archive'].includes(operation)&&!canRestore.value)return;",''],
  ['advanced U-Boot panel removed','App','data-advanced="uboot"','v-if="false" data-advanced="uboot"'],
  ['app confirmation removed','App',"||appConfirm.value!=='K11C'",''],
  ['app confirmation reversed','App',"||appConfirm.value!=='K11C'","||appConfirm.value==='K11C'"],
  ['app target change retained','App','if(appPlan.value)cancelApp();','if(false)cancelApp();'],
  ['app false success','App','if(accept(r)){resetCompletedWork();accept(r);appResult.value=r.data;}','appResult.value={operation:"reinstall"};'],
  ['successful storage reset removed','App','resetCompletedWork();accept(r);writeResult.value=r.data;','writeResult.value=r.data;'],
  ['successful app reset removed','App','resetCompletedWork();accept(r);appResult.value=r.data;','appResult.value=r.data;'],
  ['completed wizard position retained','App',"error.value=null;wizardStep.value=0;wizardSession.value++;",'error.value=null;wizardSession.value++;'],
  ['completed address retained','App',"haToken.value='';haAddress.value='http://homeassistant.local:8123';","haToken.value='';"],
];
const report = [];
if (process.argv.includes('--mutation')) {
  const readinessMutations=[
    ['advanced preparation handler disconnected','@click="connectAdvanced"','@click="refresh"'],
    ['advanced device binding guard removed',"if(tab.value!=='advanced'||!canInspect.value||writePlan.value||confirmOpen.value)return;","if(tab.value!=='advanced'||!device.value||writePlan.value||confirmOpen.value)return;"],
    ['advanced Loader inspection omitted',"if(device.value.mode==='Loader'||readyObservation.value)await inspect();","if(device.value.mode==='Loader'||readyObservation.value){}"],
    ['advanced preparation advances wizard',"if(ready&&canRead.value&&tab.value==='prepare')wizardStep.value=2;",'if(ready&&canRead.value)wizardStep.value=2;'],
    ['scope visible before inspection','<template v-if="readyObservation"><label for="backup-kind">','<template><label for="backup-kind">'],
    ['uninspected disk presented as non-HAOS',"v-if=\"readyObservation&&observation?.os!=='HAOS'\"",'v-if="true"'],
  ];
  for(const[name,before,after]of readinessMutations){
    assert.equal(sources.App.split(before).length-1,1,name);
    try{await runAdvancedReadinessSuite({App:sources.App.replace(before,after)});}catch(error){if(error.code!=='ERR_ASSERTION')throw error;report.push({name,killed:true});console.log(`KILLED: ${name}`);continue;}
    throw Error(`SURVIVED: ${name}`);
  }
  await runAdvancedReadinessSuite();
  const taskMutations=[
    ['restore entry refresh omitted','App',"if(source==='backup')await loadBackups();",''],
    ['restore entry refresh reversed','App',"if(source==='backup')await loadBackups();","if(source!=='backup')await loadBackups();"],
    ['advanced restore backup bypass omitted','App',"tab.value==='advanced'&&['restore','restore-archive'].includes(operation)",'false'],
    ['raw display name omitted','App','{{b.display_name??b.id}}','{{b.id}}'],
    ['catalog deduplication omitted','App','if(busy.value||catalogLoading.value)return;\n const generation','if(busy.value)return;\n const generation'],
    ['backup badge removed','App','class="backup-badge"','class="missing-badge"'],
    ['backup badge guessed from filename','App',"return ['FULL','BOOT','HAOS'].includes(row.kind)?row.kind:t('미확인');","return row.id.split('-')[0];"],
    ['backup notes not localized','App','{{t(backupHint(b))}}','{{backupHint(b)}}'],
    ['invalid row handler allowed','App',"if(!row?.restorable||!['FULL','BOOT','HAOS'].includes(row.kind))return;",''],
    ['row busy lock removed','App',"function selectRestore(id:string){\n if(busy.value||catalogLoading.value||writePlan.value)return;","function selectRestore(id:string){\n if(catalogLoading.value||writePlan.value)return;"],
    ['row plan lock removed','App',"function selectRestore(id:string){\n if(busy.value||catalogLoading.value||writePlan.value)return;","function selectRestore(id:string){\n if(busy.value||catalogLoading.value)return;"],
    ['task chooser bypassed','App',"v-if=\"advancedBackupMode===null\"",'v-if="false"'],
    ['task form selection reversed','App','v-if="advancedBackupMode===\'backup\'" data-backup-pane="backup"','v-if="advancedBackupMode!==\'backup\'" data-backup-pane="backup"'],
    ['task busy lock removed','App','if(busy.value||writePlan.value||confirmOpen.value)return;','if(writePlan.value||confirmOpen.value)return;'],
    ['task pending plan lock removed','App','if(busy.value||writePlan.value||confirmOpen.value)return;','if(busy.value||confirmOpen.value)return;'],
    ['task lock reversed','App','if(busy.value||writePlan.value||confirmOpen.value)return;','if(!busy.value||writePlan.value||confirmOpen.value)return;'],
    ['wrong task backup allowed','App',"if(tab.value==='advanced'&&advancedBackupMode.value!=='backup')return false;",''],
    ['task completion retained','App',"backupKind.value='FULL';advancedBackupMode.value=null;","backupKind.value='FULL';"],
    ['unknown archive kind allowed','App',"&&['FULL','BOOT','HAOS'].includes(restoreInfo.value.kind)",''],
    ['advanced scope removed','App','<RestoreSummary :info="restoreInfo"/>',''],
    ['confirmation scope removed','App','<RestoreSummary v-if="[\'restore\',\'restore-archive\'].includes(writePlan.operation)" :info="restoreInfo"/>',''],
    ['wizard file scope removed','InstallWizard','<RestoreSummary :info="restoreInfo"/>',''],
    ['wizard review scope removed','InstallWizard','<RestoreSummary v-if="mode===\'restore\'" :info="restoreInfo"/>',''],
    ['all scopes falsely FULL','RestoreSummary',')[props.info?.kind]',")[\"FULL\"]"],
    ['scope descriptions not localized','RestoreSummary','{{t(scope.detail)}}','{{scope.detail}}'],
  ];
  for(const[name,file,before,after]of taskMutations){
    assert.equal(sources[file].split(before).length-1,1,name);
    try{await runBackupTaskSuite({[file]:sources[file].replace(before,after)});}catch(error){if(error.code!=='ERR_ASSERTION')throw error;report.push({name,killed:true});console.log(`KILLED: ${name}`);continue;}
    throw Error(`SURVIVED: ${name}`);
  }
  const archiveMutations=[
    ['wizard requests partial backup','App',"kind:tab.value==='prepare'?'FULL':backupKind.value","kind:tab.value==='prepare'?'BOOT':backupKind.value"],
    ['recovery reuse omitted','App','operation,source,recovery}','operation,source,recovery:\'\'}'],
    ['advanced scope restriction removed','App','v-if="observation?.os===\'HAOS\'" value="HAOS"','value="HAOS"'],
    ['restore choice resets to install','App',"wizardMode.value=mode;clearImage();","wizardMode.value='install';clearImage();"],
    ['restore import wrong next step','App',"r.data.kind==='FULL')wizardStep.value=4;","r.data.kind==='FULL')wizardStep.value=3;"],
  ];
  for(const[name,file,before,after]of archiveMutations){
    assert.equal(sources[file].split(before).length-1,1,name);
    try{await runArchiveSuite({[file]:sources[file].replace(before,after)});}catch(error){if(error.code!=='ERR_ASSERTION')throw error;report.push({name,killed:true});console.log(`KILLED: ${name}`);continue;}
    throw Error(`SURVIVED: ${name}`);
  }
  for (const [name, file, before, after] of mutations) {
    assert.equal(sources[file].split(before).length - 1, 1, `${name}: unique source anchor`);
    try { await runSuite({ [file]: sources[file].replace(before, after) }); }
    catch (error) {
      if (error.code !== 'ERR_ASSERTION') throw error;
      report.push({ name, killed: true, assertion: error.message }); console.log(`KILLED: ${name}`); continue;
    }
    throw new Error(`SURVIVED: ${name}`);
  }
  await runSuite();
  const factoryMutations=[
    ['factory readiness removed','if(!canFactory.value)return;','if(false)return;'],
    ['factory readiness reversed','if(!canFactory.value)return;','if(canFactory.value)return;'],
    ['factory source guard removed',"if(busy.value||writePlan.value||advancedBackupMode.value!=='restore'||restoreSource.value!=='manufacturer')return;",'if(false)return;'],
    ['factory source change retains selection',"restoreSource.value=source;restoreId.value='';factoryImage.value=null;error.value=null;","restoreSource.value=source;restoreId.value='';error.value=null;"],
    ['factory chooser busy guard removed',"if(busy.value||writePlan.value||confirmOpen.value||advancedBackupMode.value!=='restore')return;",'if(false)return;'],
    ['factory execution routes to backup','factory?\'factory_execute\':\'storage_execute\'',"'storage_execute'"],
  ];
  for(const[name,before,after]of factoryMutations){
    assert.equal(sources.App.split(before).length-1,1,name);
    try{await runFactorySuite({App:sources.App.replace(before,after)});}catch(error){if(error.code!=='ERR_ASSERTION')throw error;report.push({name,killed:true});console.log(`KILLED: ${name}`);continue;}
    throw Error(`SURVIVED: ${name}`);
  }
  await runFactorySuite();
  const languageMutations=[
    ['completed child selection retained','App',':key="wizardSession"',''],
    ['theme control disconnected','App',"setTheme(($event.target as HTMLSelectElement).value)","setTheme('dark')"],
    ['theme change discards image','App',"setTheme(($event.target as HTMLSelectElement).value)","setTheme(($event.target as HTMLSelectElement).value); clearImage()"],
    ['theme change discards confirmation','App',"setTheme(($event.target as HTMLSelectElement).value)","setTheme(($event.target as HTMLSelectElement).value); writeConfirm=''"],
    ['language control disconnected','App',"setLocale(($event.target as HTMLSelectElement).value)","setLocale('en')"],
    ['language change discards image','App',"setLocale(($event.target as HTMLSelectElement).value)","setLocale(($event.target as HTMLSelectElement).value); clearImage()"],
    ['native picker locale omitted','App',"version?{version}:{locale:locale.value}","version?{version}:{}"],
    ['notice translation removed','App','{{t(progressLabel)}}','{{progressLabel}}'],
    ['wizard primary translation removed','InstallWizard','{{t(primaryLabel)}}','{{primaryLabel}}'],
    ['write confirmation translation removed','App',"t('{operation} 확인',{operation:t(writeNames[writePlan.operation])})","writeNames[writePlan.operation]"],
  ];
  for(const[name,file,before,after]of languageMutations){
    assert.equal(sources[file].split(before).length-1,1,name);
    try{await runLanguageSuite({[file]:sources[file].replace(before,after)});}catch(error){if(error.code!=='ERR_ASSERTION')throw error;report.push({name,killed:true,assertion:error.message});console.log(`KILLED: ${name}`);continue;}
    throw Error(`SURVIVED: ${name}`);
  }
  await runLanguageSuite();
  const spacingMutations=[
    ['driver spacing scope removed','App', 'panel driver-settings', 'panel'],
    ['driver spacing removed','settingsCss', 'margin-top: 20px;', 'margin-top: 0;'],
    ['driver spacing reversed','settingsCss', 'margin-top: 20px;', 'margin-top: -20px;'],
    ['driver spacing only when expanded','settingsCss', '.driver-settings > details + .secondary', '.driver-settings > details[open] + .secondary'],
  ];
  for(const[name,file,before,after]of spacingMutations){
    const source=file==='settingsCss'?settingsCss:sources[file];
    // The stylesheet also has a grid margin; mutate only the scoped driver rule.
    const original=file==='settingsCss'&&before.startsWith('margin-')?'.driver-settings > details + .secondary { '+before+' }':before;
    const replacement=original===before?after:'.driver-settings > details + .secondary { '+after+' }';
    assert.equal(source.split(original).length-1,1,name);
    try{await runLanguageSuite({[file]:source.replace(original,replacement)});}catch(error){if(error.code!=='ERR_ASSERTION')throw error;report.push({name,killed:true,assertion:error.message});console.log(`KILLED: ${name}`);continue;}
    throw Error(`SURVIVED: ${name}`);
  }
  await runLanguageSuite();
}
assert.equal(readFileSync('src/typography.css','utf8'),settingsCss);
for (const [name, source] of Object.entries(sources)) assert.equal(readFileSync(`src/${name}.vue`, 'utf8'), source);
writeFileSync(process.argv.includes('--mutation') ? 'test-results/ui-mutations.json' : 'test-results/ui-verification.json', JSON.stringify({ passed: true, production_sfc: true, mocked_boundary: 'Tauri IPC only; Vue custom renderer', mutations: report, source_unchanged: true }, null, 2));
console.log(`UI_VERIFIED mutations=${report.length}; production source unchanged`);
