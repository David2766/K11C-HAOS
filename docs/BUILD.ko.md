# K11C 빌드 — U-Boot r24 / Installer 0.5.0

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

Windows x64에 Node.js, Go 1.26.3 이상, Rust MSVC, Visual C++ Build Tools를 준비합니다.
GitHub의 **Code → Download ZIP**으로 소스를 받거나 저장소를 복제합니다.
`installer/resources`에는 USB 로더, 서명된 드라이버, 제조사 도구, 현재 U-Boot와
Windows 파일시스템 도구·DLL이 포함됩니다. 사전설치 준비용 Go 실행 파일은
빌드 중 `installer/native` 소스에서 생성합니다.
기존 포터블 ZIP, SDK, HAOS·Connectivity 이미지는 필요하지 않습니다.

```powershell
cd installer
npm ci
node scripts/build.mjs --check-inputs
npm run build
npm run package
```

바이너리 입력의 해시는 실제 실행 코드에 지정된 값과 대조합니다.
개발자의 `C:\rom`이나 SDK 폴더는 필요하지 않습니다.
결과는 `installer/release/K11C-Installer-0.5.0-windows-x64.zip`과 `.sha256`입니다.
Go 경로를 지정하려면 `K11C_GO`를 사용합니다. 일반 소스 ZIP 빌드에서는
PATH의 `go`를 사용하며, 완성된 포터블을 실행할 때는 Go가 필요하지 않습니다.
준비 엔진과 파일시스템 도구의 소스는 [native 안내](../installer/native/README.md)에 있습니다.

프로그램 실행 후 저장소의 호환 목록을 확인하고, 공식 HAOS와 GHCR의 Connectivity
이미지를 받아 Windows에서 초기 데이터 영역을 생성합니다. 포터블 실행에는
Windows x64·WebView2와 설치 파일 다운로드를 위한 인터넷이 필요합니다.
Docker·WSL·에뮬레이터 설치는 필요하지 않습니다.
Rockusb는 내장되어 있으며 없을 때 관리자 승인으로 설치합니다.
`npm run package` 검사는 드라이버를 설치하지 않습니다.
호환 목록을 공개하는 순서는 [배포 안내](RELEASE.md#installer-구성요소-배포)를 참고하세요.

## GPT 복구 도구

```bash
python3 tools/boot-fix/test_boot_fix.py
python3 tools/boot-fix/package.py --output ../build/k11c-boot-fix-r2.tar
```

테스트는 임시 파일 디스크를 사용합니다. 실제 보드에서 복구할 때에는
[도구 안내](../tools/boot-fix/README.md)의 백업·대조 절차를 따르세요.
