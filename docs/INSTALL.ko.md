# K11C HAOS 설치

이 안내는 4 GB RAM과 32 GB eMMC가 장착된 KickPi K11C V1.2에서 확인했습니다.
이미지를 기록하기 전에 보드 리비전과 저장장치를 직접 확인하세요.

## microSD에 기록

같은 GitHub Release에서 다음 파일을 받습니다.

- `k11c-haos-<버전>-sd.img.xz`
- `SHA256SUMS`
- `k11c-haos-<버전>-sd.release.txt`

이미지 해시가 `SHA256SUMS`와 같은지 확인합니다. Windows에서는 다음 명령을
사용할 수 있습니다.

```powershell
Get-FileHash .\k11c-haos-<버전>-sd.img.xz -Algorithm SHA256
```

압축을 풀지 말고 `.img.xz` 파일을 balenaEtcher 또는 Rufus로 microSD에 바로
기록합니다. Rufus가 방식을 물으면 DD 이미지 모드를 선택합니다.

카드를 보드에 넣고 전원을 켭니다. 첫 부팅은 약 2분 걸릴 수 있습니다. 처음
Home Assistant를 설정할 때는 유선 LAN을 사용합니다.

UART 설정은 1,500,000 baud, 데이터 8비트, 패리티 없음, 정지 1비트입니다.

## Wi-Fi와 Bluetooth

배포 이미지는 K11C 장치 트리를 포함하지만 SeekWave 펌웨어와 미리 컴파일한
커널 모듈은 포함하지 않습니다. [CONNECTIVITY.ko.md](CONNECTIVITY.ko.md)에 따라 완성된 App
번들을 먼저 만듭니다.

완성된 `k11c_connectivity` 폴더를 Home Assistant OS의
`/addons/k11c_connectivity/`에 복사합니다. Terminal & SSH App에서 다음을
실행합니다.

```bash
ha store reload
ha store apps install local_k11c_connectivity
ha apps start local_k11c_connectivity
ha apps logs local_k11c_connectivity
```

K11C Connectivity App 설정을 다음과 같이 바꿉니다.

```yaml
enabled: true
antenna_mode: share
allow_unverified_board: false
```

App을 다시 시작합니다. 정상적으로 끝나면 다음 로그가 나옵니다.

```text
K11C Wi-Fi and Bluetooth are active on <kernel release>
```

Wi-Fi는 **설정 > 시스템 > 네트워크**, Bluetooth는
**설정 > 기기 및 서비스 > Bluetooth**에서 사용할 수 있습니다.

SeekWave 모듈은 HAOS 커널 릴리스와 정확히 일치해야 합니다. Home Assistant
Core 업데이트는 호스트 커널을 바꾸지 않습니다. HAOS 운영체제 업데이트는
커널을 바꿀 수 있으므로 `uname -r`이 달라지면 새 커널에 맞춰 App payload를
다시 만들어야 합니다. 이때를 대비해 유선 LAN을 사용할 수 있게 두세요.

## 순정 eMMC 백업

테스트한 순정 Android에서는 eMMC가 `/dev/block/mmcblk2`였습니다. 자신의
보드에서도 장치 이름을 먼저 확인합니다.

```text
adb pull /dev/block/mmcblk2boot0 emmc-boot0.bin
adb pull /dev/block/mmcblk2boot1 emmc-boot1.bin
adb pull /dev/block/mmcblk2 emmc-full-32gb.img
```

백업과 SHA-256 값은 보드 밖에 보관합니다.

## U-Boot에서 eMMC로 설치

microSD에서 HAOS가 정상 동작하는지 확인한 뒤 보드의 전원을 끕니다. 시험에
사용한 카드에 같은 Release 이미지를 다시 기록하거나, 같은 이미지를 새로
기록한 두 번째 카드를 준비합니다. 시험 부팅 중 HAOS가 데이터 파티션과 SD의
GPT를 확장할 수 있으므로 다시 기록하는 과정이 필요합니다.

새로 기록한 카드를 넣고 HAOS가 처음 시작되기 전에 U-Boot에서 자동 부팅을
멈춥니다. 아래 수동 방법으로 시험한 HAOS 이미지를 eMMC에 설치합니다. 앞서
microSD에서 만든 설정은 복사되지 않습니다.

**경고: 이 작업은 eMMC 사용자 영역을 덮어씁니다. MMC 장치 번호나 이미지
크기를 잘못 입력하면 다른 저장장치를 망가뜨릴 수 있습니다. 먼저 순정 eMMC를
백업하세요.**

**이미지를 새로 기록하고 아직 HAOS를 한 번도 부팅하지 않은 microSD를
사용하세요.** HAOS는 첫 부팅 때 SD 데이터 파티션을 확장하고 GPT를 바꿀 수
있습니다. 이미 HAOS로 부팅한 카드라면 Release 이미지를 다시 기록한 뒤 이
절차를 시작하세요.

테스트한 K11C V1.2에서는 다음과 같이 확인했습니다.

- `mmc1`: microSD
- `mmc0`: 29.1 GiB로 표시되는 eMMC

microSD에 기록한 이미지와 같은 Release manifest에서
`image.raw_sectors_hex`와 `image.raw_bytes_hex` 값을 확인합니다. 해당 값을
각각 `SECTORS`와 `BYTES` 자리에 넣습니다. 다른 Release의 값을 재사용하면
안 됩니다.

확인한 `k11c-haos-18.2-sd` 이미지의 값은 다음과 같습니다.

```text
SECTORS = 1d3828
BYTES   = 3a705000
```

U-Boot 자동 부팅을 멈춥니다. 원본 microSD를 확인하고 Release 이미지 영역만
RAM으로 읽은 뒤 CRC를 기록합니다.

```text
mmc dev 1
mmc info
mmc part
mmc read 40000000 0 SECTORS
crc32 40000000 BYTES
```

eMMC를 선택하고 쓰기 명령을 입력하기 전에 장치와 용량을 확인합니다.

```text
mmc dev 0
mmc info
mmc part
```

`mmc0`이 예상한 29.1 GiB eMMC일 때만 계속합니다.

```text
mmc write 40000000 0 SECTORS
mmc read 40000000 0 SECTORS
crc32 40000000 BYTES
```

원본과 기록 후 CRC가 같아야 합니다. 다르면 다음 단계로 진행하지 마세요.
eMMC 전체 용량에 맞게 백업 GPT를 복구하고 결과를 확인합니다.

```text
gpt repair mmc 0
gpt verify mmc 0
mmc part
```

**eMMC 기록을 마친 뒤 microSD가 꽂힌 상태로 부팅하거나 재시작하지 마세요.**
보드의 전원을 완전히 끄고 전원 케이블을 분리한 다음 microSD를 제거하세요.
그 뒤 전원을 다시 연결하여 반드시 eMMC만으로 처음 부팅해야 합니다. 설치 후
HAOS 첫 부팅은 약 2분 걸릴 수 있습니다.

eMMC에도 HAOS가 설치된 상태에서 전체 HAOS microSD 이미지로 부팅하면 HAOS가
eMMC 데이터 파티션을 외부 데이터 디스크로 판단할 수 있습니다. 이 경우
`hassos-data` 파일시스템 레이블이 `hassos-data-dis`로 변경되어 다음 eMMC
부팅에서 데이터 파티션을 마운트하지 못할 수 있습니다.
