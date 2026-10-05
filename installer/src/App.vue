<script setup lang="ts">
import { t, locale, setLocale, languages, number, dateTime } from './i18n';
import { themePreference, resolvedTheme, setTheme } from './theme';
import { ref, computed, onMounted, onUnmounted, watch } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { Cpu, Wrench, History, Settings, Usb, Check, ArrowRight, RefreshCw, ShieldCheck, Info, HardDrive, ChevronRight, AlertCircle, Folder, Cable, FileCheck, X, Languages, Sun, Moon } from 'lucide-vue-next';
import { call,native,type Reply,type ImageRelease,type PreparedImage,type ImageProgress } from './api';
import InstallWizard from './InstallWizard.vue';
import RestoreSummary from './RestoreSummary.vue';
const tab=ref('prepare');const busy=ref(false);const polling=ref(false);
const wizardStep=ref(0);
const wizardSession=ref(0);
const devices=ref<any[]>([]);const driver=ref<any>(null);const selected=ref('');
const notice=ref('');const error=ref<{code:string;detail:string}|null>(null);
const observation=ref<any>(null);const backupResult=ref<any>(null);const records=ref<any[]>([]);
const confirmOpen=ref(false);const confirmed=ref(false);const progress=ref<any>(null);
const releases=ref<ImageRelease[]>([]);const preparedImage=ref<PreparedImage|null>(null);
const releaseState=ref<'idle'|'loading'|'ready'|'error'>('idle');const releaseError=ref('');
const imageProgress=ref<ImageProgress|null>(null);const imageWorking=ref(false);const cancelling=ref(false);
const writePlan=ref<any>(null);const writeConfirm=ref('');const writeResult=ref<any>(null);
const bootSelection=ref<any>(null);
const haAddress=ref('http://homeassistant.local:8123');const haToken=ref('');
const appPlan=ref<any>(null);const appConfirm=ref('');const appResult=ref<any>(null);
const backupChoices=ref<any[]>([]);const restoreId=ref('');const gptResult=ref<any>(null);
const catalogLoading=ref(false);let catalogGeneration=0;
const backupDone=ref(false);
const wizardMode=ref<'install'|'restore'>('install');const backupKind=ref('FULL');
const backupPolicy=ref<'full'|'skip'>('full');
const storageExecuting=ref(false);const storageCancelling=ref(false);
const canCancelStorage=computed(()=>storageExecuting.value&&!storageCancelling.value&&progress.value?.phase==='preflight-write');
const advancedBackupMode=ref<'backup'|'restore'|null>(null);
const restoreSource=ref<'manufacturer'|'backup'|null>(null);
const factoryImage=ref<any>(null);
const canFactory=computed(()=>canRead.value&&!!factoryImage.value&&!writePlan.value&&tab.value==='advanced'&&advancedBackupMode.value==='restore'&&restoreSource.value==='manufacturer');
const restoreInfo=computed(()=>backupChoices.value.find(b=>b.id===restoreId.value));
function backupBadge(row:any){return ['FULL','BOOT','HAOS'].includes(row.kind)?row.kind:t('미확인');}
function backupHint(row:any){
 if(!row.restorable)return '형식 확인 불가 · 복원할 수 없음';
 return ({FULL:'전체 디스크 · OS와 데이터 포함',BOOT:'부팅 영역만 · OS와 데이터 제외',HAOS:'HAOS 파티션 · 별도 부팅 펌웨어 제외'} as Record<string,string>)[row.kind]??'형식 확인 불가 · 복원할 수 없음';
}
function selectRestore(id:string){
 if(busy.value||catalogLoading.value||writePlan.value)return;
 const row=backupChoices.value.find(b=>b.id===id);
 if(!row?.restorable||!['FULL','BOOT','HAOS'].includes(row.kind))return;
 restoreId.value=id;
}
const writeNames:Record<string,string>={install:'HAOS 설치',uboot:'U-Boot 업데이트',restore:'부팅 영역 복원','restore-archive':'백업 복원','gpt-repair':'GPT 복구',factory:'제조사 이미지 설치','factory-raw':'제조사 이미지 설치'};
const canExecute=computed(()=>!busy.value&&canRead.value&&!!writePlan.value&&writeConfirm.value==='ok'&&writePlan.value.device.instance_id===selected.value&&writePlan.value.device.location===device.value?.location);
const canRestore=computed(()=>canRead.value&&!catalogLoading.value&&!!restoreInfo.value?.restorable&&['FULL','BOOT','HAOS'].includes(restoreInfo.value.kind)&&restoreInfo.value.identity?.sectors===observation.value?.sectors&&(restoreInfo.value.kind==='FULL'||observation.value?.os==='HAOS'));
let unlistenClose:UnlistenFn|undefined;
let unlistenImage:UnlistenFn|undefined;
let timer:ReturnType<typeof setInterval>|undefined;let unlisten:UnlistenFn|undefined;
const device=computed(()=>devices.value.find(d=>d.instance_id===selected.value));
const supported=computed(()=>device.value?.vid===0x2207&&[0x350a,0x300a].includes(device.value?.pid));
const canInspect=computed(()=>!busy.value&&supported.value&&device.value?.binding&&['Maskrom','Loader'].includes(device.value?.mode));
function matchesObservation(value:any,id:string,location:string){return value?.ready===true&&value.instance_id===id&&value.location===location;}
const readyObservation=computed(()=>matchesObservation(observation.value,selected.value,device.value?.location));
const canRead=computed(()=>canInspect.value&&readyObservation.value);
const canPrepare=computed(()=>canInspect.value&&device.value?.mode==='Maskrom'&&!readyObservation.value);
const tabs=[{id:'prepare',label:'설치·복구',icon:Usb},{id:'advanced',label:'고급 기능',icon:Wrench},{id:'history',label:'작업 기록',icon:History},{id:'settings',label:'설정',icon:Settings}];
const phaseNames:Record<string,string>={upload:'Loader 전송 중',reconnect:'장치 다시 연결 중',read:'eMMC 읽는 중','read-compress':'eMMC 읽기·압축 중',verify:'백업 대조 중','verify-backup-file':'백업 파일 검사 중',complete:'백업 완료','preflight-write':'이미지·기록 범위 확인 중',write:'eMMC 기록 중','verify-write':'기록한 데이터 읽기 대조 중','write-complete':'기록·검증 완료'};
phaseNames['write-starting']='eMMC 기록 중';
phaseNames['factory-check']='제조사 이미지 검사 중';phaseNames['factory-write']='제조사 이미지 기록 중';
const progressLabel=computed(()=>phaseNames[progress.value?.phase]??'장치 확인 중');
const percentage=computed(()=>progress.value?.total?Math.min(100,Math.round(progress.value.completed/progress.value.total*100)):null);
const errorText:Record<string,string>={
 FACTORY_FORMAT:'지원하지 않는 이미지입니다.',
 FACTORY_IMAGE:'제조사 이미지가 손상되었거나 전체 시스템 이미지가 아닙니다.',
 FACTORY_TOOL:'제조사 이미지 도구 실행에 실패했습니다. 상세 정보를 확인하세요.',
 AMBIGUOUS_DEVICE:'제조사 이미지 설치 시 Rockchip 장치는 한 대만 연결하세요.',
 PREVIEW_ONLY:'브라우저 미리보기에서는 PC나 USB에 접근하지 않습니다.',
 HA_AUTH:'Home Assistant 관리자 계정의 장기 액세스 토큰을 확인하세요.',
 HA_ADDRESS:'Home Assistant 주소와 액세스 토큰을 확인하세요.',
 HA_TARGET:'확인한 장치·OS·앱 상태가 달라졌습니다. 다시 확인하세요.',
 HA_BACKUP:'Connectivity 백업을 확인하지 못했습니다. 기존 앱은 유지됩니다.',
 HA_NETWORK:'Home Assistant 연결이 끊겼습니다. 앱 상태를 확인한 뒤 다시 시도하세요.',
 HA_CATALOG_CHANGED:'앱 스토어와 설치 패키지 버전이 다릅니다. 최신 호환성 정보를 확인하세요.',
 HA_VERIFY:'요청한 앱 상태를 확인하지 못했습니다. Home Assistant의 앱 로그를 확인하세요.',
 COMPONENT_INCOMPATIBLE:'선택한 HAOS를 지원하는 Connectivity 설치 패키지가 없습니다. 지원되는 버전을 선택하세요.',
 COMPONENT_UNAVAILABLE:'설치 패키지 목록을 가져오지 못했습니다. 인터넷 연결과 릴리스 게시 상태를 확인하세요.',
 COMPONENT_CATALOG:'설치 패키지의 호환성 정보를 확인하지 못했습니다.',
 COMPONENT_HASH:'설치 구성 파일이 손상됐습니다. 다시 준비해 주세요.',
 COMPONENT_SELECTION:'Connectivity를 포함한 설치 이미지를 먼저 준비하세요.',
 COMPONENT_IO:'구성 파일을 읽거나 저장하지 못했습니다. 공간과 권한을 확인하세요.',
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
 LOADER_READ_RESTRICTED:'현재 Loader는 전체 읽기를 제한합니다. 보드 전원을 분리한 뒤 MASKROM으로 연결하고 이 프로그램에서 장치를 다시 준비하세요.',
 LOADER_CAPABILITY:'Loader의 읽기 기능을 확인하지 못했습니다. MASKROM으로 다시 연결하고 장치를 준비하세요.',
 USB_READ_UNTRUSTED:'실제 디스크 내용인지 확인할 수 없는 데이터입니다. GPT 복구를 진행하지 말고 MASKROM으로 다시 연결해 새 백업을 만드세요.',
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
 RESTORE_LAYOUT:'부분 복원은 동일한 HAOS 파티션 구성이 필요합니다. OS를 바꾸려면 FULL 백업을 사용하세요.',
 BACKUP_DEVICE:'현재 장치와 용량이 일치하는 검증된 백업을 선택하세요.',
 RESTORE_CAPACITY:'백업의 원본 eMMC 용량과 현재 장치의 용량이 다릅니다.',
 BACKUP_FORMAT:'전체 디스크 .img 또는 .k11cbackup 파일을 선택하세요. 제조사 펌웨어 패키지는 사용할 수 없습니다.',
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
async function inspect(){if(!canInspect.value)return false;const id=selected.value,location=device.value.location;busy.value=true;error.value=null;progress.value=null;observation.value=null;try{const r=await call('inspect_device',{instanceId:id});if(!accept(r))return false;if(!matchesObservation(r.data,id,location)||selected.value!==id||device.value?.location!==location){error.value={code:'DEVICE_CHANGED',detail:'Readiness observation does not match selection'};return false;}observation.value=r.data;return true;}finally{busy.value=false;}}
async function connectNext(){
 if(!canInspect.value)return;
 if(device.value.mode==='Loader'||readyObservation.value){if(await inspect())wizardStep.value=2;}
 else{confirmed.value=false;confirmOpen.value=true;}
}
async function connectAdvanced(){
 if(tab.value!=='advanced'||!canInspect.value||writePlan.value||confirmOpen.value)return;
 if(device.value.mode==='Loader'||readyObservation.value)await inspect();
 else{confirmed.value=false;confirmOpen.value=true;}
}
async function prepare(){if(!canPrepare.value||!confirmed.value)return;
 const id=selected.value;const location=device.value.location;confirmOpen.value=false;busy.value=true;error.value=null;progress.value={phase:'inspect'};
 let ready=false;
 try{const r=await call('prepare_device',{instanceId:id,location,confirmedK11c:true});if(accept(r)){updateDevices([r.data.device]);selected.value=r.data.device.instance_id;ready=await scan()&&selected.value===r.data.device.instance_id&&device.value?.location===location&&matchesObservation(r.data.observation,selected.value,location);if(ready){observation.value=r.data.observation;notice.value='장치 준비 완료';}else{observation.value=null;error.value={code:'DEVICE_CHANGED',detail:'Prepared device/readiness did not match the selected USB port'};}}else await scan();}
 finally{busy.value=false;progress.value=null;confirmed.value=false;}
 if(ready&&canRead.value&&tab.value==='prepare')wizardStep.value=2;
}
async function backup(){if(!canRead.value)return false;if(tab.value==='advanced'&&advancedBackupMode.value!=='backup')return false;busy.value=true;error.value=null;backupResult.value=null;progress.value={phase:'read-compress'};
 try{const r=await call('backup_device',{instanceId:selected.value,kind:tab.value==='prepare'?'FULL':backupKind.value});if(!accept(r))return false;backupResult.value=r.data;backupDone.value=tab.value==='advanced';return true;}
 finally{busy.value=false;progress.value=null;}
}
async function backupNext(){if(!canRead.value)return;if(backupPolicy.value==='skip'||(backupResult.value?.verified&&backupResult.value?.kind==='FULL')||await backup())wizardStep.value=3;}
function chooseBackupPolicy(policy:'full'|'skip'){if(busy.value||writePlan.value)return;backupPolicy.value=policy;}
function chooseMode(mode:'install'|'restore'){if(busy.value||writePlan.value)return;wizardMode.value=mode;clearImage();restoreId.value='';}
async function chooseBackup(){
 if(tab.value==='advanced'&&restoreSource.value!=='backup')return;
 if(busy.value||catalogLoading.value)return;busy.value=true;error.value=null;restoreId.value='';progress.value={phase:'verify-backup-file'};
 try{const r=await call('backup_select',{locale:locale.value});if(accept(r)&&r.data){backupChoices.value=[r.data,...backupChoices.value.filter(b=>b.id!==r.data.id)];restoreId.value=r.data.id;if(tab.value==='prepare'&&r.data.kind==='FULL')wizardStep.value=4;}}
 finally{busy.value=false;progress.value=null;}
}
async function chooseBackupMode(mode:'backup'|'restore'|null){
 if(busy.value||writePlan.value||confirmOpen.value)return;
 if(backupDone.value)resetCompletedWork();
 advancedBackupMode.value=mode;restoreId.value='';backupKind.value='FULL';error.value=null;restoreSource.value=null;factoryImage.value=null;
}
async function chooseRestoreSource(source:'manufacturer'|'backup'|null){
 if(busy.value||writePlan.value||confirmOpen.value||advancedBackupMode.value!=='restore')return;
 restoreSource.value=source;restoreId.value='';factoryImage.value=null;error.value=null;
 if(source==='backup')await loadBackups();
}
function restoreWrite(){if(tab.value==='advanced'&&(advancedBackupMode.value!=='restore'||restoreSource.value!=='backup'))return;void planWrite(restoreInfo.value?.legacy?'restore':'restore-archive');}
async function chooseFactory(){
 if(busy.value||writePlan.value||advancedBackupMode.value!=='restore'||restoreSource.value!=='manufacturer')return;
 busy.value=true;error.value=null;factoryImage.value=null;progress.value={phase:'factory-check'};
 try{const r=await call('factory_select',{locale:locale.value});if(accept(r)&&r.data)factoryImage.value=r.data;}
 finally{busy.value=false;progress.value=null;}
}
async function planFactory(){
 if(!canFactory.value)return;
 busy.value=true;error.value=null;writeResult.value=null;progress.value={phase:'factory-check'};
 try{const r=await call('factory_plan',{instanceId:selected.value,location:device.value.location,source:factoryImage.value.id});if(accept(r)){writePlan.value=r.data;writeConfirm.value='';}}
 finally{busy.value=false;progress.value=null;}
}
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
 try{const r=await call(version?'image_download':'image_select',version?{version}:{locale:locale.value});
  if(r.error?.code==='IMAGE_CANCELLED')notice.value='이미지 준비를 취소했습니다.';
  else if(accept(r)&&r.data){preparedImage.value=r.data;wizardStep.value=4;}
 }finally{imageWorking.value=false;busy.value=false;imageProgress.value=null;cancelling.value=false;}
}
async function cancelImage(){if(!imageWorking.value||cancelling.value)return;cancelling.value=true;const r=await call('image_cancel');if(!accept(r))cancelling.value=false;}
async function checkGpt(){if(!canRead.value)return;busy.value=true;error.value=null;gptResult.value=null;try{const r=await call('gpt_check',{instanceId:selected.value});if(accept(r))gptResult.value=r.data;}finally{busy.value=false;}}
async function loadBackups(){
 if(busy.value||catalogLoading.value)return;
 const generation=++catalogGeneration;catalogLoading.value=true;
 try{const r=await call('backup_catalog');if(generation===catalogGeneration&&accept(r)){backupChoices.value=r.data;if(!r.data.some((b:any)=>b.id===restoreId.value&&b.restorable))restoreId.value='';}}
 finally{if(generation===catalogGeneration)catalogLoading.value=false;}
}
async function planWrite(operation:string){
 if(!canRead.value||writePlan.value)return;
 if(operation==='install'&&!preparedImage.value)return;
 if(['restore','restore-archive'].includes(operation)&&!canRestore.value)return;
 if(tab.value==='prepare'&&backupPolicy.value!=='skip'&&(!backupResult.value?.verified||backupResult.value.kind!=='FULL'))return;
 if(tab.value==='prepare'&&wizardMode.value==='restore'&&restoreInfo.value?.kind!=='FULL')return;
 if(operation==='gpt-repair'&&!gptResult.value?.repairable)return;
 let source=operation==='install'?preparedImage.value!.sha256:['restore','restore-archive'].includes(operation)?restoreId.value:'';
 busy.value=true;error.value=null;writeResult.value=null;progress.value={phase:'preflight-write'};notice.value='';
 try{
  if(operation==='uboot'){const boot=await call('boot_prepare');if(!accept(boot))return;bootSelection.value=boot.data;source=boot.data.sha256;}
  const directRestore=tab.value==='advanced'&&['restore','restore-archive'].includes(operation);
  const skipBackup=tab.value==='prepare'&&backupPolicy.value==='skip';
  const recovery=!directRestore&&!skipBackup&&backupResult.value?.verified&&backupResult.value.kind==='FULL'?backupResult.value.id:'';
  const r=await call('storage_plan',{instanceId:selected.value,location:device.value.location,directRestore,skipBackup,operation,source,recovery});
  if(accept(r)){if(r.data.no_changes)notice.value='이미 정상 상태입니다. 변경할 내용이 없습니다.';else{writePlan.value=r.data;writeConfirm.value='';}}
 }finally{busy.value=false;progress.value=null;}
}
function cancelWrite(){if(busy.value)return;writePlan.value=null;writeConfirm.value='';}
async function planApp(operation:string){
 if(busy.value)return;busy.value=true;appPlan.value=null;appResult.value=null;appConfirm.value='';error.value=null;
 try{const r=await call('connectivity_plan',{address:haAddress.value.trim(),token:haToken.value,operation});if(accept(r))appPlan.value=r.data;}
 finally{busy.value=false;}
}
async function executeApp(){
 if(busy.value||!appPlan.value||appConfirm.value!=='K11C')return;
 busy.value=true;error.value=null;appPlan.value=null;appConfirm.value='';haToken.value='';
 try{const r=await call('connectivity_execute',{confirmed:true});if(accept(r)){resetCompletedWork();accept(r);appResult.value=r.data;}}
 finally{busy.value=false;}
}
function cancelApp(){if(busy.value)return;appPlan.value=null;appConfirm.value='';void call('connectivity_forget');}
watch([haAddress,haToken],()=>{if(appPlan.value)cancelApp();});
// Clear this window's completed work, never saved backups, images or history.
function resetCompletedWork(){
 writePlan.value=null;writeConfirm.value='';writeResult.value=null;
 preparedImage.value=null;backupResult.value=null;backupDone.value=false;restoreId.value='';backupChoices.value=[];
 catalogGeneration++;catalogLoading.value=false;
 observation.value=null;gptResult.value=null;bootSelection.value=null;
 appPlan.value=null;appConfirm.value='';appResult.value=null;
 haToken.value='';haAddress.value='http://homeassistant.local:8123';
 confirmOpen.value=false;confirmed.value=false;progress.value=null;
 imageProgress.value=null;imageWorking.value=false;cancelling.value=false;
 notice.value='';error.value=null;wizardStep.value=0;wizardSession.value++;
 wizardMode.value='install';backupKind.value='FULL';backupPolicy.value='full';advancedBackupMode.value=null;restoreSource.value=null;factoryImage.value=null;
}
function dismissCompletedWork(){if(busy.value)return;resetCompletedWork();}
async function executeWrite(){
 if(!canExecute.value)return;
 const id=writePlan.value.plan_id;const factory=['factory','factory-raw'].includes(writePlan.value.operation);busy.value=true;error.value=null;writeResult.value=null;writePlan.value=null;writeConfirm.value='';progress.value={phase:factory?'factory-write':'preflight-write'};storageExecuting.value=!factory;storageCancelling.value=false;
 try{const r=await call(factory?'factory_execute':'storage_execute',{planId:id,confirmed:true});if(r.error?.code==='STORAGE_CANCELLED')notice.value='기록 전에 취소했습니다. eMMC 데이터는 변경되지 않았습니다.';else if(accept(r)&&r.data.verified){resetCompletedWork();accept(r);writeResult.value=r.data;}}
 finally{busy.value=false;progress.value=null;storageExecuting.value=false;storageCancelling.value=false;}
}
async function cancelStorage(){
 if(!canCancelStorage.value)return;storageCancelling.value=true;
 const r=await call('storage_cancel');
 if(r.error?.code==='WRITE_IN_PROGRESS'){progress.value={phase:'write-starting'};storageCancelling.value=false;}
 else if(!accept(r))storageCancelling.value=false;
}
async function navigate(id:string){if(busy.value||writePlan.value)return;if(writeResult.value||appResult.value||backupDone.value)resetCompletedWork();if(id!=='advanced'&&appPlan.value)cancelApp();tab.value=id;if(id==='history'){const r=await call('history');if(accept(r))records.value=r.data;}if(id==='advanced')await loadBackups();}
function operationName(op:string){return ({preflight:'연결 확인','driver-setup':'드라이버 준비',inspect:'저장장치 확인',prepare:'장치 준비',backup:'백업','backup-import':'백업 가져오기','image-releases':'HAOS 버전 조회','image-download':'HAOS 다운로드','image-import':'이미지 가져오기','gpt-check':'GPT 검사','backup-catalog':'백업 목록','storage-plan':'기록 준비','storage-execute':'eMMC 기록·검증','factory-import':'제조사 이미지','factory-plan':'설치 내용 확인','factory-execute':'제조사 이미지 설치'} as Record<string,string>)[op]??op;}
function size(bytes:number){return number(bytes/1024**3,1)+' GiB';}
function clearImage(){if(!busy.value)preparedImage.value=null;}
watch(()=>[selected.value,device.value?.location,device.value?.mode,device.value?.binding].join('|'),()=>{observation.value=null;backupResult.value=null;backupPolicy.value='full';confirmed.value=false;confirmOpen.value=false;if(notice.value==='장치 준비 완료')notice.value='';},{flush:'sync'});
watch(()=>observation.value?.os,()=>{backupKind.value='FULL';});
watch(()=>[selected.value,device.value?.location,device.value?.mode,device.value?.binding,preparedImage.value?.sha256,restoreId.value,factoryImage.value?.id,restoreSource.value].join('|'),()=>{writePlan.value=null;writeConfirm.value='';gptResult.value=null;writeResult.value=null;},{flush:'sync'});
onMounted(async()=>{if(native){unlisten=await listen('usb-progress',e=>{if(busy.value&&progress.value!==null)progress.value=e.payload;});unlistenImage=await listen<ImageProgress>('image-progress',e=>{if(imageWorking.value)imageProgress.value=e.payload;});unlistenClose=await listen('write-close-blocked',()=>{if(busy.value)notice.value='기록·검증이 끝날 때까지 프로그램과 USB 연결을 유지하세요.';});await setup();timer=setInterval(poll,2000);}});
onUnmounted(()=>{if(timer)clearInterval(timer);unlisten?.();unlistenImage?.();unlistenClose?.();});
</script>

<template>
 <div class="shell">
  <aside>
   <div class="brand"><img class="brand-mark" src="/app-icon.svg" alt="" width="40" height="40"/><div>K11C <b>Installer</b><small>{{ t("DEVICE TOOLS") }}</small></div></div>
   <div class="nav-label">{{ t("WORKSPACE") }}</div>
   <nav><button v-for="item in tabs" :key="item.id" :class="{active:tab===item.id}" :disabled="busy" @click="navigate(item.id)"><component :is="item.icon" :size="18"/>{{t(item.label)}}<ChevronRight v-if="tab===item.id" :size="15"/></button></nav>
   <div class="aside-bottom"><span class="portable-dot"></span> {{ t("Portable") }} <span class="muted">0.5.0</span></div>
  </aside>
  <main>
   <header><div><div class="eyebrow">{{ t("K11C DEVICE WORKSPACE") }}</div><h1>{{t(tabs.find(t=>t.id===tab)?.label)}}</h1><p>{{tab==='prepare'?t("K11C에 Home Assistant OS를 설치합니다."):tab==='advanced'?t("부팅 펌웨어와 저장장치를 관리합니다."):tab==='history'?t("이 PC에서 진행한 작업을 확인합니다."):t("USB 드라이버와 저장 위치를 확인합니다.")}}</p></div><div class="header-actions"><div class="appearance-controls"><label class="language-picker"><Languages :size="17" aria-hidden="true"/><select id="display-language" :aria-label="t('언어 선택')" :value="locale" @change="setLocale(($event.target as HTMLSelectElement).value)"><option v-for="language in languages" :key="language.id" :value="language.id" :lang="language.id">{{language.name}}</option></select></label><label class="theme-picker"><Sun v-if="resolvedTheme==='light'" :size="17" aria-hidden="true"/><Moon v-else :size="17" aria-hidden="true"/><select id="display-theme" :aria-label="t('테마 선택')" :value="themePreference" @change="setTheme(($event.target as HTMLSelectElement).value)"><option value="system">{{t('시스템 설정')}}</option><option value="light">{{t('라이트')}}</option><option value="dark">{{t('다크')}}</option></select></label></div><span class="badge"><ShieldCheck :size="14"/> K11C</span></div></header>
   <section class="device-card">
    <div class="device-icon"><Usb :size="28"/></div>
    <div class="device-text"><div class="device-title">{{device?t("Rockchip USB 장치"):t("K11C를 연결해 주세요")}}<span class="pill" :class="{'muted-pill':!device}">{{device?.mode??t("미연결")}}</span></div><p v-if="device">USB {{device.vid.toString(16)}}:{{device.pid.toString(16)}} · {{device.binding?t("연결됨"):t("드라이버 확인 필요")}}</p><p v-else>{{ t("USB 데이터 케이블로 연결하고 MASKROM 모드로 진입하세요.") }}</p></div>
    <button class="icon-button" :disabled="busy" :title="t('연결 다시 확인')" :aria-label="t('연결 다시 확인')" @click="refresh"><RefreshCw :size="18" :class="{spin:busy}"/></button>
   </section>
   <select v-if="devices.length>1" class="device-select" :disabled="busy" v-model="selected"><option value="">{{ t("장치를 선택하세요") }}</option><option v-for="d in devices" :value="d.instance_id">{{d.location}} · {{d.mode}} · {{d.instance_id}}</option></select>
   <div v-if="notice" class="status-line" role="status"><Check v-if="driver?.installed" :size="15"/><Info v-else :size="15"/>{{t(notice)}}</div>
   <div v-if="error" class="error" role="alert"><AlertCircle :size="18"/><div><strong>{{t(errorText[error.code])||t("작업을 완료하지 못했습니다. 상세 내용을 확인해 주세요.")}}</strong><details><summary>{{ t("상세 정보") }}</summary><pre>{{error.code + '\n' + error.detail}}</pre></details></div></div>
   <section v-if="busy&&progress" class="panel progress-panel" role="status"><div class="setting-row"><b>{{t(progressLabel)}}</b><span>{{percentage===null?t("잠시 기다려 주세요"):percentage+'%'}}</span></div><progress v-if="percentage!==null" :value="percentage" max="100"/><progress v-else/><button v-if="storageExecuting&&progress.phase==='preflight-write'" class="folder-button" data-storage-cancel :disabled="!canCancelStorage" @click="cancelStorage">{{storageCancelling?t('취소하는 중'):t('취소')}}</button></section>

   <section v-if="writeResult" class="panel write-result" role="status"><Check :size="24"/><div><h3>{{t('{operation} 완료',{operation:t(writeNames[writeResult.operation])})}}</h3><p>{{ t("기록한 데이터를 다시 읽어 확인했습니다.") }}</p><p v-if="writeResult.operation!=='gpt-repair'">{{ t("USB와 전원을 분리하고, SD 카드 없이 전원을 다시 연결하세요.") }}<span v-if="writeResult.operation==='install'"> {{ t("최초 부팅에는 몇 분 정도 걸릴 수 있습니다.") }}</span></p><details><summary>{{ t("백업·작업 기록") }}</summary><p class="path">{{writeResult.backup_path}}</p><p class="path">{{writeResult.journal}}</p></details></div></section>
   <button v-if="writeResult||appResult||backupDone" class="secondary" data-completion-dismiss :disabled="busy" @click="dismissCompletedWork">{{ t('확인') }}</button>
   <InstallWizard :key="wizardSession" v-show="tab==='prepare'" v-model:step="wizardStep" :mode="wizardMode" :restore-info="restoreInfo" @choose-mode="chooseMode" @select-backup="chooseBackup" :active="tab==='prepare'" :busy="busy||confirmOpen||!!writePlan" :can-connect="canInspect" :loader="device?.mode==='Loader'||readyObservation" :can-install="canRead" :device-label="device?(observation?'eMMC · '+size(observation.bytes):'Rockchip USB · '+device.mode):t('연결 안 됨')" :backup-skipped="backupPolicy==='skip'" :backup-path="backupPolicy==='full'&&backupResult?.verified&&backupResult?.kind==='FULL'?backupResult.path:undefined"
    :releases="releases" :prepared-image="preparedImage" :image-progress="imageProgress" :cancelling="cancelling"
    :releases-loading="releaseState==='loading'" :release-error="t(releaseError)"
    @connect="connectNext" @backup="backupNext" @clear-image="clearImage" @enter-official="ensureReleases" @load-releases="loadReleases" @download="prepareImage" @select-image="prepareImage()" @cancel-image="cancelImage" @install="wizardMode==='install'?planWrite('install'):restoreWrite()">
    <template #connection>
    <section class="panel"><div class="panel-top"><Cable :size="26"/><div><h3>{{ t("보드 연결 준비") }}</h3><p>{{ t("SD 카드를 빼고 K11C를 MASKROM으로 연결하세요.") }}</p></div><span v-if="readyObservation" class="pill">{{ t("준비됨") }}</span></div>
     <ol class="connection-steps"><li>{{ t("Home Assistant를 종료하고 보드 전원을 분리하세요.") }}</li><li>{{ t("MASKROM 버튼을 누른 상태로 USB 데이터 케이블을 PC에 연결하세요.") }}</li><li>{{ t("장치가 표시되면 버튼을 놓고 아래에서 장치를 준비하세요.") }}</li></ol>
     <p class="body-note">{{ t("RKDevTool 등 다른 USB 작업 프로그램은 닫아 주세요.") }}</p>
     <div v-if="observation" class="storage-summary"><HardDrive :size="18"/><strong>eMMC · {{size(observation.bytes)}}</strong><span>{{t('{count} 섹터',{count:number(observation.sectors)})}}</span></div>
    </section>
    </template>
    <template #backup>
    <div class="source-grid" role="group" :aria-label="t('백업 선택')">
     <button class="source-card" data-backup-policy="full" :class="{selected:backupPolicy==='full'}" :aria-pressed="backupPolicy==='full'" :disabled="busy" @click="chooseBackupPolicy('full')"><FileCheck :size="24"/><span><strong>{{t('전체 백업')}}</strong><small>{{t('OS·설정·데이터를 압축 저장하고 원본과 대조합니다.')}}</small></span></button>
     <button class="source-card" data-backup-policy="skip" :class="{selected:backupPolicy==='skip'}" :aria-pressed="backupPolicy==='skip'" :disabled="busy" @click="chooseBackupPolicy('skip')"><ArrowRight :size="24"/><span><strong>{{t('백업 없이 진행')}}</strong><small>{{t('현재 OS와 데이터를 백업하지 않습니다.')}}</small></span></button>
    </div>
    <p v-if="backupPolicy==='skip'" class="body-note warning">{{t('설치·복원 시 기존 데이터가 삭제됩니다. 필요한 백업이 있는지 확인하세요.')}}</p>
    <template v-else>
    <section class="panel"><div class="panel-top"><FileCheck :size="26"/><div><h3>{{ t("전체 백업") }}</h3><p>{{ t("OS·설정·데이터를 압축 저장하고 원본과 대조합니다.") }}</p></div></div>
     <p class="body-note">{{ t("eMMC 전체 용량을 읽으므로 시간이 걸립니다. 압축률은 저장된 데이터에 따라 달라집니다.") }}</p>
     <div class="footer-actions"><button class="folder-button" @click="openBackups"><Folder :size="16"/>{{ t("백업 폴더 열기") }}</button></div>
     <div v-if="backupResult" class="backup-result" role="status"><Check :size="20"/><div><strong>{{ t("백업 저장·검증 완료") }}</strong><p class="path">{{backupResult.path}}</p><p v-if="backupResult.gpt.healthy">{{ t("GPT 정상") }}</p><p v-else class="warning">{{ t("GPT 확인 필요 — 상세 정보에서 검사 결과를 확인하세요.") }}</p><details><summary>{{ t("상세 정보") }}</summary><pre>{{JSON.stringify(backupResult,null,2)}}</pre></details></div></div>
    </section>
    </template>
    </template>
   </InstallWizard>

   <template v-if="tab==='advanced'">
    <div class="advanced-grid">
<section class="panel" data-advanced="backup">
      <div class="panel-top"><FileCheck :size="26"/><div><h3>{{t('고급 백업·복원')}}</h3><p>{{t('전체 디스크 또는 HAOS의 필요한 영역을 백업하고 복원합니다.')}}</p></div></div>
      <div v-if="!readyObservation" class="backup-readiness" data-backup-readiness>
       <strong>{{t('저장장치 확인 필요')}}</strong>
       <p class="body-note">{{device?t('장치를 준비하면 저장장치와 백업 범위를 확인합니다.'):t('USB 데이터 케이블로 연결하고 MASKROM 모드로 진입하세요.')}}</p>
       <button class="secondary" data-advanced-connect :disabled="!canInspect||busy||!!writePlan||confirmOpen" @click="connectAdvanced"><Usb :size="16"/>{{t('장치 준비')}}</button>
      </div>
      <div v-if="advancedBackupMode===null" class="source-grid" :aria-label="t('작업 선택')">
       <button class="source-card" data-backup-mode="backup" :disabled="busy||!!writePlan||confirmOpen" @click="chooseBackupMode('backup')"><FileCheck :size="24"/><span><strong>{{t('백업')}}</strong><small>{{t('연결한 장치의 내용을 PC에 저장합니다.')}}</small></span></button>
       <button class="source-card" data-backup-mode="restore" :disabled="busy||!!writePlan||confirmOpen" @click="chooseBackupMode('restore')"><HardDrive :size="24"/><span><strong>{{t('복원')}}</strong><small>{{t('제조사 이미지나 저장된 백업으로 되돌립니다.')}}</small></span></button>
      </div>
      <template v-else>
       <div class="backup-task-heading"><h4>{{t(advancedBackupMode==='backup'?'백업':'복원')}}</h4><button class="folder-button" data-backup-back :disabled="busy||!!writePlan||confirmOpen" @click="chooseBackupMode(null)">{{t('작업 다시 선택')}}</button></div>
       <div v-if="advancedBackupMode==='backup'" data-backup-pane="backup">
      <template v-if="readyObservation"><label for="backup-kind">{{t('백업 범위')}}</label><select id="backup-kind" class="wizard-input" :disabled="busy||!canRead" v-model="backupKind"><option value="FULL">{{t('FULL · 전체 디스크')}}</option><option v-if="observation?.os==='HAOS'" value="BOOT">{{t('BOOT · 부팅 펌웨어와 GPT')}}</option><option v-if="observation?.os==='HAOS'" value="HAOS">{{t('HAOS · OS·앱·설정·데이터')}}</option></select></template>
      <p v-if="readyObservation&&observation?.os!=='HAOS'" class="body-note">{{t('HAOS가 아닌 디스크는 전체 백업만 지원합니다.')}}</p>
      <details><summary>{{t('백업 범위 안내')}}</summary><p>{{t('FULL은 eMMC 사용자 영역 전체입니다. 별도 하드웨어 영역인 boot0·boot1·RPMB·OTP는 포함하지 않습니다.')}}</p><p>{{t('BOOT에는 OS와 사용자 데이터가 없으며, HAOS에는 별도 부팅 펌웨어가 없습니다.')}}</p></details>
      <div class="advanced-actions"><button class="secondary" :disabled="!canRead" @click="backup">{{t('백업 시작')}}</button><button class="folder-button" :disabled="busy" @click="openBackups"><Folder :size="16"/>{{t('백업 폴더 열기')}}</button></div>
       </div>
       <div v-else-if="restoreSource===null" class="source-grid" :aria-label="t('복원할 이미지 종류')">
        <button class="source-card" data-restore-source="manufacturer" :disabled="busy||!!writePlan||confirmOpen" @click="chooseRestoreSource('manufacturer')"><Cpu :size="24"/><span><strong>{{t('제조사 이미지')}}</strong><small>{{t('Android·Linux 전체 시스템 이미지를 설치합니다.')}}</small></span></button>
        <button class="source-card" data-restore-source="backup" :disabled="busy||!!writePlan||confirmOpen" @click="chooseRestoreSource('backup')"><FileCheck :size="24"/><span><strong>{{t('저장된 백업')}}</strong><small>{{t('백업한 OS와 데이터를 되돌립니다.')}}</small></span></button>
       </div>
       <div v-else-if="restoreSource==='manufacturer'" data-backup-pane="manufacturer">
        <div class="backup-task-heading"><h4>{{t('제조사 이미지')}}</h4><button class="folder-button" data-restore-source-back :disabled="busy||!!writePlan||confirmOpen" @click="chooseRestoreSource(null)">{{t('이미지 종류 다시 선택')}}</button></div>
        <p class="body-note">{{t('KICKPI K11C용 전체 시스템 이미지를 선택하세요.')}}</p>
        <div class="advanced-actions"><button class="secondary" data-factory-select :disabled="busy||!!writePlan" @click="chooseFactory">{{t('이미지 파일 선택')}}</button></div>
        <div v-if="factoryImage" class="backup-result"><FileCheck :size="20"/><div><strong>{{factoryImage.filename}}</strong><p>{{factoryImage.os}} · {{factoryImage.format}} · {{size(factoryImage.bytes)}}</p><p class="body-note">{{factoryImage.tool}}</p><p class="path">{{factoryImage.path}}</p></div></div>
        <p class="warning">{{t('현재 디스크의 OS와 사용자 데이터가 교체됩니다.')}}</p>
        <div class="advanced-actions"><button class="primary" data-storage-action="factory" :disabled="!canFactory" @click="planFactory">{{t('설치 내용 확인')}}</button></div>
       </div>
       <div v-else data-backup-pane="restore">
      <div class="backup-task-heading"><h4>{{t('저장된 백업')}}</h4><button class="folder-button" data-restore-source-back :disabled="busy||!!writePlan||confirmOpen" @click="chooseRestoreSource(null)">{{t('이미지 종류 다시 선택')}}</button></div>
      <p id="restore-backup-label" class="backup-list-heading">{{t('복원할 백업')}}</p>
      <div id="restore-backup" class="backup-choice-list" role="radiogroup" aria-labelledby="restore-backup-label">
       <label v-for="b in backupChoices" :key="b.id" class="backup-choice" :class="{selected:restoreId===b.id,unavailable:!b.restorable}" :data-backup-id="b.id">
        <input type="radio" name="restore-backup-choice" :value="b.id" :checked="restoreId===b.id" :disabled="busy||catalogLoading||!!writePlan||!b.restorable||!['FULL','BOOT','HAOS'].includes(b.kind)" @change="selectRestore(b.id)"/>
        <span class="backup-choice-copy"><span class="backup-choice-title"><span class="backup-badge">{{backupBadge(b)}}</span><span class="backup-choice-name">{{b.display_name??b.id}}</span></span><small class="backup-choice-note">{{t(backupHint(b))}}</small></span>
       </label>
       <p v-if="catalogLoading" class="body-note" role="status">{{t('백업 목록을 불러오는 중입니다.')}}</p>
       <p v-else-if="!backupChoices.length" class="body-note">{{t('저장된 백업이 없습니다. 백업 파일을 가져오세요.')}}</p>
      </div>
      <div class="advanced-actions"><button class="folder-button" :disabled="busy||catalogLoading" @click="chooseBackup">{{t('백업 파일 가져오기')}}</button><button class="folder-button" :disabled="busy||catalogLoading" @click="loadBackups">{{t('목록 새로고침')}}</button></div>
      <p v-if="restoreInfo" class="path">{{restoreInfo.path}}</p>
      <RestoreSummary :info="restoreInfo"/>
      <p class="body-note">{{t('현재 장치를 추가로 백업하지 않고 복원합니다.')}}</p>
      <p v-if="restoreInfo?.restorable&&readyObservation&&restoreInfo.identity?.sectors!==observation?.sectors" class="warning">{{t('백업의 원본 eMMC 용량과 현재 장치의 용량이 다릅니다.')}}</p>
      <div class="advanced-actions"><button class="primary" data-storage-action="restore" :disabled="!canRestore" @click="restoreWrite">{{t('백업 복원')}}</button></div>
       </div>
      </template>
      <div v-if="backupDone&&backupResult" class="backup-result" role="status"><Check :size="20"/><div><strong>{{t('백업 저장·검증 완료')}}</strong><p class="path">{{backupResult.path}}</p><p>{{size(backupResult.bytes)}} → {{size(backupResult.stored_bytes)}}</p><details><summary>{{t('상세 정보')}}</summary><pre>{{JSON.stringify(backupResult,null,2)}}</pre></details></div></div>
     </section>
<section class="panel" data-advanced="uboot"><div class="panel-top"><Cpu :size="26"/><div><h3>{{ t("U-Boot만 업데이트") }}</h3><p>{{ t("OS와 사용자 데이터를 유지하고 부팅 펌웨어를 교체합니다.") }}</p></div></div><p class="body-note">{{ t("GitHub 릴리스 확인 · 기존 펌웨어 자동 백업") }}</p><p v-if="bootSelection" class="body-note">{{bootSelection.revision}} · {{bootSelection.source==='github'?'GitHub':t("보관된 릴리스")}}</p><div class="advanced-actions"><button class="secondary" data-storage-action="uboot" :disabled="!canRead" @click="planWrite('uboot')">{{ t("U-Boot 업데이트") }}</button></div></section>
<section class="panel" data-advanced="connectivity"><div class="panel-top"><Wrench :size="26"/><div><h3>{{ t("Connectivity 관리") }}</h3><p>{{ t("부팅된 Home Assistant에 연결해 앱을 제거하거나 새로 설치합니다.") }}</p></div></div>
     <label for="ha-address">{{ t("Home Assistant 주소") }}</label><input id="ha-address" class="wizard-input" v-model="haAddress" :disabled="busy" placeholder="http://192.168.1.100:8123"/>
     <label for="ha-token">{{ t("관리자 액세스 토큰") }}</label><input id="ha-token" type="password" class="wizard-input" v-model="haToken" :disabled="busy" autocomplete="off"/>
     <p class="body-note">{{ t("HA 프로필 → 보안 → 장기 액세스 토큰에서 발급하세요. 토큰은 이 창을 닫으면 사라집니다. HTTP 연결은 신뢰하는 내부 네트워크에서 사용하세요.") }}</p>
     <div class="advanced-actions"><button class="secondary" data-app-action="remove" :disabled="busy||!haToken" @click="planApp('remove')">{{ t("제거 준비") }}</button><button class="secondary" data-app-action="reinstall" :disabled="busy||!haToken" @click="planApp('reinstall')">{{ t("새로 설치 준비") }}</button></div>
     <div v-if="appPlan" class="backup-result"><div><h3>{{appPlan.operation==='remove'?t("Connectivity 제거"):t("Connectivity 새로 설치")}}</h3><p>{{appPlan.target.hostname}} · HAOS {{appPlan.target.haos}} · {{appPlan.target.kernel}}</p><p v-if="appPlan.candidate">{{ t("설치할 앱") }} {{appPlan.candidate.app_version}}</p><p class="warning">{{ t("기존 Connectivity 설정·데이터를 삭제합니다. 복구용 백업은 HA에 남습니다. Wi-Fi 연결이 끊길 수 있으므로 유선 LAN을 사용하세요.") }}</p><p v-if="appPlan.operation==='reinstall'">{{ t("기존 설정을 복원하지 않고 기본 설정으로 시작합니다.") }}</p><label for="app-confirm">{{ t("연결한 보드가 K11C인지 확인하고 K11C를 입력하세요.") }}</label><input id="app-confirm" class="wizard-input" v-model="appConfirm" :disabled="busy" autocomplete="off"/><div class="advanced-actions"><button class="secondary" :disabled="busy" @click="cancelApp">{{ t("취소") }}</button><button class="primary" data-app-action="execute" :disabled="busy||appConfirm!=='K11C'" @click="executeApp">{{appPlan.operation==='remove'?t("제거"):t("새로 설치")}}</button></div></div></div>
     <div v-if="appResult" class="backup-result" role="status"><Check :size="20"/><div><strong>{{appResult.operation==='remove'?t("제거 완료"):t("새로 설치 완료")}}</strong><p>{{ t("이미 로드된 드라이버까지 반영하려면 HAOS를 재부팅하세요.") }}</p><p v-if="appResult.backup">{{ t("복구용 HA 백업:") }} {{appResult.backup}}</p></div></div>
    </section>
<section class="panel"><div class="panel-top"><HardDrive :size="26"/><div><h3>{{ t("저장장치 정보") }}</h3><p>{{ t("eMMC 용량과 GPT 헤더 정보를 읽습니다.") }}</p></div></div><p class="body-note">{{ t("연결된 장치의 준비 상태를 확인합니다.") }}</p><button class="secondary" :disabled="!canInspect" @click="inspect"><RefreshCw :size="16"/>{{ t("정보 확인") }}</button><template v-if="observation"><div class="setting-row"><span>{{ t("저장장치") }}</span><b>eMMC · {{size(observation.bytes)}}</b></div><div class="setting-row"><span>{{ t("주 GPT 헤더") }}</span><b>{{observation.primary_gpt_signature?t("있음"):t("없음")}}</b></div><div class="setting-row"><span>{{ t("보조 GPT 헤더") }}</span><b>{{observation.backup_gpt_signature?t("있음"):t("없음")}}</b></div><details><summary>{{ t("상세 정보") }}</summary><pre>{{JSON.stringify(observation,null,2)}}</pre></details></template></section>
<section class="panel" data-advanced="gpt"><div class="panel-top"><ShieldCheck :size="26"/><div><h3>{{ t("GPT 검사·복구") }}</h3><p>{{ t("주·보조 파티션 테이블을 대조하고 불일치를 복구합니다.") }}</p></div></div><div class="advanced-actions"><button class="secondary" data-storage-action="gpt-check" :disabled="!canRead" @click="checkGpt">{{ t("GPT 검사") }}</button><button class="secondary" data-storage-action="gpt-repair" :disabled="!canRead||!gptResult?.repairable" @click="planWrite('gpt-repair')">{{ t("GPT 복구") }}</button></div><div v-if="gptResult" class="body-note"><strong :class="gptResult.gpt.healthy?'success':'warning'">{{gptResult.gpt.healthy?t("GPT 정상"):gptResult.repairable?t("GPT 복구 가능"):t("자동 복구 불가")}}</strong><details><summary>{{ t("검사 결과") }}</summary><pre>{{JSON.stringify(gptResult,null,2)}}</pre></details></div></section>
    </div>
   </template>
   <template v-if="tab==='settings'"><section class="panel driver-settings"><div class="panel-top"><ShieldCheck :size="25"/><div><h3>{{ t("USB 드라이버") }}</h3><p>Rockchip Rockusb</p></div><span class="pill" :class="{'muted-pill':!driver?.installed}">{{driver?.installed?t("준비됨"):t("확인 필요")}}</span></div><div class="setting-row"><span>{{ t("설치된 버전") }}</span><b>{{driver?.version??t("미확인")}}</b></div><details><summary>{{ t("상세 정보") }}</summary><pre>{{driver?.published_inf??t("설치 정보 없음")}}</pre></details><button class="secondary" :disabled="busy" @click="refresh"><RefreshCw :size="16"/>{{ t("상태 다시 확인") }}</button></section><section class="panel compact"><h3>{{ t("저장 위치") }}</h3><p>{{ t('백업: backup') }}<br>{{ t("작업 기록: data / operations.jsonl") }}</p><button class="folder-button" @click="openBackups"><Folder :size="16"/>{{ t("백업 폴더 열기") }}</button></section></template>
   <template v-if="tab==='history'"><section class="panel"><h3>{{ t("최근 작업") }}</h3><div v-if="!records.length" class="empty-feature"><History :size="30"/><p>{{ t("아직 기록된 작업이 없습니다.") }}</p></div><div v-for="(r,i) in records" :key="i" class="history-row"><span :class="r.result.ok?'success':'failed'">{{r.result.ok?t("완료"):t("확인 필요")}}</span><b>{{t(operationName(r.operation))}}</b><time>{{dateTime(r.time)}}</time><details><summary>{{ t("상세 정보") }}</summary><pre>{{JSON.stringify(r.result,null,2)}}</pre></details></div></section></template>
  </main>
  <div v-if="writePlan" class="modal-backdrop" @keydown.esc="cancelWrite"><section class="modal write-modal" role="dialog" aria-modal="true" aria-labelledby="write-title">
   <button class="close-button" :aria-label="t('기록 취소')" @click="cancelWrite"><X :size="20"/></button><HardDrive :size="28"/>
   <h2 id="write-title">{{t('{operation} 확인',{operation:t(writeNames[writePlan.operation])})}}</h2>
   <p>KICKPI K11C · eMMC {{size(writePlan.identity.sectors*512)}}<br>{{writePlan.device.location}}</p>
   <RestoreSummary v-if="['restore','restore-archive'].includes(writePlan.operation)" :info="restoreInfo"/>
   <p class="install-warning" v-if="writePlan.erases_user_data">{{ t('모든 데이터가 삭제됩니다. 확인하셨다면 아래에 ok를 입력해주세요.') }}</p><p v-else>{{ t('아래 기록 범위의 데이터를 교체합니다. 전원과 USB 연결을 유지하세요.') }}</p>
   <p v-if="writePlan.backup_path" class="body-note">{{ t('복구용 백업') }} <span class="path">{{writePlan.backup_path}}</span></p><p v-else class="body-note">{{t('현재 장치를 추가로 백업하지 않고 복원합니다.')}}</p><details><summary>{{ t("기록 범위") }}</summary><div v-for="r in writePlan.ranges" :key="r.lba" class="setting-row"><span>{{t(r.label)}}</span><b>LBA {{r.lba}} · {{number(r.bytes/1024**2,1)}} MiB</b></div></details>
   <label for="write-confirm">{{ t('확인하셨다면 아래에 ok를 입력해주세요.') }}</label><input id="write-confirm" class="wizard-input" autocomplete="off" :value="writeConfirm" @input="writeConfirm=($event.target as HTMLInputElement).value"/>
   <div class="modal-actions"><button class="secondary" @click="cancelWrite">{{ t("취소") }}</button><button class="primary" data-storage-action="execute" :disabled="!canExecute" @click="executeWrite">{{ t("기록 시작") }}<ArrowRight :size="16"/></button></div>
  </section></div>
  <div v-if="confirmOpen" class="modal-backdrop" @keydown.esc="confirmOpen=false"><section class="modal" role="dialog" aria-modal="true" aria-labelledby="confirm-title"><button class="close-button" :aria-label="t('닫기')" @click="confirmOpen=false"><X :size="20"/></button><Usb :size="30"/><h2 id="confirm-title">{{ t("K11C 장치 준비") }}</h2><p>{{ t("장치를 확인하고, 필요한 경우 RAM Loader를 전송합니다.") }}<br>{{ t("전송이 끝날 때까지 케이블을 연결해 두세요.") }}</p><p class="body-note">{{device?.location}} · USB {{device?.vid.toString(16)}}:{{device?.pid.toString(16)}}</p><label class="confirm-check"><input type="checkbox" v-model="confirmed"/>{{ t("연결한 보드가 KICKPI K11C입니다.") }}</label><div class="modal-actions"><button class="secondary" @click="confirmOpen=false">{{ t("취소") }}</button><button class="primary" :disabled="!confirmed||!canPrepare" @click="prepare">{{ t("준비 시작") }}<ArrowRight :size="16"/></button></div></section></div>
 </div>
</template>
