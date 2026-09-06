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
as described in [CONNECTIVITY.md](CONNECTIVITY.md).

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

## Install to eMMC from U-Boot

After confirming that HAOS works from microSD, power off the board. Flash the
same release image to that card again, or prepare a second freshly flashed
card. Reflashing is required because HAOS can expand the data partition and
rewrite the SD GPT during the test boot.

Insert the freshly flashed card and stop U-Boot before HAOS starts for the
first time. The manual method below installs that tested HAOS image to eMMC.
Settings made during the earlier microSD test are not copied.

**Warning: this overwrites the eMMC user area. A wrong MMC device number or
image size can destroy another storage device. Back up the factory eMMC first.**

**Use a freshly flashed microSD card and stop U-Boot before its first HAOS
boot.** HAOS can expand the SD data partition and rewrite its GPT on first
boot. If the card has already booted HAOS, flash the release image to it again
before following this procedure.

The following mapping was confirmed on the tested K11C V1.2:

- `mmc1`: microSD
- `mmc0`: eMMC, reported as 29.1 GiB

Read `image.raw_sectors_hex` and `image.raw_bytes_hex` from the release
manifest matching the image on the microSD card. Substitute those values for
`SECTORS` and `BYTES`. Do not reuse values from another release.

For the tested `k11c-haos-18.2-sd` image, the values are:

```text
SECTORS = 1d3828
BYTES   = 3a705000
```

Stop autoboot at the U-Boot prompt. Confirm the source card, load only the
release image area into RAM, and record its CRC:

```text
mmc dev 1
mmc info
mmc part
mmc read 40000000 0 SECTORS
crc32 40000000 BYTES
```

Select eMMC and verify its identity and capacity before entering the write
command:

```text
mmc dev 0
mmc info
mmc part
```

Only continue when `mmc0` is the expected 29.1 GiB eMMC:

```text
mmc write 40000000 0 SECTORS
mmc read 40000000 0 SECTORS
crc32 40000000 BYTES
```

The source and destination CRC values must match. Do not continue if they are
different. Repair the backup GPT for the full eMMC capacity and inspect the
result:

```text
gpt repair mmc 0
gpt verify mmc 0
mmc part
```

Power off completely, remove the microSD card, and boot from eMMC. The first
HAOS boot can take about two minutes.
