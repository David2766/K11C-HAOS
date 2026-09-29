# 현재 K11C의 GPT 복구 — r2

현재 보드에서 확인한 **주·보조 GPT의 디스크 GUID 불일치만** 복구합니다.
HAOS 재빌드·재설치, U-Boot 재기록, Connectivity 업데이트가 아닙니다.
공식 HAOS OTA 경로, 파티션 배치·PARTUUID·내용, U-Boot 예약 영역,
콘솔 설정을 유지합니다. 보드에 자동으로 접속하거나 재부팅하지 않습니다.

원인은 2026-09-02 SD→eMMC 설치 당시 주 GPT만 새 GUID로 기록하고,
디스크 끝에 남은 Android 보조 GPT를 `gpt repair`에 맡긴 절차였습니다.
해당 U-Boot는 양쪽 GPT가 각각 유효하면 GUID가 달라도 repair 성공을 반환합니다.
HAOS의 데이터 파티션 확장도 각 헤더의 기존 GUID를 유지했습니다.

## 대상

- 기존 KICKPI K11C, `/dev/mmcblk0`, 512바이트 섹터 61071360개.
- 주 GUID `353A3772-554F-408B-A80B-05747613DEA7`.
- 파티션 8개, 첫 파티션 LBA 34816, `/mnt/boot`는 mmcblk0p1.
- 이 조건은 **진단한 현재 보드용**입니다. 판매용 모든 보드에 이 GUID를 강제하지 않습니다.
  신규 설치에는 이미지별 GPT를 계산하는 `scripts/prepare-emmc-install.py`를 사용합니다.

## 1. PC PowerShell — 도구 복사

보드를 켜고 Terminal & SSH 앱의 22번 SSH를 사용합니다.

```powershell
scp -O "C:\rom\HAOS\artifacts\K11C\boot-fix-r1\k11c-boot-fix-r2.tar" root@192.168.29.226:/share/
```

## 2. 보드 시리얼 호스트 `#` — 검증 및 새 백업

`[core-ssh ~]$`가 아닌 HAOS 호스트 셸입니다. 현재 설치와 데이터는 유지됩니다.

```sh
mkdir -p /mnt/data/k11c-boot-fix-r2
tar -xf /mnt/data/supervisor/share/k11c-boot-fix-r2.tar -C /mnt/data/k11c-boot-fix-r2
cd /mnt/data/k11c-boot-fix-r2
sha256sum -c SHA256SUMS
sh ./boot-fix.sh backup /mnt/data/supervisor/share/k11c-boot-backup-gpt-r2
```

모두 `OK`, 마지막 `K11C_BACKUP_READY=...`를 확인합니다. 기존 백업은 덮어쓰지 않습니다.
백업 디렉터리가 이미 있다면 지우지 말고 결과부터 확인합니다.

## 3. PC PowerShell — 쓰기 전에 백업을 PC로 보관

```powershell
scp -O -r root@192.168.29.226:/share/k11c-boot-backup-gpt-r2 "C:\rom\HAOS\artifacts\K11C\boot-fix-r1"
```

## 4. 보드 시리얼 호스트 `#` — GPT만 복구하고 확인

전원이 안정적인 상태에서 실행합니다. 중간에 전원을 끄지 마세요.

```sh
sh /mnt/data/k11c-boot-fix-r2/boot-fix.sh gpt /mnt/data/supervisor/share/k11c-boot-backup-gpt-r2
sh /mnt/data/k11c-boot-fix-r2/boot-fix.sh check-gpt /mnt/data/supervisor/share/k11c-boot-backup-gpt-r2
```

완료 기준은 `K11C_GPT_PASS`와 `K11C_GPT_CHECK_PASS`입니다.
도구의 종료 코드만 보지 않고 실제 양쪽 GUID·파티션 배열과 GPT 검증 보고서를 확인합니다.
주 GPT/MBR·U-Boot 예약 영역·파티션 배치가 백업과 같은지 검사하고,
보조 헤더에서는 GUID와 헤더 CRC 이외의 변경을 허용하지 않습니다.
이미 복구되었다면 `already_clean=1 writes=0`으로 끝납니다.

이번 작업에 재부팅은 필수가 아닙니다. 과거 `dmesg` 경고는 지금 고쳐도
현재 부팅 로그에 남아 있습니다. 다음 정상 재부팅 후 새 경고가 없는지 확인하면 됩니다.

## 실패 시

`FAIL`이 나오면 다음 단계로 진행하지 말고 출력과 백업 폴더의 `repair.log`,
`verify.*`를 전달합니다. GPT 쓰기 실패 시 자동으로 raw 복원하거나 재부팅하지 않습니다.
백업 GPT를 무조건 되돌리면 원래의 GUID 불일치까지 복원되므로 일반 원복 명령은 제공하지 않습니다.

예전 도구의 `console`, `restore-console`, `check` 명령은 호환용으로 남겨뒀지만
**이번 GPT 복구에는 사용하지 않습니다.** USB·RTC·OTP·Bluetooth도 변경하지 않습니다.

로컬 검증 범위와 실기 미확인 사항은 [VERIFICATION.md](VERIFICATION.md)에 구분했습니다.
