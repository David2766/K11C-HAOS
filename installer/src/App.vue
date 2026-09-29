<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { Cpu, Wrench, History, Settings, Usb, Check, ArrowRight, RefreshCw, ShieldCheck, Info, HardDrive, ChevronRight, AlertCircle, Folder, Cable, FileCheck, X } from 'lucide-vue-next';
import { call,native,type Reply,type ImageRelease,type PreparedImage,type ImageProgress } from './api';
import InstallWizard from './InstallWizard.vue';
const tab=ref('prepare');const busy=ref(false);const polling=ref(false);
const wizardStep=ref(0);
const devices=ref<any[]>([]);const driver=ref<any>(null);const selected=ref('');
const notice=ref('');const error=ref<{code:string;detail:string}|null>(null);
const observation=ref<any>(null);const backupResult=ref<any>(null);const records=ref<any[]>([]);
const confirmOpen=ref(false);const confirmed=ref(false);const progress=ref<any>(null);
const releases=ref<ImageRelease[]>([]);const preparedImage=ref<PreparedImage|null>(null);
const releaseState=ref<'idle'|'loading'|'ready'|'error'>('idle');const releaseError=ref('');
const imageProgress=ref<ImageProgress|null>(null);const imageWorking=ref(false);const cancelling=ref(false);
const writePlan=ref<any>(null);const writeConfirm=ref('');const writeResult=ref<any>(null);
const backupChoices=ref<any[]>([]);const restoreId=ref('');const gptResult=ref<any>(null);
const writeNames:Record<string,string>={install:'HAOS 설치',uboot:'U-Boot 업데이트',restore:'부팅 영역 복원','gpt-repair':'GPT 복구'};
const canExecute=computed(()=>!busy.value&&canRead.value&&!!writePlan.value&&writeConfirm.value==='K11C'&&writePlan.value.device.instance_id===selected.value&&writePlan.value.device.location===device.value?.location);
const canRestore=computed(()=>canRead.value&&backupChoices.value.some(b=>b.id===restoreId.value&&b.restorable&&b.device.instance_id===selected.value&&b.device.location===device.value?.location));
let unlistenClose:UnlistenFn|undefined;
let unlistenImage:UnlistenFn|undefined;
let timer:ReturnType<typeof setInterval>|undefined;let unlisten:UnlistenFn|undefined;
const device=computed(()=>devices.value.find(d=>d.instance_id===selected.value));
const supported=computed(()=>device.value?.vid===0x2207&&[0x350a,0x300a].includes(device.value?.pid));
const canRead=computed(()=>!busy.value&&supported.value&&device.value?.binding&&device.value?.mode==='Loader');
const canPrepare=computed(()=>!busy.value&&supported.value&&device.value?.binding&&device.value?.mode==='Maskrom');
const tabs=[{id:'prepare',label:'설치·복구',icon:Usb},{id:'advanced',label:'고급 기능',icon:Wrench},{id:'history',label:'작업 기록',icon:History},{id:'settings',label:'설정',icon:Settings}];
const phaseNames:Record<string,string>={upload:'Loader 전송 중',reconnect:'장치 다시 연결 중',read:'부팅 영역 읽는 중',verify:'백업 대조 중',complete:'백업 완료','preflight-write':'이미지·기록 범위 확인 중',write:'eMMC 기록 중','verify-write':'기록한 데이터 읽기 대조 중','write-complete':'기록·검증 완료'};
const progressLabel=computed(()=>phaseNames[progress.value?.phase]??'장치 확인 중');
const percentage=computed(()=>progress.value?.total?Math.min(100,Math.round(progress.value.completed/progress.value.total*100)):null);
const errorText:Record<string,string>={
 DEVICE_GONE:'장치 연결이 끊겼습니다. 케이블을 확인하고 다시 연결하세요.',
 DRIVER_BINDING:'USB 드라이버 연결을 확인해 주세요. 다른 USB 작업 프로그램은 닫아 주세요.',
 DEVICE_BUSY:'다른 작업이 진행 중입니다. 완료된 뒤 다시 시도하세요.',
 USB_OPEN:'장치를 열 수 없습니다. RKDevTool 등 다른 USB 프로그램을 닫고 다시 연결하세요.',
 USB_READ:'장치를 읽지 못했습니다. 케이블과 전원을 확인하세요.',
 USB_UPLOAD:'Loader 전송에 실패했습니다. 전원을 다시 연결하고 MASKROM으로 진입해 주세요.',
 USB_NOT_READY:'장치가 응답하지 않습니다. 연결 상태를 확인하세요.',
 USB_TIMEOUT:'장치 응답 시간이 초과됐습니다. 케이블과 전원을 확인하세요.',
 LOADER_RECONNECT:'장치가 다시 연결되지 않았습니다. 같은 USB 포트에서 MASKROM으로 다시 연결하세요.',
 LOADER_HASH:'Loader 파일이 손상됐습니다. 프로그램 ZIP을 다시 풀어 주세요.',
 LOADER_MISSING:'Loader 파일을 찾지 못했습니다. 프로그램 ZIP을 모두 풀어 주세요.',
 RESOURCE_HASH:'프로그램 파일이 손상됐습니다. ZIP을 다시 풀어 주세요.',
 RESOURCE_MISSING:'프로그램 파일이 누락됐습니다. ZIP을 모두 풀어 주세요.',
 NOT_EMMC:'eMMC가 선택된 장치가 아닙니다. 연결한 보드를 확인하세요.',
 UNSUPPORTED_DEVICE:'지원하는 USB 장치가 아닙니다. K11C 연결을 확인하세요.',
 BACKUP_IO:'백업을 저장하지 못했습니다. 실행 폴더의 쓰기 권한과 남은 공간을 확인하세요.',
 BACKUP_VERIFY:'저장된 백업의 검증에 실패했습니다. 다른 저장 위치에서 다시 실행해 주세요.',
 SHORT_READ:'장치에서 데이터를 끝까지 읽지 못했습니다. 케이블을 확인하고 다시 시도하세요.',
 DEVICE_CHANGED:'장치 정보 또는 읽은 데이터가 달라졌습니다. 연결을 확인하고 다시 시도하세요.',
 UAC_CANCELLED:'드라이버 설치가 취소됐습니다. 설치하려면 프로그램을 다시 열고 관리자 권한을 허용하세요.',
 ELEVATION_FAILED:'드라이버 설치에 관리자 권한이 필요합니다.',
 IMAGE_NETWORK:'다운로드 서버에 연결하지 못했습니다. 인터넷 연결을 확인하고 다시 시도하세요.',
 IMAGE_RATE_LIMIT:'GitHub 요청 한도에 도달했습니다. 잠시 후 다시 시도하세요.',
 IMAGE_NO_DIGEST:'이 릴리스에는 공식 SHA-256이 없습니다. 다른 버전을 선택하세요.',
 IMAGE_HASH:'이미지의 SHA-256이 일치하지 않습니다. 파일을 다시 받아 주세요.',
 IMAGE_SIZE:'이미지 크기가 맞지 않습니다. 다운로드가 완료된 파일인지 확인하세요.',
 IMAGE_FILE:'이미지 파일을 읽지 못했습니다. 파일 위치와 접근 권한을 확인하세요.',
 IMAGE_FORMAT:'generic-aarch64용 .img.xz 또는 .img 파일을 선택하세요.',
 IMAGE_PLATFORM:'K11C에서 사용할 수 있는 HAOS generic-aarch64 이미지가 아닙니다.',
 IMAGE_GPT:'이미지의 파티션 테이블이 손상됐거나 지원하지 않는 형식입니다.',
 IMAGE_BOOT:'이미지의 부팅 파일을 읽지 못했습니다. 공식 이미지를 다시 받아 주세요.',
 IMAGE_DECODE:'이미지 압축을 풀지 못했습니다. 파일을 다시 받아 주세요.',
 IMAGE_CHANGED:'검사 중 이미지가 변경됐습니다. 파일 복사를 마친 뒤 다시 선택하세요.',
 IMAGE_IO:'이미지를 저장하지 못했습니다. 실행 폴더의 남은 공간과 쓰기 권한을 확인하세요.',
 IMAGE_RELEASE:'공식 릴리스 정보를 읽지 못했습니다. 목록을 다시 불러오세요.',
 IMAGE_URL:'공식 릴리스의 다운로드 주소를 확인하지 못했습니다.',
 USB_WRITE:'eMMC 기록이 중단됐습니다. 아래 기록과 백업 위치를 확인하세요. 설치 중이었다면 다시 설치해야 합니다.',
 SHORT_WRITE:'기록이 끝까지 전달되지 않았습니다. 연결 상태와 작업 기록을 확인하세요.',
 WRITE_VERIFY:'기록 후 읽기 대조에 실패했습니다. 정상 부팅을 보장할 수 없습니다. 작업 기록과 백업을 확인하세요.',
 FIRMWARE_HASH:'K11C 부팅 펌웨어가 손상됐습니다. 프로그램 ZIP을 다시 풀어 주세요.',
 CAPACITY:'선택한 이미지를 설치하기에 eMMC 용량이 부족합니다.',
 IMAGE_LAYOUT:'K11C용으로 가공하지 않은 공식 generic-aarch64 이미지를 선택하세요.',
 GPT_LAYOUT:'K11C 설치용 파티션 구성이 아닙니다. 새 설치가 필요할 수 있습니다.',
 GPT_INVALID:'GPT가 정상인지 먼저 검사하세요.',
 GPT_AMBIGUOUS:'두 GPT가 서로 다른 파티션을 가리킵니다. 자동 복구할 수 없습니다.',
 GPT_NO_VALID_COPY:'정상 GPT를 찾지 못했습니다. 검증된 백업을 확인하세요.',
 RESTORE_LAYOUT:'백업 이후 파티션 구성이 바뀌었습니다. 부팅 영역 백업으로 OS·사용자 데이터를 되돌릴 수는 없습니다.',
 BACKUP_DEVICE:'이 장치·USB 포트와 일치하는 백업을 선택하세요.',
 PLAN_CHANGED:'설치 파일이나 작업 내용이 변경됐습니다. 작업을 다시 준비하세요.',
 TRANSACTION_IO:'작업 파일을 읽거나 저장하지 못했습니다. 실행 폴더의 공간·권한을 확인하세요.',
};
function accept(r:Reply){if(!r.ok){error.value=r.error??{code:'ERROR',detail:''};return false;}if(r.history_warning)notice.value='작업 기록을 저장하지 못했습니다. 실행 폴더의 쓰기 권한을 확인하세요.';return true;}
function updateDevices(list:any[]){devices.value=list;if(!list.some(d=>d.instance_id===selected.value))selected.value=list.length===1?list[0].instance_id:'';}
async function scan(){const r=await call('preflight');if(!accept(r))return false;driver.value=r.data.driver;updateDevices(r.data.devices);return true;}
async function refresh(){if(busy.value)return;busy.value=true;error.value=null;try{await scan();}finally{busy.value=false;}}
async function poll(){if(!native||busy.value||polling.value||confirmOpen.value)return;polling.value=true;try{const r=await call('list_devices');if(r.ok)updateDevices(r.data);else if(r.error?.code!=='DEVICE_BUSY'){devices.value=[];selected.value='';}}finally{polling.value=false;}}
async function setup(){busy.value=true;notice.value='USB 드라이버 확인 중';try{
 const r=await call('ensure_driver');if(accept(r)){
  const messages:Record<string,string>={reused:'USB 드라이버 준비됨',installed:'USB 드라이버 설치 완료',reboot_required:'Windows를 다시 시작하면 드라이버 설치가 완료됩니다.',not_retried:'드라이버 설치를 다시 시도하려면 프로그램을 다시 열어 주세요.'};notice.value=messages[r.data.state]??'';
 }else notice.value='';await scan();
}finally{busy.value=false;}}
async function inspect(){if(!canRead.value)return false;busy.value=true;error.value=null;progress.value=null;try{const r=await call('inspect_device',{instanceId:selected.value});if(!accept(r))return false;observation.value=r.data;return true;}finally{busy.value=false;}}
async function connectNext(){
 if(!canRead.value&&!canPrepare.value)return;
 if(device.value.mode==='Loader'){if(await inspect())wizardStep.value=1;}
 else{confirmed.value=false;confirmOpen.value=true;}
}
async function prepare(){if(!canPrepare.value||!confirmed.value)return;
 const id=selected.value;const location=device.value.location;confirmOpen.value=false;busy.value=true;error.value=null;progress.value={phase:'upload'};
 let ready=false;
 try{const r=await call('prepare_device',{instanceId:id,location,confirmedK11c:true});if(accept(r)){updateDevices([r.data.device]);selected.value=r.data.device.instance_id;observation.value=r.data.observation;notice.value='장치 준비 완료';ready=await scan()&&selected.value===r.data.device.instance_id&&device.value?.location===location;}else await scan();}
 finally{busy.value=false;progress.value=null;confirmed.value=false;}
 if(ready&&canRead.value)wizardStep.value=1;
}
async function backup(){if(!canRead.value)return false;busy.value=true;error.value=null;backupResult.value=null;progress.value={phase:'read'};
 try{const r=await call('backup_device',{instanceId:selected.value});if(!accept(r))return false;backupResult.value=r.data;return true;}
 finally{busy.value=false;progress.value=null;}
}
async function backupNext(){if(!canRead.value)return;if(backupResult.value?.path||await backup())wizardStep.value=2;}
async function openBackups(){accept(await call('open_backups'));}
async function loadReleases(){
 if(busy.value||releaseState.value==='loading')return;
 releaseState.value='loading';releaseError.value='';
 const r=await call('image_releases');
 if(r.ok){releases.value=r.data;releaseState.value='ready';}
 else{releaseState.value='error';releaseError.value=errorText[r.error?.code??'']??'버전 목록을 불러오지 못했습니다. 다시 시도해 주세요.';}
}
function ensureReleases(){if(releaseState.value==='idle')void loadReleases();}
async function prepareImage(version?:string){
 if(busy.value)return;
 if(version!==undefined&&!releases.value.some(r=>r.version===version&&r.sha256))return;
 busy.value=true;imageWorking.value=true;cancelling.value=false;error.value=null;preparedImage.value=null;notice.value='';
 imageProgress.value={phase:version?'resolve':'choose',completed:0,total:null};
 try{const r=await call(version?'image_download':'image_select',version?{version}:{});
  if(r.error?.code==='IMAGE_CANCELLED')notice.value='이미지 준비를 취소했습니다.';
  else if(accept(r)&&r.data){preparedImage.value=r.data;wizardStep.value=3;}
 }finally{imageWorking.value=false;busy.value=false;imageProgress.value=null;cancelling.value=false;}
}
async function cancelImage(){if(!imageWorking.value||cancelling.value)return;cancelling.value=true;const r=await call('image_cancel');if(!accept(r))cancelling.value=false;}
async function checkGpt(){if(!canRead.value)return;busy.value=true;error.value=null;gptResult.value=null;try{const r=await call('gpt_check',{instanceId:selected.value});if(accept(r))gptResult.value=r.data;}finally{busy.value=false;}}
async function loadBackups(){if(busy.value)return;const r=await call('backup_catalog');if(accept(r)){backupChoices.value=r.data;if(!r.data.some((b:any)=>b.id===restoreId.value))restoreId.value='';}}
async function planWrite(operation:string){
 if(!canRead.value||writePlan.value)return;
 if(operation==='install'&&!preparedImage.value)return;
 if(operation==='restore'&&!canRestore.value)return;
 if(operation==='gpt-repair'&&!gptResult.value?.repairable)return;
 const source=operation==='install'?preparedImage.value!.sha256:operation==='restore'?restoreId.value:'';
 busy.value=true;error.value=null;writeResult.value=null;progress.value={phase:'preflight-write'};notice.value='';
 try{const r=await call('storage_plan',{instanceId:selected.value,location:device.value.location,operation,source});
  if(accept(r)){if(r.data.no_changes)notice.value='이미 정상 상태입니다. 변경할 내용이 없습니다.';else{writePlan.value=r.data;writeConfirm.value='';}}
 }finally{busy.value=false;progress.value=null;}
}
function cancelWrite(){if(busy.value)return;writePlan.value=null;writeConfirm.value='';}
async function executeWrite(){
 if(!canExecute.value)return;
 const id=writePlan.value.plan_id;busy.value=true;error.value=null;writeResult.value=null;writePlan.value=null;writeConfirm.value='';progress.value={phase:'preflight-write'};
 try{const r=await call('storage_execute',{planId:id,confirmed:true});if(accept(r)&&r.data.verified){writeResult.value=r.data;gptResult.value=null;backupResult.value=null;}}
 finally{busy.value=false;progress.value=null;}
}
async function navigate(id:string){if(busy.value||writePlan.value)return;tab.value=id;if(id==='history'){const r=await call('history');if(accept(r))records.value=r.data;}if(id==='advanced')await loadBackups();}
function operationName(op:string){return ({preflight:'연결 확인','driver-setup':'드라이버 준비',inspect:'저장장치 확인',prepare:'장치 준비',backup:'부팅 영역 백업','image-releases':'HAOS 버전 조회','image-download':'HAOS 다운로드','image-import':'이미지 가져오기','gpt-check':'GPT 검사','backup-catalog':'백업 목록','storage-plan':'기록 준비·자동 백업','storage-execute':'eMMC 기록·검증'} as Record<string,string>)[op]??op;}
function size(bytes:number){return (bytes/1024**3).toFixed(1)+' GiB';}
function clearImage(){if(!busy.value)preparedImage.value=null;}
watch(()=>[selected.value,device.value?.location,device.value?.mode,device.value?.binding].join('|'),()=>{observation.value=null;backupResult.value=null;confirmed.value=false;confirmOpen.value=false;if(notice.value==='장치 준비 완료')notice.value='';},{flush:'sync'});
watch(()=>[selected.value,device.value?.location,device.value?.mode,device.value?.binding,preparedImage.value?.sha256,restoreId.value].join('|'),()=>{writePlan.value=null;writeConfirm.value='';gptResult.value=null;writeResult.value=null;},{flush:'sync'});
onMounted(async()=>{if(native){unlisten=await listen('usb-progress',e=>{progress.value=e.payload;});unlistenImage=await listen<ImageProgress>('image-progress',e=>{if(imageWorking.value)imageProgress.value=e.payload;});unlistenClose=await listen('write-close-blocked',()=>{notice.value='기록·검증이 끝날 때까지 프로그램과 USB 연결을 유지하세요.';});await setup();timer=setInterval(poll,2000);}});
onUnmounted(()=>{if(timer)clearInterval(timer);unlisten?.();unlistenImage?.();unlistenClose?.();});
</script>

<template>
 <div class="shell">
  <aside>
   <div class="brand"><span class="brand-mark"><Cpu :size="23"/></span><div>K11C <b>Installer</b><small>DEVICE TOOLS</small></div></div>
   <div class="nav-label">WORKSPACE</div>
   <nav><button v-for="item in tabs" :key="item.id" :class="{active:tab===item.id}" :disabled="busy" @click="navigate(item.id)"><component :is="item.icon" :size="18"/>{{item.label}}<ChevronRight v-if="tab===item.id" :size="15"/></button></nav>
   <div class="aside-bottom"><span class="portable-dot"></span> Portable <span class="muted">0.4.0</span></div>
  </aside>
  <main>
   <header><div><div class="eyebrow">K11C DEVICE WORKSPACE</div><h1>{{tabs.find(t=>t.id===tab)?.label}}</h1><p>{{tab==='prepare'?'K11C에 Home Assistant OS를 설치합니다.':tab==='advanced'?'부팅 펌웨어와 저장장치를 관리합니다.':tab==='history'?'이 PC에서 진행한 작업을 확인합니다.':'USB 드라이버와 저장 위치를 확인합니다.'}}</p></div><span class="badge"><ShieldCheck :size="14"/> K11C</span></header>
   <section class="device-card">
    <div class="device-icon"><Usb :size="28"/></div>
    <div class="device-text"><div class="device-title">{{device?'Rockchip USB 장치':'K11C를 연결해 주세요'}}<span class="pill" :class="{'muted-pill':!device}">{{device?.mode??'미연결'}}</span></div><p v-if="device">USB {{device.vid.toString(16)}}:{{device.pid.toString(16)}} · {{device.binding?'연결됨':'드라이버 확인 필요'}}</p><p v-else>USB 데이터 케이블로 연결하고 MASKROM 모드로 진입하세요.</p></div>
    <button class="icon-button" :disabled="busy" title="연결 다시 확인" aria-label="연결 다시 확인" @click="refresh"><RefreshCw :size="18" :class="{spin:busy}"/></button>
   </section>
   <select v-if="devices.length>1" class="device-select" :disabled="busy" v-model="selected"><option value="">장치를 선택하세요</option><option v-for="d in devices" :value="d.instance_id">{{d.location}} · {{d.mode}} · {{d.instance_id}}</option></select>
   <div v-if="notice" class="status-line" role="status"><Check v-if="driver?.installed" :size="15"/><Info v-else :size="15"/>{{notice}}</div>
   <div v-if="error" class="error" role="alert"><AlertCircle :size="18"/><div><strong>{{errorText[error.code]??'작업을 완료하지 못했습니다. 상세 내용을 확인해 주세요.'}}</strong><details><summary>상세 정보</summary><pre>{{error.code + '\n' + error.detail}}</pre></details></div></div>
   <section v-if="busy&&progress" class="panel progress-panel" role="status"><div class="setting-row"><b>{{progressLabel}}</b><span>{{percentage===null?'잠시 기다려 주세요':percentage+'%'}}</span></div><progress v-if="percentage!==null" :value="percentage" max="100"/><progress v-else/></section>

   <section v-if="writeResult" class="panel write-result" role="status"><Check :size="24"/><div><h3>{{writeNames[writeResult.operation]}} 완료</h3><p>기록한 데이터를 다시 읽어 확인했습니다.</p><p v-if="writeResult.operation!=='gpt-repair'">USB와 전원을 분리하고, SD 카드 없이 전원을 다시 연결하세요.<span v-if="writeResult.operation==='install'"> 최초 부팅에는 몇 분 정도 걸릴 수 있습니다.</span></p><details><summary>백업·작업 기록</summary><p class="path">{{writeResult.backup_path}}</p><p class="path">{{writeResult.journal}}</p></details></div></section>
   <InstallWizard v-show="tab==='prepare'" v-model:step="wizardStep" :active="tab==='prepare'" :busy="busy||confirmOpen||!!writePlan" :can-connect="canRead||canPrepare" :loader="device?.mode==='Loader'" :can-install="canRead" :device-label="device?(observation?'eMMC · '+size(observation.bytes):'Rockchip USB · '+device.mode):'연결 안 됨'" :backup-path="backupResult?.path"
    :releases="releases" :prepared-image="preparedImage" :image-progress="imageProgress" :cancelling="cancelling"
    :releases-loading="releaseState==='loading'" :release-error="releaseError"
    @connect="connectNext" @backup="backupNext" @clear-image="clearImage" @enter-official="ensureReleases" @load-releases="loadReleases" @download="prepareImage" @select-image="prepareImage()" @cancel-image="cancelImage" @install="planWrite('install')">
    <template #connection>
    <section class="panel"><div class="panel-top"><Cable :size="26"/><div><h3>보드 연결 준비</h3><p>SD 카드를 빼고 K11C를 MASKROM으로 연결하세요.</p></div><span v-if="device?.mode==='Loader'&&device?.binding" class="pill">준비됨</span></div>
     <ol class="connection-steps"><li>Home Assistant를 종료하고 보드 전원을 분리하세요.</li><li>MASKROM 버튼을 누른 상태로 USB 데이터 케이블을 PC에 연결하세요.</li><li>장치가 표시되면 버튼을 놓고 아래에서 장치를 준비하세요.</li></ol>
     <p class="body-note">RKDevTool 등 다른 USB 작업 프로그램은 닫아 주세요.</p>
     <div v-if="observation" class="storage-summary"><HardDrive :size="18"/><strong>eMMC · {{size(observation.bytes)}}</strong><span>{{observation.sectors.toLocaleString()}} sectors</span></div>
    </section>
    </template>
    <template #backup>
    <section class="panel"><div class="panel-top"><FileCheck :size="26"/><div><h3>U-Boot · 파티션 테이블</h3><p>부팅 영역을 PC에 저장하고 원본과 대조합니다.</p></div></div>
     <p class="body-note">HA 설정과 사용자 데이터는 Home Assistant의 백업 기능으로 별도 보관하세요.</p>
     <div class="footer-actions"><button class="folder-button" @click="openBackups"><Folder :size="16"/>백업 폴더 열기</button></div>
     <div v-if="backupResult" class="backup-result" role="status"><Check :size="20"/><div><strong>백업 저장·검증 완료</strong><p class="path">{{backupResult.path}}</p><p v-if="backupResult.gpt.healthy">GPT 정상</p><p v-else class="warning">GPT 확인 필요 — 상세 정보에서 검사 결과를 확인하세요.</p><details><summary>상세 정보</summary><pre>{{JSON.stringify(backupResult,null,2)}}</pre></details></div></div>
    </section>
    </template>
   </InstallWizard>

   <template v-if="tab==='advanced'">
    <div class="advanced-grid">
     <section class="panel"><div class="panel-top"><HardDrive :size="26"/><div><h3>저장장치 정보</h3><p>eMMC 용량과 GPT 헤더 정보를 읽습니다.</p></div></div><p class="body-note">Loader 상태에서 확인할 수 있습니다.</p><button class="secondary" :disabled="!canRead" @click="inspect"><RefreshCw :size="16"/>정보 확인</button><template v-if="observation"><div class="setting-row"><span>저장장치</span><b>eMMC · {{size(observation.bytes)}}</b></div><div class="setting-row"><span>주 GPT 헤더</span><b>{{observation.primary_gpt_signature?'있음':'없음'}}</b></div><div class="setting-row"><span>보조 GPT 헤더</span><b>{{observation.backup_gpt_signature?'있음':'없음'}}</b></div><details><summary>상세 정보</summary><pre>{{JSON.stringify(observation,null,2)}}</pre></details></template></section>
     <section class="panel" data-advanced="uboot"><div class="panel-top"><Cpu :size="26"/><div><h3>U-Boot만 업데이트</h3><p>OS와 사용자 데이터를 유지하고 부팅 펌웨어를 교체합니다.</p></div></div><p class="body-note">동봉된 K11C r24 · 기존 펌웨어 자동 백업</p><div class="advanced-actions"><button class="secondary" data-storage-action="uboot" :disabled="!canRead" @click="planWrite('uboot')">U-Boot 업데이트</button></div></section>
     <section class="panel" data-advanced="backup"><div class="panel-top"><FileCheck :size="26"/><div><h3>부팅 영역 백업·복원</h3><p>U-Boot와 파티션 테이블을 PC에 보관합니다.</p></div></div><p class="body-note">HA 설정·사용자 데이터와 OS는 복원하지 않습니다.</p><div class="advanced-actions"><button class="secondary" :disabled="!canRead" @click="backup">백업 시작</button><button class="folder-button" :disabled="busy" @click="loadBackups">목록 새로고침</button></div><label for="restore-backup">복원할 백업</label><select id="restore-backup" class="wizard-input" :disabled="busy" :value="restoreId" @change="restoreId=($event.target as HTMLSelectElement).value"><option value="">백업을 선택하세요</option><option v-for="b in backupChoices" :key="b.id" :value="b.id" :disabled="!b.restorable||b.device.instance_id!==selected||b.device.location!==device?.location">{{b.id}}{{b.restorable?'':' · GPT 확인 필요'}}</option></select><div class="advanced-actions"><button class="secondary" data-storage-action="restore" :disabled="!canRestore" @click="planWrite('restore')">백업 복원</button><button class="folder-button" @click="openBackups"><Folder :size="16"/>백업 폴더 열기</button></div><div v-if="backupResult" class="backup-result" role="status"><Check :size="20"/><div><strong>백업 저장·검증 완료</strong><p class="path">{{backupResult.path}}</p><p :class="{warning:!backupResult.gpt.healthy}">{{backupResult.gpt.healthy?'GPT 정상':'GPT 확인 필요'}}</p><details><summary>상세 정보</summary><pre>{{JSON.stringify(backupResult,null,2)}}</pre></details></div></div></section>
     <section class="panel" data-advanced="gpt"><div class="panel-top"><ShieldCheck :size="26"/><div><h3>GPT 검사·복구</h3><p>주·보조 파티션 테이블을 대조하고 불일치를 복구합니다.</p></div></div><div class="advanced-actions"><button class="secondary" data-storage-action="gpt-check" :disabled="!canRead" @click="checkGpt">GPT 검사</button><button class="secondary" data-storage-action="gpt-repair" :disabled="!canRead||!gptResult?.repairable" @click="planWrite('gpt-repair')">GPT 복구</button></div><div v-if="gptResult" class="body-note"><strong :class="gptResult.gpt.healthy?'success':'warning'">{{gptResult.gpt.healthy?'GPT 정상':gptResult.repairable?'GPT 복구 가능':'자동 복구 불가'}}</strong><details><summary>검사 결과</summary><pre>{{JSON.stringify(gptResult,null,2)}}</pre></details></div></section>
    </div>
   </template>
   <template v-if="tab==='settings'"><section class="panel"><div class="panel-top"><ShieldCheck :size="25"/><div><h3>USB 드라이버</h3><p>Rockchip Rockusb</p></div><span class="pill">{{driver?.installed?'준비됨':'확인 필요'}}</span></div><div class="setting-row"><span>설치된 버전</span><b>{{driver?.version??'미확인'}}</b></div><details><summary>상세 정보</summary><pre>{{driver?.published_inf??'설치 정보 없음'}}</pre></details><button class="secondary" :disabled="busy" @click="refresh"><RefreshCw :size="16"/>상태 다시 확인</button></section><section class="panel compact"><h3>저장 위치</h3><p>백업: data / backups<br>작업 기록: data / operations.jsonl</p><button class="folder-button" @click="openBackups"><Folder :size="16"/>백업 폴더 열기</button></section></template>
   <template v-if="tab==='history'"><section class="panel"><h3>최근 작업</h3><div v-if="!records.length" class="empty-feature"><History :size="30"/><p>아직 기록된 작업이 없습니다.</p></div><div v-for="(r,i) in records" :key="i" class="history-row"><span :class="r.result.ok?'success':'failed'">{{r.result.ok?'완료':'확인 필요'}}</span><b>{{operationName(r.operation)}}</b><time>{{new Date(r.time*1000).toLocaleString('ko-KR')}}</time><details><summary>상세 정보</summary><pre>{{JSON.stringify(r.result,null,2)}}</pre></details></div></section></template>
  </main>
  <div v-if="writePlan" class="modal-backdrop" @keydown.esc="cancelWrite"><section class="modal write-modal" role="dialog" aria-modal="true" aria-labelledby="write-title"><button class="close-button" aria-label="기록 취소" @click="cancelWrite"><X :size="20"/></button><HardDrive :size="28"/><h2 id="write-title">{{writeNames[writePlan.operation]}} 확인</h2><p>KICKPI K11C · eMMC {{size(writePlan.identity.sectors*512)}}<br>{{writePlan.device.location}}</p><p class="install-warning" v-if="writePlan.erases_user_data">기존 OS·HA 설정·사용자 데이터가 삭제됩니다.<br>부팅 영역 백업으로 사용자 데이터를 복원할 수 없습니다.</p><p v-else>부팅 영역을 변경합니다. 완료될 때까지 전원과 USB 연결을 유지하세요.</p><p class="body-note">자동 백업 <span class="path">{{writePlan.backup_path}}</span></p><details><summary>기록 범위</summary><div v-for="r in writePlan.ranges" :key="r.lba" class="setting-row"><span>{{r.label}}</span><b>LBA {{r.lba}} · {{(r.bytes/1024**2).toFixed(1)}} MiB</b></div></details><label for="write-confirm">연결한 보드를 확인하고 K11C를 입력하세요.</label><input id="write-confirm" class="wizard-input" autocomplete="off" :value="writeConfirm" @input="writeConfirm=($event.target as HTMLInputElement).value"/><div class="modal-actions"><button class="secondary" @click="cancelWrite">취소</button><button class="primary" data-storage-action="execute" :disabled="!canExecute" @click="executeWrite">기록 시작<ArrowRight :size="16"/></button></div></section></div>
  <div v-if="confirmOpen" class="modal-backdrop" @keydown.esc="confirmOpen=false"><section class="modal" role="dialog" aria-modal="true" aria-labelledby="confirm-title"><button class="close-button" aria-label="닫기" @click="confirmOpen=false"><X :size="20"/></button><Usb :size="30"/><h2 id="confirm-title">K11C 장치 준비</h2><p>연결된 보드에 RAM Loader를 전송합니다.<br>전송이 끝날 때까지 케이블을 연결해 두세요.</p><p class="body-note">{{device?.location}} · USB {{device?.vid.toString(16)}}:{{device?.pid.toString(16)}}</p><label class="confirm-check"><input type="checkbox" v-model="confirmed"/>연결한 보드가 KICKPI K11C입니다.</label><div class="modal-actions"><button class="secondary" @click="confirmOpen=false">취소</button><button class="primary" :disabled="!confirmed||!canPrepare" @click="prepare">준비 시작<ArrowRight :size="16"/></button></div></section></div>
 </div>
</template>
