# K11C Connectivity App 빌드

이 작업은 SD 카드용 HAOS 이미지 빌드와 별개입니다. 제조사의 SeekWave
VS/SWT6621S 드라이버 소스를 실제 HAOS 호스트 커널에 맞춰 컴파일하고, 저장소에
포함된 호환성 패치를 적용한 뒤 필요한 펌웨어를 로컬 App 폴더에 넣습니다.

공개 저장소에는 미리 빌드된 커널 모듈과 SeekWave 펌웨어가 들어 있지 않습니다.
제조사 소스를 직접 받은 뒤 라이선스 조건을 확인하고 로컬에서 빌드해야 합니다.

아래 절차는 HAOS 18.2용 `6.18.39-haos` 모듈을 만들 때 확인한 방법입니다.
Ubuntu 또는 WSL2에서 저장소 루트를 현재 디렉터리로 두고 실행합니다.

Connectivity 빌더가 직접 사용하는 호스트 도구를 설치합니다.

```bash
sudo apt update
sudo apt install -y coreutils file findutils gawk git grep kmod make patch
```

## 1. HAOS 18.2 빌드 트리 준비

Docker를 설치하고 현재 사용자가 `sudo`를 통해 HAOS 빌드 컨테이너를 실행할 수
있게 준비합니다. 호스트 준비 사항은
[Home Assistant OS 공식 빌드 안내](https://developers.home-assistant.io/docs/operating-system/getting-started/)를
참고하세요.

확인된 HAOS 소스는 다음과 같습니다.

```text
저장소: https://github.com/home-assistant/operating-system.git
태그:   18.2
커밋:   3c196f5144ae78e124d2ae1a067c1841af71a51c
보드:   generic_aarch64
커널:   6.18.39-haos
```

```bash
mkdir -p ../k11c-connectivity-inputs
git clone --branch 18.2 --recurse-submodules \
  https://github.com/home-assistant/operating-system.git \
  ../k11c-connectivity-inputs/haos-18.2
test "$(git -C ../k11c-connectivity-inputs/haos-18.2 rev-parse HEAD)" = \
  3c196f5144ae78e124d2ae1a067c1841af71a51c

cd ../k11c-connectivity-inputs/haos-18.2
scripts/enter.sh make generic_aarch64
cd -
```

드라이버 빌더는 완성된 HAOS 트리의 다음 파일을 사용합니다.

```text
output/build/linux-6.18.39/.config
output/host/bin/aarch64-buildroot-linux-gnu-gcc
```

다음 명령으로 확인합니다.

```bash
test -f ../k11c-connectivity-inputs/haos-18.2/output/build/linux-6.18.39/.config
test -x ../k11c-connectivity-inputs/haos-18.2/output/host/bin/aarch64-buildroot-linux-gnu-gcc
make -s \
  -C ../k11c-connectivity-inputs/haos-18.2/output/build/linux-6.18.39 \
  ARCH=arm64 kernelrelease
```

마지막 명령은 `6.18.39-haos`를 출력해야 합니다. 다른 커널용 모듈을 이 HAOS에
복사해서 사용하면 안 됩니다.

## 2. 제조사 Wi-Fi/Bluetooth 소스 준비

[제조사 소스코드 폴더](https://1drv.ms/f/c/106b4b0b39a75ee5/IgAyC7gbEYJTSbLqbvcl01nTAbTgoKx-YKAY90ieirpIKmA?e=ityIrU)에서
다음 압축 파일을 받습니다.

```text
linux-kernel-6.1/rk-linux6.1-2026060914/rk-linux6.1-2026060914.tar.gz
MD5: 6974a91407767c1a9000929d1ed65544
소스 태그: rk-linux6.1-2026060914
소스 커밋: 544b6630ad1cf1321ef102c813141598c73baeed
```

저장소 밖에 압축을 풉니다.

받은 압축 파일을 `../k11c-connectivity-inputs/rk-linux6.1-2026060914.tar.gz`로
놓은 뒤 실행합니다.

```bash
echo '6974a91407767c1a9000929d1ed65544  ../k11c-connectivity-inputs/rk-linux6.1-2026060914.tar.gz' | md5sum -c -
mkdir -p ../k11c-connectivity-inputs/vendor-linux-6.1
tar -xf ../k11c-connectivity-inputs/rk-linux6.1-2026060914.tar.gz \
  -C ../k11c-connectivity-inputs/vendor-linux-6.1
```

드라이버는 압축 파일 안의 다음 폴더에서 가져옵니다.

```text
external/rkwifibt/drivers/skw6621s
```

펌웨어와 NVRAM은 정확히 다음 파일을 사용합니다.

```text
external/rkwifibt/firmware/seekwave/ea6x21qx/SWT6621S_DRAM_SDIO.bin
external/rkwifibt/firmware/seekwave/ea6x21qx/SWT6621S_IRAM_SDIO.bin
external/rkwifibt/firmware/seekwave/ea6x21qx/SWT6621S_NV_SDIO_ALONE.bin
external/rkwifibt/firmware/seekwave/ea6x21qx/SWT6621S_NV_SDIO_SHARE.bin
external/rkwifibt/firmware/seekwave/ea6x21qx/SWT6621S_SEEKWAVE_R00000.bin
external/rkwifibt/firmware/seekwave/ea6x21qx/SWT6621S_SEEKWAVE_R00001.bin
external/rkwifibt/firmware/seekwave/ea6x21qx/SWT6621S_SEEKWAVE_R04000.bin
external/rkwifibt/firmware/seekwave/ea6x21qx/SWT6621S_SEEKWAVE_R04001.bin
external/rkwifibt/drivers/skw6621s/skwbt/sv6160.nvbin
external/rkwifibt/firmware/seekwave/ea6x21qx/sv6160lite.nvbin
external/rkwifibt/drivers/skw6621s/skwbt/sv6316.nvbin
```

각 펌웨어의 확인된 hash는 `source/connectivity/firmware.sha256`에 있습니다.
빌더는 `drivers/skw6621s` 전체 소스도 확인하므로 다른 제조사 소스나 다른 버전의
파일이면 컴파일 전에 중단합니다.

압축을 푼 트리에서 `external/rkwifibt`를 포함하는 루트를 찾습니다.

```bash
RKWIFIBT=$(find ../k11c-connectivity-inputs/vendor-linux-6.1 -type d \
  -path '*/external/rkwifibt' -print -quit)
VENDOR_ROOT=${RKWIFIBT%/external/rkwifibt}
test -d "$VENDOR_ROOT/external/rkwifibt/drivers/skw6621s"
```

또는 `external/rkwifibt` 폴더 자체를 `--vendor-tree`에 지정해도 됩니다.

## 3. 모듈과 App payload 빌드

App 골격은 `k11c_connectivity/`에 있습니다. 빌더는 제조사 드라이버를 임시
폴더로 복사하고 `source/connectivity/driver-patches/0001`부터 `0006`까지 적용한
뒤 SDIO Wi-Fi와 Bluetooth 모듈만 빌드합니다. 제조사 원본 폴더는 수정하지
않습니다.

```bash
bash source/connectivity/scripts/build-driver-bundle.sh \
  --haos-tree ../k11c-connectivity-inputs/haos-18.2 \
  --vendor-tree "$VENDOR_ROOT" \
  --app-dir "$PWD/k11c_connectivity"
```

생성되는 모듈은 정확히 다음 세 개입니다.

```text
k11c_connectivity/modules/6.18.39-haos/skw_sdio_lite.ko
k11c_connectivity/modules/6.18.39-haos/swt6621s_wifi.ko
k11c_connectivity/modules/6.18.39-haos/skwbt.ko
```

위에서 지정한 11개 펌웨어는 다음 폴더로 복사됩니다.

```text
k11c_connectivity/firmware/
```

이 두 생성 폴더는 프로젝트의 AGPL 라이선스와 배포 조건이 다르므로 Git에서
무시됩니다.

## 4. 완성된 App 폴더 검증

```bash
bash source/connectivity/scripts/verify-connectivity-bundle.sh \
  "$PWD/k11c_connectivity"
```

검증기는 App 설정, 모듈 이름, AArch64 형식, 커널 `vermagic`, 필수 모듈 인자,
모듈 checksum, 펌웨어 파일과 checksum, 모듈 로드 순서를 확인합니다. 정상 완료
메시지는 다음과 같습니다.

```text
K11C connectivity bundle verification passed
```

완성된 `k11c_connectivity` 폴더가 로컬 Home Assistant App의 빌드 컨텍스트입니다.
이 폴더를 HAOS의 `/addons/k11c_connectivity/`로 복사하고 로컬 App을 설치하면
Supervisor가 App 컨테이너를 빌드합니다. 이후 절차는
[INSTALL.ko.md](INSTALL.ko.md#wi-fi와-bluetooth)를 참고하세요.

HAOS 운영체제 업데이트 후 `uname -r`이 달라졌다면 그 버전의 HAOS 빌드 트리를
준비해 모듈을 다시 빌드해야 합니다. Home Assistant Core만 업데이트한 경우에는
호스트 커널이 바뀌지 않습니다.
