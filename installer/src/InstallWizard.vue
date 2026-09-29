<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { ArrowLeft, ArrowRight, Download, FolderOpen, HardDrive, Info, ShieldCheck } from 'lucide-vue-next';
import type { ImageRelease, PreparedImage, ImageProgress } from './api';

const props=defineProps<{ active:boolean; busy: boolean; step:number; canConnect:boolean; loader:boolean; deviceLabel: string; backupPath?: string; releases:ImageRelease[]; releasesLoading:boolean; releaseError:string; preparedImage:PreparedImage|null; imageProgress:ImageProgress|null; cancelling:boolean; canInstall:boolean }>();
const emit=defineEmits<{ 'update:step':[step:number]; connect:[]; backup:[]; 'enter-official':[]; 'load-releases':[]; download:[version:string]; 'select-image':[]; 'clear-image':[]; 'cancel-image':[]; install:[] }>();
const steps = ['연결', '백업', '이미지 선택', '설치'];
const current = computed(()=>props.step);
const direction = ref('forward');
watch(current,(next,previous)=>{direction.value=next>previous?'forward':'backward';},{flush:'sync'});
const source = ref<'official' | 'local'>('official');
const version=ref('');
watch(()=>props.active && current.value===2 && source.value==='official' && !props.busy,visible=>{if(visible)emit('enter-official');},{immediate:true});
watch([()=>props.releases,()=>props.busy],([rows,locked])=>{if(!locked&&!rows.some(r=>r.version===version.value))chooseVersion(rows.find(r=>r.sha256)?.version??'');});
const selectedRelease=computed(()=>props.releases.find(r=>r.version===version.value));
const canDownload=computed(()=>!props.busy&&!props.releasesLoading&&!!selectedRelease.value?.sha256);
const phaseNames:Record<string,string>={choose:'이미지 파일 선택',resolve:'공식 릴리스 확인 중',download:'다운로드 중','verify-download':'다운로드 파일 검사 중',expand:'압축 푸는 중',copy:'이미지 가져오는 중',inspect:'파티션·부팅 파일 검사 중','verify-image':'저장된 이미지 검사 중','verify-cache':'기존 이미지 검사 중',ready:'이미지 준비 완료'};
const percent=computed(()=>props.imageProgress?.total?Math.min(100,Math.round(props.imageProgress.completed/props.imageProgress.total*100)):null);
function bytes(n:number){return n>=1024**3?(n/1024**3).toFixed(2)+' GiB':(n/1024**2).toFixed(1)+' MiB';}
function selectImage(){if(!props.busy)emit('select-image');}
function loadReleases(){if(!props.busy&&!props.releasesLoading)emit('load-releases');}
function chooseSource(next:'official'|'local'){if(props.busy||source.value===next)return;source.value=next;emit('clear-image');}
function chooseVersion(next:string){if(props.busy||version.value===next)return;version.value=next;if(source.value==='official')emit('clear-image');}
const primaryDisabled=computed(()=>props.busy || (current.value===0 ? !props.canConnect : current.value===1 ? !props.canInstall : current.value===2 ? !props.preparedImage&&source.value==='official'&&!canDownload.value : !props.canInstall||!props.preparedImage));
const primaryLabel=computed(()=>{
  if(props.busy)return ['장치 준비 중…','백업 중…','이미지 준비 중…','설치 준비 중…'][current.value];
  if(current.value===0)return props.loader?'장치 확인 후 다음':'장치 준비 후 다음';
  if(current.value===1)return props.backupPath?'다음: 이미지 선택':'백업 후 다음';
  if(current.value===2)return props.preparedImage?'다음: 설치':source.value==='official'?'다운로드 후 다음':'파일 선택 후 다음';
  return '설치 준비';
});
function primary(){
  if(primaryDisabled.value)return;
  if(current.value===0)emit('connect');
  else if(current.value===1)emit('backup');
  else if(current.value===2){
    if(props.preparedImage)goTo(3,false);
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
  <section class="install-wizard" aria-label="설치·복구 단계">
    <ol class="wizard-steps" aria-label="단계 선택">
      <li v-for="(step, index) in steps" :key="step">
        <button :aria-current="current === index ? 'step' : undefined" :disabled="busy" @click="goTo(index, busy)">
          <span class="wizard-number">{{ String(index + 1).padStart(2, '0') }}</span>
          <span>{{ step }}</span>
        </button>
      </li>
    </ol>

    <div :key="current" class="wizard-slide" :class="direction" :data-step="current" role="region" :aria-label="steps[current]">
      <div class="wizard-heading"><h2 aria-live="polite">{{ steps[current] }}</h2></div>
      <slot v-if="current === 0" name="connection" />
      <slot v-else-if="current === 1" name="backup" />

      <template v-else-if="current === 2">
        <div class="source-grid" role="group" aria-label="HAOS 이미지 가져오기">
          <button class="source-card" :class="{ selected: source === 'official' }" :aria-pressed="source === 'official'" :disabled="busy" @click="chooseSource('official')">
            <Download :size="24"/><span><strong>공식 다운로드</strong><small>Home Assistant OS 버전 선택</small></span><span class="radio" aria-hidden="true"><span v-if="source === 'official'" class="radio-dot"/></span>
          </button>
          <button class="source-card" :class="{ selected: source === 'local' }" :aria-pressed="source === 'local'" :disabled="busy" @click="chooseSource('local')">
            <FolderOpen :size="24"/><span><strong>내 PC에서 선택</strong><small>다운로드한 HAOS 이미지 사용</small></span><span class="radio" aria-hidden="true"><span v-if="source === 'local'" class="radio-dot"/></span>
          </button>
        </div>
        <section v-if="source === 'official'" class="panel image-panel" aria-label="공식 HAOS 다운로드" :aria-busy="releasesLoading">
          <div class="panel-top"><span class="ha-mark"><Download :size="24"/></span><div><h3>Home Assistant OS</h3><p>공식 릴리스에서 설치할 버전을 선택하세요.</p></div></div>
          <div class="input-row">
            <div><label for="haos-version">설치 버전</label><select id="haos-version" class="wizard-input" :value="version" :disabled="busy||releasesLoading||!releases.length" @change="chooseVersion(($event.target as HTMLSelectElement).value)"><option value="">{{releasesLoading?'버전 목록 불러오는 중…':'버전을 선택하세요'}}</option><option v-for="r in releases" :key="r.version" :value="r.version" :disabled="!r.sha256">HAOS {{r.version}}{{r.sha256?'':' · 공식 해시 없음'}}</option></select></div>
            <div class="platform"><label for="haos-platform">플랫폼</label><input id="haos-platform" class="wizard-input" value="generic-aarch64" disabled/></div>
          </div>
          <p v-if="releasesLoading" class="release-status" role="status">공식 버전 목록을 불러오는 중입니다.</p>
          <p v-else-if="releaseError" class="release-status warning" role="alert">{{releaseError}}</p>
          <div class="footer-actions image-actions"><button class="folder-button" :disabled="busy||releasesLoading" @click="loadReleases">{{releaseError?'다시 시도':'버전 목록 새로고침'}}</button><span v-if="selectedRelease" class="image-size">{{bytes(selectedRelease.size)}}</span></div>
        </section>
        <section v-else class="panel image-panel" aria-label="로컬 HAOS 이미지">
          <div class="panel-top"><FolderOpen :size="26"/><div><h3>HAOS 이미지 파일</h3><p>공식 generic-aarch64 이미지 · .img.xz 또는 .img</p></div></div>
          <div class="file-placeholder"><HardDrive :size="24"/><span>{{preparedImage?.filename??'아래에서 파일을 선택하세요.'}}</span><button v-if="preparedImage" class="folder-button" :disabled="busy" @click="selectImage">파일 변경</button></div>
        </section>
        <section v-if="imageProgress" class="panel image-work" role="status"><div class="setting-row"><b>{{cancelling?'취소하는 중':phaseNames[imageProgress.phase]??'이미지 준비 중'}}</b><span>{{percent===null?bytes(imageProgress.completed):percent+'%'}}</span></div><progress v-if="percent!==null" :value="percent" max="100"/><progress v-else/><button v-if="imageProgress.phase!=='choose'" class="folder-button" :disabled="cancelling" @click="emit('cancel-image')">취소</button></section>
        <section v-if="preparedImage" class="panel image-result" role="status"><ShieldCheck :size="22"/><div><strong>이미지 준비 완료</strong><p>{{preparedImage.filename}} · {{bytes(preparedImage.bytes)}}</p><p>{{preparedImage.verification==='github_sha256'?'공식 SHA-256 · 파티션 · ARM64 부팅 파일 확인':'파티션 · ARM64 부팅 파일 확인 · 공식 해시 대조 안 됨'}}</p><details><summary>파일 정보</summary><dl><dt>저장 위치</dt><dd>{{preparedImage.path}}</dd><dt>이미지 SHA-256</dt><dd>{{preparedImage.sha256}}</dd><dt>원본 파일 SHA-256</dt><dd>{{preparedImage.source_sha256}}</dd></dl></details></div></section>
      </template>

      <section v-else class="panel install-review" aria-label="설치 내용 확인">
        <div class="panel-top"><HardDrive :size="26"/><div><h3>설치 내용 확인</h3><p>설치할 장치와 이미지를 확인하세요.</p></div></div>
        <dl class="review-list">
          <div><dt>대상 장치</dt><dd>{{ deviceLabel }}</dd></div>
          <div><dt>부팅 영역 백업</dt><dd :class="{ 'review-path': backupPath }">{{ backupPath || '백업 없음' }}</dd></div>
          <div><dt>HAOS 이미지</dt><dd>{{preparedImage?.filename??'이미지 선택 안 됨'}}</dd></div>
        </dl>
        <div class="install-warning"><Info :size="18"/><p>새로 설치하면 eMMC의 기존 OS와 데이터가 삭제됩니다.<br>Home Assistant 백업을 별도로 보관하세요.</p></div>
        <p class="body-note">설치 준비에서 부팅 영역을 자동 백업합니다.</p>
      </section>
    </div>

    <footer class="wizard-navigation">
      <button class="wizard-back" :disabled="busy || current === 0" @click="goTo(current - 1, busy)"><ArrowLeft :size="16"/>이전</button>
      <span class="wizard-position">{{ current + 1 }} / {{ steps.length }}</span>
      <button class="primary wizard-next" data-wizard-primary :data-storage-action="current===3?'install':undefined" :disabled="primaryDisabled" @click="primary">{{primaryLabel}}<ArrowRight :size="17"/></button>
    </footer>
  </section>
</template>
