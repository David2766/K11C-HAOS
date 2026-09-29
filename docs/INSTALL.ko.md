# K11C HAOS 설치

현재 Windows 포터블 설치 도구는 [사용 안내](../installer/README.md)와
[배포 구성](RELEASE.md)을 참고하세요. 아래 SD 이미지 절차는 기존 수동 방식입니다.
여기서 사용하는 주소를 가공하지 않은 공식 HAOS 이미지에 그대로 적용하면 안 됩니다.

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

기존의 **이미지 복사 후 `gpt repair`에 맡기는 절차는 사용하지 않습니다.**
해당 U-Boot는 주·보조 헤더가 각각 유효하면 GUID가 달라도 성공을 반환합니다.
그래서 디스크 끝의 기존 Android 보조 GPT가 남을 수 있으며,
`gpt verify` 성공 메시지만으로 두 사본이 일치한다고 판단할 수 없습니다.

저장소의 `source/scripts/prepare-emmc-install.py`와 `source/scripts/gpt_image.py`를
사용합니다. 도구는 압축을 푼 K11C `.img`를 읽고 이미지별 U-Boot 명령을
네 단계로 출력합니다. 원본 이미지 수정, 보드 접속, HAOS 컴파일·설치는 하지 않습니다.
출력된 명령은 이미지의 GUID와 같은 파티션 배열로 **주·보조 GPT를 모두 기록**하고,
이미지 영역과 보조 GPT를 각각 다시 읽어 CRC를 확인합니다.

개발 PC에서의 예시입니다. SHA 값은 해당 Release manifest의
`image.raw_sha256`으로 바꿉니다.

```powershell
python .\source\scripts\prepare-emmc-install.py --image C:\K11C-inputs\k11c-haos-18.2-sd.img --sha256 RAW_SHA256_FROM_MANIFEST --target-sectors 61071360
```

확인한 eMMC는 **512바이트 섹터 61071360개**입니다. `29.1 GiB`라는 반올림된
표시만으로 정확한 용량을 정하지 않습니다. 예를 들어 HAOS에서 eMMC 장치가
mmcblk0인지 확인한 뒤 `cat /sys/class/block/mmcblk0/size`로 확인할 수 있습니다.
저장장치가 다른 제품에 이 숫자를 그대로 쓰면 안 됩니다.

출력된 단계별로 MMC 명령 성공과 예상 CRC를 확인하며 진행합니다.
보고서 전체를 한꺼번에 붙여넣지 않습니다. 2단계는 RAM만 바꾸고,
4단계는 **대상 eMMC를 덮어씁니다.** 마지막 읽기 검증의 CRC 두 개가 모두
일치해야 합니다. 명령 실패나 CRC 불일치가 있으면 멈춥니다.

이 명령 순서는 로컬 디스크 파일 시뮬레이션과 실제 HAOS ARM64의 최초
파티션 확장 프로그램으로 검증했습니다. 새 절차를 이용한 실기 설치는 별도
인수 검증 대상입니다. 이를 위해 정상 사용 중인 보드를 재설치하지 않습니다.
이미 설치된 보드는 GPT만 복구하는 도구를 사용합니다.

**eMMC 기록을 마친 뒤 microSD가 꽂힌 상태로 부팅하거나 재시작하지 마세요.**
보드의 전원을 완전히 끄고 전원 케이블을 분리한 다음 microSD를 제거하세요.
그 뒤 전원을 다시 연결하여 반드시 eMMC만으로 처음 부팅해야 합니다. 설치 후
HAOS 첫 부팅은 약 2분 걸릴 수 있습니다.

첫 부팅과 자동 용량 확장 후 HAOS 호스트 셸에서 확인합니다.

```sh
sgdisk --print --verify /dev/mmcblk0
```

종료 코드 0만 보지 말고 보고서의 `No problems found.`를 확인합니다.
이 작업은 **최초 설치 시 디스크 메타데이터 처리**입니다. 공식 HAOS 파티션과
네이티브 OTA 경로는 그대로이며, 일반 HAOS 업데이트 때 설치 도구를 다시
실행하지 않습니다.

eMMC에도 HAOS가 설치된 상태에서 전체 HAOS microSD 이미지로 부팅하면 HAOS가
eMMC 데이터 파티션을 외부 데이터 디스크로 판단할 수 있습니다. 이 경우
`hassos-data` 파일시스템 레이블이 `hassos-data-dis`로 변경되어 다음 eMMC
부팅에서 데이터 파티션을 마운트하지 못할 수 있습니다.
