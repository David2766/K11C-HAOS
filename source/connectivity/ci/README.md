# Connectivity automatic build and release

One App source remains at `k11c_connectivity/`. The existing exporter copies
reviewed development files into the existing K11C-HAOS checkout; it does not
build, commit, push, delete unrelated files, or overwrite the public catalog.
No second repository or external-input hosting is required.

## Automatic path

`.github/workflows/build.yaml` runs on relevant source pushes to main, manual
dispatch, and daily at 03:17 UTC (11:17 Hong Kong). It:

1. Resolves official stable HAOS tags to commits and hashes the actual source inputs.
2. Skips inputs already published successfully. Includes HAOS 18.2 plus every
   previously published supported release and the new target (no silent retirement).
3. Builds the official Generic AArch64 kernel/toolchain and all five external
   modules for each distinct kernel ABI; never builds/flashes an OS or YAML ROM.
   CI prepares kernels sequentially, retains the built modules/provenance, then
   removes only that newly created temporary kernel tree before the next one.
   User-supplied local build trees are never removed. This limits runner disk use.
   The disposable GitHub Linux runner also removes its unused preinstalled
   Android/.NET SDKs to make room for the full configured kernel. This cleanup
   is guarded to GitHub-hosted runners and is not run on a local PC or board.
4. Builds an ARM64 App with an automatically incremented three-part version.
   Tests the actual installed image and each included kernel bundle.
5. Pushes that exact tested image ID to
   `ghcr.io/<owner>/<repository>-connectivity:<version>` (lowercase).
6. Pulls it without credentials and verifies image identity. Only then commits
   public `config.yaml`, `RELEASE.json`, `RELEASES.md` and `SHA256SUMS`.

Version selection starts above both the source candidate and public App version.
Orphan image tags from a failed prior publication are skipped, never overwritten.
Repository commits are non-force pushes. Concurrent user commits/branch protection
can stop catalog publication but cannot be overwritten. A failed build, push or
anonymous pull does not advertise an update. Next scheduled/manual run replans.

The image is in GHCR; Actions keeps only small plan/result/manifest/log reports
for seven days. No GitHub image.tar. Local Docker images can still be archived
manually using `docker image save` with the tested image ID from result.json.

## First deployment (owner, once)

After local verification, use the EXISTING exporter. In PC PowerShell:

```powershell
cd C:\repos\K11C-HAOS
git status --short
git pull --ff-only
wsl -d Ubuntu-24.04 -e bash /mnt/c/rom/HAOS/K11C-port/scripts/export-repository.sh
if ($LASTEXITCODE -ne 0) { throw 'Export failed' }
git diff --stat
git add -- k11c_connectivity source/connectivity .github/workflows/build.yaml docs/CONNECTIVITY-CI.md .gitignore .gitattributes SHA256SUMS EXPORT-MANIFEST.txt CONNECTIVITY-EXPORT.json
git diff --cached --stat
git commit -m "Enable Connectivity automatic releases"
git push origin main
```

Review/resolve any existing local work before pull; do not reset it. On future
exports, pull the bot's catalog commit first. The exporter preserves that catalog
and release state. The initial push triggers the workflow automatically.

Repository Settings -> Actions must permit Actions and the workflow's
`contents: write` / `packages: write`. No PAT is required for ordinary publishing;
the workflow uses its scoped GITHUB_TOKEN. Protected main rules must allow the
release bot's catalog commit, otherwise publication stops without force pushing.

IMPORTANT: GitHub creates new container packages PRIVATE by default. After the
first image push, open the package settings and set its visibility to Public.
For David2766/K11C-HAOS the package is `k11c-haos-connectivity`. The first run may
stop at anonymous pull until this is done; the old catalog stays unchanged.
Then Actions -> K11C Connectivity automatic release -> Run workflow, with
`publish` enabled. A new free version is selected automatically. This is one-time
GHCR setup, not a manual gate on each future HAOS release.

Manual `publish=false` performs the same build/tests without publishing. The
regular scheduled/source-push path publishes automatically after software tests.
Only the default branch can publish; PRs have no publishing workflow.

## Users and existing local installations

Users add `https://github.com/David2766/K11C-HAOS` to the HA App store, install
the repository Connectivity App, and enable its Auto update toggle if desired.
Repository registration alone does not enable automatic installation.

`local_k11c_connectivity` is a DIFFERENT installation identity. For the first
migration, back up HA and privately copy its options (including API key) using
the UI; install the repository App but keep it stopped, stop/disable autostart
on the local App, transfer options and required protection settings, then start
the repository App. Do not run both controllers together. Verify Wi-Fi/Bluetooth,
NPU/native readiness as applicable before removing the old App. The new App may
need to register its own existing Frigate-file manifest; identical owned payload
bytes are reused. Never print the API key in shared diagnostics. Repository-App
updates subsequently retain Supervisor-managed options and data.

Update Connectivity BEFORE installing a newly supported HAOS. App auto-update
does not coordinate/intercept HAOS update buttons, force-reload modules, reboot,
or repair a broken kernel API automatically. A firmware/ABI/source incompatibility
stops CI and needs a source fix. Already loaded different module builds can require
a planned reboot; the loader will not unload a shared live driver.

## Verification and boundaries

`result.json` proves compilation/software tests, not board acceptance.
`publication.json` exists only after verified registry push/pull AND catalog push.
`hardware_tested` remains false unless a separate real-board acceptance is done.
No always-on board test runner, new HA service, updater daemon or added App
permission is required. Current App options, model, clocks and driver behavior
are not changed by the release automation.

Local build uses `ci/pipeline.py --haos-tree ... --vendor-tree ... --output NEWDIR
--image local/k11c-connectivity:test`, optionally `--release-plan plan.json`.
Use repeated --haos-tree for supported HAOS releases. `prepare-haos.sh RELEASE
DIRECTORY` is resumable for that same official tag and uses directory-local
download/compiler caches, not privileged /cache. Build only on Linux/WSL.

`test-release.py` exercises publication logic and real disposable Git remotes;
fault mutations must fail tests. Its optional integration mode uses an actual
pipeline image, a loopback-only registry and a local bare Git remote. It never
publishes to GitHub. See development `LOCAL-VALIDATION.ko.md` for recorded results.

All selected vendor inputs and license notices are included. This does not claim
that publication of driver source independently establishes firmware licensing.
