<!-- K11C CURRENT RELEASE -->
# K11C HAOS

Home Assistant OS for **KickPi K11C V1.2 — RK3566, 4 GB RAM, 32 GB eMMC**.

The official generic-aarch64 HAOS image runs with K11C boot firmware and the
Connectivity App. Once installed, HAOS updates through Home Assistant as usual;
a new custom OS image is not needed for each release.

공식 HAOS를 사용하는 K11C용 부팅 펌웨어와 Connectivity 앱입니다.
설치 후에는 Home Assistant의 기본 OS 업데이트를 그대로 사용합니다.

[한국어](#한국어) · [Downloads](https://github.com/David2766/K11C-HAOS/releases) ·
[Installer](installer/README.md) · [Build](docs/BUILD.md)
<!-- END K11C CURRENT RELEASE -->

## Hardware support

| Feature | Support |
| --- | --- |
| Boot | eMMC and microSD |
| eMMC | HS200, 200 MHz, 8-bit bus |
| Ethernet | Gigabit LAN |
| Wi-Fi / Bluetooth | SeekWave module, enabled by the Connectivity App |
| NPU | RKNN inference; chip-specific voltage selection, up to 900 MHz |
| Video decoding | H.264 through Hantro / V4L2 Request |
| Audio | RK809: 3.5 mm output, internal speaker and microphone headers |
| RTC | Battery-backed HYM8563 as `rtc0`, used to restore the boot clock |
| Display / USB | HDMI and USB host |

The board has been tested with HAOS 18.2 and 18.3, including an in-place OS
update. The [Connectivity release metadata](k11c_connectivity/RELEASE.json)
lists the HAOS versions and kernel modules included in the published App.

Bluetooth has shown intermittent controller resets during extended use.
eDP/MIPI, camera headers, general-purpose GPIO and RTC alarm/wake are not yet
validated.

## Installation

Use a wired network connection for initial Home Assistant setup.

The [Windows portable installer](installer/README.md) supports official HAOS
image downloads or local `.img.xz` / `.img` files. It prepares the K11C boot
layout and GPT for the target eMMC. Advanced tools provide U-Boot-only updates,
boot-area backup/restore and GPT inspection/repair.

Installer **0.5.0** is a pre-release with physical USB flashing validation still
pending. Its source is in this repository; downloadable builds are listed in
[Releases](https://github.com/David2766/K11C-HAOS/releases). The current boot
firmware source is [U-Boot r24](source/boot-release.json).

**A fresh OS installation erases the existing OS and user data.** Keep a Home
Assistant backup outside the board. The installer's boot-area backup is not a
backup of Home Assistant settings or recordings.

Do not write an unmodified generic-aarch64 image directly over the K11C eMMC:
the board needs its boot firmware and disk layout. The older K11C SD-image
installation method is documented in the [manual installation guide](docs/INSTALL.md).

## Connectivity App

Add this repository in the Home Assistant App Store:

```text
https://github.com/David2766/K11C-HAOS
```

Install **K11C Connectivity**, enable **Start on boot**, and set the wireless
options below. Save and start the App, or restart it if it is already running.

```yaml
enabled: true
antenna_mode: share
allow_unverified_board: false
```

Wi-Fi is then configured through Home Assistant's network settings. Bluetooth
devices use the Home Assistant Bluetooth integration.

NPU and Frigate support are optional and separate from wireless activation:

| Option | Purpose |
| --- | --- |
| `npu_enabled` | Load the NPU driver |
| `inference_enabled` | Run the HTTP object-detection API |
| `frigate_native_enabled` | Prepare direct NPU inference in official Frigate Full Access |
| `frigate_native_app` | Select the Frigate Full Access App to prepare |

Native Frigate mode requires `npu_enabled: true` and `inference_enabled: false`.
It adds the RKNN runtime, model, detector adapter and Hantro FFmpeg bundle to the
official Frigate App. Camera and hardware-decoding settings still belong in
Frigate's configuration. Native mode requires protection mode to be disabled
for Connectivity and Frigate Full Access, granting broad host access.

See [App configuration](k11c_connectivity/DOCS.md) and
[HTTP inference](k11c_connectivity/INFERENCE.md) for setup details.

## Updates

- **HAOS / Home Assistant Core:** use Home Assistant's normal update controls.
- **Connectivity:** update through the App Store; automatic installation is
  available when App auto-update is enabled. GitHub Actions checks official
  HAOS releases daily and publishes a compatible App after a successful build.
- **U-Boot:** updated separately. Routine HAOS or Connectivity updates do not
  flash the boot firmware.

Before an OS update, check that the published Connectivity App includes its
kernel. A failed build or a new kernel incompatibility can delay App support.
Keep wired LAN available for recovery.

Frigate remains the official App and keeps its normal update path. Changes to
Frigate's detector interface may require a matching Connectivity update.

## 한국어

### 지원 기능

K11C V1.2의 eMMC·microSD 부팅, 기가비트 LAN, HDMI, USB 호스트,
RK809 오디오 출력·마이크 입력을 확인했습니다. eMMC는 **HS200 200MHz**로
동작하며, 배터리가 연결된 HYM8563 RTC의 시각을 부팅 시 복원합니다.
**HAOS 18.2 → 18.3 기본 OS 업데이트**도 확인했습니다.

Connectivity 앱은 Wi-Fi·Bluetooth 드라이버와 선택 기능인 RKNN NPU 추론을
제공합니다. NPU는 칩별 전압 설정을 적용하며 최대 900MHz로 동작합니다.
Frigate Full Access에서는 직접 NPU 추론과 Hantro 기반 H.264 영상 디코딩을
사용할 수 있습니다. 카메라 연결과 영상 가속은 Frigate에서 별도로 설정합니다.

장시간 사용 중 Bluetooth 컨트롤러가 간헐적으로 재시작되는 문제가 있습니다.
eDP/MIPI, 카메라 헤더, 범용 GPIO 및 RTC 알람·깨우기는 아직 검증하지 않았습니다.

### 설치와 앱 설정

[Windows 설치 도구](installer/README.md)는 공식 HAOS 이미지 다운로드,
로컬 이미지 선택, eMMC 설치와 U-Boot 업데이트를 제공합니다.
**0.5.0은 실물 USB 기록 검증이 남은 사전 배포 버전**입니다.
현재 부팅 펌웨어 소스는 r24이며, 다운로드 파일은
[Releases](https://github.com/David2766/K11C-HAOS/releases)에서 확인할 수 있습니다.
기존 SD 이미지 설치 방법은 [수동 설치 안내](docs/INSTALL.ko.md)를 참고하세요.

**새 OS 설치는 기존 OS와 사용자 데이터를 삭제합니다.** Home Assistant 전체
백업을 보드 밖에 보관하세요. 설치 도구의 부팅 영역 백업에는 HA 설정·녹화가
포함되지 않습니다. 공식 generic-aarch64 이미지를 eMMC에 직접 덮어쓰지 말고,
K11C 부팅 펌웨어와 디스크 구성을 함께 준비해야 합니다.

첫 설정은 유선 LAN으로 진행합니다. HA 앱 스토어에 이 저장소 주소를 추가하고
**K11C Connectivity**를 설치하세요. **부팅 시 시작**을 켜고 위 무선 설정의
`enabled: true`를 적용한 뒤 앱을 시작합니다. 실행 중이었다면 재시작합니다.
이후 Wi-Fi는 HA 네트워크 설정에서, BLE 기기는 Bluetooth 통합에서 관리합니다.

Frigate 직접 추론은 `npu_enabled: true`, `inference_enabled: false`,
`frigate_native_enabled: true`로 설정합니다. Connectivity와 Frigate Full Access의
보호 모드를 꺼야 하며, 이 설정은 호스트에 대한 강한 접근 권한을 부여합니다.
자세한 설정은 [앱 사용 안내](k11c_connectivity/DOCS.md)를 참고하세요.

### 업데이트

HAOS와 Core는 Home Assistant의 기본 업데이트를 사용합니다. OS 버전마다
별도의 커스텀 이미지를 다시 설치할 필요가 없습니다. Connectivity는 앱
스토어에서 업데이트하며, 앱 자동 업데이트를 켜면 새 버전을 자동 설치합니다.
U-Boot는 별도로 갱신하며 일반 OS·앱 업데이트로 덮어쓰지 않습니다.

GitHub Actions가 공식 HAOS 릴리스를 매일 확인하고 빌드에 성공하면 Connectivity
업데이트를 배포합니다. 새 커널과의 호환 문제로 배포가 늦어질 수 있으므로,
OS 업데이트 전에 [지원 커널 목록](k11c_connectivity/RELEASE.json)을 확인하고
복구용 유선 LAN 연결을 확보하세요.

## Build and source

- [Build instructions](docs/BUILD.md) / [빌드 안내](docs/BUILD.ko.md)
- [Installer source](installer) / [Connectivity CI](docs/CONNECTIVITY-CI.md)
- [Manufacturer system images](https://1drv.ms/f/c/106b4b0b39a75ee5/IgARVEENQ10FSLVOu16Z0ZyPAetlTUk5cbx6VCfiH2qOzHo?e=mfTLG8)
- [Manufacturer source](https://1drv.ms/f/c/106b4b0b39a75ee5/IgAyC7gbEYJTSbLqbvcl01nTAbTgoKx-YKAY90ieirpIKmA?e=ityIrU)

## License / 라이선스

Original project code and documentation are licensed under **AGPL-3.0-or-later**.
U-Boot, Linux, Rockchip, SeekWave and other third-party components retain their
own licenses. See [LICENSE](LICENSE), [third-party notices](THIRD_PARTY_NOTICES.md)
and the notices shipped with each component.

프로젝트 자체 코드와 문서는 **AGPL-3.0-or-later**입니다. U-Boot, Linux,
Rockchip, SeekWave 등 외부 구성요소에는 각 원본 라이선스가 적용됩니다.
