# K11C HAOS

Home Assistant OS support for the KickPi K11C V1.2.

This project boots the generic AArch64 HAOS image with mainline U-Boot and
adds the board-specific device tree. The HAOS partitions themselves are not
modified. Wi-Fi and Bluetooth are loaded by the K11C Connectivity App.

## Current status

Tested on this configuration:

- KickPi K11C V1.2
- RK3566, 4 GB RAM, 32 GB eMMC
- mainline U-Boot 2026.04
- HAOS generic AArch64 18.2
- Linux `6.18.39-haos`
- VS6621SR80/SWT6621S Wi-Fi and Bluetooth

Working on the tested board:

- microSD and eMMC boot
- wired Ethernet
- Wi-Fi and Bluetooth
- HDMI
- USB host and USB OTG
- basic RTC timekeeping

Not tested yet:

- eDP/MIPI and camera
- NPU
- analog audio
- RTC alarm and wake
- the remaining GPIO and expansion interfaces

The eMMC currently runs at 52 MHz. HS200 has not been tested yet.

## What is included

- K11C Linux and U-Boot device-tree sources
- the K11C U-Boot board hook for the Maxio Ethernet PHY
- scripts used to build and repack the test image
- patches for building the SeekWave driver on Linux 6.18
- K11C Connectivity App source

This repository does not include a ready-to-flash image, vendor OS backup,
prebuilt kernel modules, or SeekWave firmware.

## Build

The current reference build uses WSL/Linux, a configured HAOS 18.2 build
tree, U-Boot 2026.04, Rockchip DDR/BL31 binaries, and the K11C vendor source.
The paths in the script match the machine used for testing, so change them for
your own environment.

```bash
bash source/reference/build-k11c-r16-eee-image.sh
```

The full image produced by the script is:

```text
haos_generic-aarch64-18.2-k11c-lan-eee-r16.img
```

## Boot from microSD

1. Write the full r16 image to a microSD card with Rufus or balenaEtcher.
2. Use DD image mode if Rufus asks.
3. Insert the card and connect UART at 1,500,000 baud, 8N1.
4. Power on and check for `Model: KICKPI K11C`.
5. Finish the first HAOS setup over wired Ethernet.

Check the physical target carefully before writing an image. The selected
card will be overwritten.

## Back up the factory eMMC

The factory Android image on the tested board exposed the eMMC as
`/dev/block/mmcblk2`. Check the device name on your board first.

```text
adb pull /dev/block/mmcblk2boot0 emmc-boot0.bin
adb pull /dev/block/mmcblk2boot1 emmc-boot1.bin
adb pull /dev/block/mmcblk2 emmc-full-32gb.img
```

Keep the backup and its SHA-256 values outside the board.

## Install to eMMC

Test the full image from microSD first. The commands below are only for the
tested 916 MiB r16 image and the K11C eMMC with `0x3a3e000` 512-byte sectors.
Do not use them if the image size, eMMC capacity, or MMC numbering differs.

Prepare the installer card under Linux. Replace `/dev/sdX` with the verified
microSD device:

```bash
sudo dd if=k11c-r16-eee-uboot-only-sd.img of=/dev/sdX \
  bs=4M conv=fsync status=progress
sudo dd if=haos_generic-aarch64-18.2-k11c-lan-eee-r16.img of=/dev/sdX \
  bs=1M seek=1024 conv=notrunc,fsync status=progress
sync
```

Boot from that card and stop at U-Boot. On the tested build, `mmc1` is the
microSD card and `mmc0` is the eMMC.

```text
mmc dev 1
mmc read 40000000 200000 1ca028
mw.b 400001ca ff
mw.b 400001cb df
mw.b 400001cc a3
mw.b 400001cd 03
mw.l 40000220 03a3dfff
mw.l 40000230 03a3dfde
mw.l 40000210 1ee819b0
crc32 40000000 39405000

mmc dev 0
mmc info
mmc write 40000000 0 1ca028
mmc read 40000000 0 1ca028
crc32 40000000 39405000
gpt repair mmc 0
gpt verify mmc 0
mmc part
```

The two CRC values, taken after the GPT changes and after the eMMC readback,
must match. Remove the microSD card after powering off, then boot from eMMC.

## Build the Wi-Fi/Bluetooth App

Download the vendor source linked at the bottom of this README. Point
`VENDOR_TREE` to its `external/rkwifibt` directory. `HAOS_TREE` must contain
the configured build tree for the same kernel version used by the board.

```bash
HAOS_TREE=/path/to/haos-18.2 \
VENDOR_TREE=/path/to/vendor/external/rkwifibt \
OUTPUT_ROOT="$PWD/k11c_connectivity" \
bash source/connectivity/scripts/build-driver-bundle.sh

bash source/connectivity/scripts/verify-connectivity-bundle.sh \
  k11c_connectivity
```

This creates the local `modules` and `firmware` directories required by the
App.

## Install the App

Copy the completed `k11c_connectivity` directory to:

```text
/addons/k11c_connectivity/
```

Then run:

```bash
ha store reload
ha store apps install local_k11c_connectivity
ha apps start local_k11c_connectivity
ha apps logs local_k11c_connectivity
```

The App is disabled on its first start. In **Settings > Apps > K11C
Connectivity > Configuration**, set `enabled` to `true`. The tested antenna
setting is `antenna_mode: share`.

Wi-Fi can then be configured under **Settings > System > Network**. Bluetooth
appears under **Settings > Devices & services > Bluetooth**.

## Updates

Home Assistant Core updates do not change the host kernel. HAOS updates can.
If `uname -r` changes, rebuild the three SeekWave modules before relying on
Wi-Fi or Bluetooth.

The current generic AArch64 update layout leaves the raw U-Boot area alone.
Run `source/reference/validate-generic-ota-contract.sh` again when testing a
new HAOS version.

## License

Original code and documentation written for this project are licensed under
`AGPL-3.0-or-later`. Files based on Linux, U-Boot, device-tree sources, or the
SeekWave driver keep their original licenses. See `LICENSE` and
`THIRD_PARTY_NOTICES.md`.

---

# K11C HAOS 한국어 안내

KickPi K11C V1.2에서 Home Assistant OS를 사용하기 위한 프로젝트입니다.

generic AArch64 HAOS 이미지의 파티션은 수정하지 않고, K11C용 mainline
U-Boot와 DTS를 추가합니다. Wi-Fi와 Bluetooth는 부팅 후 K11C Connectivity
App이 로드합니다.

## 현재 상태

확인한 환경:

- KickPi K11C V1.2
- RK3566, RAM 4 GB, eMMC 32 GB
- mainline U-Boot 2026.04
- HAOS generic AArch64 18.2
- Linux `6.18.39-haos`
- VS6621SR80/SWT6621S Wi-Fi와 Bluetooth

확인한 기능:

- microSD 및 eMMC 부팅
- 유선 LAN
- Wi-Fi 및 Bluetooth
- HDMI
- USB host 및 USB OTG
- RTC 기본 시간

아직 확인하지 않은 기능:

- eDP/MIPI 및 카메라
- NPU
- 아날로그 오디오
- RTC alarm 및 wake
- 나머지 GPIO와 확장 인터페이스

eMMC는 현재 52 MHz로 동작합니다. HS200은 아직 확인하지 않았습니다.

## 저장소 내용

- K11C Linux/U-Boot DTS
- Maxio Ethernet PHY를 위한 U-Boot 보드 코드
- 테스트 이미지 빌드 및 재패킹 스크립트
- Linux 6.18용 SeekWave 드라이버 패치
- K11C Connectivity App 소스

바로 구울 수 있는 이미지, 제조사 OS 백업, 컴파일된 커널 모듈과 SeekWave
펌웨어는 저장소에 포함하지 않습니다.

## 빌드

현재 스크립트는 WSL/Linux, 빌드 설정을 마친 HAOS 18.2 트리, U-Boot
2026.04, Rockchip DDR/BL31 바이너리와 K11C 제조사 소스를 사용합니다. 테스트에
사용한 PC 경로가 들어 있으므로 자신의 환경에 맞게 바꿔야 합니다.

```bash
bash source/reference/build-k11c-r16-eee-image.sh
```

생성되는 전체 이미지는 다음과 같습니다.

```text
haos_generic-aarch64-18.2-k11c-lan-eee-r16.img
```

## microSD 부팅

1. Rufus 또는 balenaEtcher로 전체 r16 이미지를 microSD에 기록합니다.
2. Rufus가 기록 방식을 물으면 DD 이미지 모드를 선택합니다.
3. UART를 1,500,000 baud, 8N1로 연결합니다.
4. 전원을 켜고 `Model: KICKPI K11C`가 나오는지 확인합니다.
5. 유선 LAN으로 HAOS 초기 설정을 마칩니다.

이미지를 기록하기 전에 대상 디스크를 다시 확인하세요. 선택한 카드의 기존 내용은
삭제됩니다.

## 순정 eMMC 백업

테스트한 순정 Android에서는 eMMC가 `/dev/block/mmcblk2`였습니다. 자신의
보드에서도 장치명을 먼저 확인하세요.

```text
adb pull /dev/block/mmcblk2boot0 emmc-boot0.bin
adb pull /dev/block/mmcblk2boot1 emmc-boot1.bin
adb pull /dev/block/mmcblk2 emmc-full-32gb.img
```

백업 파일과 SHA-256 값은 보드 밖에 보관합니다.

## eMMC 설치

먼저 전체 이미지를 microSD에서 확인하세요. 아래 명령은 916 MiB r16 이미지와
512바이트 섹터 수가 `0x3a3e000`인 K11C eMMC에서 사용한 값입니다. 이미지
크기, eMMC 용량 또는 MMC 번호가 다르면 사용하면 안 됩니다.

Linux에서 설치용 SD 카드를 만듭니다. `/dev/sdX`는 실제 SD 카드 장치를 확인한
뒤 바꿔 입력합니다.

```bash
sudo dd if=k11c-r16-eee-uboot-only-sd.img of=/dev/sdX \
  bs=4M conv=fsync status=progress
sudo dd if=haos_generic-aarch64-18.2-k11c-lan-eee-r16.img of=/dev/sdX \
  bs=1M seek=1024 conv=notrunc,fsync status=progress
sync
```

이 카드로 부팅한 뒤 U-Boot에서 멈춥니다. 테스트한 U-Boot에서는 `mmc1`이
microSD, `mmc0`이 eMMC입니다.

```text
mmc dev 1
mmc read 40000000 200000 1ca028
mw.b 400001ca ff
mw.b 400001cb df
mw.b 400001cc a3
mw.b 400001cd 03
mw.l 40000220 03a3dfff
mw.l 40000230 03a3dfde
mw.l 40000210 1ee819b0
crc32 40000000 39405000

mmc dev 0
mmc info
mmc write 40000000 0 1ca028
mmc read 40000000 0 1ca028
crc32 40000000 39405000
gpt repair mmc 0
gpt verify mmc 0
mmc part
```

GPT 수정 후 CRC와 eMMC를 다시 읽은 뒤의 CRC가 같아야 합니다. 전원을 끄고
microSD를 제거한 다음 eMMC로 부팅합니다.

## Wi-Fi/Bluetooth App 빌드

README 맨 아래의 제조사 소스를 내려받고 `VENDOR_TREE`를 그 안의
`external/rkwifibt`로 지정합니다. `HAOS_TREE`는 보드에서 실행 중인 커널과 같은
커널의 빌드 트리여야 합니다.

```bash
HAOS_TREE=/path/to/haos-18.2 \
VENDOR_TREE=/path/to/vendor/external/rkwifibt \
OUTPUT_ROOT="$PWD/k11c_connectivity" \
bash source/connectivity/scripts/build-driver-bundle.sh

bash source/connectivity/scripts/verify-connectivity-bundle.sh \
  k11c_connectivity
```

App에 필요한 `modules`와 `firmware` 폴더가 생성됩니다.

## App 설치

완성된 `k11c_connectivity` 폴더를 다음 위치에 복사합니다.

```text
/addons/k11c_connectivity/
```

Home Assistant Terminal & SSH에서 실행합니다.

```bash
ha store reload
ha store apps install local_k11c_connectivity
ha apps start local_k11c_connectivity
ha apps logs local_k11c_connectivity
```

처음에는 App이 꺼진 상태입니다. **설정 > 앱 > K11C Connectivity > 구성**에서
`enabled`를 `true`로 바꿉니다. 테스트한 안테나 설정은
`antenna_mode: share`입니다.

이후 **설정 > 시스템 > 네트워크**에서 Wi-Fi를 연결할 수 있습니다. Bluetooth는
**설정 > 기기 및 서비스 > Bluetooth**에 나타납니다.

## 업데이트

Home Assistant Core 업데이트는 호스트 커널을 바꾸지 않습니다. HAOS 업데이트는
커널을 바꿀 수 있습니다. `uname -r`이 바뀌면 Wi-Fi와 Bluetooth를 사용하기 전에
SeekWave 모듈 3개를 새 커널에 맞춰 다시 빌드해야 합니다.

새 HAOS 버전을 확인할 때는
`source/reference/validate-generic-ota-contract.sh`도 다시 실행합니다.

## 라이선스

이 프로젝트에서 새로 작성한 코드와 문서는 `AGPL-3.0-or-later`입니다. Linux,
U-Boot, DTS 또는 SeekWave 드라이버를 바탕으로 한 파일은 원래 라이선스를
유지합니다. 자세한 내용은 `LICENSE`와 `THIRD_PARTY_NOTICES.md`를 참고하세요.

## Third-party materials and manufacturer resources / 제3자 자료 및 제조사 참고 자료

The system images and source archives below belong to their respective
authors and rights holders. They were not created or licensed by this project.
Different license terms may apply. A public download link does not by itself
grant permission to redistribute the files.

아래 시스템 이미지와 소스코드는 이 프로젝트에서 만든 자료가 아닙니다. 저작권과
라이선스는 제조사 및 각 권리자에게 있으며 별도 조건이 적용될 수 있습니다. 링크가
공개되어 있다는 사실만으로 재배포가 허용되는 것은 아닙니다.

SeekWave driver files contain per-file GPL-2.0 and MIT notices. No clear
redistribution license was found for the firmware/NVRAM binaries, so they are
not included in the public repository.

SeekWave 드라이버에는 파일별 GPL-2.0 및 MIT 고지가 있습니다. 펌웨어/NVRAM
바이너리에서는 명확한 재배포 라이선스를 찾지 못해 공개 저장소에 포함하지
않습니다.

- [Manufacturer system images / 제조사 시스템 이미지](https://1drv.ms/f/c/106b4b0b39a75ee5/IgARVEENQ10FSLVOu16Z0ZyPAetlTUk5cbx6VCfiH2qOzHo?e=mfTLG8)
- [Manufacturer source code / 제조사 소스코드](https://1drv.ms/f/c/106b4b0b39a75ee5/IgAyC7gbEYJTSbLqbvcl01nTAbTgoKx-YKAY90ieirpIKmA?e=ityIrU)
