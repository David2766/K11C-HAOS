# K11C Installer IPC contract — 0.4

## Confirmed storage transactions

`storage_plan(instanceId, location, operation, source)` prepares `install`,
`uboot`, `restore` or `gpt-repair`. Source is a managed image SHA256 for install,
a managed backup directory name for restore, and empty otherwise. No caller may
supply write addresses, executables, firmware files or arbitrary image paths.
The plan validates its inputs, takes a verified boot/GPT backup, and returns a
content-addressed plan ID, exact ranges, device capacity and recovery directory.
It does not write eMMC. `storage_execute(planId, confirmed)` requires explicit
confirmation and consumes that one-use plan. It reopens the same USB instance
and port, checks identity and every backed-up sector against the plan, rehashes
locked input files, and reconstructs the operation before the first write.

Every write is sector-aligned and bounded by its derived plan. Each range is
read back and hashed. GPT is reread as a pair at the physical disk ends. Failure
stops without retry/reset/automatic rollback and preserves the journal/backup.
Installation deletes the existing OS and user data; the automatic backup covers
only boot/GPT, NOT user data. All eight official HAOS partition payloads, GUIDs,
types, attributes and labels are preserved; starts/ends move by 32768 sectors.
The bundled K11C r24 firmware occupies the reserved area starting at LBA 64.
The backup GPT is explicitly written at the physical eMMC end. Official HAOS
first boot performs its usual data-partition expansion. No custom HAOS build.

U-Boot update requires an existing healthy K11C layout and touches only the
bundled firmware's range. Restore selects a complete app-managed backup with
matching USB identity/capacity and unchanged partition entries; it restores only
the four backed-up ranges. It cannot undo a fresh OS installation. GPT repair
uses a valid copy; differing valid partition arrays are ambiguous and rejected.
For equal arrays with differing disk GUIDs, the primary header is authoritative.
`gpt_check(instanceId)` is read-only; `backup_catalog()` lists validated managed
backups. The UI displays target, effects and backup before confirmation. Device
or input changes invalidate the visible plan. No persistent writes occur on
startup, polling, image selection, planning, cancellation or window close.

## HAOS images

`image_releases()` returns stable official generic-aarch64 `.img.xz` releases
from home-assistant/operating-system. `image_download(version)` re-resolves the
version on the server side; IPC never accepts a download URL or checksum.
`image_select()` opens a native file picker and imports the selected `.img` or
`.img.xz`. Both return a prepared-image report only after the same inspection.
Picker cancellation returns null. `image_cancel()` requests cancellation of the
active image operation without waiting for the operation mutex.

The visible official-image slide loads releases automatically on its first entry.
Loading is deduplicated and successful lists are reused for this app session.
Metadata loading does not lock navigation or local-file selection. Failures stay
inline in the official panel until explicit retry; there is no automatic retry loop.
Refreshing the list never downloads or prepares an image.

`image-progress` events carry phase/completed/total; total may be null while
expanding XZ. The UI locks navigation during image work and keeps installation preparation disabled.
An unsuccessful new operation clears its pending selection. Merely selecting a
version is not image readiness. Prepared reports are not authorization to flash;
the storage transaction revalidates the saved image itself.

Downloads allow HTTPS GitHub release hosts only, enforce declared length and the
SHA256 fetched from that release's assets[].digest (no compiled HAOS digests),
and publish cache files only after completion. A missing official digest returns
IMAGE_NO_DIGEST, never a checksum-verified result. A cached download
is reused only after rehashing against the official digest. Import never writes
the user's input file. Streaming copy/decompression uses bounded memory and output
size, then verifies both GPT copies/CRCs, HAOS partitions and ARM64 EFI boot file.
Local structure validation is not represented as official checksum verification.
Managed images live under executable-relative `data/images`; no board is required.
Cancellation/errors clean only this operation's temporary files. Existing valid
images remain reusable. Network cancellation completes at a read/request boundary.

CLI equivalents for diagnostics: `image-releases`, `image-download <version>`,
`image-import <path>`. They call the same production functions as the UI and do
not themselves write eMMC. Image work and USB work are serialized across processes.
In the GUI, a user image action waits for any in-flight background device poll;
the poll uses try_lock and skips when a user operation owns the mutex.

## Commands

Tauri commands return `{ok:true,data}` or `{ok:false,error:{code,detail}}`.
`preflight`, `ensure_driver`, `inspect_device(instanceId)` and `history` remain.
`list_devices` enumerates only (poll every 2 seconds when idle, no history entry).
`prepare_device(instanceId, location, confirmedK11c)` uploads the one bundled
RAM loader on explicit user action. `confirmedK11c` must be true: USB IDs alone
do not establish the board model. `backup_device(instanceId)` saves boot-area
and GPT bytes under executable-relative `data/backups/`. `open_backups` opens
only that fixed folder. No caller-controlled executable, flash address or path.

CLI allowlist: `preflight`, `driver-status`, `devices`, `inspect <id>`,
`prepare <id> <location> --confirmed-k11c`, `backup <id>`, and internal
`driver-install-only`, plus the image commands above and `gpt-check <id>`,
`backup-catalog`, `storage-plan <id> <location> <operation> <source>`,
`storage-execute <plan-id> --confirmed-k11c-write`.
One JSON response on stdout; progress NDJSON on stderr.
Progress contains phase/completed/total. GUI relays it as `usb-progress`;
operations are serialized and polling pauses during work.
Short helpers: 30 seconds; prepare: 90 seconds; backup/catalog/GPT check:
300 seconds; storage plan: 1200 seconds; storage execution: 3600 seconds.
A timed-out helper is killed. Incomplete backups end in `.partial`; interrupted
writes retain their consumed plan, journal and pre-write backup, not a success.
The GUI attaches helpers to a kill-on-close Windows Job Object so closing or
crashing the app does not leave an orphan USB operation running.

## Driver and USB identity

Reuse valid installed Rockusb. If missing, verify the bundled signed package
and request UAC for fixed pnputil `/add-driver ... /install`. No force/rebind,
ADB change, unsigned policy change, or automatic reboot. Cancel is not retried
automatically in that app process. Driver installed and USB binding are distinct.

Enumerate VID 2207 and descriptor-based mode; reads require supported RK356x,
Loader and official Rockusb binding. Re-enumerate the selected instance before
opening. Prepare also checks the selected physical bus/port-chain location.
Only one supported, bound Loader on the SAME location can complete preparation;
a different USB port, ambiguous result or timeout is not a success. The Loader
must answer ready, storage and capacity queries. Storage must already be eMMC.

## RAM preparation

Read bundled `resources/loader/k11c-usb-loader-v1.23.114.bin` once, check compiled
SHA256 and parse all bounded records before sending any bytes. Send only the
0x471 and 0x472 entries, in file order, honoring delays. FlashBoot entries are
not sent. Missing/tampered files cause zero uploads. No arbitrary loader.
Already-Loader selections are inspected without re-uploading. Failure never
triggers an automatic retry. Physical MASKROM entry remains a user action.

## Backup transaction

Snapshot the first 34816 sectors (GPT and K11C reserved boot area) and physical
last 33 sectors, split into named files with exact LBA/length/SHA256 in manifest.
This is NOT an HA user-data backup or eMMC boot0/boot1 dump. GPT validation
reports header/array CRC, reciprocal pointers and equal GUID/entries for the
captured standard 128x128 GPT layout; unsupported layouts are reported, not
called healthy. Damaged GPT does not prevent saving raw recovery bytes.

Read in bounded chunks, reject short transfers, sync files, re-read from USB
and compare every file digest, then verify saved file lengths/digests and
device storage/capacity identity. Only then write manifest `complete:true` and
rename the unique `.partial` directory. A disconnect, short read, changed data,
disk-full or timeout never publishes a successful backup. Existing backups are
never overwritten. Backup cannot invoke eMMC writes, erase, reset or storage switch.

## Portable UI and verification

Local webview only, resources/data next to executable. Helper operations use an
OS mutex across CLI/GUI instances. No startup service or background daemon.
Customer copy describes state/actions, not implementation milestones. Raw
diagnostics are under expandable details.

The installation wizard has one primary footer action per step. Connection
preparation/inspection, backup and image preparation advance only after success;
failure or cancellation leaves the current step unchanged. MASKROM upload must
also re-enumerate as a supported, bound Loader at the same physical port. The
step tabs allow image preparation before connecting a board. A ready image or
same-device/port backup can be reused; changing its source or target invalidates
the corresponding result. Advancing to review never starts storage execution.

Advanced UI exposes storage info, U-Boot update, boot-area backup/restore and GPT
inspection/repair. Write actions prepare a preview and verified recovery backup,
then require typing K11C before the explicit write action. Closing the native
window is prevented during storage execution. The UI has no write-cancel action.
Image cancellation remains independent and cannot interrupt a storage write.

Tests use production loader parsing/upload and backup transaction with fake USB
boundaries; real filesystem hashing/rename is used. Mutation tests remove/reverse
critical guards and must fail. Actual packaged helper, Unicode relocation,
resource tampering, no-device and UI are checked separately. Real board USB
transfers and fresh-Windows driver setup require hardware testing.
