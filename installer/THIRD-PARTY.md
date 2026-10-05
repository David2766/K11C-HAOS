# Third-party components

- Rockchip Windows upgrade_tool 2.46: bundled original executable and config,
  mirrored at https://github.com/hiifong/rk-upgrade-tool/tree/8fea0c85ac733ffabdb4e5de629c87dc6ed4acc7/windows_x86-64.
  Executable SHA256: 789c509dde39206d27b2f8915a5a169ff5eafb5b21fdb2108d0709bdda9ebde0.
  Config SHA256: f08ed32575209de48aca01cf3e88337bc72dd8ed2191921f80c62390400c4c08.
  This vendor CLI is a binary, not an open-source flashing implementation.
  Public availability does not grant commercial redistribution rights; confirm
  those terms before customer release. No unrelated third-party GUI is bundled.

- rockusb / rockchiprs: Collabora and contributors, MIT OR Apache-2.0.
  Windows driver transport from https://github.com/hiifong/rockchiprs,
  pinned commit 31d8bb996d841e270d4dd55887f60cf9ffe27103.
  Only the licensed library is used, not the unrelated rkdevtool GUI.
- Tauri: MIT / Apache-2.0; Vue: MIT; Vite: MIT; Lucide: ISC.
  Exact transitive dependencies are locked in Cargo.lock and package-lock.json.
- Icon build tooling: resvg-js 2.6.2 (MPL-2.0), used only to render the original
  K11C vector artwork. The renderer is not included in the portable application.
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
  The official OS image is downloaded on request. Windows-native preparation
  adds published Connectivity content to a copy of the stock data partition.
- Backup compression: zstd 0.13.3 (MIT), zstd-safe 7.3.0 and zstd-sys
  2.1.0+zstd.1.5.7 (BSD-3-Clause), with Zstandard 1.5.7 (BSD-3-Clause).
  The native library is linked into the helper; no separate codec installation
  is needed. Original licenses are included in notices/.
- K11C RAM loader v1.23.114: Rockchip rkbin components, previously used by this
  board project. SHA256 b0228bbe9d3be4ea13f399df58289a82b5dfe62d1f67d75512a9ddd2d9b25454.
  The original binary is bundled unchanged. Two instructions in the RAM-only
  USB plug upload buffer are modified for full read access and its capability
  report; DDR and persistent boot firmware are not changed by preparation.
  Exact substitutions and digests: docs/ram-loader-read-access.md.
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

## Factory data

The downloaded installation catalog records the exact HAOS, Supervisor, Docker
and Connectivity releases. Factory data is generated during installation, not
bundled in the portable ZIP or required to compile the installer. Their code,
binaries and licenses are not relicensed by the installer. Source and build references:

- HAOS: https://github.com/home-assistant/operating-system (matching release tag).
- Supervisor: https://github.com/home-assistant/supervisor (version in catalog).
- Connectivity: https://github.com/David2766/K11C-HAOS (published RELEASE.json,
  module build sources and bundled third-party notices).

Keep the seed build manifest, corresponding source and original license notices
with binary releases. Vendor firmware retains its original redistribution terms.

## Native preparation tools

The Go helper uses containerd 2.3.4 (Apache-2.0), bbolt (MIT), go-git (Apache-2.0)
and the dependencies pinned by native/go.sum. It is a Windows executable, not
a Linux daemon or ARM emulator. Original notices are collected by the build.
Windows e2fsprogs 1.47.4 utilities use Cygwin's native POSIX API DLLs. Cygwin
is not a VM; its open-source exception and original licenses apply. Exact
binary hashes are in resources/native/tools.json. Source versions, build
instructions and the one-line e2fsprogs portability patch are in native/README.md.
The original e2fsprogs source archive and portability patch are included.
Runtime-library source locations and exact archive hashes are listed in
`notices/native/SOURCES.json`; original license notices are in the same folder.
