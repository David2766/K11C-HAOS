# Third-party notices

This repository is a mixed-license work. The `AGPL-3.0-or-later` license in
the root `LICENSE` file applies only to original project material that does
not carry a different license notice. It does not relicense third-party code,
firmware, operating-system images, or other vendor material.

## Component licensing

| Component | License or status |
| --- | --- |
| Original K11C HAOS scripts, App control code, documentation, and build glue | `AGPL-3.0-or-later`, unless the file says otherwise |
| `rk3566-kickpi-k11c.dts` and `rk3566-kickpi-k11c-u-boot.dtsi` | `(GPL-2.0-or-later OR MIT)`, preserving the source SPDX expression `GPL-2.0+ OR MIT` |
| U-Boot board hook and other copied U-Boot files | Their upstream U-Boot license, including `GPL-2.0-or-later` where marked |
| SeekWave VS/SWT6621S driver source and patches against that source | The license of each affected source file; observed notices include `GPL-2.0-only`, `GPL-2.0-or-later`, and `MIT` |
| Linux, U-Boot, Home Assistant OS, Home Assistant, Buildroot, and their dependencies | Their respective upstream licenses |
| SeekWave firmware and NVRAM binaries | No redistribution license was identified in the obtained vendor package |

`MODULE_LICENSE()` strings describe a module to the Linux module loader; they
are not a substitute for the license notices in the source files. See the
[Linux kernel licensing rules](https://docs.kernel.org/process/license-rules.html).

## Binary distribution boundary

The public repository export intentionally omits:

- prebuilt `*.ko` kernel modules;
- SeekWave `*.bin` and `*.nvbin` firmware/NVRAM files;
- ready-to-flash HAOS images and vendor OS backups.

The local build script can copy firmware from a vendor tree supplied by the
user and can compile kernel-matched modules. Those locally generated files are
ignored and preserved by the exporter, but they are not covered by the
project's AGPL license and are not included in the public checksum manifest.

Before redistributing any generated App bundle, obtain and retain the license
terms or written permission that covers the exact firmware files. If binary
GPL modules are distributed, provide the corresponding source and build
information required by their licenses.

## Warranty and affiliation

This is an independent experimental port. It is not an official KickPi,
SeekWave, Rockchip, Home Assistant, or Nabu Casa release. No warranty is
provided.

---

# 제3자 라이선스 고지

이 저장소에는 여러 라이선스가 섞여 있습니다. 루트 `LICENSE`의
`AGPL-3.0-or-later`는 별도 라이선스 고지가 없는 프로젝트 자체 작성물에만
적용됩니다. 제3자 코드, 펌웨어, 운영체제 이미지나 제조사 자료를 AGPL로
재라이선스하지 않습니다.

- K11C HAOS용 자체 스크립트, App 제어 코드, 문서와 빌드 보조 코드:
  별도 표시가 없으면 `AGPL-3.0-or-later`
- K11C DTS/DTSI: 원본의 `(GPL-2.0-or-later OR MIT)` 유지
- U-Boot 코드: 해당 파일의 upstream U-Boot 라이선스 유지
- SeekWave 드라이버와 그 소스에 적용되는 패치: 대상 파일별 라이선스 유지
- SeekWave 펌웨어/NVRAM 바이너리: 확보한 제조사 패키지에서 재배포 허가를
  확인하지 못함

따라서 공개 export에서는 미리 빌드한 커널 모듈, 제조사 펌웨어와 완성된 디스크
이미지를 제외합니다. 사용자가 자신의 제조사 자료와 HAOS 빌드 환경에서 만든
로컬 결과물은 exporter가 삭제하지 않지만, 공개 배포물이나 공개 해시 목록에는
포함하지 않습니다.
