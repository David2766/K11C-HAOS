# Third-party components

- rockusb / rockchiprs: Collabora and contributors, MIT OR Apache-2.0.
  Windows driver transport from https://github.com/hiifong/rockchiprs,
  pinned commit 31d8bb996d841e270d4dd55887f60cf9ffe27103.
  Only the licensed library is used, not the unrelated rkdevtool GUI.
- Tauri: MIT / Apache-2.0; Vue: MIT; Vite: MIT; Lucide: ISC.
  Exact transitive dependencies are locked in Cargo.lock and package-lock.json.
- Rockchip Rockusb 5.13 (2023-11-09), original signed x64 Windows package from
  the local DriverAssitant_v5.13 distribution. INF/CAT/SYS are copied unchanged.
  Catalog signature verified locally as Microsoft Windows Hardware Compatibility
  Publisher. Signature validity is not a grant of commercial redistribution rights.
  Public commercial redistribution terms must be confirmed before customer release.
- Microsoft WebView2 Runtime: Microsoft license, not relicensed by this project.
- HAOS image preparation: reqwest (MIT / Apache-2.0), xz2 and lzma-sys
  (MIT / Apache-2.0), fatfs (MIT), rfd (MIT). Bundled liblzma source is the
  crate's XZ 5.2; its COPYING explains the public-domain library and separately
  licensed build tools. Relevant original notices are included in notices/.
  HAOS images are downloaded on request from the official repository, not bundled.
- K11C RAM loader v1.23.114: Rockchip rkbin components, previously used by this
  board project. SHA256 b0228bbe9d3be4ea13f399df58289a82b5dfe62d1f67d75512a9ddd2d9b25454.
  The original binary is bundled unchanged; only its RAM entries are uploaded.
  Original license: notices/rockchip-rkbin-LICENSE.txt.

This is a local engineering build, not a public product release.

## K11C boot firmware

The r24 U-Boot image is bundled separately from official HAOS downloads.
U-Boot base commit: 88dc2788777babfd6322fa655df549a019aa1e69 (2026.04).
Binary SHA256: 8a98fbc73fa93eb106a1202cece0bcb3d05355c4f21cab5c151f5b3d900f6260.
U-Boot is GPL-2.0-or-later; the combined Rockchip image also contains rkbin DDR
and BL31 components under their original terms. See the firmware manifest and
notices/uboot-COPYING.GPLv2, notices/rockchip-rkbin-LICENSE.txt.
The exported repository contains the exact r24 inputs under source/device-tree,
source/u-boot and source/scripts, with hashes in source/boot-release.json.
See docs/BUILD.md. Publish binaries against the corresponding source commit;
upstream U-Boot and the pinned Rockchip DDR/BL31 inputs retain their own terms.
