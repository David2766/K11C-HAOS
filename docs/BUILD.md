# Build K11C HAOS

The repository does not contain HAOS images, Rockchip binary blobs, SeekWave
firmware, or prebuilt SeekWave kernel modules. Obtain those files from their
rights holders and check their license terms.

Run the build scripts in Linux or WSL2. They do not overwrite existing output
files.

## Build a ready-to-flash image

Required inputs:

- an official `haos_generic-aarch64-<version>.img.xz` and its SHA-256 value
- a clean upstream U-Boot checkout at the pinned commit
- `rk3566_ddr_1056MHz_v1.23.bin`
- `rk3568_bl31_v1.44.elf`

The Rockchip files are available through the manufacturer source material or
Rockchip `rkbin`, subject to their own license terms.

Example for the tested HAOS 18.2 input:

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

The output directory contains the raw image, compressed image, U-Boot files,
release manifest, validation manifest, notices, and `SHA256SUMS`. Keep these
large outputs outside the Git repository. Publish the compressed `.img.xz`
and its small metadata files as GitHub Release assets if needed.

The image builder verifies the pinned U-Boot source and output, moves all
eight generic HAOS partitions without changing their contents, validates the
new GPT, and checks deliberately damaged image variants are rejected.

## Build the K11C Connectivity App payload

Required inputs:

- this repository
- a configured HAOS build tree for the exact target kernel
- the manufacturer source tree containing `external/rkwifibt`

The App skeleton is under `k11c_connectivity`. Run:

```bash
bash source/connectivity/scripts/build-driver-bundle.sh \
  --haos-tree /path/to/configured/haos-build-tree \
  --vendor-tree /path/to/manufacturer-source \
  --app-dir "$PWD/k11c_connectivity"

bash source/connectivity/scripts/verify-connectivity-bundle.sh \
  "$PWD/k11c_connectivity"
```

The builder checks the tested vendor source and firmware hashes, applies the
included Linux 6.18 compatibility patches, and builds these AArch64 modules:

- `skw_sdio_lite.ko`
- `swt6621s_wifi.ko`
- `skwbt.ko`

It writes kernel-specific modules under
`k11c_connectivity/modules/<kernel release>/` and firmware under
`k11c_connectivity/firmware/`. Both directories are intentionally ignored by
Git and omitted from repository exports.

The current patch set targets `6.18.39-haos`. Re-run the build and verification
for a new HAOS kernel instead of reusing old modules.

## Manufacturer files

- [System images](https://1drv.ms/f/c/106b4b0b39a75ee5/IgARVEENQ10FSLVOu16Z0ZyPAetlTUk5cbx6VCfiH2qOzHo?e=mfTLG8)
- [Source code](https://1drv.ms/f/c/106b4b0b39a75ee5/IgAyC7gbEYJTSbLqbvcl01nTAbTgoKx-YKAY90ieirpIKmA?e=ityIrU)

