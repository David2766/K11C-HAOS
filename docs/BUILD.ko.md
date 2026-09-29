# K11C 빌드 — U-Boot r24 / Installer 0.4.0

HAOS는 공식 generic-aarch64 배포 파일을 사용합니다. 이 저장소에서 HAOS나
커널 ROM을 새로 만들 필요는 없습니다. Connectivity 커널 모듈 빌드는 별도
[Connectivity CI](CONNECTIVITY-CI.md)가 담당합니다.

## U-Boot r24

공개된 `source/device-tree`는 실제 r24 빌드의 최종 입력입니다.
NPU 제조사 설정, 기존 DT 참조 번호, RTC 순서, OTP 중복 방지, DFI 비활성화가
포함되어 있습니다. 이전 진단 폴더나 개발 PC의 임시 작업 폴더는 필요하지 않습니다.

Ubuntu 24.04에서 준비할 도구:

```bash
sudo apt-get install build-essential gcc-aarch64-linux-gnu bison flex swig python3-dev python3-setuptools python3-pyelftools python3-jsonschema device-tree-compiler libssl-dev libgnutls28-dev
git clone https://github.com/u-boot/u-boot.git ../u-boot
git -C ../u-boot checkout 88dc2788777babfd6322fa655df549a019aa1e69
python3 source/scripts/build-release-uboot.py --check
```

Rockchip 원본 입력은 아래 두 파일입니다. 정확한 해시는
`source/boot-release.json`에 있습니다. 파일명만 같은 다른 버전은 사용하지 않습니다.

- `rk3566_ddr_1056MHz_v1.23.bin`
- `rk3568_bl31_v1.44.elf`

```bash
python3 source/scripts/build-release-uboot.py \
  --uboot-tree ../u-boot \
  --ddr-blob ../inputs/rk3566_ddr_1056MHz_v1.23.bin \
  --bl31 ../inputs/rk3568_bl31_v1.44.elf \
  --output ../build/u-boot-k11c-dfi-r24.bin
```

빌드 후 배포된 r24 바이너리와 DTB의 SHA-256까지 일치해야 성공합니다.
출력 파일이 이미 있으면 덮어쓰지 않습니다. 빌드는 보드에 연결하지 않습니다.

## Windows 포터블 설치 도구

Node.js, Rust MSVC, Visual C++ Build Tools를 준비합니다.
기존 0.4.0 ZIP의 `resources` 폴더를 별도 위치에 풀고 지정합니다.
이 폴더에는 `loader`, `firmware`, `rockusb`가 있어야 합니다.

```powershell
cd installer
$env:K11C_RELEASE_INPUTS = 'C:\K11C-inputs\resources'
npm ci
node scripts/build.mjs --check-inputs
npm run build
npm run package
```

바이너리 입력의 해시는 실제 실행 코드에 지정된 값과 대조합니다.
개발자의 `C:\rom`이나 SDK 폴더는 필요하지 않습니다.
`npm run package`의 패키지 검사는 기존 Rockusb 드라이버가 설치된 Windows PC에서
수행합니다. 드라이버 자동 설치를 테스트 목적으로 실행하지는 않습니다.
실물 USB 기록·전원 차단 복구는 별도의 보드 검증 항목입니다.

## GPT 복구 도구

```bash
python3 tools/boot-fix/test_boot_fix.py
python3 tools/boot-fix/package.py --output ../build/k11c-boot-fix-r2.tar
```

테스트는 임시 파일 디스크를 사용합니다. 실제 보드에서 복구할 때에는
[도구 안내](../tools/boot-fix/README.md)의 백업·대조 절차를 따르세요.
