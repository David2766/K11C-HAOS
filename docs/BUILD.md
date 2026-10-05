# K11C build — U-Boot r24 / Installer 0.5.0

Use the official generic-aarch64 HAOS image. No HAOS or kernel ROM rebuild is
required. Connectivity external modules are built by the separate
[Connectivity CI](CONNECTIVITY-CI.md). [한국어](BUILD.ko.md)

## U-Boot r24

The exported device trees are the complete inputs used for r24, including
the vendor NPU overlay and stable phandles. They do not require the original
diagnostic directories or temporary build tree.

On Ubuntu 24.04:

```bash
sudo apt-get install build-essential gcc-aarch64-linux-gnu bison flex swig python3-dev python3-setuptools python3-pyelftools python3-jsonschema device-tree-compiler libssl-dev libgnutls28-dev
git clone https://github.com/u-boot/u-boot.git ../u-boot
git -C ../u-boot checkout 88dc2788777babfd6322fa655df549a019aa1e69
python3 source/scripts/build-release-uboot.py --check
python3 source/scripts/build-release-uboot.py \
  --uboot-tree ../u-boot \
  --ddr-blob ../inputs/rk3566_ddr_1056MHz_v1.23.bin \
  --bl31 ../inputs/rk3568_bl31_v1.44.elf \
  --output ../build/u-boot-k11c-dfi-r24.bin
```

Supply the original Rockchip DDR/BL31 inputs with the digests in
`source/boot-release.json`. The wrapper fixes release metadata and requires
the rebuilt firmware and DTB to match the published hashes. Existing output
files are not overwritten. The original upstream checkout must be clean.
Original licensing remains applicable; see the repository notices.

## Portable Windows installer

Install Node.js, Go 1.26.3 or later, Rust MSVC and Visual C++ Build Tools on
Windows x64. Download the repository source ZIP with **Code → Download ZIP**,
or clone it. `installer/resources` contains the USB loader, signed driver,
manufacturer tool, current U-Boot and native filesystem utilities with their
DLLs. The Go preparation helper is compiled from `installer/native` during
the build. No prior portable release, SDK folder, HAOS image or Connectivity
image is needed.

```powershell
cd installer
npm ci
node scripts/build.mjs --check-inputs
npm run build
npm run package
```

Output: `installer/release/K11C-Installer-0.5.0-windows-x64.zip` and its `.sha256`.
The actual runtime constants authenticate build inputs; no private developer
paths or SDK tree are used. `K11C_GO` can select a Go executable; otherwise the
normal source-ZIP build uses `go` from PATH. Go is a build-time dependency only.
See [native preparation](../installer/native/README.md) for the helper and
filesystem utility sources.

When the installer runs, it reads the repository's compatibility catalog,
downloads official HAOS and the pinned Connectivity image from GHCR, and
prepares the initial data partition on Windows. The portable requires Windows
x64, WebView2 and network access for installation downloads, not Docker, WSL
or an emulator. Rockusb is bundled and installed with administrator approval
when missing. Packaging tests do not install the driver.
See [release setup](RELEASE.md#installer-구성요소-배포) for catalog publication.

## Repair utilities

```bash
python3 tools/boot-fix/test_boot_fix.py
python3 tools/boot-fix/package.py --output ../build/k11c-boot-fix-r2.tar
```

Tests use disposable file-backed disks. Follow
[the repair guide](../tools/boot-fix/README.md) before any board-side writes.
