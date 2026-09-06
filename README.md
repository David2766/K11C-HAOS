# K11C HAOS

Home Assistant OS support for the KickPi K11C V1.2.

This project combines the official generic AArch64 HAOS image with a lightly
patched mainline U-Boot and a K11C device tree. The eight HAOS partitions are
copied without changing their contents.

## Status

Tested with HAOS 18.2 on this hardware:

- Rockchip RK3566
- 4 GB DDR3L and 32 GB eMMC
- Maxio MAE0621-Q2C Ethernet PHY
- SeekWave VS6621SR80 Wi-Fi/Bluetooth module using SWT6621S firmware

Confirmed working:

- microSD boot, including complete power-off boot
- eMMC boot on the tested board
- Gigabit Ethernet
- onboard Wi-Fi and Bluetooth through the K11C Connectivity App
- HDMI
- USB host and USB OTG
- basic RTC timekeeping

Not checked yet: generic HAOS operating-system OTA, eDP/MIPI, camera, NPU,
analog audio, RTC alarm/wake, remaining GPIO, and eMMC HS200. The tested eMMC
configuration currently runs at 52 MHz. The installation guide includes the
manual U-Boot procedure for installing the tested image to eMMC after the
microSD test.

## Documentation

- [Install, first boot, wireless setup, and eMMC installation](docs/INSTALL.md)
- [Build the ready-to-flash HAOS image](docs/BUILD.md)
- [Build the K11C Connectivity App](docs/CONNECTIVITY.md)
- [한국어 설치 안내](docs/INSTALL.ko.md)
- [한국어 HAOS 이미지 빌드](docs/BUILD.ko.md)
- [한국어 Connectivity App 빌드](docs/CONNECTIVITY.ko.md)

Ready-to-flash images, when published, are available under
[Releases](https://github.com/David2766/K11C-HAOS/releases). Large images are
not stored in the Git repository.

The release image includes the K11C U-Boot and device tree. It does not include
SeekWave firmware or prebuilt SeekWave modules. Those files must be obtained
from their rights holders and built for the exact HAOS kernel.

## Manufacturer files

- [System images](https://1drv.ms/f/c/106b4b0b39a75ee5/IgARVEENQ10FSLVOu16Z0ZyPAetlTUk5cbx6VCfiH2qOzHo?e=mfTLG8)
- [Source code](https://1drv.ms/f/c/106b4b0b39a75ee5/IgAyC7gbEYJTSbLqbvcl01nTAbTgoKx-YKAY90ieirpIKmA?e=ityIrU)

---

# K11C HAOS 한국어 안내

KickPi K11C V1.2에서 Home Assistant OS를 사용하기 위한 프로젝트입니다.

공식 generic AArch64 HAOS 이미지에 K11C 장치 트리와 가볍게 수정한 mainline
U-Boot를 결합합니다. HAOS의 8개 파티션 내용은 바꾸지 않고 그대로 옮깁니다.

## 현재 상태

다음 구성의 보드에서 HAOS 18.2로 확인했습니다.

- Rockchip RK3566
- DDR3L 4 GB, eMMC 32 GB
- Maxio MAE0621-Q2C Ethernet PHY
- SWT6621S 펌웨어를 사용하는 SeekWave VS6621SR80 Wi-Fi/Bluetooth 모듈

확인된 기능:

- microSD 부팅과 완전 전원 차단 후 부팅
- 테스트 보드의 eMMC 부팅
- 기가비트 유선 LAN
- K11C Connectivity App을 통한 온보드 Wi-Fi와 Bluetooth
- HDMI
- USB host와 USB OTG
- RTC 기본 시간 유지

generic HAOS 운영체제 OTA, eDP/MIPI, 카메라, NPU, 아날로그 오디오, RTC
alarm/wake, 나머지 GPIO, eMMC HS200은 아직 확인하지 않았습니다. 확인한 eMMC
설정은 현재 52 MHz로 동작합니다. SD에서 시험을 마친 뒤 같은 이미지를
eMMC에 설치하는 수동 U-Boot 절차는 설치 안내에서 확인할 수 있습니다.

## 문서

- [설치, 첫 부팅, 무선 설정, eMMC 설치](docs/INSTALL.ko.md)
- [SD 카드용 HAOS 이미지 빌드](docs/BUILD.ko.md)
- [K11C Connectivity App 빌드](docs/CONNECTIVITY.ko.md)
- [English installation guide](docs/INSTALL.md)
- [English HAOS image build guide](docs/BUILD.md)
- [English Connectivity App build guide](docs/CONNECTIVITY.md)

바로 기록할 수 있는 이미지는 준비된 경우
[Releases](https://github.com/David2766/K11C-HAOS/releases)에 올립니다. 큰 이미지
파일은 Git 저장소에 넣지 않습니다.

배포 이미지는 K11C U-Boot와 장치 트리를 포함하지만 SeekWave 펌웨어와 미리
컴파일한 SeekWave 모듈은 포함하지 않습니다. 해당 파일은 각 권리자에게서
직접 구하고 정확히 일치하는 HAOS 커널용으로 빌드해야 합니다.

## 제조사 자료

- [시스템 이미지](https://1drv.ms/f/c/106b4b0b39a75ee5/IgARVEENQ10FSLVOu16Z0ZyPAetlTUk5cbx6VCfiH2qOzHo?e=mfTLG8)
- [소스코드](https://1drv.ms/f/c/106b4b0b39a75ee5/IgAyC7gbEYJTSbLqbvcl01nTAbTgoKx-YKAY90ieirpIKmA?e=ityIrU)

## License / 라이선스

Original project code and documentation are licensed under
`AGPL-3.0-or-later`. Files derived from Linux, U-Boot, device-tree, Rockchip,
or SeekWave material keep their own license terms. See `LICENSE`,
`THIRD_PARTY_NOTICES.md`, per-file notices, and the licenses shipped with each
release.

이 프로젝트에서 새로 작성한 코드와 문서는 `AGPL-3.0-or-later`로
배포합니다. Linux, U-Boot, 장치 트리, Rockchip, SeekWave 자료에서 가져온
부분은 각 자료의 원래 라이선스를 따릅니다. 자세한 내용은 `LICENSE`,
`THIRD_PARTY_NOTICES.md`, 파일별 고지와 각 Release에 포함된 라이선스를
확인하세요.
