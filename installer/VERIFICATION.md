# Verification — 0.4.0

## Production storage path

The CLI and native GUI call the same `flash::plan` and `flash::execute` functions.
Tests replace only `FlashIo`/`UsbIo`, using real source files, source sharing locks,
hashing, backup manifests, one-use plans and durable journals.

- Install preserves every partition payload byte, partition GUID, type, label and
  attributes while moving starts/ends by 32768 sectors. The physical-end backup
  GPT, primary GPT, PMBR capacity, firmware and cleared old image-tail GPT are checked.
- U-Boot updates change only the firmware extent. Boot restoration reproduces
  the saved boot/GPT bytes; it cannot restore OS or user-data partitions.
- GPT repair reconciles equal partition arrays with mismatched disk GUIDs and
  rebuilds a damaged header from its valid peer. A healthy pair causes no write.
  Different valid partition arrays, unfamiliar layouts and no valid copy are rejected.
- Missing confirmation, changed device/port/capacity, stale disk data, modified
  plan/source/backup/firmware, out-of-bounds or overlapping ranges stop before writing.
- Short writes, disconnects, readback corruption and post-write identity changes
  cannot publish success. Failed plans are consumed and retain their journal.
- Source handles deny concurrent file writes/deletion. Every written extent is
  reread in full and hashed; the final GPT pair is validated again.

`scripts/mutation-flash.mjs` removes or reverses 17 production conditions.
All 17 mutations cause test assertion failures; the restored baseline passes.
Report: `test-results/flash-mutations.json`.

The opt-in `actual_official_image_full_transaction` test uses the real official
HAOS image and bundled r24 firmware through the production transaction, with an
ordinary temporary PC file as eMMC. It independently hashes all eight source and
target partitions and checks identities, relocation and the physical-end GPT.
Run with `K11C_TEST_OFFICIAL_IMAGE` pointing to an already prepared official image:

```
cargo test --locked -p k11c-usb --lib actual_official_image_full_transaction -- --ignored --nocapture
```

This file-backed test is not a physical USB or board boot test.

Executed against HAOS 18.3: PASS, eight identical partition hashes, 922 bounded
writes and target capacity 1,884,200 sectors. Prepared source SHA256:
`65f6f62ce0c906e7f9d2172a2764cc9ea11c870027de9b443612504d4faa57f2`.
This digest records the tested input; it is not a production HAOS version pin.

## Driver, RAM loader, backup and image preparation

The full engine suite also covers the existing production paths:

- Existing-driver reuse; signed/digest-checked installation, UAC cancellation and
  post-install proof. Driver installation success is not USB binding proof.
- Selected USB instance and physical port, RK356x Loader binding, eMMC capacity;
  pinned RAM loader parsing, 471/471/472 ordering and exact upload lengths.
- Two-pass USB boot/GPT backup and saved-file SHA256/length verification;
  short reads, disconnect, corruption, identity changes and incomplete output.
- Official release metadata and digest retrieval, allowed HTTPS hosts, cache
  rehash, bounded download, raw/XZ import, cancellation and GPT/ARM64 EFI validation.

The offline suite has 40 engine tests plus the opt-in official-image test.
Existing mutation suites cover driver/USB/backup conditions (22), image conditions
(14), and native image-operation locking (1). Their reports remain under
`test-results`; new storage and UI mutations are run for this release.

## UI and portable package

`scripts/test-ui.mjs` compiles and mounts the actual Vue App and InstallWizard.
Only native IPC is substituted; real state, slots and handlers execute.

- One slide and one primary footer action at a time; navigation preserves source
  choice and current stage. Device preparation/inspection, backup and image
  preparation advance only on success. Failure, picker cancellation and cancelled
  download stay on the same step. Completed image/backup reuse does not repeat work.
- MASKROM preparation requires the existing explicit confirmation, then successful
  same-port Loader re-enumeration. Failed upload, failed scan, wrong port and still-
  MASKROM results cannot advance. An already-Loader device is inspected, not uploaded.
- Changing image source/version clears the old image; changing device/port/mode/
  binding invalidates the old backup. Advanced backup does not advance the wizard.
- First entry into the official-image page fetches metadata automatically,
  deduplicates/reuses the list, and provides explicit retry on failure.
- Image selection alone is not readiness; cancellation/failure clears selection.
- All four advanced cards remain visible with real action handlers.
- Install requires an image and readable target. Restore requires a matching,
  verified managed backup. GPT repair requires a repairable check result.
- Planning does not write. The final modal identifies target, effects, exact
  extents and automatic backup. Execution requires typing K11C.
- Cancellation, changed physical port and unconfirmed direct handler calls do
  not execute. USB work locks navigation. A failed write is not a success.
- The native window refuses ordinary close requests while storage execution is
  active. Abrupt process termination or PC power loss can still interrupt a write.

The UI suite kills 39 removed/reversed production-condition mutations and verifies
restored source identity. Reports: `test-results/ui-verification.json` and
`test-results/ui-mutations.json`.

`scripts/verify.mjs` tests the actual packaged Windows helper, existing Rockusb
driver detection, denied dispatch/unconfirmed write, absent selection, fixed
resource digests and relocation under a Unicode path. It performs no USB writes.
`scripts/package.mjs` excludes local data, cached images and WebView profiles.

Native Windows 0.4.0 acceptance: the existing Rockusb 5.13 driver was reused;
all four advanced cards rendered without an error; opening image selection
automatically fetched the real official release list with HAOS 18.3 selected.
No USB device was connected. Browser and native screenshot/accessibility checks
confirmed the layout; confirmed-write dialog state is covered by production Vue tests.

## Remaining physical acceptance

1. On a physical K11C: MASKROM upload, same-port Loader reconnection, eMMC query
   and two-pass backup using the released executable.
2. U-Boot-only update and backup restore: readback PASS followed by a real boot.
3. Fresh official-image installation: initial boot/data expansion, clean GPT,
   HAOS generic-aarch64 operation and a subsequent official OTA update.
4. GPT repair on a deliberately prepared test disk; not on customer data.
5. Fresh Windows driver installation/UAC and WebView2 availability. Native local
   file-picker selection to completion also remains a manual acceptance item;
   the shared raw/XZ import engine is covered separately.

Boot-area backup is not an HA data backup. Restore refuses changed partition
entries or a disk with neither GPT copy usable. Do not interrupt a real write to
exercise failure tests; those fault cases are injected at the test transport.
# Repository build inputs

The source exporter includes only the installer source/lockfiles/notices and
tests. Runtime input files come from `K11C_RELEASE_INPUTS` (a released ZIP's
resources directory), verified against the engine's compiled loader, firmware
and Rockusb digests. Build tools use PATH or explicit K11C_BUILD_TOOLS; no
developer SDK or artifact path is required. `test-build-inputs.mjs` exercises
all five real inputs, missing/tampered files and wrong firmware metadata, then
removes/reverses the checks to prove the contract rejects those mutations.
