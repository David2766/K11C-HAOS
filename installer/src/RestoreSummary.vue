<script setup lang="ts">
import { computed } from 'vue';
import { t, number } from './i18n';

const props=defineProps<{ info:any }>();
// Kind comes from the parsed archive/raw-image report, never its filename.
const scope=computed(()=>({
  FULL:{title:'FULL · 전체 디스크',detail:'부팅 펌웨어·GPT·OS·앱·설정·사용자 데이터를 백업 시점으로 되돌립니다. 현재 디스크의 모든 데이터가 교체됩니다.'},
  BOOT:{title:'BOOT · 부팅 펌웨어와 GPT',detail:'부팅 펌웨어와 GPT만 교체합니다. OS·앱·설정·사용자 데이터는 복원하지 않습니다.'},
  HAOS:{title:'HAOS · OS·앱·설정·데이터',detail:'HAOS 파티션의 OS·앱·설정·사용자 데이터를 교체합니다. 별도 부팅 펌웨어와 GPT는 유지합니다.'},
} as Record<string,{title:string;detail:string}>)[props.info?.kind]);
</script>

<template>
  <section v-if="info" class="restore-summary" :aria-label="t('복원 범위 안내')" aria-live="polite" :data-restore-kind="scope?info.kind:'unknown'">
    <div class="setting-row"><span>{{t('백업 종류')}}</span><strong>{{scope?t(scope.title):t('미확인')}}</strong></div>
    <p v-if="scope" class="body-note">{{t(scope.detail)}}</p>
    <p v-else class="warning">{{t('백업 종류를 확인할 수 없습니다. 다른 파일을 선택하세요.')}}</p>
    <p v-if="scope&&info.kind!=='FULL'" class="body-note">{{t('부분 복원은 동일한 HAOS 파티션 구성이 필요합니다. OS를 바꾸려면 FULL 백업을 사용하세요.')}}</p>
    <p v-if="scope&&info.kind==='FULL'" class="body-note">{{t('FULL은 eMMC 사용자 영역 전체입니다. 별도 하드웨어 영역인 boot0·boot1·RPMB·OTP는 포함하지 않습니다.')}}</p>
    <div v-if="info.identity?.sectors" class="setting-row"><span>{{t('원본 eMMC 용량')}}</span><b>{{number(info.identity.sectors*512/1024**3,1)}} GiB</b></div>
  </section>
</template>
