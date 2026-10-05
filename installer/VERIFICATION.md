# Verification — 0.5.0

## Windows-native preparation — 2026-10-05

The customer installation path uses a schema-2 compatibility profile and the
Windows helper in `native/`. QEMU, ARM execution, Docker, WSL and prebuilt data
partition downloads are not customer dependencies. The earlier seed/QEMU results
below describe the superseded preparation path and developer validation history.

The actual `actual_windows_native_production_path` integration test passed with
HAOS 18.3, kernel 6.18.52-haos, Connectivity 0.5.6 and r24 firmware, including a
Unicode portable directory. It used the real Windows helper, GHCR content,
filesystem utilities, production composition and installation receipt validation.
Only the catalog response was supplied locally, because the new schema-2 catalog
has not been published. The official input remained unchanged; all seven stock
OS partition payloads matched byte-for-byte and both composed GPTs validated.

`test-results/native-production/portable-unicode-시험/native-production-result.json`
records 28.659 seconds for native data preparation and 41.558 seconds for the
production preparation/composition/verification operation. The old QEMU data
preparation measured 518.721 seconds. These are local measurements, not download
or board-flash times. The native run reused verified registry blobs and fetched
the repository commit normally. It registers compressed content; standard Docker
unpack is performed on first boot rather than on the customer's PC.

`seed/check_native.py` independently accepted the final composed image with Linux
e2fsck, normal first-boot expansion, offline Docker 29.7.2 and stock Supervisor
2026.09.2. All eight official image references remained, Connectivity was present,
offline create/export unpacked the kernel module, Git fsck and main/origin tracking
passed, the factory state matched stock normalization, and real App.write_options
succeeded. Root ownership, directory/file modes, empty app data and absence of
customer identity were checked. This is developer-only acceptance on disposable
files; `native-offline-acceptance.json` records the result. No physical board
startup or OTA acceptance is inferred.

The full release-profile engine and GUI suites passed. Fifteen production
component/maintenance guard mutations and eleven native guard mutations were
rejected, followed by restored-source baseline passes. They cover pinned image,
OS/kernel pairing, source/content hashes, verified profile, state hash, enabled
flag, platform, GC references and executable integrity. Source-archive tests check
that pinned native inputs survive Git checkout with exact bytes. The modern
e2fsprogs source patch also passed a dry-run against its original release archive.

## Standalone source archive and download-on-install packaging

Git export includes nine original pinned build-input files: the RAM loader,
U-Boot and manifest, three signed Rockusb files, and the manufacturer executable,
configuration and revision notice, plus the pinned native filesystem utilities,
their runtime DLLs and manifest. No HAOS images, Connectivity factory data,
portable release, private data or build cache is required to compile.

Build-input tests exercise default source-relative resolution and alternate
resource roots, missing files, changed bytes and the firmware manifest. Seven
removed/reversed guard mutations must fail. Export tests additionally verify
byte-exact Git tracking/checkout, input integrity before destination writes,
repeatability and preservation of unrelated files and the public app catalog.

The Windows production build runs release-profile engine and native-host tests.
The 0.5.0 source-ZIP build passed 111 engine tests and six native-host tests,
then produced both EXEs and the portable ZIP without OS/app installation data.
The isolated exporter suite passed 12 tests and killed 13 guard mutations;
the seven build-input mutations also failed their intended contracts.
Packaging invokes the actual compiled helper at its normal and relocated paths,
checks resource digests and version consistency, rejects unauthorized dispatch
and single-partition imports, and requires no bundled app/OS data. Verification
does not require a preinstalled Rockusb driver; an existing driver's INF is
checked for changes. Installation retains the existing catalog compatibility,
download, composition and write/readback guards.

## Manufacturer image or saved backup restore

Advanced Restore now requires a source choice. Manufacturer RKFW packages use
the pinned Rockchip `upgrade_tool` 2.46; complete RAW disk images use the existing
bounded write/readback engine. Saved backups retain their existing restore path.
Neither advanced path creates an additional backup. Single-partition images are
rejected before a plan is available.

Release-profile engine tests: 103 passed, four opt-in tests excluded from the
default run; native host tests: six passed. The two local K11C manufacturer
Android packages additionally passed the opt-in read-only SFI, source rehash,
PARM checksum and complete sparse-expansion bounds test. Linux RKFW metadata and
complete GPT/MBR RAW transactions are covered by fixtures, not a Linux board boot.

All 12 manufacturer-path guard mutations fail their intended tests, followed by
a passing restored baseline. They remove/reverse confirmation, source binding,
sparse bounds, payload readback, PARM CRC and single-device selection. Production
Vue tests kill all 106 UI mutations. Browser checks cover 240 views across five
languages, light/dark themes and two widths, including both restore sources.

Reports: `test-results/factory-mutations.json`, `test-results/ui-mutations.json`
and `test-results/factory-source-*.json`. Run the optional real-package test with
`K11C_TEST_FACTORY_IMAGE` set, using `cargo test --release --locked -p k11c-usb
--lib actual_manufacturer_package -- --ignored --nocapture`.

Physical manufacturer installation remains an acceptance test: confirmed UF,
same-port re-enumeration, independent partition/GPT readback and OS boot. Vendor
Loader transforms are checked by the vendor tool; sparse DONT_CARE regions are
not compared as payload. No physical USB writes occur in the automated tests.

## Transfer pipeline

The production restore engine uses the same bounded pipeline for RAW, XZ and
Zstandard sources, with sequential USB commands and three reusable 1-MiB
buffers. Tests prove read-ahead while the first transfer buffer is held,
buffer-count bounds, ordered hashing, exact partial final blocks, worker
shutdown, source errors/panics, and locked source handles through readback.

Virtual eMMC tests check every written and untouched byte, readback only after
the complete range has been written, corruption of an earlier block during the
last write, short reads/writes, disconnection, one-use plans, and no completion
event or successful journal record after failure. RAW preflight progress is
strictly increasing through one complete pass against the original import hash.

Mutation commands:

```text
node scripts/mutation-pipeline.mjs
node scripts/mutation-flash.mjs --pipeline-only
node scripts/mutation-archive.mjs --pipeline-only
```

All 13 targeted mutations fail their corresponding tests, followed by passing
restored baselines. The checks include removed/reversed comparisons, skipped
hash updates, reordered blocks, a weakened buffer bound and ignored short writes.

`pipeline_speed_report` is an opt-in release test using real files/codecs and
the production transaction, with 10-ms/block simulated USB latency. Its report
is `test-results/pipeline-speed.json`. On the development PC, the 36-MiB write
loop changed from 0.406 to 0.376 seconds for RAW, 0.445 to 0.378 for XZ, and
0.402 to 0.377 for Zstandard; readback changed from 0.393-0.396 to 0.375-0.376
seconds. These are single-run simulated-transport results, not board speeds.
Actual transactions record stage times and MiB/s in their existing journals
and operation history for physical-device comparison.

## Advanced restore and persistent catalog

Advanced restore uses a dedicated no-new-backup plan path. Production transaction
tests restore RAW, FULL, BOOT, HAOS and legacy boot sources into virtual eMMC,
compare selected and unselected bytes, and require no additional backup files.
The wizard's recovery reuse and installation backup tests remain enabled.
Tests cover wrong capacity/port, changed target boot bytes, changed source,
missing confirmation, restricted read access, failed writes, full readback and
one-use execution. Direct-restore plans retain a target boot/GPT fingerprint;
this is not a recovery copy of the current disk.

Catalog tests use existing path-only RAW receipts without a managed backup
directory. Missing sources are disabled; listing checks only bounded metadata
and restoration rejects payload changes against the original imported digest.
Production Vue tests cover automatic refresh on Restore entry and after completion
reset, imported filenames, refresh deduplication, direct-restore IPC selection,
the no-additional-backup notice and unchanged wizard backup behavior.

Targeted mutation commands: `node scripts/mutation-flash.mjs --direct-only`,
`node scripts/mutation-archive.mjs --raw-only`, and
`node scripts/test-ui.mjs --mutation`. Release-profile tests exercise the same
optimized engine used by the portable build. These tests write temporary PC
fixtures, not the connected board.

## Backup compression path

New backups use the production Zstandard level-1 writer with two background
workers and bounded job/window sizes. Existing format-3 XZ and raw imports retain
their readers. Virtual eMMC tests exercise creation, external archive import,
write planning, actual restore/readback and FULL/BOOT/HAOS range preservation.
They also cover disconnection/short reads after compression jobs have been
queued: no device writes or published backup, and only a non-restorable partial
file remains. Codec tests reject a mismatched format signature, truncation,
trailing frames/bytes, oversized decoder windows and wrong input lengths.

The opt-in `archive_speed_report` test uses a synthetic 64-MiB mixture of zero,
structured and pseudorandom bytes. Three local release-mode trials measured
8.27–8.69 seconds for the previous XZ level-1 write/hash path versus 0.073–0.088
seconds for the new production read/hash/compress path. New full create,
including simulated-device reread and real stored-file verification, took
0.20–0.23 seconds. These are PC codec measurements, not physical USB/eMMC rates.
Results: `test-results/archive-speed.json`. The same original bytes and SHA256
were verified in every trial; no filesystem blocks are skipped.

Successful backups retain the three stage timings in `data/operations.jsonl`.
Physical full-media backup/restore acceptance remains separate from these tests.

`scripts/mutation-archive.mjs` covers metadata, payload, framing, restore scope,
capacity, source identity and recovery reuse. Its Zstandard window-limit
mutation raises the cap from 27 to 28:
deleting the explicit setting alone leaves the library's identical default in
place and therefore does not weaken the guard.

## Backup task selection and restore scope

The advanced panel starts with task selection and renders one task form at a
time. Production Vue tests cover task switching, busy/pending-plan locks,
completion reset, stale handlers, and rejected file selections. The shared
restore summary reads FULL/BOOT/HAOS from parsed reports, including legacy BOOT;
misleading filenames cannot change its description. Selection, wizard review
and final write confirmation are covered in all five display languages.

Unselected catalog rows show metadata-based badges and localized scope notes.
Unknown/invalid rows are disabled; row handlers also reject stale events while
busy or while a write plan is pending. Row selection makes no IPC calls.

Advanced now has a direct preparation action. Production component tests cover
no-device/unbound-device rejection, hot attachment, confirmation cancellation,
failed preparation, duplicate-event locking, a ready Maskrom descriptor,
disconnect invalidation, Loader inspection, wrong-port rejection and HAOS versus
non-HAOS scope choices. Preparation never starts a backup/write or advances the
hidden installation wizard. Uninspected disks show neither a default scope nor
the non-HAOS restriction message.

`node scripts/test-ui.mjs --mutation` passes 95 removal/reversal cases, including
six Advanced-readiness and 20 task/scope/list regressions. Existing confirmations, target identity, backup reuse
and backend commands remain covered. `scripts/test-backup-browser.mjs` runs the
production web UI with an isolated fake IPC boundary: 200 views across five
locales, both themes and two viewport sizes, without USB access. Screenshots and
layout results are in `test-results/backup-ui`. The browser enters Advanced with
an attached Loader, checks the unprepared state, and uses its own preparation
action rather than relying on the separate storage-information card. Real-device restoration remains
a separate hardware acceptance test.

## Full archives and completion state

New GUI commands run `archive::create`, `flash::plan_backed` and the existing
`flash::execute` write/readback engine. Virtual eMMC tests use those production
functions and real compressed files. They cover FULL unknown-OS/raw roundtrips,
BOOT/HAOS range-exact restoration, current-data comparison, recovery backup
reuse, capacity/layout rejection, damaged metadata/payload, and read capability.
The FULL result is checked byte-for-byte across the entire simulated device.
Mutation guards: `scripts/mutation-archive.mjs`.
The complete build test run passes 76 engine tests and four native-shell tests.
The official-image fixture remains opt-in; the performance test runs separately.
The release-mode archive subset passes 10 correctness tests. Four codec guards,
eight existing archive guards and two
reuse regressions were killed; restored baselines passed. Reopening a cancelled preview reuses an
unchanged pending plan, and selecting a managed archive creates no duplicate.

The production Vue component test covers five steps, FULL-only wizard backups,
raw import, partial-backup rejection in the wizard, Advanced range choices,
`ok` confirmation, backup reuse and completion reset. Five-language/theme state
and failure paths remain covered. `scripts/test-ui.mjs --mutation` removes or
reverses the critical UI conditions; its report records actual outcomes.

Physical full-media HAOS/Android roundtrip and interruption recovery still require
a separately authorized board test. Existing boot-only hardware tests do not
establish those results. No claim of RPMB/OTP or encrypted Android data recovery.

## USB full-read correction

The previous connected backup's repeated tail reads were identical 0xCC data,
not verified disk contents. Any older report of successful hardware backup must
not be treated as full-read verification. Those files are retained as evidence
and are rejected by the updated catalog/restore/transaction checks.

The correction checks actual ReadCapability before storage access, rejects
all-0xCC GPT responses separately from GPT damage, and records capability
evidence in format-2 backups. Healthy format-1 backups remain compatible;
legacy damaged/ambiguous backups require a new capture.

`loader::tests` checks the production RAM-only patch and uploaded payload hash.
`scripts/test-ram-loader.py` executes the original and patched ARM initialization,
eMMC capability handler and both read-selection paths, with hardware calls
stubbed. It confirms the original 32-MiB restriction and full-range selection
through the actual disk's final LBA for the modified code. It is not physical
USB upload or write verification.

`node scripts/mutation.mjs --read-access-only` exercises removal/inversion of
the patch, capability gates, sentinel checks, saved-backup evidence and transport
access guard. The normal engine suite covers all four storage operations.

Hardware acceptance: fresh physical MASKROM, RAM preparation, full-read capability,
consistent reads around LBA 65536, valid primary/tail GPT and a newly verified
backup. Follow with a separately authorized write/readback test before customer
release. No physical write is part of the offline test suite.

## Production storage path

The native GUI and current CLI use `flash::plan_backed` and `flash::execute`.
The legacy CLI retains `flash::plan` for old boot-only backup compatibility.
Tests replace only `FlashIo`/`UsbIo`, using real source files, source sharing locks,
hashing, backup manifests, one-use plans and durable journals.

- Install preserves every prepared partition payload byte, partition GUID, type, label and
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

The opt-in `actual_official_image_full_transaction` test composes the real official
HAOS image, factory data and r24 firmware through the production transaction, with
an ordinary temporary PC file as eMMC. It independently checks seven unchanged OS
partitions, the factory data hash, identities, relocation and the physical-end GPT.
Set `K11C_TEST_OFFICIAL_IMAGE` to the official raw image and `K11C_TEST_COMPONENTS`
to the directory containing the catalog and assets:

```
cargo test --release --locked -p k11c-usb --lib actual_official_image_full_transaction -- --ignored --nocapture
```

This file-backed test is not a physical USB or board boot test.

Executed against HAOS 18.3 with Connectivity 0.5.6: PASS, seven unchanged official
partition hashes, matching factory data, 3,028 bounded writes and target capacity
6,198,993 sectors. Composed image SHA256:
`276454114bba866f5ca8bb9450551092b2b8332eebd75a83c13b1ffdc9583d13`.
This digest records the tested output; it is not a compiled HAOS version pin.
Report: `test-results/actual-factory-install.txt`.

## Driver, RAM loader, backup and image preparation

The full engine suite also covers the existing production paths:

- Existing-driver reuse; signed/digest-checked installation, UAC cancellation and
  post-install proof. Driver installation success is not USB binding proof.
- Selected USB instance and physical port, RK356x Rockusb binding, verified
  command readiness independently of descriptor mode, eMMC capacity;
  pinned RAM loader parsing, 471/471/472 ordering and exact upload lengths.
- Two-pass USB boot/GPT backup and saved-file SHA256/length verification;
  short reads, disconnect, corruption, identity changes and incomplete output.
- Official release metadata and digest retrieval, allowed HTTPS hosts, cache
  rehash, bounded download, raw/XZ import, cancellation and GPT/ARM64 EFI validation.

The offline suite includes the opt-in official-image test.
Existing mutation suites cover driver/USB/backup conditions, image conditions
(14), and native image-operation locking (1). Their reports remain under
`test-results`; new storage and UI mutations are run for this release.

## UI and portable package

`scripts/test-ui.mjs` compiles and mounts the actual Vue App and InstallWizard.
Only native IPC is substituted; real state, slots and handlers execute.

- One slide and one primary footer action at a time; navigation preserves source
  choice and current stage. Device preparation/inspection, backup and image
  preparation advance only on success. Failure, picker cancellation and cancelled
  download stay on the same step. Completed image/backup reuse does not repeat work.
- MASKROM preparation requires explicit confirmation and a same-port readiness
  observation. A ready device may retain its Maskrom descriptor and still advance.
  Failed upload, failed scan, wrong port, missing readiness and mismatched observation
  cannot advance. Already-ready devices are inspected without another upload.
- Advanced backup, GPT and writing require a matching observation, including when
  enumeration says Loader. Backend readiness and identity are checked again at
  execution. Ready-Maskrom virtual storage transactions retain confirmation,
  eMMC-only, backup and exact-write-range conditions.
- Changing image source/version clears the old image; changing device/port/mode/
  binding invalidates the old backup. Advanced backup does not advance the wizard.
- First entry into the official-image page fetches metadata automatically,
  deduplicates/reuses the list, and provides explicit retry on failure.
- Image selection alone is not readiness; cancellation/failure clears selection.
- All five advanced cards remain visible with real action handlers.
- Install requires an image and readable target. Restore requires a matching,
  verified managed backup. GPT repair requires a repairable check result.
- Planning does not write. The final modal identifies target, effects, exact
  extents and recovery backup. Storage execution requires typing `ok`.
- Cancellation, changed physical port and unconfirmed direct handler calls do
  not execute. USB work locks navigation. A failed write is not a success.
- The native window refuses ordinary close requests while storage execution is
  active. Abrupt process termination or PC power loss can still interrupt a write.

The USB driver refresh button has a scoped 20px gap below its disclosure in both
collapsed and expanded states. The mounted production card and loaded stylesheet
are checked across all five languages; removing/reversing the gap, removing its
scope or restricting it to expanded details causes test failures.

The UI suite kills 68 removed/reversed production-condition mutations and verifies
restored source identity. Reports: `test-results/ui-verification.json` and
`test-results/ui-mutations.json`.

## Display languages

The production Vue templates and shared translation module support English,
Korean, Traditional Chinese only, Spanish and Japanese. The same mounted App is
switched through all five languages with ready images, pending operations and
confirmed write/Connectivity previews. Device, image, version, plan, confirmation
and busy state remain intact; switching performs no IPC. Native picker calls
carry the selected locale. Technical error details, paths and identifiers remain
unaltered. Presentation never substitutes for storage authorization.

`scripts/test-i18n.mjs` checks all five-column messages, UTF-8, unique keys,
placeholder parity, template coverage, Traditional-only Chinese, system language
selection, saved preference, unavailable storage and date/number formatting.
Ten removed/reversed language conditions all fail their assertions. Six of the
The UI mutations also cover language wiring and state preservation.
The native picker has fixed-label and unsupported-locale tests; it cannot accept
arbitrary dialog text or change allowed image types.

Earlier browser checks covered the UI in all five languages at the 1180×840 default
window size, and Spanish at the 980×680 minimum size. Longer content scrolls
vertically without horizontal overflow. Reload preserves the language choice.
Native Windows file-dialog controls still follow the Windows display language;
the installer supplies its translated title and file-type label.

`scripts/verify.mjs` tests the actual packaged Windows helper, existing Rockusb
driver detection, denied dispatch/unconfirmed write, absent selection, fixed
resource digests and relocation under a Unicode path. It performs no USB writes.
`scripts/package.mjs` excludes local data, cached images and WebView profiles.

Earlier native Windows 0.4.0 acceptance: the existing Rockusb 5.13 driver was reused;
all four advanced cards rendered without an error; opening image selection
automatically fetched the real official release list with HAOS 18.3 selected.
No USB device was connected. Browser and native screenshot/accessibility checks
confirmed the layout; confirmed-write dialog state is covered by production Vue tests.
The subsequently added Connectivity panel is covered by the production Vue tests;
its native visual and real HA connection acceptance remain pending.

## Appearance

Light, dark and system themes share semantic color tokens. Text contrast checks
cover primary, secondary, accent, success, warning and error text in both themes;
body controls use 14px and captions at least 12px. Buttons retain distinct hover,
pressed, focused and disabled states. Reduced-motion and Windows forced-color
preferences are respected. Driver-details spacing remains 20px.

`scripts/test-theme.mjs --mutation` executes the production theme modules with
browser/native-window boundaries substituted. All 13 mutations fail: persisted
preference, validation, OS changes, explicit-mode isolation, DOM/native controls,
serialized title-bar updates, recovery after a failed native call and cleanup.
The mounted App suite additionally tests actual theme controls at each workflow
state and kills three mutations that disconnect the control or discard state.

`scripts/test-appearance-browser.mjs` checks the actual browser preview at 1180x840
and 980x680 in all five languages and both themes: 143 view checks, no horizontal
overflow, captions >=12px, theme persistence, system changes, visible keyboard
focus and collapsed/expanded disclosure spacing. Screenshots and the report are
under `test-results/appearance`. The browser preview cannot access USB.
Those appearance screenshots precede the five-step backup/restore revision.
The current revision has mounted-Vue regression coverage; its native visual
acceptance remains pending. The optional test uses playwright-core;
K11C_PLAYWRIGHT may name its module entry.

The UI, favicon and Windows ICO use `public/app-icon.svg`. The build renders and
verifies identical artwork at 16, 20, 24, 32, 40, 48, 64, 128 and 256px. Icon
rendering is build-time only; no new runtime or installer dependency is shipped.
Native title-bar theme permissions and calls are checked by tests and the Windows
build. Visual confirmation of the title bar remains pending: the Windows UI
automation service was unavailable during this appearance pass.

## Remaining physical acceptance

1. Cold MASKROM upload followed by readiness detection. Already-running RAM Loader
   reuse, eMMC queries and two-pass backup passed on a connected K11C (see below).
2. U-Boot-only update and backup restore: readback PASS followed by a real boot.
3. Fresh official-image installation: initial boot/data expansion, clean GPT,
   HAOS generic-aarch64 operation, seeded Connectivity startup and a subsequent
   official OTA update. Advanced Connectivity reinstall/remove against a running
   board also needs acceptance; mocked API tests are not an actual HA connection.
4. GPT repair on a deliberately prepared test disk; not on customer data.
5. Fresh Windows driver installation/UAC and WebView2 availability. Native local
   file-picker selection to completion also remains a manual acceptance item;
   the shared raw/XZ import engine is covered separately.

## USB readiness correction — 2026-09-30

The connected RK356x RAM USB plug retained its `Maskrom` descriptor while
TestUnitReady, chip info and eMMC queries succeeded. Descriptor-only gating was
replaced with a bounded read-only readiness probe throughout the storage entry
points. UI readiness also requires a matching device/port observation.

- Engine: 57 passing tests, one opt-in image test not run in this pass; native: 3.
- USB/readiness guard mutations: 15 killed; restored baseline passes.
- Production Vue UI mutations: 58 killed.
- Packaged CLI on the connected board: `inspect`, `prepare` with `uploaded:false`,
  two-pass boot/GPT backup and repeat `inspect` all passed. Capacity: 61,071,360
  sectors; USB descriptor remained Maskrom. No eMMC writes or RAM upload were made.
- The physical check used a helper directory without firmware/Loader resources,
  preventing accidental retransmission. Identity and header hashes matched before
  and after. The backup's existing GPT issues (`backup_header`, `entries_crc`,
  `copies_differ`) were reported, not repaired.

Report: `test-results/connected-readonly-Xv5Oa3/report.json`. Re-run explicitly with
`node scripts/verify-connected-readonly.mjs` when the intended board is already
ready and is the only connected Rockchip device. This is a read-only hardware test,
not physical installation/write acceptance.

Boot-area backup is not an HA data backup. Restore refuses changed partition
entries or a disk with neither GPT copy usable. Do not interrupt a real write to
exercise failure tests; those fault cases are injected at the test transport.
## Repository build inputs

The source exporter includes the installer source/lockfiles/notices/tests and
nine small build-input files. Default resolution uses source-relative resources;
K11C_RELEASE_INPUTS is an optional override, not a portable-ZIP dependency.
Build tools use PATH or explicit K11C_BUILD_TOOLS. The production build requires
no HAOS image, factory-data catalog or data package. Tests mutate the input and
manifest checks and verify source ZIP coverage and Git ignore rules.

## Release-resolved components and seeded data

The production composition test verifies the exact official image digest,
version-matched data, byte-identical OS partitions, GPT identity and changed-file
rejection. The opt-in `actual_official_image_full_transaction` additionally uses
the real official image, released U-Boot and generated data package, with only
USB replaced by a temporary disk file. Set K11C_TEST_OFFICIAL_IMAGE and
K11C_TEST_COMPONENTS; run with `--ignored --nocapture`.

`scripts/mutation-components.mjs` removes/reverses 15 conditions: exact image,
firmware pairing/hash, OS preservation, downloaded hash, kernel coverage,
confirmation, backup contents, clean removal, auto boot, enabled state and
post-start verification. Both affected test groups pass after source restoration.

The Linux factory build uses the stock ARM64 Supervisor to validate the real
preinstalled app state and executes a read-only command in the digest-pinned
Connectivity image. All OCI manifests/config/layers are hashed before import.
`seed/test_state.py` rejects absent/reversed auto-start flags and shared identity.
No full ARM board boot or RF/NPU operation is inferred from these checks.

The actual composed HAOS 18.3 image also booted under QEMU AArch64/UEFI, through
GRUB slot A, stock Linux 6.18.52-haos and filesystem checks to the Home Assistant
login prompt. The test used snapshot mode and a time limit. It does not establish
Supervisor app auto-start, K11C peripheral operation or physical flash acceptance.
Factory data builds for both 18.2 and 18.3 passed their matching stock Supervisor
schemas and Docker image checks; both include Connectivity 0.5.6.
The packaged Windows `installation-import` command prepared both real official
versions with r24 and the matching data package. Reports are
`test-results/packaged-factory-import.json` (18.3) and
`test-results/packaged-factory-18.2-import.json` (18.2).

## Factory app data directory regression

The first physical installation exposed a missing
`supervisor/apps/data/157e89e9_k11c_connectivity` directory. Supervisor rejected
its options write before creating the app container. JSON schema validation
and OS boot alone did not detect this missing install-time side effect.

`seed/build.py` now prepares the root-owned 0755 directory and executes the
matching stock Supervisor's `App.write_options` against the factory filesystem.
The probe uses the real option schema and atomic JSON writer; only secrets
reload and executor scheduling are substitutes. It removes its generated file,
and the builder checks that no customer options or generated identity remain.
This is not a replacement for the physical app/driver startup test.

Linux regression commands (root is required for ownership tests):

```sh
sudo python3 seed/test_state.py -v
sudo python3 seed/test_state.py --mutation
python3 seed/test_catalog.py -v
python3 seed/test_catalog.py --mutation
```

Mutation cases remove directory creation, change the target path, remove/reverse
permissions checks, remove ownership/empty-state checks, and follow symlinks.
Catalog generation refuses seeds without the layout and Supervisor-write checks;
four additional mutations remove/reverse those two publication conditions.

## Manufacturer grow-partition regression — 2026-10-04

The physical Android installation completed the vendor UF operation and every
non-DONT_CARE payload comparison. Its final layout check incorrectly required
userdata:grow to use the entire GPT usable range. Captured primary/backup GPT
copies were healthy and all 14 fixed partitions matched the package. The vendor
left 31 sectors before the backup GPT area, with the exclusive userdata end
aligned to 64 sectors.

The production readback path now accepts a final grow partition ending at full
capacity or the nearest lower 64-sector boundary. Names, starts, fixed ends,
partition count, GPT copies/CRCs and all payload comparisons remain checked.
The entire raw/sparse extent, including DONT_CARE, is revalidated against the
actual partition size. Arbitrary shortening and a shortened GPT usable-limit
header cannot bypass the physical-capacity rule.

Regression tests exercise the full production readback path with bounded fake
USB storage, including exact-fit and one-sector-overflow raw/sparse inputs.
The opt-in captured_manufacturer_gpt_matches_package_with_grow_alignment test
uses the actual package and physical GPT captures as PC files, without USB I/O.
Its report is test-results/factory-layout-regression.json. Set
K11C_TEST_FACTORY_IMAGE and K11C_TEST_FACTORY_GPT_DIR and run that test with
--ignored. The old exact-end condition fails the new alignment regression.
scripts/mutation-factory.mjs removes/reverses the relevant checks; --debug runs
those same production modules without release optimization. Release-profile
regressions and the packaged-helper checks remain separate.

## Repeated restore transaction regression

An advanced restore of the same source to the same device state produced the
same content-derived plan ID as a previously completed transaction. The consumed
plan correctly prevented replay, but the planner incorrectly treated a new,
explicitly confirmed restore as that replay. The source backup itself was valid.

The production planner now advances an internal attempt counter past consumed
IDs. Attempt zero retains the original format-1 serialization and ID. Reopening
an unchanged, unconsumed preview reuses its ID; changed pending-plan bytes are
rejected. Consumed plans and their journals remain unchanged. Advanced restore
still does not create an additional recovery backup.

The repeated_restore tests run the production planner and transaction against
bounded fake USB storage, with real files, hashes and archive decompression.
They cover repeated RAW/FULL/BOOT restores, failed writes, wizard recovery reuse,
confirmation, physical selection/capacity, loader restrictions, stale target,
source and plan tampering, and corrupt full-disk readback. Planning must perform
no writes, selected ranges must match, and unselected bytes must remain intact.

The ignored captured_consumed_restore_plan_can_be_renewed_without_changing_history
test reads actual consumed plans and journals from K11C_TEST_USED_PLAN_DIR. It
copies their small metadata into a temporary test directory, verifies the old
IDs and serialization, renews the plans through production save_plan, and checks
that both original and copied history are byte-identical. Its report is
test-results/restore-plan-renewal.json. It performs no physical USB I/O.

scripts/mutation-flash.mjs --debug --renewal-only removes or reverses thirteen
critical renewal and transaction conditions. Every mutation must fail a
regression test, followed by a passing baseline after source restoration.
The normal build additionally runs the complete release-profile helper suite,
native GUI tests and packaged-helper integrity checks.
