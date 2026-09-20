# K11C Connectivity

Wi-Fi/Bluetooth, the NPU driver, HTTP inference and native Frigate preparation
are separate options. Keep your existing wireless settings when updating.

## Options

- `enabled`: existing Wi-Fi/Bluetooth activation switch.
- `antenna_mode`, `allow_unverified_board`: existing wireless options.
- `npu_enabled`: load the optional Rockchip NPU driver at App startup. Defaults
  to `false`, independently of `enabled`. NPU board checks cannot be overridden.
- `inference_enabled`: run the existing HTTP detection API. See [INFERENCE.md](INFERENCE.md).
- `frigate_native_enabled`: prepare direct inference inside official Frigate Full Access.
  Requires `npu_enabled: true` and `inference_enabled: false`; defaults to false.
- `frigate_native_app`: Full Access App slug; default `ccab4aaf_frigate-fa`.

## Automatic FFmpeg installation (0.5.3)

With native preparation enabled, Connectivity also installs the tested FFmpeg
bundle at `/config/k11c-ffmpeg-vpu-r1` inside the selected Full Access App.
An identical manual installation is reused; only missing or recognized changed
files are copied. Files retained in Frigate's `/config` survive container
recreation and are not copied again. Modified user files are not overwritten.
If everything already matches, no Frigate restart is needed.

`K11C_FFMPEG_READY ... copied=0 reused=33` means the existing FFmpeg was reused.
A fresh install reports `copied=33 reused=0`. Existing camera YAML, detector
count, model, resolution, FPS and acceleration settings are not changed.
No host reboot is needed when updating from 0.5.2. Keep both Apps running for
preparation; a cleanly stopped Frigate is left stopped until you start it.

For a new installation, select `path: /config/k11c-ffmpeg-vpu-r1` in Frigate's
`ffmpeg` settings, with the existing `v4l2request` hardware acceleration settings.
Copying the bundle alone does not enable hardware decoding.

## Native Frigate (0.5.0)

Disable protection on Connectivity and Frigate Full Access. Connectivity 0.5.0
requests Docker socket access, which can control the host. The native option
controls its use, not the scope of that permission.

Start both Apps. Connectivity adds its detector and existing C API runtime/model
to the selected official container, then restarts it once. Wait for
`K11C_NATIVE_PREPARED`. In Frigate's own configuration editor, use:

```yaml
detectors:
  k11c:
    type: k11c_rknn
model:
  path: /opt/k11c-native/model.rknn
  model_type: yolox
  width: 416
  height: 416
  input_tensor: nhwc
  input_pixel_format: rgb
  labelmap_path: /labelmap/coco-80.txt
  labelmap:
    5: bus
    7: truck
```

Keep camera/decoding settings unchanged and remove other detector entries for
the initial one-detector test. Save/restart Frigate; look for `K11C_NATIVE_READY`
and verify real detections. One native detector creates one RKNN context.
The native path uses Frigate's RKNN postprocessing without JPEG or HTTP transport.
No RKNNLite SDK is added. Board performance of this build has not yet been measured.

Official updates remain enabled. Connectivity reapplies additional files after
container recreation; it does not overwrite original Frigate code or change
AppArmor. Future changes to the internal detector interface may require a
Connectivity update. Clean user stops are respected. Preparation failures are
logged without repeated restarts. Disabling preparation does not switch an
already-running native detector: restore the previous DeepStack/model settings
and enable the HTTP API for rollback. Update instructions are in the `TEST.md`
distributed beside the local ZIP. Manual VPU-file transfer is no longer needed.

## NPU driver and OS updates

Keep your current wireless options when updating. With `npu_enabled: true`,
the expected log is `K11C_NPU_READY ...`, followed by `NPU driver ready`.
This confirms driver readiness, not an inference test.

The NPU requires the compatible r22 K11C device tree and **6.18.39-haos**.
Version 0.5.1 selects the highest enabled manufacturer OPP up to **900 MHz**.
Voltage follows OTP/PVTM/bin and cold compensation, not a fixed voltage setting.
The read-only OTP module and U-Boot/DT are unchanged. **Reboot the host after
updating** to replace the loaded RKNPU module; no U-Boot flash is needed.
Keep the current two-detector Frigate configuration when upgrading an existing
installation. This change does not edit Frigate configuration or video decoding.

Enable **Start on boot** in Home Assistant to load it after a reboot. An App
restart reuses the same verified module. Disabling NPU or stopping the App
does not unload a module another App might be using. To remove it for the
current session, disable `npu_enabled`, save, and reboot the host.

In HTTP mode, inference runs inside Connectivity; Frigate's DeepStack client
does not need the RKNN runtime or NPU device access.

Core updates do not replace the host kernel. **HAOS updates can.** For an
unsupported kernel the NPU service refuses to load the module; it does not
patch HAOS, force-load an incompatible module, or block Wi-Fi/BT startup.
Wireless kernel compatibility is still governed by its own existing bundle.

## Native timing capture (0.5.2)

In Frigate's own YAML, add `timing_samples: 256` to each `k11c_rknn` detector.
The optional `timing_delay_seconds` defaults to 60. Save/restart Frigate; keep
the same cameras, model, thresholds, NPU profile and decoder configuration.
After the delay, each process measures its next 256 real detections and logs
one `K11C_NATIVE_TIMING_SUMMARY`. Quiet scenes take longer to fill the sample.
There are no extra inference requests or recorded images. Remove the two fields
or set `timing_samples: 0` after collecting; the default is disabled. Otherwise
each subsequent Frigate restart rearms that bounded capture.

SDK spans include SDK waiting/transfer/conversion; they are not pure NPU time.
The current 50 ms dashboard value is a different, outer smoothed measurement.
Use stage means for additive attribution; stage medians/p95 do not add.
Updating from 0.5.1 does not need a host reboot; the NPU binary is unchanged.

## 한국어

### FFmpeg 자동 설치 (0.5.3)

native 옵션을 켜면 지정한 Full Access 앱의 `/config/k11c-ffmpeg-vpu-r1`에
검증된 FFmpeg 묶음도 준비한다. 수동으로 설치한 파일과 내용이 같으면 재사용하고,
누락된 파일만 추가한다. 앱 컨테이너가 재생성돼도 `/config`에 남아 있는 파일은
다시 복사하지 않는다. 사용자가 수정한 파일과 카메라 YAML은 덮어쓰지 않는다.
모든 파일이 같으면 Frigate를 재시작하지 않는다.

`K11C_FFMPEG_READY ... copied=0 reused=33`은 기존 설치를 그대로 사용했다는 뜻이다.
새로 설치하면 `copied=33 reused=0`이 나온다. 해상도·FPS·탐지기·모델·가속 설정은
바뀌지 않으며, 0.5.2에서 업데이트할 때 호스트 재부팅은 필요 없다.
준비할 때 두 앱을 켜둔다. 사용자가 정상 중지한 Frigate는 임의로 켜지 않는다.
새 설치에서는 Frigate의 `ffmpeg.path`에 위 경로와 기존 `v4l2request` 설정을 지정한다.
파일 설치만으로 영상 가속 설정까지 자동 변경되지는 않는다.

기존 무선 옵션은 그대로 유지한다. 직접 추론은 `npu_enabled: true`,
`inference_enabled: false`, `frigate_native_enabled: true`로 켠다.
대상은 `frigate_native_app`에 지정한 공식 Frigate Full Access 앱이다.
Connectivity와 Full Access의 보호 모드를 꺼야 한다. Connectivity에 추가되는
Docker 접근 권한은 호스트를 제어할 수 있는 강한 권한이다.

두 앱을 시작하고 `K11C_NATIVE_PREPARED`를 확인한 뒤, Frigate 자체 설정 편집기에서
위 YAML을 적용한다. 최초 준비 때 Full Access를 한 번 재시작한다. 처음에는 탐지기
하나로 확인하며 카메라·가속 설정은 유지한다. 탐지기 하나당 RKNN 컨텍스트 하나를
사용한다. `K11C_NATIVE_READY`와 실제 감지를 확인한다. 이 빌드의 보드 성능은
아직 확인하지 않았다. 기존 API로 돌아가려면 Frigate의 DeepStack/model 설정을
복원하고 Connectivity의 native 옵션을 끈 뒤 HTTP API 옵션을 다시 켠다.

공식 Frigate 업데이트는 그대로 받는다. 컨테이너 재생성 시 추가 파일을 다시 넣으며,
Frigate 원본 코드와 AppArmor는 바꾸지 않는다. 내부 인터페이스가 바뀌면 Connectivity
수정이 필요할 수 있다. 업데이트 방법은 ZIP 옆의 `TEST.md`에 있다. VPU 파일은 이제
수동으로 복사하지 않아도 된다.

0.5.1은 제조사 OPP 중 해당 칩에서 허용된 최대 900MHz를 기본값으로 사용한다.
전압은 OTP/PVTM·칩 등급·저온 보정에 따라 결정하며 고정하지 않는다.
기존 r22 U-Boot/DT와 OTP 모듈은 그대로다. **업데이트 후 호스트를 한 번 재부팅**해야
새 NPU 드라이버가 로드된다. U-Boot를 다시 굽거나 Frigate를 재설치할 필요는 없다.
사용 중인 두 탐지기와 영상 가속 설정은 유지한다. `K11C_NPU_READY`는 드라이버 준비
완료 표시이며 실제 추론 성공 표시와는 다르다.

HA의 **부팅 시 시작**도 켜야 재부팅 후 자동 로딩된다. NPU를 끄거나 App을 중지해도
사용 중일 수 있는 드라이버를 강제로 내리지 않는다. 완전히 내리려면 NPU 옵션을 끄고
저장한 뒤 호스트를 재부팅한다.

HAOS 커널이 바뀌면 새 커널에 맞는
모듈이 필요하며, 지원하지 않는 커널에서는 NPU 로딩을 중단한다. 무선 드라이버의
커널 호환성은 기존 무선 번들에 따라 별도로 결정된다.

### 구간별 측정 (0.5.2)

Frigate 자체 YAML의 각 `k11c_rknn` 탐지기에 `timing_samples: 256`을 추가하고
저장·재시작한다. 기본 60초 대기 후 실제 요청 256개를 측정해 탐지기마다
`K11C_NATIVE_TIMING_SUMMARY` 한 줄을 남긴다. 움직임이 적으면 완료까지 더 걸린다.
모델·임계값·입력·탐지 결과는 바뀌지 않으며 추가 추론이나 이미지 저장은 없다.
수집 후 항목을 지우거나 `timing_samples: 0`으로 되돌린다. 설정이 남아 있으면
다음 Frigate 재시작 때 다시 한 번 측정한다. SDK 시간은 순수 NPU 연산 시간이 아니다.
0.5.1에서 업데이트할 때는 NPU 모듈이 같으므로 호스트 재부팅이 필요 없다.
