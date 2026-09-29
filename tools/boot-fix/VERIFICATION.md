# Local verification — r2, 2026-09-28

## GPT-only r2 acceptance

No board connection or write, HAOS/U-Boot build, source image overwrite,
repository export, Git commit, or publication was performed for r2.

The production `boot-fix-lib.sh` passed **12 cases and 8 mutation tests**
using real sgdisk 1.0.10. The unmodified host entry point also rejects a
non-root development shell. New coverage includes:

- The actual board's saved primary GPT, backup GPT and complete U-Boot
  reservation (`artifacts/K11C/boot-fix-r1/k11c-boot-backup-r1`).
- Only secondary-header GUID bytes 56..71 and CRC bytes 16..19 change.
- Primary GPT/MBR, full U-Boot reservation, both partition arrays,
  partition-data sentinels and console bytes stay identical.
- Direct GUID/array comparisons reject independently valid, inconsistent
  GPTs. Repetition on a clean disk performs no GPT writes.
- Removing either comparison or the actual GPT write causes the tests to fail.
- `check-gpt` is independent of console speed and reboot state.

Both packaged shell files also passed `-n` with the actual HAOS 18.2 ARM64
`output/target/bin/sh` under qemu. The old historical r3 image produces
source CRC `529d596e`, patched CRC `9e700612` and primary-header CRC
`1ee819b0`, exactly matching the recorded 2026-09-02 installation. The new
sequence additionally writes the explicit secondary GPT; the old one did not.

The new read-only PC helper `scripts/prepare-emmc-install.py` prints an
image-specific fresh-install sequence. It is NOT a repair command. Its
tests execute the emitted `mw.b`, `mw.l`, `cp.b`, MMC read/write and CRC
operations on disposable sparse disk files containing a pre-existing GPT.
They compare the entire protected image payload byte-for-byte, validate both
headers/CRCs/entry arrays, and run the actual HAOS 18.2 ARM64 sfdisk 2.40.4
first-boot expansion under qemu. Expanded GPT copies remain consistent.

Negative/mutation coverage includes omission of the explicit backup write,
removal of the GUID comparison, wrong image SHA-256, wrong target capacity,
and damaged header or partition-array CRCs. Production image validation is
tested with the existing release image, not a newly compiled HAOS image:

```text
k11c-haos-18.2-sd.img
SHA256 aa98241627b65cd4957867489f6a35955d2175f22f89a5406bd5fe704f96f891
```

The image validator compares all eight official HAOS partition payloads,
PARTUUIDs, raw U-Boot bytes, GRUB slot references, and the paired GPTs.
A copy with a valid-CRC, wrong-GUID backup must be rejected; removing the
production paired-GPT validator call makes this negative test fail.

### Remaining hardware acceptance

The current board still needs `backup`, `gpt`, then `check-gpt` run as shown
in README.md. A separate **fresh-install** hardware acceptance run is needed
before treating the generated installation sequence as customer-validated.
Do not erase the working board to obtain that result. No customer GUI or
one-click installer is claimed. Native HAOS OTA is not modified or replaced.

---

## Earlier r1 verification (historical, includes optional console work)

Board writes: **none**. Noninteractive SSH authentication was unavailable.
No deployment, app/driver change, ROM build or reboot was performed.

## Executed tests

`python3 test_boot_fix.py` under Ubuntu-24.04 WSL, unprivileged user,
real sgdisk 1.0.10, disposable sparse disk with the observed board geometry:

```text
PASS production_entrypoint_rejects_nonroot
PASS roundtrip
PASS wrong_guid
PASS wrong_layout
PASS changed_backup
PASS changed_cmdline
PASS ambiguous_console
PASS whitespace_console
PASS later_restore_edits
PASS changed_primary
MUTATION_REJECTED disk_identity_guard_removed
MUTATION_REJECTED layout_guard_inverted
MUTATION_REJECTED checksum_guard_removed
MUTATION_REJECTED later_cmdline_guard_removed
MUTATION_REJECTED wrong_console_speed
LOCAL_PASS cases=9 mutations=5 entrypoint_negative=1 real_sgdisk=1 hardware_applied=0
```

The same production library is executed against the fixtures. Assertions compare
the full MBR/primary GPT, reserved U-Boot area, backup partition entries, per-partition
first/last-sector data sentinels, and console bytes. Only the backup GPT header GUID
and its CRC may differ after repair. Console restore preserves later user edits by
refusing to overwrite them. Repeating successful fixes performs no additional writes.

The unmodified production entry point rejects a non-root development shell.
Its positive path still needs execution on the board: local fixtures do not emulate
the actual sysfs tree, eMMC controller or mounted boot partition.

Both shell files additionally passed `sh -n` using the existing HAOS 18.2 ARM64
BusyBox 1.37.0 under qemu-aarch64-static. That binary advertises sed `-E` support.
This is shell compatibility checking, not a full ARM64 runtime/device test.

`package.py` checks LF-only source files, archive member names, and byte-for-byte
round trip of all members, then writes an external archive SHA256SUMS.

## Source checks

- HAOS 18.3 `buildroot-external/meta`: `HAOS_ID="haos"`.
- HAOS 18.3 generic-aarch64 `grub.cfg`: reads boot `cmdline.txt` with `file_env`.
- HAOS 18.3 `buildroot-external/ota/rauc-hook`: preserves boot `*.txt` in the
  generic install_boot path.
- Existing HAOS serial-getty unit uses agetty `--keep-baud`.
- Linux 8250 console setup uses 9600 without explicit speed options; the user
  independently measured ttyS0 at 9600 on the running board.

For the earlier combined r1 console procedure only, completion requires the post-reboot `check` action to report
`K11C_BOOT_FIX_PASS GPT=clean console=1500000`. Boot-time improvement remains
unmeasured until that reboot. No Bluetooth-firmware fix is claimed.
