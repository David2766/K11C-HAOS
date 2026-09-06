# Install K11C HAOS

These instructions apply to the tested KickPi K11C V1.2 with 4 GB RAM and
32 GB eMMC. Check the board revision and storage device before writing.

## Flash a microSD card

Download these files from the matching GitHub Release:

- `k11c-haos-<version>-sd.img.xz`
- `SHA256SUMS`
- `k11c-haos-<version>-sd.release.txt`

Compare the image hash with `SHA256SUMS`. On Windows:

```powershell
Get-FileHash .\k11c-haos-<version>-sd.img.xz -Algorithm SHA256
```

Write the compressed `.img.xz` directly to a microSD card with balenaEtcher
or Rufus. Select DD image mode if Rufus asks. Do not extract the image first.

Insert the card and power on the board. First boot can take about two minutes.
Use wired Ethernet for initial Home Assistant setup.

UART settings are 1,500,000 baud, 8 data bits, no parity, and 1 stop bit.

## Wi-Fi and Bluetooth

The release image contains the K11C device tree but does not contain the
SeekWave firmware or prebuilt kernel modules. Build the completed App bundle
as described in [BUILD.md](BUILD.md).

Copy the completed `k11c_connectivity` directory to
`/addons/k11c_connectivity/` on Home Assistant OS. Then run from the Terminal
& SSH App:

```bash
ha store reload
ha store apps install local_k11c_connectivity
ha apps start local_k11c_connectivity
ha apps logs local_k11c_connectivity
```

Open the K11C Connectivity App configuration and set:

```yaml
enabled: true
antenna_mode: share
allow_unverified_board: false
```

Restart the App. A successful start ends with:

```text
K11C Wi-Fi and Bluetooth are active on <kernel release>
```

Wi-Fi is then available under **Settings > System > Network**. Bluetooth is
available under **Settings > Devices & services > Bluetooth**.

The SeekWave modules must match the exact HAOS kernel release. A Home
Assistant Core update does not change that kernel. An HAOS operating-system
update can change it, so rebuild the App payload for the new kernel when
`uname -r` changes. Keep wired Ethernet available during that update.

## Back up the factory eMMC

The factory Android image on the tested board exposes eMMC as
`/dev/block/mmcblk2`. Confirm the device name on the board before copying.

```text
adb pull /dev/block/mmcblk2boot0 emmc-boot0.bin
adb pull /dev/block/mmcblk2boot1 emmc-boot1.bin
adb pull /dev/block/mmcblk2 emmc-full-32gb.img
```

Store the backup and its SHA-256 hashes away from the board.

## Install from microSD to eMMC

This operation overwrites the eMMC user area. Confirm that microSD boot,
Ethernet, and Home Assistant work before continuing.

Stop autoboot at the U-Boot prompt. On the tested board, `mmc1` is microSD and
`mmc0` is eMMC. Read `image.raw_sectors_hex` and `image.raw_bytes_hex` from
the matching release manifest and substitute them for `SECTORS` and `BYTES`.

Load the image from microSD and calculate its CRC:

```text
mmc dev 1
mmc read 40000000 0 SECTORS
crc32 40000000 BYTES
```

Select eMMC and verify that `mmc info` reports the expected device and 29.1
GiB capacity before writing:

```text
mmc dev 0
mmc info
mmc write 40000000 0 SECTORS
mmc read 40000000 0 SECTORS
crc32 40000000 BYTES
gpt repair mmc 0
gpt verify mmc 0
mmc part
```

The CRC before and after the write must match. Power off completely, remove
the microSD card, and boot from eMMC.

