# K11C Installer IPC contract — 0.5.0

## Advanced restore source selection

Advanced Backup/Restore → Restore first selects Manufacturer image or Saved
backup. No source is preselected. Source changes and completion clear selections
and previews; selection never starts a write. Existing backup restore is unchanged.
Single-partition inputs (rootfs.img/boot.img, filesystem/sparse payloads) are rejected.

`factory_select(locale?)` uses a native .img picker and read-only inspection.
Complete RKFW Android/Linux packages use the pinned Windows Rockchip upgrade_tool
SFI command, not a new RKFW flashing implementation. Entry bounds, RK356x chip tag,
complete partition set and GPT parameter geometry are required. A content-addressed
receipt references the locked source; source hash and derived metadata are rechecked
for every plan/execution. Complete raw GPT/MBR images are accepted; HAOS official
images belong to the existing HAOS installation path. Extended MBR is not supported.

`factory_plan(instanceId,location,source)` verifies current eMMC readiness/full-read
capability, USB identity/port, source capacity and sparse RAW/FILL/DONT_CARE bounds.
It creates no recovery backup. RKFW plans also bind current partition-header bytes.
`factory_execute(planId,confirmed)` requires explicit confirmation and consumes its
plan once. RKFW uses only fixed UF <validated-image> -noreset arguments. Both vendor
executable and unchanged readback-enabled config must match compiled digests.
Only one Rockchip USB device may be connected, since UF has no port-selection API.
The transport handle is closed before UF opens it. Kill-on-close job containment,
serialized operations, bounded tool logs, and a transaction journal remain in force.
Vendor success/exit status is necessary but insufficient: every non-DONT_CARE
partition payload byte is independently reread through RockUSB, and both physical
GPT copies/CRCs and all fixed parameter partition extents must match. A final
grow partition keeps its exact name/start and ends either at the physical GPT
usable limit or that exclusive limit rounded down to 64 sectors, matching the
pinned vendor's observed K11C output. Arbitrary shortening is rejected. Every
raw or fully expanded sparse image, including DONT_CARE extents, must also fit
the actual partition capacity. BootROM-transformed
Loader bytes are checked by the vendor tool, not claimed as our independent raw
comparison. DONT_CARE regions and absent package partitions are not saved user data.

Raw manufacturer images use operation factory-raw through the existing bounded
write/readback transaction. Partition starts, identities and all system payloads
are preserved. GPT headers/PMBR are adapted to the actual disk end; partition sizes
are not enlarged. Stale source-end GPT is cleared. MBR images clear uncovered
physical-end GPT metadata. No HAOS relocation, K11C firmware or Connectivity seed
is injected into manufacturer images. They are not backup archives.

Helper allowlist additions: factory-import <path>, factory-plan <id> <location>
<source-id>, factory-execute <plan-id> --confirmed-k11c-write. The helper has no
general vendor-command, executable, address or argument passthrough. Production
UF/boot verification requires an actual K11C trial; fixture tests do not certify it.

## Archive and restore revision (current)

The wizard now has five slides: operation, connection, FULL backup, input,
review. Install uses a prepared HAOS image; restore uses a verified FULL archive
or sector-aligned full raw `.img`. Both reuse the wizard's verified FULL recovery
backup rather than creating a second one. Advanced backup offers FULL, BOOT and
HAOS on a healthy K11C HAOS layout; other/unknown layouts permit FULL only.

`backup_device(instanceId, kind)` writes a streaming lossless Zstandard archive under
executable-relative `backup/`, named KIND-YYMMDD-HHMMSS.k11cbackup. No per-backup
directories. FULL includes every sector in the eMMC user area; BOOT includes
the first 34816 and last 33 sectors; HAOS includes all eight HAOS partitions.
None includes eMMC boot0/boot1, RPMB or OTP. Header/footer metadata, exact ranges
and SHA256 are internal. USB reread and stored-payload verification precede
publication. Incomplete files retain `.partial`. No filesystem free-space
heuristics or skipped sectors; compression reduces storage, not USB read work.

`backup_select(locale?)` imports a `.k11cbackup` or registers a full raw `.img`
selected by a native picker. Firmware packages RKFW/RKAF and Android sparse
images are rejected. Raw imports are locked and hashed, not copied. Catalog
listing checks bounded metadata; planning/execution recheck every payload byte.
Legacy boot backups remain selectable in Advanced only.
RAW import receipts are also included in catalog listings, even when there is no
managed `backup/` directory. Listing reads only the receipt and disk-image headers,
labels rows as unverified, and disables missing/invalid files. Full payload hashes
are rechecked against the original import ID before restore planning/execution.
Selecting an archive already in the portable backup folder verifies and reuses it,
without copying it to a second file.

`storage_plan` additionally accepts `recovery` (verified backup ID) and operation
`restore-archive`. The helper equivalent is `storage-plan-backed <id> <location>
<operation> <source> <recovery>`. New `archive-backup <id> <kind>` uses the same
production backup engine. Inspect returns disk OS classification and allowed
backup kinds. The actual device must have full-read capability and matching
capacity. Full restore replaces layouts; partial restore requires matching HAOS
partition entries. Restore writes the saved bytes, never current release assets.
Input files are locked throughout verification and execution. Full recovery is
compared with current disk contents before writes; backups remain usable after
USB reconnection. The one-use plan still binds the current USB instance/port.
Writes and rereads are bounded by derived ranges. Full restore accepts the saved
OS/GPT state, including a damaged table, rather than forcing HAOS validation.

Advanced Restore sets `storage_plan.directRestore=true` with an empty `recovery`.
It dispatches `storage-plan-direct-restore <id> <location> <operation> <source>`;
only `restore` and `restore-archive` are permitted. This path creates no recovery
backup, and returns `backup_path:null`. It verifies the selected source and saves
only a hash of the target boot/GPT state in the one-use plan, not a recovery copy.
Execution rechecks that hash, device identity/port/capacity, source contents and
write ranges, then requires the same explicit confirmation and full readback.
The wizard, installation, U-Boot and GPT repair retain their existing backup policy.

Confirmation requires lowercase `ok`. Full writes warn that all data is replaced;
partial writes identify their limited scope. Helpers doing full-media work have
a 24-hour outer bound plus bounded individual USB transfers. Successful work
clears transient UI state; failed work preserves recovery context and files.

## Confirmed storage transactions

Plans have content-addressed IDs, exact ranges, current USB instance/port,
capacity, source and recovery policy. An optional internal attempt counter
distinguishes a new transaction when the same fully revalidated inputs match a
consumed plan. Attempt zero omits the field, preserving existing format-1 IDs.
Completed/failed plans and journals remain unchanged. An unconsumed preview is
reused only when its stored bytes match; tampering is still an error. Sources
and backups remain reusable across transactions without an extra recovery copy
in Advanced Restore. They perform no eMMC writes. Execution
requires explicit confirmation, locks/revalidates inputs, compares the current
disk with the recovery copy, and consumes the plan once before writing.
Direct Advanced Restore instead checks the planned boot/GPT fingerprint without
making a recovery backup. Every range is reread and hashed; final GPT bytes must match the intended result.
Failure retains the consumed plan, journal and backup; no automatic retry or rollback.
Reopening a cancelled preview can reuse its byte-identical unconsumed plan after
revalidating the device and inputs. A changed or consumed plan is not reused.

Install preserves official OS payloads and identities, relocates starts by 32768
sectors, adds selected K11C U-Boot and version-matched factory Connectivity data,
and writes both GPT copies at their physical locations. Native HAOS OTA remains.
U-Boot writes only its reserved extent. GPT repair accepts only an unambiguous
valid copy. Legacy `restore` restores only boot/GPT and requires unchanged HAOS
partition entries; unlike FULL archives it cannot undo a fresh installation.
The older CLI `storage-plan` is retained for legacy boot-backup compatibility.
The GUI uses `storage-plan-backed` / `flash::plan_backed` except for Advanced
Restore, which uses the explicit direct-restore dispatcher.

Storage transfers use a bounded three-buffer, 1-MiB pipeline. One CPU worker
reads/decompresses and hashes source bytes ahead of the single USB owner. Readback
uses a hashing worker while that same owner reads the next block. USB commands
remain sequential, sector-aligned and at most 1 MiB. Each complete range is
reread after all its writes; no sampling, skipped zeros or concurrent USB commands.
Workers validate offsets/counts, propagate errors and join on both success and
failure. Source handles remain locked through the transaction. RAW preflight
performs one full pass against the original imported SHA256, not two identical
passes. Separate planning and execution checks remain in place.

Successful storage results include additive `timings`: preflight_s, write_s,
readback_s, total_s, source_work_s, source_wait_s, usb_write_s, readback_hash_s,
write_mib_s, readback_mib_s, chunk_bytes and buffers. Worker/USB times overlap
and must not be summed. The journal records timings before the final complete
record and write-complete event. Timings are observations, not speed guarantees.

## HAOS images

`image_releases()` returns stable official generic-aarch64 `.img.xz` releases
from home-assistant/operating-system. `image_download(version)` re-resolves the
version on the server side; IPC never accepts a download URL or checksum.
`image_select(locale?)` opens a native file picker and imports the selected `.img` or
`.img.xz`. Both compose the version-matched factory data and verify the selected
U-Boot before returning a prepared-image report.
Picker cancellation returns null. `image_cancel()` requests cancellation of the
active image operation without waiting for the operation mutex.

The optional locale selects fixed picker title/filter labels (`en`, `ko`,
`zh-TW`, `es`, `ja`); absent or unknown values use English. It cannot change file
types, paths, downloads or validation. Windows supplies the dialog's OS buttons.

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
`image-import <path>` inspect/cache official inputs only. `installation-import
<path>` adds factory components through the same production pipeline as the UI;
`boot-prepare` resolves the verified U-Boot selection. These commands do
not themselves write eMMC. Image work and USB work are serialized across processes.
In the GUI, a user image action waits for any in-flight background device poll;
the poll uses try_lock and skips when a user operation owns the mutex.

## Commands

Tauri commands return `{ok:true,data}` or `{ok:false,error:{code,detail}}`.
`preflight`, `ensure_driver`, `inspect_device(instanceId)` and `history` remain.
`list_devices` enumerates only (poll every 2 seconds when idle, no history entry).
`prepare_device(instanceId, location, confirmedK11c)` probes readiness and uploads
the bundled RAM loader only when needed, on explicit user action. `confirmedK11c` must be true: USB IDs alone
do not establish the board model. `backup_device(instanceId,kind)` saves verified compressed archives under executable-relative `backup/`. `open_backups` opens
only that fixed folder. No caller-controlled executable, flash address or path.

CLI allowlist: `preflight`, `driver-status`, `devices`, `inspect <id>`,
`prepare <id> <location> --confirmed-k11c`, `backup <id>`, and internal
`driver-install-only`, plus the image commands above and `gpt-check <id>`,
`backup-catalog`, `storage-plan <id> <location> <operation> <source>`,
`storage-execute <plan-id> --confirmed-k11c-write`, plus the archive commands above.
One JSON response on stdout; progress NDJSON on stderr.
Progress contains phase/completed/total. GUI relays it as `usb-progress`;
operations are serialized and polling pauses during work.
Short helpers: 30 seconds; prepare: 90 seconds; backup/catalog/GPT check:
300 seconds; legacy storage plan: 1200 seconds; full-media archive/plan/execution: 86400 seconds.
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
official Rockusb binding and a successful bounded TestUnitReady exchange.
Descriptor mode is not readiness: RK356x RAM USB plug may keep `Maskrom`.
Re-enumerate the selected instance before
opening. Prepare also checks the selected physical bus/port-chain location.
Only one supported, bound, ready device on the SAME location can complete preparation;
a different USB port, ambiguous result or timeout is not a success. The Loader
must answer ready, chip, storage and capacity queries. Storage must already be eMMC.
Read/backup/GPT/planning/execution additionally require ReadCapability (0xAA)
bit 3, `Read LBA On`. An absent bit returns `LOADER_READ_RESTRICTED`, not a
damaged-GPT report. A failed query returns `LOADER_CAPABILITY`. Each opened
Windows transport starts with storage access disabled until that query succeeds;
both its read and write methods enforce this. A GPT response consisting entirely
of 0xCC returns `USB_READ_UNTRUSTED`, never a repair proposal. Capability checks
do not themselves unlock an existing vendor Loader.
`inspect_device` returns `ready:true`, `instance_id` and `location` only after
these queries and header reads succeed. A blank/damaged GPT is not a readiness
failure. The UI requires this matching observation for backup and write actions;
each backend operation independently checks current readiness and identity.

## RAM preparation

Read bundled `resources/loader/k11c-usb-loader-v1.23.114.bin` once, check compiled
SHA256 and parse all bounded records before sending any bytes. Send only the
0x471 and 0x472 entries, in file order, honoring delays. FlashBoot entries are
not sent. Missing/tampered files cause zero uploads. No arbitrary loader.
The original packaged vendor file remains unchanged. In the in-memory CODE472
payload only, replace the two pinned AArch64 instructions documented in
`docs/ram-loader-read-access.md`: the read limit and its capability flag.
Validate the original plug digest, instruction preimages and entire resulting
plug digest before any upload. No persistent boot firmware or DDR changes.
Probe both descriptor modes before loading a payload. A successful ready response
reuses the running loader without any upload only if inspection also confirms
full read access. An already-running restricted Loader requires a physical
power-cycle into MASKROM; never overwrite its running code automatically.
A protocol-level busy response,
malformed response, access error or failed storage inspection must not cause an
upload. Only an unavailable bulk command service on a Maskrom descriptor permits
the initial upload. The readiness query uses overlapped I/O with cancellation
and a 1200ms bound per transfer. After upload, poll the same port for up to 25s;
accept either descriptor mode only after readiness and inspection succeed.
Failure never
triggers an automatic retry. Physical MASKROM entry remains a user action.

## Backup transaction

New format 4 archives use K11CBK04 magic and one Zstandard frame. Existing format
3 (K11CBK03, XZ) archives remain readable. Both have bounded JSON metadata,
metadata SHA256, metadata length and matching trailing magic. The metadata format
must match the signature. Zstandard uses level 1, two library-owned background
workers, 2-MiB jobs and an 8-MiB window, overlapping compression with single-owner
USB reads. Finalization must succeed before USB reread and file verification.
Progress reports `read-compress`, then `verify`, then `verify-backup-file`.
Successful creation adds `timings` in seconds: `read_compress_s`,
`device_verify_s`, `file_verify_s`, and `total_s`.
The XZ decoder has a 128-MiB memory limit; the Zstandard decoder caps its window
at 128 MiB and consumes exactly one frame, rejecting unused or trailing compressed
bytes. Ranges are independently derived from kind/capacity/GPT, not trusted
from a caller; every payload hash and uncompressed byte count is checked.
FULL/BOOT metadata headers must match actual payload headers. Invalid/truncated
metadata, extra payload and damaged compressed data fail verification.
New files use exclusive creation and never overwrite existing backups.

Full-read capability is required before USB reads and before publication.
All-0xCC GPT responses are rejected separately from damaged GPT. Saved sectors,
USB reread and compressed-file contents must agree before the final filename is
published. Legacy format-1/2 directory backups remain readable with their
original validation rules. No erase, boot partition switch, RPMB or OTP API exists.

## Portable UI and verification

Local webview only, resources/data next to executable. Helper operations use an
OS mutex across CLI/GUI instances. No startup service or background daemon.
Customer copy describes state/actions, not implementation milestones. Raw
diagnostics are under expandable details.

The UI supports English, Korean, Traditional Chinese only, Spanish and Japanese.
The saved display language takes precedence over the system language; unsupported
languages fall back to English. Chinese system locales select Traditional Chinese.
Changing language updates presentation only: no IPC, reset, new plan or change to
confirmation text (`ok` for storage; `K11C` for app maintenance). Language preference is stored in WebView local storage
inside the portable data profile. Error codes and raw technical details are intact.

Appearance offers system, light and dark modes, defaulting to system. The saved
choice lives in the same portable WebView profile. Changes update color tokens,
native controls, window background and title bar only, never operation state or
storage IPC. Explicit themes ignore system color changes; system mode follows
them. Native window updates are serialized. Missing preference storage permits
session-only use. The main window grants only the additional theme/background
permissions; the existing storage confirmations and write locks are unchanged.

The installation wizard has one primary footer action per step. Connection
preparation/inspection, backup and image preparation advance only after success;
failure or cancellation leaves the current step unchanged. MASKROM upload must
also yield a supported, bound device with verified readiness at the same physical
port; retaining the Maskrom descriptor is allowed. The
step tabs allow image preparation before connecting a board. A ready image or
same-device/port backup can be reused; changing its source or target invalidates
the corresponding result. Advancing to review never starts storage execution.

After a verified storage write or successful Connectivity maintenance, transient
work state is cleared: prepared image, backup/restore selection, readiness/GPT
observation, boot selection, confirmations, maintenance address/token, progress
and wizard position. Remounting the wizard also clears its local source/version.
The completion receipt remains until confirmation or navigation. A completed
standalone advanced backup is dismissed the same way; a wizard backup remains
available for the subsequent installation. Failed operations retain their error
and recovery details. Reset does not call deletion APIs or remove backup files,
downloaded image cache, saved history, language/theme preferences or USB drivers.
Late progress events are ignored when no matching work is active.

Advanced UI exposes storage info, U-Boot update, full/partial backup/restore and GPT
inspection/repair. Write actions prepare a preview and, except for Advanced Restore, a verified recovery backup,
then require typing ok before the explicit write action. Closing the native
window is prevented during storage execution. The UI has no write-cancel action.
Image cancellation remains independent and cannot interrupt a storage write.

Advanced backup/restore starts with a task chooser and shows only the selected
task's controls. Returning to the chooser clears the restore selection. Busy or
pending-write states lock task changes. Completion resets the chooser as well.
Selecting Saved backup within advanced Restore refreshes the catalog automatically,
including after completion reset. The install wizard still refreshes on its restore
entry. Concurrent refreshes are deduplicated and late results after reset are ignored.
Advanced also exposes explicit device preparation without entering the install
wizard. Loader-mode devices are inspected; unverified Maskrom devices use the
existing confirmed prepare command, which reuses a ready loader when possible.
Preparation in Advanced does not advance the install wizard or start backup/write.
Until matching readiness is established, scope selection and non-HAOS wording
are withheld. Reconnection invalidates readiness and requires a fresh check.
Restore descriptions use the engine report's kind (FULL/BOOT/HAOS), not filenames;
legacy reports explicitly identify BOOT. File selection, wizard review and the
write confirmation display scope and data impact. Unknown kinds cannot be planned
from the GUI. Catalog scope metadata is not presented as payload verification;
the existing backend verification before writes remains authoritative.
The advanced catalog displays unselected backups as radio rows with a compact
kind badge and scope note. Invalid/unknown entries are labeled and disabled.
Selecting a row only changes the selection; no verify/backup/write IPC starts.
External Windows file dialogs keep their standard presentation; imported files
receive the same metadata-based row after the existing import checks.

Tests use production loader parsing/upload and backup transaction with fake USB
boundaries; real filesystem hashing/rename is used. Mutation tests remove/reverse
critical guards and must fail. Actual packaged helper, Unicode relocation,
resource tampering, no-device and UI are checked separately. Real board USB
transfers and fresh-Windows driver setup require hardware testing.

## Release components and first-boot state

The fixed repository `installer-catalog.json` is checked for schema, K11C board,
ARM64 architecture, exact raw official-image digest, kernel coverage, app image
digest and boot/data pairing. No semantic-version compatibility guessing. URLs
are limited to this repository's HTTPS Release assets or raw repository files. Downloads and cached files
must match declared length and SHA256. An unavailable network can use a validated
saved/bundled catalog; invalid remote metadata never silently falls back.

The source archive and portable installer contain the pinned USB loader, signed
Rockusb driver, manufacturer tool and boot firmware. They do not contain HAOS or
Connectivity factory-data files. Compilation requires neither of those files.
Catalog schema 2 contains locally verified compatibility profiles and normalized
Supervisor state, not a prebuilt data-partition download URL. Installation fetches
the pinned GHCR manifest/blobs and matching repository commit; a fixed Windows
helper registers them using containerd's metadata/content APIs and builds a new
data-partition copy with pinned Windows e2fsprogs tools. No Docker, WSL, QEMU,
VM, ARM execution or remote CI preparation is required on the customer's PC.
Content hashes, platform/version, kernel coverage, repo config, installed state,
root ownership/modes, filesystem readback and e2fsck are checked before composition.
Native children share a kill-on-close Job Object. Registry cache survives cancellation.

The stock OS partitions remain byte-identical. A factory-only data.ext4 is created
using a profile validated locally against the official Docker/Supervisor; it contains the
digest-pinned published Connectivity image, its normal repository registration,
enabled=true, boot=auto and protected=false. No first-boot shell hook, Core token,
personal configuration or pre-generated app UUID/token is included. OS OTA keeps
the native A/B mechanism. Exact compatibility metadata is required again before
an advanced reinstall. A supported old kernel must actually be present in the
release; backwards compatibility is not assumed for every earlier OS.

Factory app state includes the root-owned 0755 `supervisor/apps/data/<slug>`
directory normally created by Supervisor's install operation. It is empty in
the prepared filesystem. During local profile validation, the matching stock Supervisor must successfully run
its real `App.write_options` with the seeded defaults before packaging; the
probe removes its generated options and checks the empty directory again. Customer
preparation checks that verified state and hash; it does not repeat ARM tests.
Compressed OCI content is registered, not fully unpacked on the PC. HAOS's normal
data growth and Docker unpack prepare the app without an app download or custom
first-boot hook. Local offline Docker create/export verifies that exact path.

Composition produces a managed installation receipt. Storage planning/execution
require that receipt and its verified boot asset. The source and write ranges
are frozen, hashed again and compared with the preview immediately before writes.

## Advanced Connectivity maintenance

`connectivity_plan(address,token,operation)` uses the HA Core Supervisor proxy for
operation `remove` or `reinstall`. It is read-only, checks generic-aarch64 and shows
hostname, HAOS/kernel, installed state and proposed version. The user separately
confirms this is K11C. Reinstall requires the catalog and matching official store
repository/version/architecture. Removal does not require GitHub availability.

`connectivity_execute(confirmed)` consumes a one-use in-memory plan, rechecks the
target and catalog version, creates and checks a partial app backup before any
uninstall, then uses remove_config=true. Reinstall uses fresh defaults rather than
restoring old options, enables the app, sets automatic boot and protection mode,
starts it and checks reported state. The Supervisor install API supplies the store
version, not a caller-provided image. It does not expose an image digest; this path
checks store identity/version, while the factory builder verifies OCI content.
On failure the backup slug remains in the error/recovery record. No automatic
retry/rollback, driver unload, Core/OS restart or other app deletion is performed.

`connectivity_forget()` drops the stored client and preview. Access tokens remain
in process memory, never history/files/URLs; redirects are refused. Changing the
address/token invalidates the UI preview. Navigation and normal window close are
blocked during maintenance execution. Backups remain on HA. The managed target
is the repository app `157e89e9_k11c_connectivity`, not legacy local copies.
