# K11C HAOS 이미지 빌드

이 문서는 바로 SD 카드에 기록할 수 있는 K11C용 HAOS 이미지를 만드는 방법만
다룹니다. K11C U-Boot을 빌드해 공식 generic AArch64 HAOS 이미지 앞에 넣으며,
HAOS의 8개 파티션 내용은 바꾸지 않고 그대로 유지합니다.

K11C Connectivity App, SeekWave 커널 모듈, SeekWave 펌웨어는 이 과정에서 만들지
않습니다. 무선 App 빌드는 [CONNECTIVITY.ko.md](CONNECTIVITY.ko.md)를 참고하세요.

Ubuntu 또는 WSL2에서 저장소 루트를 현재 디렉터리로 두고 실행합니다. 다운로드한
원본과 생성된 대용량 이미지는 Git 저장소 밖에 둡니다.

## 1. 빌드 패키지 설치

Ubuntu 24.04에서 U-Boot 빌드와 이미지 검증에 필요한 패키지는 다음과 같습니다.

```bash
sudo apt update
sudo apt install -y \
  bc bison build-essential curl device-tree-compiler fdisk file flex \
  gcc-aarch64-linux-gnu gdisk git jq libgnutls28-dev libncurses-dev \
  libssl-dev mtools python3 python3-dev python3-pyelftools \
  python3-setuptools swig uuid-dev xz-utils
```

## 2. 공식 HAOS 이미지 받기

확인된 입력 파일은 공식 generic AArch64 HAOS 18.2 이미지입니다.

```text
파일:   haos_generic-aarch64-18.2.img.xz
주소:   https://github.com/home-assistant/operating-system/releases/download/18.2/haos_generic-aarch64-18.2.img.xz
SHA256: dae257b7b2ce3860f3ce187a85d27b3e9aaaebf55e6acb50f38115e7d7b783a1
```

```bash
mkdir -p ../k11c-build-inputs ../k11c-build-output
curl -L \
  -o ../k11c-build-inputs/haos_generic-aarch64-18.2.img.xz \
  https://github.com/home-assistant/operating-system/releases/download/18.2/haos_generic-aarch64-18.2.img.xz
echo 'dae257b7b2ce3860f3ce187a85d27b3e9aaaebf55e6acb50f38115e7d7b783a1  ../k11c-build-inputs/haos_generic-aarch64-18.2.img.xz' | sha256sum -c -
```

## 3. upstream U-Boot 준비

빌드 스크립트는 다음 커밋의 수정되지 않은 U-Boot 소스만 받습니다.

```text
버전: U-Boot v2026.04
커밋: 88dc2788777babfd6322fa655df549a019aa1e69
소스: https://source.denx.de/u-boot/u-boot.git
```

```bash
git clone https://source.denx.de/u-boot/u-boot.git \
  ../k11c-build-inputs/u-boot
git -C ../k11c-build-inputs/u-boot checkout \
  88dc2788777babfd6322fa655df549a019aa1e69
test "$(git -C ../k11c-build-inputs/u-boot rev-parse HEAD)" = \
  88dc2788777babfd6322fa655df549a019aa1e69
test -z "$(git -C ../k11c-build-inputs/u-boot status --porcelain --untracked-files=all)"
```

이 U-Boot 폴더에 K11C 파일을 직접 복사하거나 패치하지 마세요.
`source/scripts/build-k11c-uboot.sh`가 임시 worktree를 만들고 저장소의 다음 파일을
자동으로 복사합니다.

- `source/device-tree/linux/rk3566-kickpi-k11c.dts`
- `source/device-tree/u-boot/rk3566-kickpi-k11c-u-boot.dtsi`
- `source/u-boot/board/hardkernel/odroid_m1s/Makefile`
- `source/u-boot/board/hardkernel/odroid_m1s/k11c.c`

원본 U-Boot checkout은 변경되지 않습니다.

## 4. Rockchip DDR 및 BL31 파일 준비

[제조사 소스코드 폴더](https://1drv.ms/f/c/106b4b0b39a75ee5/IgAyC7gbEYJTSbLqbvcl01nTAbTgoKx-YKAY90ieirpIKmA?e=ityIrU)에서
다음 경로의 SDK 압축 파일을 받습니다.

```text
linux/sdk/20260515/rk356x-linux-2026051515.tar.gz
MD5: 81812c6a8770f73b41fc191939e9345e
소스 태그: rk356x-linux-2026051515
소스 커밋: 22a87c6c96feef811d103e04085b46b7d7ec4786
```

압축을 푼 뒤 다음 두 파일을 사용합니다.

```text
rkbin/bin/rk35/rk3566_ddr_1056MHz_v1.23.bin
SHA256: 20e4bb076847bd019fcdeb7bdc15bd249890f07ecc76e9937101f22e50950982

rkbin/bin/rk35/rk3568_bl31_v1.44.elf
SHA256: 65110f822fdbdd0163ce2dabc60591e7a8a0ffbc9471780e29eef0062f9ed7b6
```

받은 압축 파일을 `../k11c-build-inputs/rk356x-linux-2026051515.tar.gz`로
놓은 뒤 실행합니다.

```bash
echo '81812c6a8770f73b41fc191939e9345e  ../k11c-build-inputs/rk356x-linux-2026051515.tar.gz' | md5sum -c -
mkdir -p ../k11c-build-inputs/vendor-sdk
tar -xf ../k11c-build-inputs/rk356x-linux-2026051515.tar.gz \
  -C ../k11c-build-inputs/vendor-sdk

DDR=$(find ../k11c-build-inputs/vendor-sdk -type f \
  -path '*/rkbin/bin/rk35/rk3566_ddr_1056MHz_v1.23.bin' -print -quit)
BL31=$(find ../k11c-build-inputs/vendor-sdk -type f \
  -path '*/rkbin/bin/rk35/rk3568_bl31_v1.44.elf' -print -quit)
test -n "$DDR" && test -n "$BL31"
echo "20e4bb076847bd019fcdeb7bdc15bd249890f07ecc76e9937101f22e50950982  $DDR" | sha256sum -c -
echo "65110f822fdbdd0163ce2dabc60591e7a8a0ffbc9471780e29eef0062f9ed7b6  $BL31" | sha256sum -c -
```

출력된 SHA-256이 위의 두 값과 같아야 합니다. 이미지 빌드 스크립트도 이 값을 다시
검사하며, 다른 파일이면 중단합니다.

## 5. 이미지 빌드

저장소 루트에서 실행합니다.

```bash
DDR=$(find ../k11c-build-inputs/vendor-sdk -type f \
  -path '*/rkbin/bin/rk35/rk3566_ddr_1056MHz_v1.23.bin' -print -quit)
BL31=$(find ../k11c-build-inputs/vendor-sdk -type f \
  -path '*/rkbin/bin/rk35/rk3568_bl31_v1.44.elf' -print -quit)

bash source/scripts/build-k11c-image.sh \
  --haos-image ../k11c-build-inputs/haos_generic-aarch64-18.2.img.xz \
  --haos-sha256 dae257b7b2ce3860f3ce187a85d27b3e9aaaebf55e6acb50f38115e7d7b783a1 \
  --uboot-tree ../k11c-build-inputs/u-boot \
  --ddr-blob "$DDR" \
  --bl31 "$BL31" \
  --output-dir ../k11c-build-output
```

비어 있는 출력 폴더를 사용하세요. 기존 결과 파일이 하나라도 있으면 스크립트가
덮어쓰지 않고 중단합니다.

생성되는 주요 파일은 다음과 같습니다.

- `k11c-haos-18.2-sd.img.xz`: SD 카드에 바로 기록할 이미지
- `k11c-haos-18.2-sd.img`: 로컬 검증용 비압축 이미지
- `k11c-haos-18.2-sd.release.txt`: release 정보와 eMMC 복사에 필요한 값
- `k11c-haos-18.2-sd.img.manifest.txt`: 파티션 검증 결과
- `u-boot-rockchip-k11c-18.2.bin`: K11C U-Boot 이미지
- `u-boot-rockchip-k11c-18.2.bin.config`: U-Boot 설정
- `u-boot-rockchip-k11c-18.2.bin.control.dtb`: U-Boot control DTB
- `u-boot-rockchip-k11c-18.2.bin.manifest.txt`: U-Boot 빌드 manifest
- `SHA256SUMS`, `THIRD_PARTY_NOTICES.md`, `ROCKCHIP_RKBIN_LICENSE.txt`

완료 후 결과 전체를 검사합니다.

```bash
(cd ../k11c-build-output && sha256sum --check --strict SHA256SUMS)
```

빌드와 checksum 검증이 모두 오류 없이 끝나야 완료입니다.
`k11c-haos-18.2-sd.img.xz`를 [INSTALL.ko.md](INSTALL.ko.md)에 따라 기록하세요.

완성된 이미지는 HAOS, K11C U-Boot, K11C device tree, Rockchip 부트 바이너리를
포함합니다. SeekWave 펌웨어와 미리 빌드된 SeekWave 모듈은 포함하지 않습니다.
