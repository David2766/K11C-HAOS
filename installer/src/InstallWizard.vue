<script setup lang="ts">
import { t, number } from './i18n';
import { ref, computed, watch } from 'vue';
import { ArrowLeft, ArrowRight, Download, FolderOpen, HardDrive, Info, ShieldCheck } from 'lucide-vue-next';
import type { ImageRelease, PreparedImage, ImageProgress } from './api';
import RestoreSummary from './RestoreSummary.vue';

const props=defineProps<{ active:boolean; busy: boolean; step:number; mode:'install'|'restore'; restoreInfo:any; canConnect:boolean; loader:boolean; deviceLabel: string; backupPath?: string; releases:ImageRelease[]; releasesLoading:boolean; releaseError:string; preparedImage:PreparedImage|null; imageProgress:ImageProgress|null; cancelling:boolean; canInstall:boolean }>();
const emit=defineEmits<{ 'update:step':[step:number]; 'choose-mode':[mode:'install'|'restore']; connect:[]; backup:[]; 'enter-official':[]; 'load-releases':[]; download:[version:string]; 'select-image':[]; 'select-backup':[]; 'clear-image':[]; 'cancel-image':[]; install:[] }>();
const steps = ['작업 선택','연결', '전체 백업', '파일 선택', '확인·실행'];
const current = computed(()=>props.step);
const direction = ref('forward');
watch(current,(next,previous)=>{direction.value=next>previous?'forward':'backward';},{flush:'sync'});
const source = ref<'official' | 'local'>('official');
const version=ref('');
watch(()=>props.active && current.value===3 && props.mode==='install' && source.value==='official' && !props.busy,visible=>{if(visible)emit('enter-official');},{immediate:true});
watch([()=>props.releases,()=>props.busy],([rows,locked])=>{if(!locked&&!rows.some(r=>r.version===version.value))chooseVersion(rows.find(r=>r.sha256)?.version??'');});
const selectedRelease=computed(()=>props.releases.find(r=>r.version===version.value));
const canDownload=computed(()=>!props.busy&&!props.releasesLoading&&!!selectedRelease.value?.sha256);
const phaseNames:Record<string,string>={choose:'이미지 파일 선택',resolve:'공식 릴리스 확인 중',download:'다운로드 중','verify-download':'다운로드 파일 검사 중',expand:'압축 푸는 중',copy:'이미지 가져오는 중',inspect:'파티션·부팅 파일 검사 중','verify-image':'저장된 이미지 검사 중','verify-cache':'기존 이미지 검사 중',ready:'이미지 준비 완료'};
Object.assign(phaseNames,{'verify-component':'설치 구성 검사 중','download-component':'U-Boot·Connectivity 다운로드 중','cache-component':'설치 구성 보관 중','download-connectivity':'Connectivity 다운로드 중','prepare-connectivity':'Connectivity 준비 중','register-connectivity':'Connectivity 사전 설치 중','verify-official':'공식 이미지 대조 중','copy-official-os':'공식 OS 복사 중','add-connectivity':'Connectivity 사전 설치 중','verify-installation':'설치 이미지 검사 중'});
const percent=computed(()=>props.imageProgress?.total?Math.min(100,Math.round(props.imageProgress.completed/props.imageProgress.total*100)):null);
function bytes(n:number){return n>=1024**3?number(n/1024**3,2)+' GiB':number(n/1024**2,1)+' MiB';}
function selectImage(){if(!props.busy)emit('select-image');}
function loadReleases(){if(!props.busy&&!props.releasesLoading)emit('load-releases');}
function chooseSource(next:'official'|'local'){if(props.busy||source.value===next)return;source.value=next;emit('clear-image');}
function chooseVersion(next:string){if(props.busy||version.value===next)return;version.value=next;if(source.value==='official')emit('clear-image');}
const restoreReady=computed(()=>props.restoreInfo?.kind==='FULL'&&props.restoreInfo?.restorable);
const primaryDisabled=computed(()=>props.busy || (current.value===0 ? false : current.value===1 ? !props.canConnect : current.value===2 ? !props.canInstall : current.value===3 ? props.mode==='install'&&!props.preparedImage&&source.value==='official'&&!canDownload.value : !props.canInstall||!props.backupPath||(props.mode==='install'?!props.preparedImage:!restoreReady.value)));
const primaryLabel=computed(()=>{
  if(props.busy)return '작업 중…';
  if(current.value===0)return '다음: 연결';
  if(current.value===1)return props.loader?'장치 확인 후 다음':'장치 준비 후 다음';
  if(current.value===2)return props.backupPath?'다음: 파일 선택':'백업 후 다음';
  if(current.value===3){if(props.mode==='restore')return restoreReady.value?'다음: 확인':'파일 선택 후 다음';return props.preparedImage?'다음: 확인':source.value==='official'?'다운로드 후 다음':'파일 선택 후 다음';}
  return props.mode==='install'?'설치 준비':'복원 준비';
});
function primary(){
  if(primaryDisabled.value)return;
  if(current.value===0)goTo(1,false);
  else if(current.value===1)emit('connect');
  else if(current.value===2)emit('backup');
  else if(current.value===3){
    if(props.mode==='restore'){if(restoreReady.value)goTo(4,false);else emit('select-backup');return;}
    if(props.preparedImage)goTo(4,false);
    else if(source.value==='official')emit('download',version.value);
    else selectImage();
  }else emit('install');
}

function goTo(next: number, locked: boolean) {
  if (locked || next < 0 || next >= steps.length || next === current.value) return;
  emit('update:step',next);
}
</script>

<template>
  <section class="install-wizard" :aria-label="t('설치·복구 단계')">
    <ol class="wizard-steps" :aria-label="t('단계 선택')">
      <li v-for="(step, index) in steps" :key="step">
        <button :aria-current="current === index ? 'step' : undefined" :disabled="busy" @click="goTo(index, busy)">
          <span class="wizard-number">{{ String(index + 1).padStart(2, '0') }}</span>
          <span>{{ t(step) }}</span>
        </button>
      </li>
    </ol>

    <div :key="current" class="wizard-slide" :class="direction" :data-step="current" role="region" :aria-label="t(steps[current])">
      <div class="wizard-heading"><h2 aria-live="polite">{{ t(steps[current]) }}</h2></div>
      <section v-if="current===0" class="source-grid">
        <button class="source-card" data-wizard-mode="install" :class="{selected:mode==='install'}" :disabled="busy" @click="emit('choose-mode','install')"><Download :size="24"/><span><strong>{{t('HAOS 설치')}}</strong><small>{{t('공식 HAOS와 K11C 지원 기능을 설치합니다.')}}</small></span></button>
        <button class="source-card" data-wizard-mode="restore" :class="{selected:mode==='restore'}" :disabled="busy" @click="emit('choose-mode','restore')"><HardDrive :size="24"/><span><strong>{{t('백업 복원')}}</strong><small>{{t('전체 백업으로 이전 OS와 데이터를 되돌립니다.')}}</small></span></button>
      </section>
      <slot v-else-if="current === 1" name="connection" />
      <slot v-else-if="current === 2" name="backup" />
      <section v-else-if="current===3&&mode==='restore'" class="panel">
        <h3>{{t('복원할 백업')}}</h3><p>{{t('FULL 백업 또는 전체 디스크 .img 파일을 선택하세요.')}}</p>
        <p class="path">{{restoreInfo?.path}}</p><p v-if="restoreInfo&&restoreInfo.kind!=='FULL'" class="warning">{{t('여기서는 전체 백업만 복원할 수 있습니다. 부분 복원은 고급 기능을 사용하세요.')}}</p>
        <RestoreSummary :info="restoreInfo"/>
        <button v-if="restoreInfo" class="folder-button" :disabled="busy" @click="emit('select-backup')">{{t('파일 변경')}}</button>
      </section>

      <template v-else-if="current === 3">
        <div class="source-grid" role="group" :aria-label="t('HAOS 이미지 가져오기')">
          <button class="source-card" :class="{ selected: source === 'official' }" :aria-pressed="source === 'official'" :disabled="busy" @click="chooseSource('official')">
            <Download :size="24"/><span><strong>{{ t("공식 다운로드") }}</strong><small>{{ t("Home Assistant OS 버전 선택") }}</small></span><span class="radio" aria-hidden="true"><span v-if="source === 'official'" class="radio-dot"/></span>
          </button>
          <button class="source-card" :class="{ selected: source === 'local' }" :aria-pressed="source === 'local'" :disabled="busy" @click="chooseSource('local')">
            <FolderOpen :size="24"/><span><strong>{{ t("내 PC에서 선택") }}</strong><small>{{ t("다운로드한 HAOS 이미지 사용") }}</small></span><span class="radio" aria-hidden="true"><span v-if="source === 'local'" class="radio-dot"/></span>
          </button>
        </div>
        <section v-if="source === 'official'" class="panel image-panel" :aria-label="t('공식 HAOS 다운로드')" :aria-busy="releasesLoading">
          <div class="panel-top"><span class="ha-mark"><Download :size="24"/></span><div><h3>Home Assistant OS</h3><p>{{ t("공식 릴리스에서 설치할 버전을 선택하세요.") }}</p></div></div>
          <div class="input-row">
            <div><label for="haos-version">{{ t("설치 버전") }}</label><select id="haos-version" class="wizard-input" :value="version" :disabled="busy||releasesLoading||!releases.length" @change="chooseVersion(($event.target as HTMLSelectElement).value)"><option value="">{{releasesLoading?t("버전 목록 불러오는 중…"):t("버전을 선택하세요")}}</option><option v-for="r in releases" :key="r.version" :value="r.version" :disabled="!r.sha256">HAOS {{r.version}}{{r.sha256?'':t(" · 공식 해시 없음")}}</option></select></div>
            <div class="platform"><label for="haos-platform">{{ t("플랫폼") }}</label><input id="haos-platform" class="wizard-input" value="generic-aarch64" disabled/></div>
          </div>
          <p v-if="releasesLoading" class="release-status" role="status">{{ t("공식 버전 목록을 불러오는 중입니다.") }}</p>
          <p v-else-if="releaseError" class="release-status warning" role="alert">{{releaseError}}</p>
          <div class="footer-actions image-actions"><button class="folder-button" :disabled="busy||releasesLoading" @click="loadReleases">{{releaseError?t("다시 시도"):t("버전 목록 새로고침")}}</button><span v-if="selectedRelease" class="image-size">{{bytes(selectedRelease.size)}}</span></div>
        </section>
        <section v-else class="panel image-panel" :aria-label="t('로컬 HAOS 이미지')">
          <div class="panel-top"><FolderOpen :size="26"/><div><h3>{{ t("HAOS 이미지 파일") }}</h3><p>{{ t("공식 generic-aarch64 이미지 · .img.xz 또는 .img") }}</p></div></div>
          <div class="file-placeholder"><HardDrive :size="24"/><span>{{preparedImage?.filename??t("아래에서 파일을 선택하세요.")}}</span><button v-if="preparedImage" class="folder-button" :disabled="busy" @click="selectImage">{{ t("파일 변경") }}</button></div>
        </section>
        <section v-if="imageProgress" class="panel image-work" role="status"><div class="setting-row"><b>{{cancelling?t("취소하는 중"):t(phaseNames[imageProgress.phase])||t("이미지 준비 중")}}</b><span>{{percent===null?bytes(imageProgress.completed):percent+'%'}}</span></div><progress v-if="percent!==null" :value="percent" max="100"/><progress v-else/><button v-if="imageProgress.phase!=='choose'" class="folder-button" :disabled="cancelling" @click="emit('cancel-image')">{{ t("취소") }}</button></section>
        <section v-if="preparedImage" class="panel image-result" role="status"><ShieldCheck :size="22"/><div><strong>{{ t("이미지 준비 완료") }}</strong><p>{{preparedImage.filename}} · {{bytes(preparedImage.bytes)}}</p><p>{{preparedImage.verification==='official_os_factory_data'?t("공식 OS 해시 · Connectivity 호환성 · U-Boot 확인"):preparedImage.verification==='github_sha256'?t("공식 SHA-256 · 파티션 · ARM64 부팅 파일 확인"):t("파티션 · ARM64 부팅 파일 확인 · 공식 해시 대조 안 됨")}}</p><details><summary>{{ t("파일 정보") }}</summary><dl><dt>{{ t("저장 위치") }}</dt><dd>{{preparedImage.path}}</dd><dt>{{ t("이미지 SHA-256") }}</dt><dd>{{preparedImage.sha256}}</dd><dt>{{ t("원본 파일 SHA-256") }}</dt><dd>{{preparedImage.source_sha256}}</dd></dl></details></div></section>
      </template>

      <section v-else class="panel install-review" :aria-label="t(mode==='restore'?'복원 내용 확인':'설치 내용 확인')">
        <div class="panel-top"><HardDrive :size="26"/><div><h3>{{ t(mode==='restore'?'복원 내용 확인':'설치 내용 확인') }}</h3><p>{{ t(mode==='restore'?'복원할 장치와 백업 파일을 확인하세요.':'설치할 장치와 이미지를 확인하세요.') }}</p></div></div>
        <dl class="review-list">
          <div><dt>{{ t("대상 장치") }}</dt><dd>{{ deviceLabel }}</dd></div>
          <div><dt>{{ t("전체 백업") }}</dt><dd :class="{ 'review-path': backupPath }">{{ backupPath || t("백업 없음") }}</dd></div>
          <div v-if="mode==='restore'"><dt>{{t('복원할 백업')}}</dt><dd class="review-path">{{restoreInfo?.path??t('백업을 선택하세요')}}</dd></div>
          <template v-else>
            <div><dt>{{ t("HAOS 이미지") }}</dt><dd>{{preparedImage?.filename??t("이미지 선택 안 됨")}}</dd></div>
            <div><dt>U-Boot</dt><dd>{{preparedImage?.uboot??t("준비 전")}}</dd></div>
            <div><dt>Connectivity</dt><dd>{{preparedImage?.connectivity??t("준비 전")}} {{ t("· 첫 부팅 자동 시작") }}</dd></div>
            <div v-if="preparedImage?.kernel"><dt>{{ t("지원 커널") }}</dt><dd>{{preparedImage.kernel}}</dd></div>
            <div v-if="preparedImage?.catalog_source==='cached'"><dt>{{ t("설치 구성") }}</dt><dd>{{ t("보관된 릴리스 사용") }}</dd></div>
          </template>
        </dl>
        <RestoreSummary v-if="mode==='restore'" :info="restoreInfo"/>
        <div class="install-warning"><Info :size="18"/><p>{{ t("기존 OS·HA 설정·사용자 데이터가 삭제됩니다.") }}</p></div>
        <p v-if="mode==='restore'" class="body-note">{{t('백업 내용을 그대로 복원합니다. U-Boot와 Connectivity를 새 버전으로 교체하지 않습니다.')}}</p>
      </section>
    </div>

    <footer class="wizard-navigation">
      <button class="wizard-back" :disabled="busy || current === 0" @click="goTo(current - 1, busy)"><ArrowLeft :size="16"/>{{ t("이전") }}</button>
      <span class="wizard-position">{{ current + 1 }} / {{ steps.length }}</span>
      <button class="primary wizard-next" data-wizard-primary :data-storage-action="current===4?mode:undefined" :disabled="primaryDisabled" @click="primary">{{t(primaryLabel)}}<ArrowRight :size="17"/></button>
    </footer>
  </section>
</template>
