# Build the K11C HAOS image

This guide builds the ready-to-flash K11C image. It builds the K11C U-Boot,
adds it in front of the official generic AArch64 HAOS image, and preserves all
eight HAOS partitions byte-for-byte.

It does **not** build the K11C Connectivity App, SeekWave kernel modules, or
SeekWave firmware. See [CONNECTIVITY.md](CONNECTIVITY.md) for that separate
build.

Run the commands from the root of this repository in Ubuntu or WSL2. Keep the
downloaded inputs and generated images outside the Git repository.

## 1. Install host packages

The following package set covers the U-Boot build and the image validators on
Ubuntu 24.04:

```bash
sudo apt update
sudo apt install -y \
  bc bison build-essential curl device-tree-compiler fdisk file flex \
  gcc-aarch64-linux-gnu gdisk git jq libgnutls28-dev libncurses-dev \
  libssl-dev mtools python3 python3-dev python3-pyelftools \
  python3-setuptools swig uuid-dev xz-utils
```

## 2. Download the official HAOS image

The tested input is the official generic AArch64 HAOS 18.2 image:

```text
File:   haos_generic-aarch64-18.2.img.xz
URL:    https://github.com/home-assistant/operating-system/releases/download/18.2/haos_generic-aarch64-18.2.img.xz
SHA256: dae257b7b2ce3860f3ce187a85d27b3e9aaaebf55e6acb50f38115e7d7b783a1
```

Download and verify it:

```bash
mkdir -p ../k11c-build-inputs ../k11c-build-output
curl -L \
  -o ../k11c-build-inputs/haos_generic-aarch64-18.2.img.xz \
  https://github.com/home-assistant/operating-system/releases/download/18.2/haos_generic-aarch64-18.2.img.xz
echo 'dae257b7b2ce3860f3ce187a85d27b3e9aaaebf55e6acb50f38115e7d7b783a1  ../k11c-build-inputs/haos_generic-aarch64-18.2.img.xz' | sha256sum -c -
```

## 3. Prepare upstream U-Boot

The build script accepts only a clean upstream U-Boot checkout at this exact
commit:

```text
Version: U-Boot v2026.04
Commit:  88dc2788777babfd6322fa655df549a019aa1e69
Source:  https://source.denx.de/u-boot/u-boot.git
```

```bash
git clone https://source.denx.de/u-boot/u-boot.git \
  ../k11c-build-inputs/u-boot
git -C ../k11c-build-inputs/u-boot checkout \
  88dc2788777babfd6322fa655df549a019aa1e69
test "$(git -C ../k11c-build-inputs/u-boot rev-parse HEAD)" = \
  88dc2788777babfd6322fa655df549a019aa1e69
test -z "$(git -C ../k11c-build-inputs/u-boot status --porcelain --untracked-files=all)"
```

Do not copy the K11C changes into this checkout. During the build,
`source/scripts/build-k11c-uboot.sh` creates a temporary worktree and copies
these repository files into it:

- `source/device-tree/linux/rk3566-kickpi-k11c.dts`
- `source/device-tree/u-boot/rk3566-kickpi-k11c-u-boot.dtsi`
- `source/u-boot/board/hardkernel/odroid_m1s/Makefile`
- `source/u-boot/board/hardkernel/odroid_m1s/k11c.c`

The original U-Boot checkout remains unchanged.

## 4. Obtain the Rockchip DDR and BL31 files

Download the manufacturer source archive from the
[manufacturer source folder](https://1drv.ms/f/c/106b4b0b39a75ee5/IgAyC7gbEYJTSbLqbvcl01nTAbTgoKx-YKAY90ieirpIKmA?e=ityIrU).
The tested archive is located inside that share at:

```text
linux/sdk/20260515/rk356x-linux-2026051515.tar.gz
MD5: 81812c6a8770f73b41fc191939e9345e
Source tag: rk356x-linux-2026051515
Source commit: 22a87c6c96feef811d103e04085b46b7d7ec4786
```

Extract it and use these exact files from the archive:

```text
rkbin/bin/rk35/rk3566_ddr_1056MHz_v1.23.bin
SHA256: 20e4bb076847bd019fcdeb7bdc15bd249890f07ecc76e9937101f22e50950982

rkbin/bin/rk35/rk3568_bl31_v1.44.elf
SHA256: 65110f822fdbdd0163ce2dabc60591e7a8a0ffbc9471780e29eef0062f9ed7b6
```

Example:

Save the downloaded archive as
`../k11c-build-inputs/rk356x-linux-2026051515.tar.gz`, then run:

```bash
echo '81812c6a8770f73b41fc191939e9345e  ../k11c-build-inputs/rk356x-linux-2026051515.tar.gz' | md5sum -c -
mkdir -p ../k11c-build-inputs/vendor-sdk
tar -xf ../k11c-build-inputs/rk356x-linux-2026051515.tar.gz \
  -C ../k11c-build-inputs/vendor-sdk

DDR=$(find ../k11c-build-inputs/vendor-sdk -type f \
  -path '*/rkbin/bin/rk35/rk3566_ddr_1056MHz_v1.23.bin' -print -quit)
BL31=$(find ../k11c-build-inputs/vendor-sdk -type f \
  -path '*/rkbin/bin/rk35/rk3568_bl31_v1.44.elf' -print -quit)
test -n "$DDR" && test -n "$BL31"
echo "20e4bb076847bd019fcdeb7bdc15bd249890f07ecc76e9937101f22e50950982  $DDR" | sha256sum -c -
echo "65110f822fdbdd0163ce2dabc60591e7a8a0ffbc9471780e29eef0062f9ed7b6  $BL31" | sha256sum -c -
```

The last output must match the two SHA-256 values shown above. The image
builder checks them again and stops on any mismatch.

## 5. Build the image

From the repository root:

```bash
DDR=$(find ../k11c-build-inputs/vendor-sdk -type f \
  -path '*/rkbin/bin/rk35/rk3566_ddr_1056MHz_v1.23.bin' -print -quit)
BL31=$(find ../k11c-build-inputs/vendor-sdk -type f \
  -path '*/rkbin/bin/rk35/rk3568_bl31_v1.44.elf' -print -quit)

bash source/scripts/build-k11c-image.sh \
  --haos-image ../k11c-build-inputs/haos_generic-aarch64-18.2.img.xz \
  --haos-sha256 dae257b7b2ce3860f3ce187a85d27b3e9aaaebf55e6acb50f38115e7d7b783a1 \
  --uboot-tree ../k11c-build-inputs/u-boot \
  --ddr-blob "$DDR" \
  --bl31 "$BL31" \
  --output-dir ../k11c-build-output
```

Use an empty output directory. The script refuses to replace an existing
artifact.

The script produces:

- `k11c-haos-18.2-sd.img.xz`: ready-to-flash image
- `k11c-haos-18.2-sd.img`: uncompressed local validation copy
- `k11c-haos-18.2-sd.release.txt`: release and eMMC copy parameters
- `k11c-haos-18.2-sd.img.manifest.txt`: partition validation results
- `u-boot-rockchip-k11c-18.2.bin`: K11C U-Boot image
- `u-boot-rockchip-k11c-18.2.bin.config`: U-Boot configuration
- `u-boot-rockchip-k11c-18.2.bin.control.dtb`: U-Boot control DTB
- `u-boot-rockchip-k11c-18.2.bin.manifest.txt`: U-Boot build manifest
- `SHA256SUMS`, `THIRD_PARTY_NOTICES.md`, and
  `ROCKCHIP_RKBIN_LICENSE.txt`

Verify the completed output:

```bash
(cd ../k11c-build-output && sha256sum --check --strict SHA256SUMS)
```

The image build is complete only when the build and checksum verification both
finish without an error. Flash `k11c-haos-18.2-sd.img.xz` as described in
[INSTALL.md](INSTALL.md).

The ready-to-flash image contains HAOS, K11C U-Boot, the K11C device tree, and
the licensed Rockchip boot components. It does not contain SeekWave firmware
or prebuilt SeekWave modules.
