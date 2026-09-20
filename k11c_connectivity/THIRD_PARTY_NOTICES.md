# K11C Connectivity third-party notices

This App is a mixed-license bundle.

- The separately packaged FFmpeg V4L2 Request runtime is the unchanged K11C r1
  build of Kwiboo/FFmpeg commit `fad85d9c76611c09b561167ab405c667a0dcdeb7`.
  It was configured with GPL and version 3 enabled. The original license texts,
  build configuration and Debian library notices are in `ffmpeg-assets/notices/`;
  source provenance is in `ffmpeg-assets/BUILD.json`. These components retain
  their own licenses, not the App's AGPL. Corresponding FFmpeg source:
  https://github.com/Kwiboo/FFmpeg/tree/fad85d9c76611c09b561167ab405c667a0dcdeb7.
  This local package does not replace the corresponding-source requirements
  described in the project's existing FFmpeg build/distribution workflow.

- The optional native Frigate adapter and external preparation controller are
  original project code under `AGPL-3.0-or-later`. They call the user's installed
  Frigate RKNN postprocessor; no Frigate source is copied into this App. The
  reused Rockchip runtime/model retain the notices already supplied under
  `inference-assets/notices/`. No RKNNLite wheel or model-conversion SDK is added.

- `python-multipart` 0.0.32 retains its Apache-2.0 license. The exact PyPI wheel
  is in `parser-assets/`; upstream source is https://github.com/Kludex/python-multipart.
  Its full license and package metadata are retained under the private runtime's
  `lib/python3.11/site-packages/python_multipart-0.0.32.dist-info/` directory.
  The legacy `multipart` import alias is not installed. The parser is unmodified.

- Original App control code and packaging are `AGPL-3.0-or-later` unless a
  file says otherwise.
- The SeekWave VS/SWT6621S kernel modules retain the licenses of their source
  files. Observed source notices include `GPL-2.0-only`,
  `GPL-2.0-or-later`, and `MIT`.
- The SeekWave firmware and NVRAM binaries are vendor material. No
  redistribution license for those binaries was identified in the obtained
  package. They are not licensed under this project's AGPL.
- Linux, Home Assistant OS, the Home Assistant base image, and other included
  components retain their respective upstream licenses.
- The optional `rknpu.ko` is derived from Rockchip's GPL-2.0 driver and
  w568w/rknpu-module, commit `a8792fe6b633f90cf2c6808267cac327537ab4cf`.
  Its corresponding modified source, original notices, GPL-2.0 text and build
  instructions are in `npu-source/` in this local App package. Driver changes
  remain under the upstream license, not the App control code's AGPL.
- r22 also contains a read-only RK3568 OTP provider and adapted NPU voltage/
  temperature policy from the manufacturer's Rockchip Linux 5.10.160 SDK.
  These retain their GPL-2.0 notices. Selected original sources and provenance
  are included in `npu-source/vendor-reference/`; adaptations are documented in
  `npu-source/VENDOR-POLICY.md`. They are not relicensed under AGPL.
- The local 0.4.0 package includes Rockchip RKNN runtime 2.3.2 and the previously
  tested converted YOLOX Nano RK3566 model. Provenance and Rockchip terms are
  in `inference-assets/notices/`. These are not relicensed under the App's AGPL.
  Model redistribution terms must be checked before publishing a binary bundle.
- The private Python environment is extracted from the pinned official Frigate
  0.17.2 image. Python, glibc, NumPy, OpenCV, Pillow and their dependencies retain
  their licenses. No Frigate application code is included. Notices are retained
  with the packages and in `/opt/k11c/npu-runtime/notices/` in the built App.
  Exact Debian package/source versions are listed there. Corresponding sources:
  https://snapshot.debian.org/. Binary distributors must meet applicable source
  obligations as well as preserving these notices.

The public export was not changed by this local integration. This local test
ZIP contains user-supplied binaries and is not a public-release package. Do not
publish it as a replacement for the source-only export.

This project is independent and is not an official KickPi, SeekWave,
Rockchip, Home Assistant, or Nabu Casa release.
