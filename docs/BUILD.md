# K11C build — U-Boot r24 / Installer 0.4.0

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

Install Node.js, Rust MSVC and Visual C++ Build Tools. Extract the existing
0.4.0 portable ZIP outside the repository and point to its resources directory
(with loader, firmware, rockusb and firmware/manifest.txt).

```powershell
cd installer
$env:K11C_RELEASE_INPUTS = 'C:\K11C-inputs\resources'
npm ci
node scripts/build.mjs --check-inputs
npm run build
npm run package
```

The actual runtime constants authenticate build inputs; no private developer
paths or SDK tree are used. Packaging verification requires a Windows PC with
Rockusb already installed; it does not install a driver for testing.
Physical USB writes and interrupted-install recovery require board acceptance.

## Repair utilities

```bash
python3 tools/boot-fix/test_boot_fix.py
python3 tools/boot-fix/package.py --output ../build/k11c-boot-fix-r2.tar
```

Tests use disposable file-backed disks. Follow
[the repair guide](../tools/boot-fix/README.md) before any board-side writes.
