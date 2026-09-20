# NPU detection API

This page describes **HTTP mode**. For direct inference introduced in 0.5.0,
see [DOCS.md](DOCS.md). Version 0.5.1 changes the default NPU request to the
highest enabled manufacturer OPP up to 900 MHz. Reboot the host after updating
to load the new module. U-Boot, HTTP processing and permissions are unchanged.

이 문서는 HTTP API 방식 안내다. 0.5.0의 직접 추론은 [DOCS.md](DOCS.md)를 참고한다.
0.5.1은 제조사 OPP 중 허용된 최대 900MHz를 기본값으로 사용한다.
업데이트 후 호스트를 재부팅해야 새 모듈이 로드된다. U-Boot·HTTP 처리·권한은 그대로다.

Requires the K11C NPU DT and `6.18.39-haos`. App 0.4.4 adds r22 manufacturer
voltage selection with the captured OPP table, OTP/PVTM, MBIST and cold-temperature
correction. The normal request is now up to 900 MHz; the idle clock may read 198/200 MHz.
There is no new load-based governor. Earlier r19/r20/r21 single-OPP DTs remain
accepted with their original voltage/frequency checks.

Install the App update and reboot the host to replace the loaded module. Updating
the App alone does not replace U-Boot or change the DT. Keep the official Frigate
App, its configuration and Connectivity options. Model, runtime, HTTP, video
decoding and permissions are unchanged. r22 operation still needs a board test.
Blank or unreadable OTP stops NPU activation; no voltage is guessed. Details and
source references are in `npu-source/VENDOR-POLICY.md`.

## Connectivity options

Keep the existing Wi-Fi/BT options. Set `npu_enabled: true` and
`inference_enabled: true`. Set `inference_api_key` to a randomly generated
32-128 character value using letters, digits, `_` or `-`. Use the same key in
Frigate. Save, restart the App, and enable **Start on boot**.

Expected logs:

```text
K11C_NPU_READY ...
K11C_API_DEVICES card=/dev/dri/card... render=/dev/dri/renderD...
K11C_API_READY backend=rknn url=http://...:8099/v1/vision/detection
```

Use the exact logged URL: it is the App bridge address, not the board's LAN IP.
The key is not logged but is stored as an App option and included in App backups.
Do not expose this HTTP endpoint to an untrusted network. A routed client may
still reach the bridge address; the API key is required regardless.

The first build downloads the pinned Frigate image to extract the tested
Python dependencies. Only that private runtime is included in Connectivity;
Frigate itself is not started or installed. Build/cache space may require
several GB. No model conversion or SDK compilation runs on the board.

## Official Frigate App

Keep the ordinary official Frigate App and its protection settings. Use its
built-in `deepstack` detector, not `rknn`. It needs neither NPU device access
nor an RKNN library for this HTTP configuration.

Merge these blocks into the existing Frigate YAML. Replace the example URL
with the URL logged by Connectivity, and replace the key. Keep existing camera
URLs and settings. Do not duplicate top-level `detectors` or `model` keys.

```yaml
detectors:
  k11c:
    type: deepstack
    api_url: http://172.30.32.1:8099/v1/vision/detection
    api_key: REPLACE_WITH_YOUR_CONNECTIVITY_API_KEY
    api_timeout: 5.0

model:
  width: 416
  height: 416
  input_tensor: nhwc
  input_pixel_format: rgb
  labelmap_path: /labelmap/coco-80.txt
  labelmap:
    5: bus
    7: truck
```

The overrides preserve the COCO names returned by this API; the stock label
file otherwise renames bus/truck to car. The stock DeepStack detector still
treats truck as car. Start with one camera and `detect.fps: 2`, keeping the
current video input/decoding settings. Video hardware decoding is separate
and has not been added. Increase the detection rate after measuring it.

## Verify

Version 0.4.1 adds `timings_ms.inference_stages` to successful detection
responses. It separates input preparation, `rknn_inputs_set`, `rknn_run`,
`rknn_outputs_get`, output copying and release. Use the r3 baseline collector
to read it; Frigate configuration and the API key do not change. The call
durations include SDK work/waiting and are not pure NPU hardware timings.

`K11C_API_READY` confirms device opening, RKNN initialization and socket binding,
not a completed camera inference. Check Frigate detector statistics/live
detections, the API `/health` completed counter, and increasing NPU interrupts.
Then check App restart and cold boot. The RGB/model accuracy question is
unchanged from the diagnostic experiment; it has not been corrected here.

To stop inference, disable `inference_enabled` and restart Connectivity.
An API error leaves Wi-Fi/BT running; it does not restart the App or reload
modules. The shared NPU module is not unloaded. To remove it, disable NPU and
reboot. HAOS kernel updates need matching modules; unsupported kernels are
refused. Core updates do not replace the host kernel.

The new `video: true` permission uses Supervisor's standard DRM/video policy
for late-created devices, and also permits other video/DRM devices. No
SYS_ADMIN, Docker API, full access or disabled AppArmor was added.

## 한국어

0.4.3은 r19 200MHz, r20 400MHz와 시험용 r21 600MHz DT를 함께 인식한다.
앱 업데이트만으로 클럭이 바뀌지는 않는다. r19/r20으로 되돌려도 같은 앱을 쓴다.

r21은 제조사 초기 클럭 600MHz와 해당 OPP 상한인 1.00V 고정 설정을 사용한다.
순정이 이 보드에서 실제 선택한 전압과 같다는 뜻은 아니다. 현재 모듈에는
OTP별 전압 선택과 BSP 온도 보정이 없으며 자동 주파수 조절도 추가하지 않았다.
기존 0.90V보다 소비전력과 발열이 늘 수 있다. 600MHz 동작과 온도는 아직
보드에서 확인하지 않았다. 앞선 400MHz 단기 카메라 추론 비교는 통과했다.
모델·런타임·HTTP·Frigate·권한·영상 디코딩은 변경하지 않는다.

0.4.1은 추론 시간을 세분화해서 측정하는 버전이다. 모델·입출력 처리·클럭은
변경하지 않는다. 기존 API 키와 Frigate 설정을 유지하고 r3 수집기로 측정한다.
SDK 호출 시간에는 내부 처리나 대기가 포함되므로 순수 NPU 연산 시간과는 다르다.

기존 0.3.0에서 NPU가 정상 동작했다면 ROM을 다시 구울 필요는 없다.
기존 Wi-Fi/BT 설정은 유지하고 아래 옵션만 설정한다.

- `npu_enabled: true`
- `inference_enabled: true`
- `inference_api_key`: 무작위 영문·숫자·`_`·`-`로 만든 32~128자 키

저장 후 App을 재시작하고 **부팅 시 시작**을 켠다. `K11C_API_READY` 뒤 URL과
같은 키를 위 Frigate 설정에 넣는다. URL은 LAN IP가 아니라 App 내부 브리지
주소다. 키는 App 백업에도 포함되므로 백업 공유에 주의한다.

Frigate는 공식 App 그대로 사용하며 보호 모드는 바꾸지 않는다. `rknn` 대신
`deepstack` 탐지기를 설정한다. 기존 카메라 주소와 영상 설정은 유지하고 카메라
한 대, 탐지 2fps부터 확인한다. 영상 하드웨어 디코딩은 별개다.

처음 빌드할 때 Python 실행 환경을 추출하려고 큰 Frigate 이미지를 받는다.
Frigate를 Connectivity 안에서 실행하는 것은 아니다. 빌드 캐시 공간은 수 GB가
필요할 수 있다. 모델 변환이나 SDK 빌드는 하지 않는다.

`K11C_API_READY`는 API 준비 완료이며 카메라 탐지 성공과는 다르다. 실제 탐지,
NPU 인터럽트 증가, App 재시작, 콜드부팅은 보드에서 확인한다. RGB 처리와 모델은
이전 실험 그대로이며 색상·정확도 문제를 수정한 버전은 아니다.

API에 오류가 나도 Wi-Fi/BT를 재시작하거나 NPU를 반복 로딩하지 않는다. API만
끄려면 `inference_enabled`를 끄고 App을 재시작한다. 공유 드라이버는 내리지 않는다.
HAOS 커널이 바뀌면 해당 커널에 맞는 모듈이 필요하다.

추가된 `video` 권한은 부팅 뒤 생기는 NPU 장치 접근을 위한 것이며 다른 DRM·영상
장치 접근도 허용한다. Frigate 권한이나 AppArmor는 변경하지 않는다.
