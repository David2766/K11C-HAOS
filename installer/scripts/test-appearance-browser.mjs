import assert from 'node:assert/strict';
import {mkdirSync,writeFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
// Isolated browser profile. Production preview denies all USB/native operations.
const {chromium}=await import(process.env.K11C_PLAYWRIGHT?pathToFileURL(process.env.K11C_PLAYWRIGHT).href:'playwright-core');
const browser=await chromium.launch({headless:true});
const context=await browser.newContext({viewport:{width:1180,height:840},colorScheme:'light',locale:'ko-KR',reducedMotion:'reduce'});
const page=await context.newPage(),errors=[],checks=[];
page.on('pageerror',error=>errors.push(String(error)));
const out='test-results/appearance';mkdirSync(out,{recursive:true});
async function layout(label){
 const result=await page.evaluate(()=>({width:document.documentElement.clientWidth,scroll:document.documentElement.scrollWidth,
  tooSmall:[...document.querySelectorAll('button,p,label,small,summary')].filter(e=>e.getClientRects().length&&parseFloat(getComputedStyle(e).fontSize)<12).map(e=>e.tagName),
  theme:document.documentElement.dataset.theme,
  background:getComputedStyle(document.documentElement).backgroundColor,
  mainOverflow:[...document.querySelectorAll('button,select,input')].filter(e=>e.getClientRects().length&&e.getBoundingClientRect().right>document.documentElement.clientWidth+1).map(e=>e.id||e.textContent)}));
 assert.ok(result.scroll<=result.width+1,label+' horizontal overflow');assert.deepEqual(result.tooSmall,[],label+' small text');assert.deepEqual(result.mainOverflow,[],label+' controls outside viewport');checks.push({label,...result});
}
try{
 await page.goto('http://127.0.0.1:1420/');await page.locator('#display-theme').waitFor();
 for(const theme of ['light','dark']){
  await page.locator('#display-theme').selectOption(theme);await layout('ko-'+theme);
  await page.screenshot({path:out+'/'+theme+'-install.png'});
  await page.locator('nav button').nth(3).click();
  const gap=async()=>page.locator('.driver-settings').evaluate(e=>{const d=e.querySelector('details').getBoundingClientRect(),b=e.querySelector('.secondary').getBoundingClientRect();return b.top-d.bottom;});
  assert.ok(await gap()>=20);await page.locator('.driver-settings summary').click();assert.ok(await gap()>=20);await page.locator('.driver-settings summary').click();
  await page.screenshot({path:out+'/'+theme+'-settings.png'});await page.locator('nav button').nth(0).click();
 }
 for(const width of [1180,980])for(const locale of ['en','ko','zh-TW','es','ja'])for(const theme of ['light','dark']){
  await page.setViewportSize({width,height:width===980?680:840});await page.locator('#display-language').selectOption(locale);await page.locator('#display-theme').selectOption(theme);
  await page.locator('nav button').nth(0).click();
  for(let step=0;step<5;step++){await page.locator('.wizard-steps button').nth(step).click();await layout(`${width}-${locale}-${theme}-step${step}`);}
  await page.locator('.wizard-steps button').nth(0).click();await page.locator('[data-wizard-mode="restore"]').click();
  for(const step of [3,4]){await page.locator('.wizard-steps button').nth(step).click();await layout(`${width}-${locale}-${theme}-restore${step}`);}
  await page.locator('.wizard-steps button').nth(0).click();await page.locator('[data-wizard-mode="install"]').click();
  for(const tab of [1,2,3]){await page.locator('nav button').nth(tab).click();await layout(`${width}-${locale}-${theme}-tab${tab}`);}
 }
 await page.setViewportSize({width:980,height:680});await page.locator('#display-language').selectOption('es');await page.locator('nav button').nth(1).click();await page.screenshot({path:out+'/spanish-minimum.png',fullPage:true});
 await page.locator('#display-theme').selectOption('dark');await page.reload();assert.equal(await page.locator('#display-theme').inputValue(),'dark');
 await page.emulateMedia({colorScheme:'light'});assert.equal(await page.locator('html').getAttribute('data-theme'),'dark');
 await page.locator('#display-theme').selectOption('system');await page.waitForFunction(()=>document.documentElement.dataset.theme==='light');
 await page.emulateMedia({colorScheme:'dark'});await page.waitForFunction(()=>document.documentElement.dataset.theme==='dark');
 await page.locator('#display-theme').selectOption('light');await page.locator('#display-theme').focus();assert.equal(await page.locator('#display-theme').evaluate(e=>getComputedStyle(e).outlineStyle),'solid');
 await page.emulateMedia({forcedColors:'active'});await layout('forced-colors');
 assert.deepEqual(errors,[]);writeFileSync(out+'/report.json',JSON.stringify({passed:true,checks,errors,hardware:false},null,2));console.log('APPEARANCE_BROWSER_PASS views='+checks.length+'; theme persistence, OS change, focus, disclosure spacing, five languages');
}finally{await browser.close();}
