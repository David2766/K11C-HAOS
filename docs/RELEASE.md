# K11C 배포 구성

현재 부팅 펌웨어는 **r24**, Windows 포터블 설치 도구는 **0.5.0**입니다.
HAOS는 공식 generic-aarch64 이미지를 설치하고 이후 공식 OS 업데이트를 받습니다.
Connectivity는 기존 앱 저장소와 GHCR 배포 경로를 사용합니다.

## Git에 올리는 파일

- `source/boot-release.json`: r24 소스·바이너리·입력 파일 해시와 빌드 시각
- `source/device-tree`, `source/u-boot`, 부팅 빌드 스크립트: 실제 r24 최종 소스
- `installer`: 소스, 잠금 파일, 테스트, 라이선스, 사용 안내와 빌드용 USB 로더·드라이버·제조사 도구·U-Boot
- `installer/native`, `installer/resources/native`: Windows 사전설치 엔진 소스와 파일시스템 도구·DLL
- `installer-catalog.json`: 로컬에서 검증한 HAOS·커널·Connectivity·U-Boot 조합과 해시
- `tools/boot-fix`: GPT 백업·복구·검증 코드, `tools/check-r23.sh`: RTC·OTP 확인
- `source/scripts/prepare-emmc-install.py`, `gpt_image.py`, `test_gpt_install.py`와 이미지 검증 스크립트: 기존 SD 수동 설치의 GPT 수정
- 변경된 Connectivity 소스와 기존 CI

카메라 영상, 보드 백업, SSH 키, BT 덤프, 빌드 캐시, SDK 전체와 설치 EXE는
Git에 넣지 않습니다. 재빌드 방법은 [BUILD.ko.md](BUILD.ko.md)를 참고하세요.
기존 SD 합성 이미지 안내는 초기 설치 방식이며, 현재 설치 도구와 구분합니다.

## GitHub Releases 첨부 파일

U-Boot r24 릴리스:

- `u-boot-k11c-dfi-r24.bin`
- 같은 이름 뒤의 `.manifest.txt`, `.control.dtb`, `.config`
- `SHA256SUMS`에 기록된 `TEST.md`, `production-inputs.json`, `verification.txt`도 함께 첨부
- `SHA256SUMS`

설치 도구 0.5.0 릴리스:

- `K11C-Installer-0.5.0-windows-x64.zip`
- `K11C-Installer-0.5.0-windows-x64.zip.sha256`

각 릴리스는 해당 소스가 들어간 커밋을 가리키게 합니다. U-Boot·Rockchip·Rockusb의
원본 라이선스 및 제3자 고지를 유지합니다. 새 사전설치 경로는 실제 보드의
첫 부팅·앱 자동 시작·공식 OTA 확인 후 정식 배포합니다. 그전에는 사전 배포로 표시합니다.
이미 공개된 같은 태그의 파일을 덮어쓰기
전에는 기존 배포 여부와 사용자 영향을 확인하세요.

선택 사항인 `k11c-boot-fix-r2.tar`와 `.sha256`은 복구 도구로 별도 첨부할 수 있습니다.
단순 U-Boot 업데이트를 위해 GPT 복구를 매번 실행할 필요는 없습니다.
이 r2 보드 측 복구 스크립트는 진단 당시 보드의 GUID·용량에 한정된 도구입니다.
다른 고객 보드에 일반 복구 도구처럼 실행하지 않습니다. 설치 도구의 GPT 기능은
별도 구현이며 대상 장치의 실제 GPT를 읽어 처리합니다.

## 내보내기 순서 (개발 PC)

PowerShell에서 변경 목록만 확인:

```powershell
wsl -d Ubuntu-24.04 -- bash /mnt/c/rom/HAOS/K11C-port/scripts/export-repository.sh --check
```

확인 후 기존 저장소로 복사:

```powershell
wsl -d Ubuntu-24.04 -- bash /mnt/c/rom/HAOS/K11C-port/scripts/export-repository.sh
git -C C:\repos\K11C-HAOS diff --stat
git -C C:\repos\K11C-HAOS status --short
```

복사 스크립트는 컴파일·커밋·푸시·GitHub 업로드를 실행하지 않습니다.
공개 앱의 `k11c_connectivity/config.yaml`은 그대로 두고, 소스 템플릿만 갱신합니다.
현재 CI는 Connectivity 전용입니다. U-Boot·Installer만 바뀌면 그 경로 변경으로는
Connectivity push 빌드를 시작하지 않습니다. 일일 예약 실행은 별개입니다.
U-Boot·Installer 릴리스 자동 게시는 아직 구성하지 않았습니다.

## 기능 범위

Installer 0.5.0은 공식 HAOS 버전 조회·다운로드, 로컬 이미지 선택, USB 준비,
설치, U-Boot 업데이트, 전체·부분 압축 백업·복원, 제조사 Android·Linux 이미지
설치와 GPT 검사·복구를 제공합니다.
설치 준비에서는 호환 목록에 맞는 U-Boot와 Connectivity 이미지를 받아 Windows에서
초기 데이터 영역을 구성합니다. 고급 기능에서는 기존 Connectivity 앱 제거·재설치가 가능합니다.
설치 도구가 고객의 HAOS 업데이트를 대신하지 않습니다.

## Installer 구성요소 배포

설치 과정의 다운로드 경로는 다음과 같습니다.

- HAOS: Home Assistant 공식 Releases의 generic-aarch64 이미지와 SHA-256
- Connectivity: GHCR에 공개된 이미지. 호환 목록에 지정한 digest로 고정
- 호환 목록: 이 저장소 `main`의 루트 `installer-catalog.json`
- U-Boot: 호환 목록의 URL. 현재는 `installer/resources/firmware/u-boot-k11c-dfi-r24.bin`
- 앱 저장소: 호환 목록에 지정한 공개 커밋의 정상 저장소 구조

Windows 준비 엔진은 받은 앱을 공식 HAOS의 데이터 영역 복사본에 등록하고,
앱 자동 시작과 저장소 등록을 설정합니다. 공식 OS 파티션 7개는 그대로 유지합니다.
앱 레이어의 압축 해제는 첫 부팅 때 HAOS의 Docker가 처리합니다.

새 조합을 공개하는 순서:

1. Connectivity 이미지와 그 버전의 `RELEASE.json`·`config.yaml`이 공개되어 있는지 확인합니다.
2. [로컬 검증 안내](../installer/seed/README.md)에 따라 정확한 HAOS·커널·앱·Docker·Supervisor
   조합과 Windows 준비 결과를 검증하고 schema 2 호환 목록을 생성합니다.
3. 기존 export 스크립트로 목록과 U-Boot를 저장소 폴더에 반영한 뒤 커밋·푸시합니다.
   목록과 그 안의 U-Boot URL에 해당하는 파일을 함께 반영합니다.
4. 공개 URL의 응답과 해시를 확인하고, 빈 다운로드 캐시에서 설치 준비를 검증합니다.
   호환 목록이 없거나 해당 조합이 등록되지 않으면 설치 준비가 중단됩니다.

배포 대상은 작은 호환 목록, U-Boot와 포터블 ZIP입니다. 별도 사전설치 데이터 이미지나
`.ext4.xz` Release 파일은 필요하지 않습니다. GitHub 소스 ZIP에서도 HAOS·앱 이미지 없이
EXE를 빌드할 수 있습니다.

Connectivity CI는 앱 빌드·게시를 담당합니다. 새 설치 조합은 로컬 검증 후 호환 목록에
반영합니다. 목록을 갱신해도 고객의 기존 HAOS 공식 OTA와 앱 저장소 업데이트는 그대로 사용합니다.
