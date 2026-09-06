# Build the K11C Connectivity App

This is a separate build from the ready-to-flash HAOS image. It compiles the
manufacturer's SeekWave VS/SWT6621S driver source against the exact HAOS host
kernel, applies the compatibility patches in this repository, and copies the
required firmware into the local App directory.

The public repository intentionally contains neither prebuilt kernel modules
nor SeekWave firmware. Build them locally after obtaining the manufacturer
source archive and checking its license terms.

The steps below reproduce the tested `6.18.39-haos` payload for HAOS 18.2.
Run them in Ubuntu or WSL2 from the root of this repository.

Install the host tools used directly by the connectivity builder:

```bash
sudo apt update
sudo apt install -y coreutils file findutils gawk git grep kmod make patch
```

## 1. Prepare a configured HAOS 18.2 build tree

Install Docker and make sure the current user can run the HAOS build through
`sudo`. Follow the
[official Home Assistant OS build setup](https://developers.home-assistant.io/docs/operating-system/getting-started/)
for the host prerequisites.

Clone the exact HAOS source used for the tested modules:

```text
Repository: https://github.com/home-assistant/operating-system.git
Tag:        18.2
Commit:     3c196f5144ae78e124d2ae1a067c1841af71a51c
Board:      generic_aarch64
Kernel:     6.18.39-haos
```

```bash
mkdir -p ../k11c-connectivity-inputs
git clone --branch 18.2 --recurse-submodules \
  https://github.com/home-assistant/operating-system.git \
  ../k11c-connectivity-inputs/haos-18.2
test "$(git -C ../k11c-connectivity-inputs/haos-18.2 rev-parse HEAD)" = \
  3c196f5144ae78e124d2ae1a067c1841af71a51c

cd ../k11c-connectivity-inputs/haos-18.2
scripts/enter.sh make generic_aarch64
cd -
```

The driver builder uses these generated paths from that HAOS tree:

```text
output/build/linux-6.18.39/.config
output/host/bin/aarch64-buildroot-linux-gnu-gcc
```

Check the prepared tree:

```bash
test -f ../k11c-connectivity-inputs/haos-18.2/output/build/linux-6.18.39/.config
test -x ../k11c-connectivity-inputs/haos-18.2/output/host/bin/aarch64-buildroot-linux-gnu-gcc
make -s \
  -C ../k11c-connectivity-inputs/haos-18.2/output/build/linux-6.18.39 \
  ARCH=arm64 kernelrelease
```

The last command must print `6.18.39-haos`. Do not build modules against a
different kernel release and copy them to this HAOS version.

## 2. Obtain the manufacturer Wi-Fi/Bluetooth source

Download the following archive from the
[manufacturer source folder](https://1drv.ms/f/c/106b4b0b39a75ee5/IgAyC7gbEYJTSbLqbvcl01nTAbTgoKx-YKAY90ieirpIKmA?e=ityIrU):

```text
linux-kernel-6.1/rk-linux6.1-2026060914/rk-linux6.1-2026060914.tar.gz
MD5: 6974a91407767c1a9000929d1ed65544
Source tag: rk-linux6.1-2026060914
Source commit: 544b6630ad1cf1321ef102c813141598c73baeed
```

Extract it outside this repository:

Save the downloaded archive as
`../k11c-connectivity-inputs/rk-linux6.1-2026060914.tar.gz`, then run:

```bash
echo '6974a91407767c1a9000929d1ed65544  ../k11c-connectivity-inputs/rk-linux6.1-2026060914.tar.gz' | md5sum -c -
mkdir -p ../k11c-connectivity-inputs/vendor-linux-6.1
tar -xf ../k11c-connectivity-inputs/rk-linux6.1-2026060914.tar.gz \
  -C ../k11c-connectivity-inputs/vendor-linux-6.1
```

The build uses this driver directory from the extracted archive:

```text
external/rkwifibt/drivers/skw6621s
```

It copies these exact firmware and NVRAM files:

```text
external/rkwifibt/firmware/seekwave/ea6x21qx/SWT6621S_DRAM_SDIO.bin
external/rkwifibt/firmware/seekwave/ea6x21qx/SWT6621S_IRAM_SDIO.bin
external/rkwifibt/firmware/seekwave/ea6x21qx/SWT6621S_NV_SDIO_ALONE.bin
external/rkwifibt/firmware/seekwave/ea6x21qx/SWT6621S_NV_SDIO_SHARE.bin
external/rkwifibt/firmware/seekwave/ea6x21qx/SWT6621S_SEEKWAVE_R00000.bin
external/rkwifibt/firmware/seekwave/ea6x21qx/SWT6621S_SEEKWAVE_R00001.bin
external/rkwifibt/firmware/seekwave/ea6x21qx/SWT6621S_SEEKWAVE_R04000.bin
external/rkwifibt/firmware/seekwave/ea6x21qx/SWT6621S_SEEKWAVE_R04001.bin
external/rkwifibt/drivers/skw6621s/skwbt/sv6160.nvbin
external/rkwifibt/firmware/seekwave/ea6x21qx/sv6160lite.nvbin
external/rkwifibt/drivers/skw6621s/skwbt/sv6316.nvbin
```

`source/connectivity/firmware.sha256` contains the tested hash for every
firmware file. The driver builder also checks the entire tested
`drivers/skw6621s` source tree. It stops before compiling if the archive does
not match.

Locate the extracted directory containing `external/rkwifibt`:

```bash
RKWIFIBT=$(find ../k11c-connectivity-inputs/vendor-linux-6.1 -type d \
  -path '*/external/rkwifibt' -print -quit)
VENDOR_ROOT=${RKWIFIBT%/external/rkwifibt}
test -d "$VENDOR_ROOT/external/rkwifibt/drivers/skw6621s"
```

Alternatively, pass the `external/rkwifibt` directory itself as
`--vendor-tree`.

## 3. Build the modules and App payload

The App skeleton is `k11c_connectivity/`. The builder makes a temporary copy
of the manufacturer driver, applies
`source/connectivity/driver-patches/0001` through `0006`, and builds only the
SDIO Wi-Fi and Bluetooth modules. The manufacturer source directory is not
modified.

```bash
bash source/connectivity/scripts/build-driver-bundle.sh \
  --haos-tree ../k11c-connectivity-inputs/haos-18.2 \
  --vendor-tree "$VENDOR_ROOT" \
  --app-dir "$PWD/k11c_connectivity"
```

The script builds and installs exactly these modules:

```text
k11c_connectivity/modules/6.18.39-haos/skw_sdio_lite.ko
k11c_connectivity/modules/6.18.39-haos/swt6621s_wifi.ko
k11c_connectivity/modules/6.18.39-haos/skwbt.ko
```

It copies the eleven firmware files into:

```text
k11c_connectivity/firmware/
```

The two generated directories are ignored by Git because their redistribution
terms differ from this project's AGPL license.

## 4. Verify the completed App directory

```bash
bash source/connectivity/scripts/verify-connectivity-bundle.sh \
  "$PWD/k11c_connectivity"
```

The verifier checks the App contract, module names, AArch64 architecture,
kernel `vermagic`, required module parameters, module checksums, firmware file
set, firmware checksums, and module loading order. Completion is reported as:

```text
K11C connectivity bundle verification passed
```

The completed `k11c_connectivity` directory is the local Home Assistant App
build context. Home Assistant Supervisor builds the App container when the
directory is copied to `/addons/k11c_connectivity/` and the local App is
installed. Continue with [INSTALL.md](INSTALL.md#wi-fi-and-bluetooth).

When an HAOS operating-system update changes `uname -r`, prepare the matching
HAOS build tree and rebuild these modules. A Home Assistant Core-only update
does not change the host kernel.
