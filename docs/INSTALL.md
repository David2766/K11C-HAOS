# Install K11C HAOS

For the current portable Windows installer, see [Installer](../installer/README.md)
and [release notes](RELEASE.md). The SD-image procedure below is the earlier
manual path. Do not apply its addresses to a bare official HAOS image.

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

Do **not** use the old image-copy + `gpt repair` procedure. The tested U-Boot
accepts two independently valid headers even when their disk GUIDs differ.
An old Android backup GPT can survive at the end of eMMC. A successful
`gpt verify` message alone does not establish that both copies agree.

Use the repository's `source/scripts/prepare-emmc-install.py` together
with `source/scripts/gpt_image.py`. The helper reads the decompressed K11C `.img`
and prints four blocks of image-specific U-Boot commands. It does not write
the source image, contact the board, compile HAOS, or perform an installation.
These commands explicitly write **both** GPTs using the image's disk GUID
and identical partition arrays, then read back the image and backup GPT.

Example on the development PC (replace the SHA value with `image.raw_sha256`
from the matching release manifest):

```powershell
python .\source\scripts\prepare-emmc-install.py --image C:\K11C-inputs\k11c-haos-18.2-sd.img --sha256 RAW_SHA256_FROM_MANIFEST --target-sectors 61071360
```

The tested eMMC contains exactly **61071360 sectors of 512 bytes**. Rounded
`29.1 GiB` alone is not an exact capacity check. Confirm the sector count
from the device before generating commands, for example with
`cat /sys/class/block/mmcblk0/size` on the verified HAOS eMMC controller.
Do not reuse this number for a different storage variant without checking it.

Follow the printed blocks in order, checking all MMC return messages and
each expected CRC before proceeding. Do not paste the whole report at once.
The second block changes only RAM; the fourth **overwrites the destination**.
Both final readback CRCs must match their expected values. Stop on any mismatch.

The command sequence is locally tested against disk-file simulation and the
actual HAOS ARM64 first-boot partition expander. Hardware installation with
this new sequence remains a separate acceptance test; do not reinstall a
working board solely to test it. Existing installs use the GPT-only repair kit.

**Do not boot or reset the board with the microSD card still inserted after
the eMMC write completes.** Power the board off completely, disconnect power,
remove the microSD card, and only then reconnect power and boot from eMMC. The
first HAOS boot after installation must be from eMMC alone and can take about
two minutes.

After that first boot and automatic data expansion, run on the HAOS host shell:

```sh
sgdisk --print --verify /dev/mmcblk0
```

Require `No problems found.` in the report, not merely exit status zero.
This is installation-time disk metadata handling. The official HAOS partitions
and native OTA update path remain unchanged; the installation helper is not
run for ordinary HAOS updates.

Booting a full HAOS microSD image while an HAOS installation is also present
on eMMC can make HAOS treat the eMMC data partition as an external data disk.
Its `hassos-data` filesystem label may be changed to `hassos-data-dis`, which
prevents the eMMC installation from mounting its data partition on the next
boot.
