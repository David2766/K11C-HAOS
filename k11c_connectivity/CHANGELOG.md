# Changelog

## 0.5.3

- Prepare the existing Hantro/V4L2 Request FFmpeg bundle in Frigate Full Access automatically.
- Reuse identical manual installations and files retained in `/config` after container recreation.
- Copy only missing or recognized changed files; preserve user-modified and unrelated files.
- Check FFmpeg execution in the target image before preparation; no camera or hardware probe is run.
- Keep detector/model, video settings, NPU/Wi-Fi/Bluetooth drivers and permissions unchanged.
- No host reboot is needed when updating from 0.5.2.

## 0.5.2

- Add opt-in, bounded timing summaries to the existing native detector.
- Separate SDK call wall time, upstream postprocessing and calling-thread CPU without changing model/input/results.
- Default capture is disabled; no additional App options, permissions, service or inference requests.
- Keep the 0.5.1 900 MHz module, voltage policy, Wi-Fi/Bluetooth and FFmpeg configuration unchanged.

## 0.5.1

- Use the highest enabled manufacturer NPU OPP up to 900 MHz at driver initialization.
- Keep OTP/PVTM-selected voltages, bin limits, cold compensation and 200 MHz runtime idle.
- Preserve the normal request when cold startup temporarily limits the applied frequency.
- Update module verification and startup checks for the new default.
- Keep Frigate detectors, model, video pipeline, Wi-Fi/Bluetooth, permissions and U-Boot unchanged.
- Reboot the host after updating to replace the loaded module. No forced module unloading.
- The existing-driver 600/900 MHz comparison completed on the board; startup with this rebuilt module still needs confirmation.

## 0.5.0

- Add optional direct NPU inference inside the official Frigate Full Access App.
- Reuse the existing C API binding and model; use installed Frigate RKNN postprocessing.
- Remove JPEG and HTTP transport from the native path without adding RKNNLite.
- Connectivity prepares additional files after container recreation; official Frigate files and AppArmor are unchanged.
- Add Docker API permission. Native preparation is off by default and requires protection disabled.
- Keep the existing HTTP API, wireless drivers, NPU driver, clocks and voltage policy unchanged.
- Requires a Frigate detector configuration change. Board inference and performance are awaiting this build's test.

## 0.4.8

- Reuse two preallocated RKNN float-output buffer sets instead of copying SDK-owned tensors for every request.
- Keep each buffer set leased until postprocessing finishes; retain two inference workers and one locked RKNN context.
- Report buffer waiting and output validation separately in inference timings.
- Model, preprocessing, postprocessing calculations, drivers, clocks, permissions and Frigate are unchanged.
- Update this App only; no host reboot or U-Boot flash is required.
- Real-board compatibility and performance of preallocated outputs have not yet been measured.

## 0.4.7

- Overlap preparation and postprocessing for two inference requests in one process.
- Keep one RKNN context; serialize input submission through output copy and release.
- Bound concurrent requests and retain a separate connection slot for health checks.
- Keep timing state separate per worker; report RKNN queue wait and worker-thread CPU time.
- Drain workers before releasing the model context on App shutdown.
- Model, image processing, thresholds, modules, firmware, clocks and permissions are unchanged.
- Update and restart this App only. No U-Boot flash or host reboot is required.
- Real-board throughput and CPU changes have not yet been measured.

## 0.4.6

- Reduced multipart parsing work for ordinary Frigate JPEG uploads.
- Preserved the original parser for legacy MIME and ambiguous boundary cases.
- Added separate request-body, upload-parsing and result-formatting timings.
- Model, image processing, thresholds, drivers, clock policy and permissions are unchanged.
- Update and restart this App only. No U-Boot flash or host reboot is required.

## 0.4.5

- Fixed r22 startup rejecting the `name` properties Linux adds to live device trees.
- Accepts only the exact node-derived names; unexpected properties, OPPs and
  changed voltage/frequency/OTP references are still rejected.
- U-Boot, kernel modules, firmware, inference processing and permissions are unchanged.
- Restart the App after updating. The r22 U-Boot does not need to be flashed again.

## 0.4.4

- Added the r22 manufacturer NPU OPP table and OTP/PVTM voltage selection.
- Ported OTP voltage offsets, MBIST minimum voltage and cold-temperature correction.
- Restored the manufacturer's 600 MHz initial request and 200 MHz runtime-idle clock.
- Added a read-only RK3568 OTP provider to the same App; no OTP programming path.
- Preserved previous single-OPP DT support, Wi-Fi/BT, inference processing and permissions.
- Blank/unreadable OTP is rejected; the BSP live-PVTM fallback is not included.
- New modules require one host reboot. Board operation has not yet been checked.

## 0.4.3

- Added the r21 fixed 600 MHz / 1.00 V NPU profile; r19/r20 remain supported.
- Uses the vendor initial clock with its 600 MHz OPP voltage ceiling, without
  assuming a chip-specific OTP voltage. This is not full vendor DVFS.
- Requires matching DT frequency/voltage and actual clock/regulator readback.
- No driver, model/runtime, HTTP, Wi-Fi/BT, Frigate or permission changes.
- 600 MHz operation, power consumption and temperature have not yet been checked
  on the board. The earlier 400 MHz short comparison passed.

## 0.4.2

- Added recognition of the r20 400 MHz / 0.90 V single-OPP test DT.
- Retained r19 200 MHz support for installation before flashing and rollback.
- Checks DT frequency, assigned clock and actual clock/voltage as one profile.
- No change to drivers, model/runtime, image processing, Wi-Fi/BT or permissions.
- 400 MHz performance and stability have not yet been checked on the board.

## 0.4.1

- Added per-request timing for input preparation, RKNN input/run/output calls,
  output copying and buffer release.
- Existing API fields and detection results are unchanged. This is an
  instrumentation update, not a performance optimization.
- Model/runtime, NPU clocks, Wi-Fi/BT, service startup and permissions are unchanged.

## 0.4.0

- Added an opt-in HTTP detection API inside Connectivity using the tested
  RKNN 2.3.2 / YOLOX Nano model and existing NPU driver.
- Uses the stock Frigate DeepStack detector; no Frigate or AppArmor changes.
- Standard video/DRM access covers NPU nodes created after App startup.
- Requires an API key and listens on the HA App bridge address only.
- Model, RGB processing, modules and Wi-Fi/BT entrypoint are unchanged.
- API errors do not restart wireless services or repeatedly reload the NPU.
- Real-board App lifecycle and camera inference are not yet checked.

## 0.3.0

- Added optional NPU loading to K11C Connectivity (`npu_enabled`, off by default).
- Uses the r19b driver tested on HAOS kernel 6.18.39-haos with the r19 NPU DT.
- Checks the loaded driver, regulator setting, clock and NPU render device.
- NPU activation is independent of Wi-Fi/BT; no automatic module reloads.
- Existing Wi-Fi/BT options and startup sequence are unchanged.
