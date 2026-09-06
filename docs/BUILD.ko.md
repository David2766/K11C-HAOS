# K11C HAOS 빌드

이 저장소에는 HAOS 이미지, Rockchip 바이너리 blob, SeekWave 펌웨어, 미리
컴파일한 SeekWave 커널 모듈이 없습니다. 각 권리자가 제공한 파일을 직접
준비하고 해당 라이선스 조건을 확인하세요.

빌드 스크립트는 Linux 또는 WSL2에서 실행합니다. 기존 출력 파일은
덮어쓰지 않습니다.

## 바로 굽는 이미지 만들기

필요한 입력은 다음과 같습니다.

- 공식 `haos_generic-aarch64-<버전>.img.xz`와 SHA-256 값
- 지정된 커밋의 깨끗한 upstream U-Boot 트리
- `rk3566_ddr_1056MHz_v1.23.bin`
- `rk3568_bl31_v1.44.elf`

Rockchip 파일은 제조사 소스 자료 또는 Rockchip `rkbin`에서 구할 수 있으며
각 파일의 라이선스 조건을 따릅니다.

확인한 HAOS 18.2 입력을 사용하는 예시입니다.

```bash
git clone https://source.denx.de/u-boot/u-boot.git
git -C u-boot checkout 88dc2788777babfd6322fa655df549a019aa1e69

bash source/scripts/build-k11c-image.sh \
  --haos-image /path/to/haos_generic-aarch64-18.2.img.xz \
  --haos-sha256 dae257b7b2ce3860f3ce187a85d27b3e9aaaebf55e6acb50f38115e7d7b783a1 \
  --uboot-tree "$PWD/u-boot" \
  --ddr-blob /path/to/rk3566_ddr_1056MHz_v1.23.bin \
  --bl31 /path/to/rk3568_bl31_v1.44.elf \
  --output-dir /path/outside/the/repository/dist
```

출력 폴더에는 원본 크기 이미지, 압축 이미지, U-Boot 파일, Release manifest,
검증 manifest, 고지문, `SHA256SUMS`가 생성됩니다. 큰 출력 파일은 Git
저장소 밖에 보관하세요. 배포할 때는 압축된 `.img.xz`와 작은 metadata 파일만
GitHub Release 첨부파일로 올릴 수 있습니다.

이미지 빌더는 지정된 U-Boot 소스와 결과물을 검증하고, generic HAOS의 8개
파티션 내용을 바꾸지 않은 채 옮깁니다. 새 GPT를 검사하고 일부러 망가뜨린
이미지가 검사에서 거부되는지도 확인합니다.

## K11C Connectivity App payload 만들기

필요한 입력은 다음과 같습니다.

- 이 저장소
- 대상 커널과 정확히 일치하도록 구성된 HAOS 빌드 트리
- `external/rkwifibt`를 포함한 제조사 소스 트리

App 골격은 `k11c_connectivity`에 있습니다. 다음을 실행합니다.

```bash
bash source/connectivity/scripts/build-driver-bundle.sh \
  --haos-tree /path/to/configured/haos-build-tree \
  --vendor-tree /path/to/manufacturer-source \
  --app-dir "$PWD/k11c_connectivity"

bash source/connectivity/scripts/verify-connectivity-bundle.sh \
  "$PWD/k11c_connectivity"
```

스크립트는 확인된 제조사 소스와 펌웨어 해시를 검사하고 Linux 6.18 호환
패치를 적용한 뒤 다음 AArch64 모듈을 만듭니다.

- `skw_sdio_lite.ko`
- `swt6621s_wifi.ko`
- `skwbt.ko`

커널별 모듈은 `k11c_connectivity/modules/<커널 릴리스>/`, 펌웨어는
`k11c_connectivity/firmware/`에 들어갑니다. 두 폴더는 Git에서 무시되며
저장소 export에도 포함되지 않습니다.

현재 패치 세트의 대상은 `6.18.39-haos`입니다. HAOS 커널이 바뀌면 이전
모듈을 재사용하지 말고 새 커널에 맞춰 빌드와 검증을 다시 실행하세요.

## 제조사 자료

- [시스템 이미지](https://1drv.ms/f/c/106b4b0b39a75ee5/IgARVEENQ10FSLVOu16Z0ZyPAetlTUk5cbx6VCfiH2qOzHo?e=mfTLG8)
- [소스코드](https://1drv.ms/f/c/106b4b0b39a75ee5/IgAyC7gbEYJTSbLqbvcl01nTAbTgoKx-YKAY90ieirpIKmA?e=ityIrU)

