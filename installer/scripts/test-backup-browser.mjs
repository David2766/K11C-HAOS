import assert from 'node:assert/strict';
import {mkdirSync,writeFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
const {chromium}=await import(process.env.K11C_PLAYWRIGHT?pathToFileURL(process.env.K11C_PLAYWRIGHT).href:'playwright-core');
const browser=await chromium.launch({headless:true});
const context=await browser.newContext({viewport:{width:1180,height:840},locale:'ko-KR',reducedMotion:'reduce'});
const page=await context.newPage(),errors=[],checks=[];
const out='test-results/backup-ui';mkdirSync(out,{recursive:true});
const device={instance_id:'fixture',vid:0x2207,pid:0x350a,binding:true,mode:'Loader',location:'TEST-PORT'};
const identity={sectors:62500000};
const rows=['FULL','BOOT','HAOS'].map(kind=>({id:`${kind==='FULL'?'BOOT':'FULL'}-misleading-${kind}.k11cbackup`,kind,path:`C:/portable/backup/fixture-${kind}.k11cbackup`,identity,restorable:true}));
// Only the IPC boundary is replaced in an isolated browser context. Production
// Vue templates, styles, handlers and component lifecycle run unchanged.
await context.route('**/src/api.ts',route=>route.fulfill({contentType:'application/javascript',body:`
export const native=false;
const device=${JSON.stringify(device)},identity=${JSON.stringify(identity)},rows=${JSON.stringify(rows)};
window.fixtureCalls=[];
export async function call(command,args){
 window.fixtureCalls.push({command,args});
 if(command==='preflight')return {ok:true,data:{driver:{installed:true},devices:[device]}};
 if(command==='inspect_device')return {ok:true,data:{...identity,ready:true,instance_id:device.instance_id,location:device.location,bytes:identity.sectors*512,os:'HAOS'}};
 if(command==='backup_catalog')return {ok:true,data:rows};
 if(command==='storage_plan')return {ok:true,data:{plan_id:'fixture-plan',operation:'restore-archive',device,identity,backup_path:'C:/portable/backup/recovery.k11cbackup',erases_user_data:rows.find(r=>r.id===args.source)?.kind==='FULL',ranges:[]}};
 return {ok:false,error:{code:'PREVIEW_ONLY',detail:command}};
}` }));
page.on('pageerror',e=>errors.push(String(e)));
async function layout(label){
 const result=await page.evaluate(()=>({width:document.documentElement.clientWidth,scroll:document.documentElement.scrollWidth,
  clipped:[...document.querySelectorAll('button,select,input,.restore-summary')].filter(e=>e.getClientRects().length&&(e.getBoundingClientRect().right>document.documentElement.clientWidth+1||e.getBoundingClientRect().left<0)).map(e=>e.id||e.tagName)}));
 assert.ok(result.scroll<=result.width+1,label);assert.deepEqual(result.clipped,[],label);checks.push({label,...result});
}
try{
 await page.goto('http://127.0.0.1:1420/');
 await page.getByRole('button',{name:'연결 다시 확인',exact:true}).click();
 await page.getByRole('button',{name:'고급 기능',exact:true}).click();
 assert.equal(await page.locator('[data-backup-pane]').count(),0);
 await page.locator('[data-backup-mode="backup"]').click();
 assert.equal(await page.locator('#backup-kind').count(),0);
 assert.equal(await page.getByRole('button',{name:'백업 시작',exact:true}).isDisabled(),true);
 for(const width of [1180,980])for(const locale of ['ko','en','zh-TW','es','ja'])for(const theme of ['light','dark']){
  await page.setViewportSize({width,height:width===980?680:840});await page.locator('#display-language').selectOption(locale);await page.locator('#display-theme').selectOption(theme);
  assert.equal(await page.locator('[data-backup-readiness]').isVisible(),true);
  assert.equal(await page.locator('[data-advanced-connect]').isEnabled(),true);
  await layout(`${width}-${locale}-${theme}-needs-preparation`);
  if(width===1180&&locale==='ko')await page.screenshot({path:`${out}/${theme}-needs-preparation.png`,fullPage:true});
 }
 await page.locator('[data-advanced-connect]').click();
 await page.locator('#backup-kind').waitFor();
 assert.equal(await page.locator('[data-backup-readiness]').count(),0);
 assert.deepEqual(await page.locator('#backup-kind option').evaluateAll(items=>items.map(item=>item.value)),['FULL','BOOT','HAOS']);
 assert.deepEqual(await page.evaluate(()=>window.fixtureCalls.filter(c=>['prepare_device','inspect_device','backup_device','storage_plan','storage_execute'].includes(c.command)).map(c=>c.command)),['inspect_device'],'Advanced Loader preparation is read-only and cannot start backup/write');
 await page.locator('[data-backup-back]').click();
 for(const width of [1180,980])for(const locale of ['ko','en','zh-TW','es','ja'])for(const theme of ['light','dark']){
  await page.setViewportSize({width,height:width===980?680:840});await page.locator('#display-language').selectOption(locale);await page.locator('#display-theme').selectOption(theme);
  await layout(`${width}-${locale}-${theme}-chooser`);
  if(width===1180&&locale==='ko')await page.screenshot({path:`${out}/${theme}-chooser.png`,fullPage:true});
  await page.locator('[data-backup-mode="backup"]').click();assert.equal(await page.locator('#restore-backup').count(),0);await layout(`${width}-${locale}-${theme}-backup`);
  await page.locator('[data-backup-back]').click();await page.locator('[data-backup-mode="restore"]').click();assert.equal(await page.locator('#backup-kind').count(),0);
  assert.equal(await page.locator('[data-restore-source]').count(),2);await layout(`${width}-${locale}-${theme}-restore-source`);
  await page.locator('[data-restore-source="manufacturer"]').click();assert.equal(await page.locator('[data-factory-select]').isVisible(),true);assert.equal(await page.locator('[data-storage-action="factory"]').isDisabled(),true);await layout(`${width}-${locale}-${theme}-manufacturer`);
  if(width===1180&&locale==='ko')await page.screenshot({path:`${out}/${theme}-manufacturer.png`,fullPage:true});
  await page.locator('[data-restore-source-back]').click();await page.locator('[data-restore-source="backup"]').click();
  assert.equal(await page.locator('#restore-backup input:checked').count(),0);
  assert.equal(await page.locator('[data-backup-pane] .restore-summary').count(),0);
  for(const row of rows){
   const preview=page.locator(`[data-backup-id="${row.id}"]`);
   assert.equal(await preview.locator('.backup-badge').innerText(),row.kind,'Badge reflects metadata before selecting the file');
   assert.ok((await preview.locator('.backup-choice-note').innerText()).length>10,'Scope note appears before selection');
  }
  await layout(`${width}-${locale}-${theme}-unselected`);
  if(width===1180&&locale==='ko')await page.screenshot({path:`${out}/${theme}-unselected.png`,fullPage:true});
  for(const row of rows){
   await page.locator(`#restore-backup input[value="${row.id}"]`).check();
   assert.equal(await page.locator('[data-backup-pane] .restore-summary').getAttribute('data-restore-kind'),row.kind);
   await layout(`${width}-${locale}-${theme}-${row.kind}`);
   if(width===1180&&locale==='ko')await page.screenshot({path:`${out}/${theme}-${row.kind}.png`,fullPage:true});
   await page.locator('[data-storage-action="restore"]').click();
   assert.equal(await page.locator('[role="dialog"] .restore-summary').getAttribute('data-restore-kind'),row.kind);
   assert.equal(await page.locator('[data-storage-action="execute"]').isDisabled(),true);
   await layout(`${width}-${locale}-${theme}-${row.kind}-confirm`);
   if(width===980&&locale==='es'&&theme==='dark'&&row.kind==='FULL')await page.screenshot({path:`${out}/minimum-confirm.png`});
   await page.locator('#write-confirm').fill('ok');
   const execute=page.locator('[data-storage-action="execute"]');assert.equal(await execute.isEnabled(),true);await execute.scrollIntoViewIfNeeded();
   const bounds=await execute.boundingBox();assert.ok(bounds.y>=0&&bounds.y+bounds.height<=page.viewportSize().height,'Confirmation action remains reachable');
   await page.locator('[role="dialog"] .close-button').click();
  }
  await page.locator('[data-backup-back]').click();await page.locator('[data-backup-mode="restore"]').click();await page.locator('[data-restore-source="backup"]').click();assert.equal(await page.locator('#restore-backup input:checked').count(),0);assert.equal(await page.locator('[data-backup-pane] .restore-summary').count(),0);
  await page.locator('[data-backup-back]').click();
 }
 assert.deepEqual(errors,[]);writeFileSync(`${out}/report.json`,JSON.stringify({passed:true,checks,errors,ipc:'fixture-only',usb:false},null,2));console.log(`BACKUP_BROWSER_PASS views=${checks.length} locales=5 themes=2`);
}finally{await browser.close();}
