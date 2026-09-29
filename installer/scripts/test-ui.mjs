import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, mkdirSync, mkdtempSync } from 'node:fs';
import path from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { parse, compileScript } from '@vue/compiler-sfc';
import ts from 'typescript';
import { createRenderer, nextTick } from 'vue';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
process.chdir(root);
mkdirSync('test-results', { recursive: true });
const out = mkdtempSync(path.join(root, 'test-results/ui-'));
const sources = Object.fromEntries(['App', 'InstallWizard'].map(name => [name, readFileSync(`src/${name}.vue`, 'utf8')]));
// Compile the production SFCs, not a duplicate state machine. Only the IPC boundary
// is substituted. Vue renders slots, directives and event handlers in the test host.
const fixture = path.join(out, 'ipc.mjs');
writeFileSync(fixture, `export const native=false; export const calls=[]; export const replies={};
export async function call(command,args){calls.push({command,args}); const r=replies[command]; return typeof r==='function'?await r():r??{ok:false,error:{code:'DEVICE_GONE',detail:'Test IPC boundary'}};}
`);
const ipc = await import(pathToFileURL(fixture));
let serial = 0;
async function compile(name, source, wizardUrl) {
  const { descriptor, errors } = parse(source, { filename: `${name}.vue` });
  assert.equal(errors.length, 0);
  const compiled = compileScript(descriptor, { id: name, inlineTemplate: true, templateOptions: { compilerOptions: { hoistStatic: false } } });
  let js = ts.transpileModule(compiled.content, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
  js = js.replace(/import ['"]\.\/wizard\.css['"];?/, '')
    .replace(/from ['"]\.\/api['"]/, `from ${JSON.stringify(pathToFileURL(fixture).href)}`);
  if (wizardUrl) js = js.replace(/from ['"]\.\/InstallWizard\.vue['"]/, `from ${JSON.stringify(wizardUrl)}`);
  const output = path.join(out, `${name}-${serial++}.mjs`);
  writeFileSync(output, js);
  return { url: pathToFileURL(output).href, component: (await import(pathToFileURL(output))).default };
}
function element(type, text = '') {
  return { type, text, props: {}, children: [], parent: null, style: {}, addEventListener() {}, removeEventListener() {} };
}
const renderer = createRenderer({
  createElement: type => element(type), createText: text => element('#text', text), createComment: text => element('#comment', text),
  insert(child, parent, anchor = null) { if (child.parent) this.remove(child); child.parent = parent; const i = anchor ? parent.children.indexOf(anchor) : -1; i < 0 ? parent.children.push(child) : parent.children.splice(i, 0, child); },
  remove(child) { if (child.parent) child.parent.children.splice(child.parent.children.indexOf(child), 1); child.parent = null; },
  setText(node, text) { node.text = text; }, setElementText(node, text) { node.children = []; node.text = text; },
  parentNode: node => node.parent, nextSibling: node => node.parent?.children[node.parent.children.indexOf(node) + 1] ?? null,
  patchProp(node, key, old, value) { node.props[key] = value; if (key === 'style' && value && typeof value === 'object') Object.assign(node.style, value); },
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
function futureDisabled(root, name) {
  const buttons = nodes(root).filter(n => visible(n) && n.props['data-storage-action'] === name);
  assert.equal(buttons.length, 1, name);
  assert.equal(disabled(buttons[0]), true, `${name} disabled`);
}
const device = (mode, id = 'K11C-TEST') => ({ instance_id: id, vid: 0x2207, pid: 0x350a, binding: true, mode, location: 'TEST-PORT' });
async function runSuite(overrides = {}) {
  ipc.calls.length = 0;
  for (const key of Object.keys(ipc.replies)) delete ipc.replies[key];
  const wizard = await compile('InstallWizard', overrides.InstallWizard ?? sources.InstallWizard);
  const app = await compile('App', overrides.App ?? sources.App, wizard.url);
  const host = element('root'); const instance = renderer.createApp(app.component); instance.mount(host); await settle();
  try {
    stage(host, 0);
    assert.equal(text(primary(host)),'장치 준비 후 다음');
    assert.equal(disabled(primary(host)), true);
    await click(primary(host),true);stage(host,0);
    assert.equal(disabled(find(host, 'button', '이전')), true);
    await click(find(host, 'button', '이전'), true); stage(host, 0);
    await click(find(host, 'button', '02백업')); stage(host, 1);
    assert.equal(disabled(primary(host)), true);
    await click(primary(host), true);stage(host,1);
    assert.equal(ipc.calls.length, 0, 'No backup IPC without a device');
    let releaseFinish;
    ipc.replies.image_releases=()=>new Promise(resolve=>{releaseFinish=resolve;});
    await click(find(host, 'button', '03이미지 선택')); stage(host, 2);
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
    await click(find(host, 'button', '04설치')); stage(host, 3);
    futureDisabled(host, 'install');
    assert.match(text(host), /연결 안 됨/);
    assert.match(text(host), /백업 없음/);
    assert.match(text(host), /이미지 선택 안 됨/);
    assert.doesNotMatch(text(host), /백업 저장·검증 완료|설치 완료/);
    assert.deepEqual(ipc.calls.map(c=>c.command),['image_releases'],'Only metadata is fetched automatically');
    releaseFinish({ok:false,error:{code:'IMAGE_NETWORK',detail:'offline'}});await settle();
    await click(find(host, 'button', '이전')); stage(host, 2);
    assert.equal(disabled(primary(host)),false); // selection survives navigation
    await click(find(host, 'button', '설정'));
    assert.equal(nodes(host).filter(n => visible(n) && n.props['data-step'] !== undefined).length, 0);
    await click(find(host, 'button', '설치·복구')); stage(host, 2);
    await click(find(host,'button','공식 다운로드Home Assistant OS 버전 선택'));
    assert.match(text(host),/다운로드 서버에 연결하지 못했습니다/);
    assert.equal(ipc.calls.filter(c=>c.command==='image_releases').length,1,'Failure does not auto-retry on re-entry');
    await click(find(host,'button','다시 시도'));
    assert.equal(ipc.calls.filter(c=>c.command==='image_releases').length,2,'Explicit retry');
    await click(find(host,'button','설정'));
    releaseFinish({ok:true,data:[{version:'99.7',filename:'haos_generic-aarch64-99.7.img.xz',size:400000000,sha256:'a'.repeat(64)},{version:'99.6',filename:'older.img.xz',size:1,sha256:null}]});await settle();
    await click(find(host,'button','설치·복구'));stage(host,2);
    await click(find(host,'button','01연결'));
    await click(find(host,'button','03이미지 선택'));
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
    const downloading=primary(host).props.onClick({});await settle();stage(host,2);
    for(const label of ['이전','04설치','설정']) assert.equal(disabled(find(host,'button',label)),true);
    assert.equal(disabled(primary(host)),true);
    const busyCalls=ipc.calls.length;await click(primary(host),true);assert.equal(ipc.calls.length,busyCalls);
    assert.doesNotMatch(text(host),/이미지 준비 완료/);
    imageFinish({ok:true,data:{filename:'haos_generic-aarch64-99.7.img.xz',path:'C:/images/verified.img',bytes:1000000000,sha256:'b'.repeat(64),source_sha256:'a'.repeat(64),verification:'github_sha256'}});await downloading;await settle();
    stage(host,3);assert.match(text(host),/haos_generic-aarch64-99.7.img.xz/);futureDisabled(host,'install');
    assert.equal(ipc.calls.some(c=>c.command==='storage_plan'||c.command==='storage_execute'),false,'Image preparation never starts installation');
    await click(find(host,'button','03이미지 선택'));
    assert.match(text(host),/이미지 준비 완료/);assert.match(text(host),/공식 SHA-256 · 파티션/);
    const reusedCalls=ipc.calls.length;await click(primary(host));stage(host,3);assert.equal(ipc.calls.length,reusedCalls,'Ready image advances without downloading again');
    await click(find(host,'button','03이미지 선택'));
    nodes(host).find(n=>n.props.id==='haos-version').props.onChange({target:{value:'99.6'}});await settle();
    assert.doesNotMatch(text(host),/이미지 준비 완료/);assert.equal(disabled(primary(host)),true,'Changing version invalidates old image');
    nodes(host).find(n=>n.props.id==='haos-version').props.onChange({target:{value:'99.7'}});await settle();
    ipc.replies.image_download={ok:false,data:{filename:'UNVERIFIED.img'},error:{code:'IMAGE_HASH',detail:'checksum mismatch'}};
    await click(primary(host));stage(host,2);assert.doesNotMatch(text(host),/이미지 준비 완료/);assert.match(text(host),/이미지의 SHA-256이 일치하지 않습니다/);
    await click(find(host,'button','04설치'));assert.match(text(host),/이미지 선택 안 됨/);
    await click(find(host,'button','03이미지 선택'));
    await click(find(host,'button','내 PC에서 선택다운로드한 HAOS 이미지 사용'));
    ipc.replies.image_select={ok:true,data:{filename:'local.img',path:'C:/images/local.img',bytes:2000000000,verification:'local_structure'}};
    await click(primary(host));stage(host,3);
    await click(find(host,'button','03이미지 선택'));assert.match(text(host),/공식 해시 대조 안 됨/);
    ipc.replies.image_select={ok:true,data:null};await click(find(host,'button','파일 변경'));stage(host,2);assert.doesNotMatch(text(host),/이미지 준비 완료/);
    await click(primary(host));stage(host,2); // Picker cancelled without an existing image.
    ipc.replies.image_select={ok:true,data:{filename:'local.img',bytes:2000000000,verification:'local_structure'}};
    await click(primary(host));stage(host,3);await click(find(host,'button','03이미지 선택'));
    await click(find(host,'button','공식 다운로드Home Assistant OS 버전 선택'));
    assert.doesNotMatch(text(host),/이미지 준비 완료/,'Changing source invalidates the old image');
    ipc.replies.image_download=()=>new Promise(resolve=>{imageFinish=resolve;});
    const cancelling=primary(host).props.onClick({});await settle();
    ipc.replies.image_cancel={ok:true,data:{requested:true}};
    await click(find(host,'button','취소'));assert.equal(disabled(find(host,'button','취소')),true);
    imageFinish({ok:false,error:{code:'IMAGE_CANCELLED',detail:'cancelled'}});await cancelling;await settle();stage(host,2);assert.match(text(host),/이미지 준비를 취소했습니다/);
    assert.deepEqual(ipc.calls.filter(c=>c.command==='image_download').map(c=>c.args),[{version:'99.7'},{version:'99.7'},{version:'99.7'}]);
    await click(find(host, 'button', '01연결')); stage(host, 0);

    ipc.replies.preflight = { ok: true, data: { driver: { installed: true }, devices: [device('Maskrom')] } };
    await click(find(host, 'button', '연결 다시 확인'));
    await click(primary(host));
    assert.equal(disabled(find(host, 'button', '준비 시작')), true);
    await click(find(host, 'button', '준비 시작'), true);
    assert.equal(ipc.calls.some(c => c.command === 'prepare_device'), false);
    // Directly invoke a disabled step to ensure the handler itself honors locks.
    assert.equal(disabled(find(host, 'button', '02백업')), true);
    await click(find(host, 'button', '02백업'), true); stage(host, 0);
    await click(find(host, 'button', '취소'));stage(host,0);

    const preflight=rows=>({ok:true,data:{driver:{installed:true},devices:rows}});
    const prepared={ok:true,data:{device:device('Loader'),observation:{bytes:32000000000,sectors:62500000}}};
    // Only successful upload plus same-port Loader re-enumeration may advance.
    for(const outcome of ['upload-failed','scan-failed','wrong-port','still-maskrom','ready']){
      ipc.replies.preflight=preflight([device('Maskrom')]);await click(find(host,'button','연결 다시 확인'));
      await click(primary(host));
      nodes(host).find(n=>n.type==='input'&&n.props.type==='checkbox').props['onUpdate:modelValue'](true);await settle();
      ipc.replies.prepare_device=outcome==='upload-failed'?{ok:false,error:{code:'USB_UPLOAD'}}:prepared;
      ipc.replies.preflight=outcome==='scan-failed'?{ok:false,error:{code:'DEVICE_GONE'}}:preflight([{...device(outcome==='still-maskrom'?'Maskrom':'Loader'),location:outcome==='wrong-port'?'WRONG-PORT':'TEST-PORT'}]);
      await click(find(host,'button','준비 시작'));stage(host,outcome==='ready'?1:0);
    }
    await click(find(host,'button','01연결'));

    ipc.replies.preflight.data.devices = [device('Loader')];
    await click(find(host, 'button', '연결 다시 확인'));
    ipc.replies.inspect_device={ok:false,error:{code:'USB_READ'}};
    await click(primary(host));stage(host,0);
    assert.equal(text(primary(host)),'장치 확인 후 다음');
    ipc.replies.inspect_device = { ok: true, data: { bytes: 32000000000, sectors: 62500000 } };
    const uploads=ipc.calls.filter(c=>c.command==='prepare_device').length;
    await click(primary(host));stage(host,1);
    assert.equal(ipc.calls.filter(c=>c.command==='prepare_device').length,uploads,'Loader connection does not re-upload');
    ipc.replies.backup_device={ok:false,error:{code:'BACKUP_VERIFY'}};
    await click(primary(host));stage(host,1);assert.doesNotMatch(text(host),/백업 저장·검증 완료/);
    let finish;
    ipc.replies.backup_device = () => new Promise(resolve => { finish = resolve; });
    const pending = primary(host).props.onClick({}); await settle();stage(host,1);
    for (const label of ['이전', '03이미지 선택', '설정']) assert.equal(disabled(find(host, 'button', label)), true, `Busy locks ${label}`);
    assert.equal(disabled(primary(host)),true);
    await click(find(host, 'button', '03이미지 선택'), true); stage(host, 1);
    const backupCalls=ipc.calls.length;await click(primary(host), true); stage(host, 1);assert.equal(ipc.calls.length,backupCalls);
    await click(find(host, 'button', '설정'), true); stage(host, 1);
    finish({ ok: true, data: { path: 'C:/backup/test', gpt: { healthy: true } } }); await pending; await settle();
    stage(host,2);
    await click(find(host,'button','02백업'));assert.match(text(host), /백업 저장·검증 완료/);
    const reusedBackupCalls=ipc.calls.length;await click(primary(host));stage(host,2);assert.equal(ipc.calls.length,reusedBackupCalls,'Same-device verified backup is reused');
    await click(find(host, 'button', '04설치')); stage(host, 3);
    assert.match(text(host), /eMMC · 29.8 GiB/);
    assert.match(text(host), /C:\/backup\/test/);
    futureDisabled(host, 'install');
    ipc.replies.preflight.data.devices=[{...device('Loader'),location:'NEW-PORT'}];
    await click(find(host,'button','연결 다시 확인'));assert.match(text(host),/백업 없음/,'Same ID on another port invalidates backup');
    ipc.replies.preflight.data.devices = [device('Loader', 'OTHER-DEVICE')];
    await click(find(host, 'button', '연결 다시 확인'));
    assert.doesNotMatch(text(host), /C:\/backup\/test/);
    assert.match(text(host), /백업 없음/);
    assert.deepEqual(ipc.calls.filter(c => c.command === 'backup_device').map(c => c.args), [{ instanceId: 'K11C-TEST' },{ instanceId: 'K11C-TEST' }]);
    await click(find(host,'button','고급 기능'));
    for(const label of ['저장장치 정보','U-Boot만 업데이트','부팅 영역 백업·복원','GPT 검사·복구'])find(host,'h3',label);
    for(const name of ['restore','gpt-repair'])futureDisabled(host,name);
    const beforeAdvanced=ipc.calls.length;
    for(const name of ['restore','gpt-repair'])await click(nodes(host).find(n=>visible(n)&&n.props['data-storage-action']===name),true);
    assert.equal(ipc.calls.length,beforeAdvanced,'Unready advanced actions cannot invoke IPC');
    ipc.replies.backup_device={ok:true,data:{path:'C:/backup/advanced',gpt:{healthy:true}}};
    await click(find(host,'button','백업 시작'));
    assert.equal(ipc.calls.at(-1).command,'backup_device');
    assert.equal(ipc.calls.at(-1).args.instanceId,'OTHER-DEVICE');
    assert.match(text(host),/백업 저장·검증 완료/);assert.match(text(host),/C:\/backup\/advanced/);
    await click(find(host,'button','설치·복구'));stage(host,3);await click(find(host,'button','고급 기능'));
    const planData=(operation)=>({plan_id:'c'.repeat(64),operation,device:device('Loader','OTHER-DEVICE'),identity:{sectors:62500000},backup_path:'C:/backup/auto',ranges:[{label:'U-Boot',lba:64,bytes:9687040}],erases_user_data:operation==='install'});
    ipc.replies.storage_plan=()=>({ok:true,data:planData('uboot')});
    await click(find(host,'button','U-Boot 업데이트'));
    assert.equal(ipc.calls.at(-1).command,'storage_plan');
    assert.deepEqual(ipc.calls.at(-1).args,{instanceId:'OTHER-DEVICE',location:'TEST-PORT',operation:'uboot',source:''});
    assert.match(text(host),/U-Boot 업데이트 확인/);assert.match(text(host),/C:\/backup\/auto/);
    futureDisabled(host,'execute');
    const beforeConfirmation=ipc.calls.length;await click(find(host,'button','기록 시작'),true);assert.equal(ipc.calls.length,beforeConfirmation);
    const input=nodes(host).find(n=>n.props.id==='write-confirm');input.props.onInput({target:{value:'wrong'}});await settle();futureDisabled(host,'execute');
    await click(find(host,'button','취소'));assert.equal(ipc.calls.some(c=>c.command==='storage_execute'),false);
    await click(find(host,'button','U-Boot 업데이트'));
    nodes(host).find(n=>n.props.id==='write-confirm').props.onInput({target:{value:'K11C'}});await settle();
    let writeFinish;ipc.replies.storage_execute=()=>new Promise(resolve=>{writeFinish=resolve;});
    const writing=find(host,'button','기록 시작').props.onClick({});await settle();
    assert.equal(disabled(find(host,'button','설치·복구')),true);
    assert.doesNotMatch(text(host),/U-Boot 업데이트 완료/);
    writeFinish({ok:false,error:{code:'WRITE_VERIFY',detail:'bad readback'}});await writing;await settle();
    assert.doesNotMatch(text(host),/U-Boot 업데이트 완료/);assert.match(text(host),/읽기 대조에 실패/);
    await click(find(host,'button','U-Boot 업데이트'));
    nodes(host).find(n=>n.props.id==='write-confirm').props.onInput({target:{value:'K11C'}});await settle();
    ipc.replies.storage_execute={ok:true,data:{verified:true,operation:'uboot',backup_path:'backup',journal:'journal'}};
    await click(find(host,'button','기록 시작'));assert.match(text(host),/U-Boot 업데이트 완료/);
    assert.deepEqual(ipc.calls.filter(c=>c.command==='storage_execute').map(c=>c.args),[{planId:'c'.repeat(64),confirmed:true},{planId:'c'.repeat(64),confirmed:true}]);
    // A fresh preview must be invalidated by a port change, even if instance ID
    // happens to remain the same. Polling can deliver precisely this transition.
    await click(find(host,'button','U-Boot 업데이트'));
    ipc.replies.preflight.data.devices=[{...device('Loader','OTHER-DEVICE'),location:'NEW-PORT'}];
    await click(find(host,'button','연결 다시 확인'));assert.doesNotMatch(text(host),/U-Boot 업데이트 확인/);
    ipc.replies.preflight.data.devices=[device('Loader','OTHER-DEVICE')];await click(find(host,'button','연결 다시 확인'));
    ipc.replies.gpt_check={ok:true,data:{gpt:{healthy:false},repairable:false}};
    await click(find(host,'button','GPT 검사'));futureDisabled(host,'gpt-repair');
    ipc.replies.gpt_check={ok:true,data:{gpt:{healthy:false},repairable:true}};await click(find(host,'button','GPT 검사'));
    ipc.replies.storage_plan={ok:true,data:planData('gpt-repair')};await click(find(host,'button','GPT 복구'));assert.equal(ipc.calls.at(-1).args.operation,'gpt-repair');await click(find(host,'button','취소'));
    ipc.replies.backup_catalog={ok:true,data:[{id:'k11c-backup',restorable:true,device:device('Loader','OTHER-DEVICE')}]};await click(find(host,'button','목록 새로고침'));
    nodes(host).find(n=>n.props.id==='restore-backup').props.onChange({target:{value:'k11c-backup'}});await settle();
    ipc.replies.storage_plan={ok:true,data:planData('restore')};await click(find(host,'button','백업 복원'));assert.equal(ipc.calls.at(-1).args.source,'k11c-backup');await click(find(host,'button','취소'));
    await click(find(host,'button','설치·복구'));await click(find(host,'button','03이미지 선택'));
    await click(find(host,'button','내 PC에서 선택다운로드한 HAOS 이미지 사용'));
    ipc.replies.image_select={ok:true,data:{filename:'haos.img',sha256:'d'.repeat(64),bytes:1000000000}};await click(primary(host));stage(host,3);
    await click(find(host,'button','04설치'));assert.equal(disabled(find(host,'button','설치 준비')),false);
    ipc.replies.storage_plan={ok:true,data:planData('install')};await click(find(host,'button','설치 준비'));
    assert.equal(ipc.calls.at(-1).args.source,'d'.repeat(64));assert.match(text(host),/기존 OS·HA 설정·사용자 데이터가 삭제됩니다/);futureDisabled(host,'execute');
    await click(find(host,'button','취소'));
    ipc.replies.preflight=preflight([]);await click(find(host,'button','연결 다시 확인'));
    await click(find(host,'button','고급 기능'));
    const missingDeviceCalls=ipc.calls.length;await click(find(host,'button','백업 시작'),true);
    assert.equal(ipc.calls.length,missingDeviceCalls,'Advanced backup handler also rejects a missing device');
  } finally { instance.unmount(); }
}

await runSuite();
console.log('UI_PASS: production wizard + App, one slide, IPC guards, navigation, busy locks, real result state');
const mutations = [
  ['wrong slide branch', 'InstallWizard', 'v-if="current === 0"', 'v-if="true"'],
  ['busy handler removed', 'InstallWizard', 'if (locked ||', 'if ('],
  ['busy handler reversed', 'InstallWizard', 'if (locked ||', 'if (!locked ||'],
  ['lower bound removed', 'InstallWizard', ' || next < 0', ''],
  ['primary readiness removed', 'InstallWizard', ':disabled="primaryDisabled"', ':disabled="false"'],
  ['duplicate primary action', 'InstallWizard', '<div class="wizard-heading">', '<div class="wizard-heading"><button class="primary">duplicate</button>'],
  ['download digest guard removed', 'InstallWizard', '!!selectedRelease.value?.sha256', '!!selectedRelease.value'],
  ['stale image retained', 'App', "error.value=null;preparedImage.value=null;notice.value='';", "error.value=null;notice.value='';"],
  ['failed image accepted', 'App', 'else if(accept(r)&&r.data)', 'else if(r.data)'],
  ['image success does not advance', 'App', 'preparedImage.value=r.data;wizardStep.value=3;', 'preparedImage.value=r.data;'],
  ['image source reset removed', 'InstallWizard', "source.value=next;emit('clear-image');", 'source.value=next;'],
  ['image version reset removed', 'InstallWizard', "if(source.value==='official')emit('clear-image');", ''],
  ['install readiness removed', 'InstallWizard', ': !props.canInstall||!props.preparedImage', ': !props.canInstall'],
  ['active step reversed', 'InstallWizard', "current === index ? 'step'", "current !== index ? 'step'"],
  ['busy prop disconnected', 'App', ':busy="busy||confirmOpen||!!writePlan"', ':busy="false"'],
  ['backup device guard removed', 'App', 'async function backup(){if(!canRead.value)return false;', 'async function backup(){'],
  ['failed backup advances', 'App', 'if(backupResult.value?.path||await backup())wizardStep.value=2;', 'await backup();wizardStep.value=2;'],
  ['backup success does not advance', 'App', 'if(backupResult.value?.path||await backup())wizardStep.value=2;', 'if(backupResult.value?.path||await backup()){}'],
  ['backup reuse removed', 'App', 'backupResult.value?.path||await backup()', 'await backup()'],
  ['failed Loader inspection advances', 'App', 'if(await inspect())wizardStep.value=1;', 'await inspect();wizardStep.value=1;'],
  ['Loader success does not advance', 'App', 'if(await inspect())wizardStep.value=1;', 'await inspect();'],
  ['failed upload advances', 'App', 'if(ready&&canRead.value)wizardStep.value=1;', 'if(canRead.value)wizardStep.value=1;'],
  ['upload success does not advance', 'App', 'if(ready&&canRead.value)wizardStep.value=1;', 'if(ready&&canRead.value){}'],
  ['post-upload port check removed', 'App', '&&device.value?.location===location;', ';'],
  ['post-upload Loader check removed', 'App', 'if(ready&&canRead.value)wizardStep.value=1;', 'if(ready)wizardStep.value=1;'],
  ['preparation confirmation removed', 'App', 'if(!canPrepare.value||!confirmed.value)return;', 'if(!canPrepare.value)return;'],
  ['sidebar busy guard removed', 'App', 'async function navigate(id:string){if(busy.value||writePlan.value)return;', 'async function navigate(id:string){'],
  ['stale backup retained', 'App', 'observation.value=null;backupResult.value=null;', 'observation.value=null;'],
  ['automatic release loading removed','InstallWizard',"if(visible)emit('enter-official');",'if(visible){}'],
  ['automatic entry guard reversed','InstallWizard',"if(visible)emit('enter-official');","if(!visible)emit('enter-official');"],
  ['release cache guard removed','App',"if(releaseState.value==='idle')void loadReleases();",'void loadReleases();'],
  ['release loading guard removed','InstallWizard','&&!props.releasesLoading&&!!selectedRelease','&&!!selectedRelease'],
  ['confirmation removed','App',"&&writeConfirm.value==='K11C'",''],
  ['confirmation reversed','App',"&&writeConfirm.value==='K11C'","&&writeConfirm.value!=='K11C'"],
  ['write handler guard removed','App','if(!canExecute.value)return;',''],
  ['stale port plan retained','App',"writePlan.value=null;writeConfirm.value='';gptResult.value=null;writeResult.value=null;",'gptResult.value=null;writeResult.value=null;'],
  ['GPT repair guard removed','App',"if(operation==='gpt-repair'&&!gptResult.value?.repairable)return;",''],
  ['restore guard removed','App',"if(operation==='restore'&&!canRestore.value)return;",''],
  ['advanced U-Boot panel removed','App','data-advanced="uboot"','v-if="false" data-advanced="uboot"'],
];
const report = [];
if (process.argv.includes('--mutation')) {
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
}
for (const [name, source] of Object.entries(sources)) assert.equal(readFileSync(`src/${name}.vue`, 'utf8'), source);
writeFileSync(process.argv.includes('--mutation') ? 'test-results/ui-mutations.json' : 'test-results/ui-verification.json', JSON.stringify({ passed: true, production_sfc: true, mocked_boundary: 'Tauri IPC only; Vue custom renderer', mutations: report, source_unchanged: true }, null, 2));
console.log(`UI_VERIFIED mutations=${report.length}; production source unchanged`);
