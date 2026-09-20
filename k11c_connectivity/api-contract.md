# Connectivity inference interfaces (0.5.3)

## FFmpeg preparation (0.5.3)

The existing opt-in native preparation also supplies the board-tested r1
V4L2 Request FFmpeg at `/config/k11c-ffmpeg-vpu-r1`. There is no new App option,
service, privilege, detector change, driver change or automatic YAML editing.
The packaged binaries, libraries, wrappers and upstream notices are unchanged
from the manual r1 installation. No test videos or probe scripts are installed.

Preparation compares actual file bytes, not just directory existence or an
installation marker. Identical files are reused, including manual installs and
the /config volume retained after container recreation. Missing files and
recognized previous-owned versions are copied individually. Missing execution
bits on identical FFmpeg programs are repaired. Different unowned/user-modified
files, symlink destinations and incompatible target-image preflight stop
preparation without overwriting those files. Unrelated /config files are untouched.
When all payload files already match, preparation does not restart Frigate;
it may only register its own manifest. The existing per-container/payload state
avoids repeated inspection/copy loops and repeated failed installation attempts.

The isolated target-image preflight additionally executes ffmpeg/ffprobe
`-version` and verifies that `-hwaccels` lists v4l2request, without opening video
devices or camera streams. After installation, bytes and execution bits are
verified before restarting the same target container once. `K11C_FFMPEG_READY`
reports the reused/copied FFmpeg file counts, not a hardware decoding result.
Existing user camera settings must select this FFmpeg path and hardware backend.
Updating from 0.5.2 requires no host reboot and does not change the HTTP API.

## Optional native timing capture (0.5.2)

The additional `k11c_rknn` detector accepts two diagnostic fields in Frigate's
own YAML: `timing_samples` (integer 0..1024, default 0) and
`timing_delay_seconds` (integer 0..600, default 60). App options/permissions and
the HTTP API are unchanged. Zero disables capture without clock polling. For
a positive count, the delay starts after that detector's RKNN initialization;
the next N successful real detector calls are measured. No synthetic request,
extra model context, thread, server, image capture or per-frame log is added.
Each detector emits one ARMED line and one SUMMARY after its own N samples,
then stops capture. Restarting Frigate rearms it if the YAML fields remain set.

The model/input bytes, SDK flags, output-buffer lease and installed Frigate's
YOLOX postprocessing are unchanged. Disabled capture uses the original path.
Enabled capture calls those same operations once, and returns the same result
object. SDK/postprocess errors are re-raised and no successful/stale report is
published. Invalid diagnostic data disables capture, not successful detections.

SUMMARY reports pid, process, count, delay, actual sample-window duration and
mean/p50/p95/min/max milliseconds. Wall parts are `prepare` (contiguous input),
`buffer_wait`, `queue_wait` (per-context Python lock only), `inputs_set`, `run`,
`outputs_get`, `outputs_validate`, `outputs_release`, `native_other`,
`postprocess` (unmodified upstream YOLOX, including NMS), and `lease_return`.
Their means sum to `total`; independent p50/p95 values do not add. The small
Python/ctypes/timer bookkeeping is included, not silently labelled hardware.
`native_thread_cpu`, `postprocess_thread_cpu`, and `total_thread_cpu` measure
only the calling thread, not SDK/NumPy helper threads or the whole container.
Record-accumulation thread CPU is reported separately. Summary construction and
logging are excluded from the per-request total but still cost CPU once.

`run`/`outputs_get` are SDK wall spans, not pure hardware execution: scheduling,
waiting, transfer or conversion can occur inside the SDK. In particular,
`queue_wait` does not measure competition between the two RKNN contexts on the
physical NPU. Frigate's outer wrapper/EMA, decoding, ROI creation and its pending
detection queue are not measured by this helper. The collector reads only
summaries for currently running detector PIDs and validates counts/stage sums.
Current dashboard/temperature/frequency snapshots taken by collection are not
continuous observations of the earlier sampling window.

At most 1024 small numeric rows are held per opted-in process and cleared on
completion/failure. No camera identifiers, pixels, predictions, URL or key are
included in the measurement report. Standard Docker/App logs retain the two
diagnostic messages; no extra media files or persistent diagnostic service.

## Kernel bundle selection (CI preparation, unreleased)

The NPU loader, HTTP device preflight and vendor-policy validator now select
`/opt/k11c/modules/<uname -r>/npu/bundle.json`. The schema contains the exact
kernel release and both NPU/OTP SHA-256 and srcversion values. Missing bundles,
kernel mismatches, corrupt modules and different loaded builds still stop
activation. No version override, fallback to another kernel, module hot-reload,
network download or new HTTP endpoint is introduced. Existing 6.18.39 binaries
are unchanged; CI candidate binaries do not imply board validation.

## NPU default profile (0.5.1)

The external module now selects the highest enabled vendor OPP at or below
900 MHz after OTP/bin preparation. The normal governor request is retained
separately from an initially cold-limited frequency, so removing the existing
cold QoS limit can restore that request. Idle still requests 200 MHz and resume
restores the saved applied frequency. Voltage tables, selection, adjustment,
readback and thermal policy are unchanged; no voltage is hard-coded to 900 mV.
The r22 U-Boot/DT and read-only OTP module are unchanged. Legacy single-OPP
initialization is unchanged. A new RKNPU binary requires a host reboot after
App update; old loaded modules are rejected, never force-unloaded. Startup
validation now expects the selected 900 MHz-or-lower normal profile (or its
existing cold limit). No App options, permissions or inference API change.

## Optional direct Frigate detector

`frigate_native_enabled` defaults to false. When enabled it requires
`npu_enabled=true`, `inference_enabled=false`, Docker API access and protection
disabled on Connectivity. `frigate_native_app` selects the official Full Access
App (default `ccab4aaf_frigate-fa`). Docker socket access is host-root-equivalent;
the target scope is an application rule, not a security boundary.

Connectivity supplies additional files at `/opt/k11c-native/`,
`/opt/frigate/frigate/detectors/plugins/k11c_rknn.py`, and
`/usr/lib/librknnrt.so`. Existing non-owned files are not overwritten. It does not
change the official image, original Frigate source, App options/YAML, devices,
network configuration or AppArmor policy. On a new container/image it first
checks the native registration/schema/library using that same local image in a
temporary container without network, cameras, host volumes or NPU devices.
It then stops the selected running App, copies and verifies the additional
files, and starts the same container once. A failed initial native boot can also
be prepared once. Cleanly stopped / never-started Apps are not started. This is
not a service-health watchdog and does not automatically retry failed attempts.
An interrupted/failed attempt is recorded in `/data/k11c-native-state.json`.

The `k11c_rknn` detector receives Frigate's original NHWC RGB uint8 region tensor
and uses the unchanged 0.4.8 `NativeRuntime.borrow_outputs` C API binding inside
the Frigate process. There is no JPEG encoding, multipart upload, HTTP server or
API key on this path. It delegates YOLOX postprocessing to the installed
Frigate `Rknn.post_process_yolox`. One Frigate detector means one RKNN context;
two configured detectors mean two contexts, not the API's shared-context model.
The supplied model remains YOLOX Nano INT8 / 416px with the existing hash. The
native detector does not control clocks or voltages; the App's NPU default
profile is described above. No model weights or benchmark-dependent thresholds
are changed. Native output is Frigate's `(20, 6)` float32 detection array.
Invalid model/board/assets and RKNN failures raise errors; no CPU or HTTP fallback.

RKNNLite is deliberately not added: version 2.3.2 also checks the inaccessible
`/proc/device-tree/compatible` path internally. The direct C API does not need
that check. Identity is exported from Connectivity's actual mounted device tree.
No global file-open hook, device-tree alias, SDK patch or policy bypass is used.

Official Frigate updates remain enabled. Additional detector discovery is an
internal Frigate interface, so future incompatibility may require a Connectivity
update. The compatibility probe is not a promise of future hardware correctness.
The old API can be selected manually for rollback; disabling preparation alone
does not switch or stop a running native detector. Switch Frigate configuration
first. Neither mode unloads the shared NPU module.

## Existing HTTP API (unchanged from 0.4.8)

0.4.8 keeps two inference workers and one RKNN context. The production backend
uses `NativeRuntime.borrow_outputs` as a context manager. Two float32 output
buffer sets are allocated once after model/shape/version validation (2,413,320
bytes total). `rknn_outputs_get` receives `want_float=1`, `is_prealloc=1`, and the
exact pointer/size of each application-owned array. Pointer, size and flags are
validated; SDK outputs are released before unlocking the context. The buffer
lease remains held through `decode_yolox`, so the next inference cannot overwrite
another request's CPU postprocessing input. Views must not escape that scope.
Every error path returns its lease; SDK release still runs after a successful
outputs_get, including validation failure. There is no retry/copy fallback.
Release descriptors retain application ownership even if returned metadata was
invalid; the SDK must not free the NumPy-owned arrays.

The owned-output `NativeRuntime.inference` convenience method remains for local
diagnostics and copies within a lease. Production never calls it. This is not
DMA zero-copy: RKNN float conversion/transfer remains inside outputs_get.
JPEG/RGB/resize, model, thresholds, NMS, output sorting, HTTP status semantics,
permissions, drivers and clock policy are unchanged. No new package dependency,
worker, RKNN context, pending image queue or automatic recovery is introduced.

Timing change: `buffer_wait` is time acquiring an output lease (five-second
limit, separate from the existing context-lock and SDK limits). `queue_wait`
still measures the context lock only. `outputs_validate` replaces
`outputs_copy`: it measures pointer/size/flag checks and array views, not a data
copy. Stages still sum to `inference`. Postprocessing remains outside the RKNN
lock but inside the buffer lease. HTTP responses remain compatible with stock
Frigate, which ignores these diagnostic fields. Real-board preallocated-output
compatibility and performance are not established by local simulated-SDK tests.

0.4.7 introduced up to two inference requests concurrently in the existing
process. Preparation and postprocessing can overlap the other request's native
inference. The single RKNN context is locked from inputs_set through output
copy and outputs_release; outputs are owned copies before unlocking. No async
RKNN flags, model changes, new dependencies, permissions or Frigate changes.

There are at most three HTTP connection workers, including one extra slot for
health checks while two POSTs are active, plus up to four pending raw sockets
for connection handoff (seven admitted connections total). At most two POST bodies/images are
processed concurrently. Extra POSTs or connections receive 503
`success: false, error_code: server_busy`, with no predictions or automatic
retry. Compute slots are returned before response delivery so completed work
does not cause spurious overload on a client's next request. No pending socket
is read into an image before admission to a worker. The TCP listen backlog
also remains four (neither queue holds decoded images). A slow
third connection can temporarily occupy the health slot; socket timeout is
still five seconds. RKNN lock acquisition has a five-second limit; the existing
SDK run timeout is also five seconds. These are separate limits, not an
end-to-end five-second request guarantee. HTTP disconnect does not cancel a
native call or free its buffers.

`timings_ms.inference_stages.queue_wait` measures RKNN context-lock waiting
and is included in `inference`. Stage state is worker-local and reset for each
call. `process_cpu` retains process-wide CPU elapsed during the request span;
with concurrent requests it includes other workers and MUST NOT be summed as
exclusive per-request cost. New `worker_thread_cpu` measures only the handling
thread, excluding other library threads. Use container CPU / completed count
over the same interval for throughput-normalized total CPU comparisons.

Shutdown stops admission, closes active client sockets to interrupt network
waits, drains workers and only then destroys the model context. In-flight native
work is not force-cancelled. It still depends on the SDK returning; there is no
new watchdog/retry process. A disconnected request that completes inference may
still increment `completed`, which is not a delivered-response counter.

0.4.6 adds a byte-preserving multipart fast path for ordinary Frigate uploads.
The pinned python-multipart 0.0.32 parser handles the binary body; only small
headers use Python's email parser. Encoded, legacy or ambiguous MIME uses the
original parser. Existing size, duplicate/missing field, authentication and JPEG
checks remain. This compatibility path never changes the inference backend.
No image, model, confidence/NMS, color, resize, driver, clock, Frigate or App
permission setting changes. No request, API key or image is stored or logged.

Successful responses also add `timings_ms.request_stages`: `body_read`,
`upload_parse` (including authentication), `predictions` (response box formatting),
and `other`. Their sum plus `decode`, `inference` and `postprocess` equals `total`.
The original timing spans retain their meaning. HTTP header parsing, queued
waiting before do_POST, response serialization and network delivery are NOT
included in `total`. Failure responses do not publish stale timings.

0.4.5 fixes live-DT validation: Linux-generated `name` properties are accepted
only on known OPP/OTP nodes and only with the exact NUL-terminated base name.
All other DT properties and runtime voltage/clock/OTP checks remain unchanged.
There are no HTTP, permission, module, model or U-Boot changes.

0.4.4 adds the r22 K11C vendor-policy path. The captured manufacturer OPP table
is retained, including L0-L3 voltages, hardware masks and disabled 1 GHz nodes.
The external driver reads OTP through a read-only RK3568 NVMEM provider, applies
bin/PVTM selection, OTP voltage adjustment, MBIST minimum and the BSP low-temperature
voltage policy. Initial request is 600 MHz; runtime suspend requests 200 MHz and
resume restores the saved request. The manufacturer's rknpu_ondemand governor
also forwards a stored request: no utilization-driven governor is introduced.

Both loader and inference startup validate the vendor DT and a locked kernel
status snapshot, allowing its active/idle clocks and selected voltage range.
Older single-OPP diagnostic DTs retain their existing checks. New modules require
a host reboot if the previous module is already loaded; no forced unloading.
HTTP, model/runtime, video processing, wireless startup and permissions do not
change. Updating the App alone does not install r22 U-Boot.

The port uses the manufacturer's 5.10.160 SDK source and the board's captured
5.10.157 DT. It is not a binary-identical BSP. Mainline lacks the BSP live-PVTM
cache: blank PVTM or unreadable OTP stops activation instead of guessing a
voltage. See `npu-source/VENDOR-POLICY.md` for the remaining differences.
Local software checks do not establish r22 board operation.

0.4.3 adds the r21 single 600 MHz / 1.00 V profile. Its exact OPP tuple is
1000000/1000000/1000000 uV; actual voltage must be 1000000 uV and the clock
580-600 MHz. The existing r19/r20 profiles and their checks remain unchanged.
This uses the vendor initial frequency with a fixed voltage at the vendor
600 MHz OPP ceiling, not the board's unknown OTP-selected voltage. It does not
restore BSP binning, temperature compensation or automatic frequency scaling.
No HTTP, model, image-processing, permission or lifecycle changes are made.
600 MHz hardware operation and thermals still require board verification.

0.4.2 leaves the HTTP contract and image/timing pipeline unchanged. Startup
accepts either the r19 single 200 MHz OPP or the r20 single 400 MHz test OPP.
The DT OPP name, frequency and assigned clock must agree. Both retain the
900000/900000/1000000 uV OPP tuple. The 400 MHz comparison additionally requires
actual regulator readback of exactly 900000 uV and 380-400 MHz clock readback;
the existing 200 MHz profile retains its 180-210 MHz / 900000-1000000 uV checks.
Unknown profiles, extra OPPs and profile/readback mismatches are refused.
This adds no clock-writing API or App option. A short board comparison confirmed
396 MHz at 900000 uV and faster inference; it is not a long-term stability test.

The HTTP envelope and image pipeline retain the board-tested
`tools/frigate-api-stage1/api-contract.md` behavior. Version 0.4.1 adds timing
instrumentation to `service.py` and `native.py`; they are no longer byte-identical
to that diagnostic implementation. The production entrypoint is
`/opt/k11c/inference/app.py`, not the diagnostic `service.py` main function.

- POST `/v1/vision/detection`: multipart fields `api_key` and `image` exactly
  once each. JPEG, <=2 MiB request, <=4096x4096 image, Content-Length required.
- Success: 200, `success: true`, `backend: rknn`, `predictions`, `timings_ms`.
  COCO-80 labels, confidence >=0.4, sorted descending, at most 20 boxes in
  submitted-image pixel coordinates. Valid empty results remain successful.
- Wrong key: 401; malformed request: 400; oversized: 413; unsupported
  encoding/media: 415; unknown endpoint: 404. Backend failure: 503,
  `success: false`, `error_code`, no predictions; no CPU fallback.
- GET `/health`: initialization readiness, backend, completed inference count.
  This is not a per-request accuracy or device-health guarantee.

## Timing breakdown (introduced in 0.4.1, current in 0.4.8)

Successful production responses add `timings_ms.inference_stages`, with
non-overlapping wall-clock durations in milliseconds:

- `prepare`: resize to 416x416 and contiguous NHWC uint8 preparation.
- `buffer_wait` (0.4.8): wait for one of the two reusable output sets.
- `queue_wait` (0.4.7): wait for exclusive ownership of the shared RKNN context.
- `inputs_set`: the checked `rknn_inputs_set` call.
- `run`: the checked `rknn_run` call, with the original flags/timeout unchanged.
- `outputs_get`: the checked `rknn_outputs_get` call, still `want_float=1`.
- `outputs_validate` (0.4.8): validate and reshape/transpose output views;
  replaces the former `outputs_copy` stage without a full tensor copy.
- `outputs_release`: the checked `rknn_outputs_release` call.
- `other`: the remaining time within the existing `timings_ms.inference` span.

These parts sum to the existing `inference` value. All five original timing
fields retain their meanings. SDK call timings are not pure hardware execution
times: execution waiting/conversion may occur inside SDK calls. No profiling
flags or extra RKNN queries are enabled. HTTP serialization of the extra fields
adds a small measurement overhead. Timing data is worker-local in the bounded
concurrent server; failed requests do not publish a previous measurement.
No per-frame log or image file is added. Stock Frigate ignores the extra fields.

## App transport and lifecycle

Opt-in: `npu_enabled` and `inference_enabled` must both be true. Both default
false. `inference_api_key` must contain 32-128 ASCII letters/digits/`_`/`-`;
choose a randomly generated key and set the same key in Frigate. Never logged.
The key is stored by Supervisor as an App option and is present in App backups.

Listen on the IPv4 address of host interface `hassio`, TCP 8099, never all
interfaces or the LAN address. There is no Docker port mapping. Routed access
to this address is still possible; authentication remains mandatory. HTTP is
intended for Apps on the same host, not an untrusted network or remote use.

Wait at most 60 seconds for the existing NPU loader. Validate current kernel,
module, voltage/frequency and BOTH NPU DRM nodes, then initialize the pinned
RKNN model before listening. Device numbers are discovered from sysfs.
Binding/model/device failure leaves API stopped; no automatic module reload,
App restart, CPU fallback or changes to Wi-Fi/BT. Restart the App after fixing
its options. API termination releases its own model context, not the module.

`video: true` uses Supervisor's standard DRM device policy, including late
created DRM nodes. This also grants other video/DRM device access. No additional
SYS_ADMIN, Docker API, privileged mode, or disabled AppArmor is requested.
Frigate's image, detector code and permissions are not changed.

This local package retains the RGB pipeline whose color/accuracy question was
deferred. App lifecycle and camera-level detection require board validation.
