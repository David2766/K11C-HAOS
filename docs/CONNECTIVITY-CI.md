# Connectivity CI in the existing K11C-HAOS repository

The repository keeps ONE App source directory, `k11c_connectivity/`.
Build scripts, patches and the pinned SeekWave inputs are under
`source/connectivity/`; `.github/workflows/build.yaml` runs the candidate CI.
Firmware, model/runtime and FFmpeg inputs are included with their notices.
No external input archive URL, secret, or separate source-export repository is
needed. Large ROM images, kernel build trees, prebuilt `.ko`, camera videos,
debug captures and Docker image archives are not copied into Git.

## Copy from the development workspace

Run the existing `scripts/export-repository.sh` using Bash/WSL. It updates only
the selected Connectivity/CI files in the existing K11C-HAOS checkout. It never
builds, commits, pushes, deletes unrelated files, modifies `.git`, or replaces
the manually maintained README, repository metadata, board/boot sources or
existing public App catalog. `.gitignore` and `.gitattributes` retain their
existing rules plus a delimited block for the selected inputs. SHA256SUMS is
updated while retaining coverage of previously listed files.

The local Windows invocation and the fixed checkout path are documented in
the development workspace's `ci/LOCAL-VALIDATION.ko.md`. After copying, review
the Git diff and commit/push yourself. The exporter does not do that for you.

## Candidate versus public App version

`k11c_connectivity/config.template.yaml` is the candidate configuration copied
from the development App. `config.yaml` remains the current public catalog.
CI replaces the catalog with the candidate configuration ONLY inside its
isolated build directory; it does not advertise an unbuilt update to HA.
The candidate remains version 0.5.3 for current local testing. Before publishing
a new release, choose a new version, rebuild/test, publish its versioned image,
and THEN update the public `config.yaml` to that version and GHCR image path.
Image tag, `io.hass.version`, and catalog version must agree. This candidate
workflow does NOT yet publish to GHCR or implement automatic catalog updates.

## Local build from this repository (WSL / Linux)

Prerequisites: a configured HAOS Generic AArch64 kernel/toolchain, Python 3.12
with PyYAML, GNU build tools, jq, kmod, Docker, and ARM64 binfmt support. From
the repository root, using your existing HAOS tree and a NEW output directory:

```sh
python3 source/connectivity/ci/pipeline.py \
  --haos-tree /path/to/haos-18.2 \
  --vendor-tree "$PWD/source/connectivity/vendor" \
  --output /path/to/NEW-build \
  --image local/k11c-connectivity:ci-test
```

Repeat `--haos-tree` for another supported kernel. All five external modules
are rebuilt for each requested kernel; NPU and OTP must both be present. The
candidate does not reuse unrebuilt `.ko` files from the checkout. Only a passing
run writes `result.json` with `LOCAL_CI_PASS`. This means build/software tests
passed, not Wi-Fi/BLE/NPU hardware acceptance. Installed ARM64 image hashes and
private Python tests run with no board devices or extra capabilities.

The image remains in your local Docker daemon. Optional LOCAL archive:

```sh
K11C_RESULT_DIR=/path/to/NEW-build
test ! -e "$K11C_RESULT_DIR/image.tar" &&
  docker image save --output "$K11C_RESULT_DIR/image.tar" \
    "$(jq -er '.image_id' "$K11C_RESULT_DIR/result.json")"
```

This uses the tested immutable image ID. Existing archives/images are not
automatically deleted. `ci/inputs.py` is retained only as a local offline pack/
unpack utility, not as a required GitHub input service.

## GitHub execution

Push the reviewed files to the existing repository's default branch. Actions
supports a manual HAOS version or the latest stable release, plus a daily UTC
03:17 schedule. It verifies repository inputs, prepares the rollback kernel
(initially HAOS 18.2) and selected target kernel, builds and tests the ARM64 App.
There is no pull-request build or automatic OS update. Identical scheduled
versions are not yet deduplicated.

The standard `ubuntu-24.04` runner installs build tools and ARM64 emulation.
Fresh two-kernel build duration/disk limits on GitHub are not yet verified.
`prepare-haos.sh` builds only the kernel/toolchain, never an OS/ROM image.
GitHub does NOT create/upload `image.tar`; only `result.json`, `build.log` and
`package-manifest.json` are retained for seven days. The CI image is disposable;
the workflow has read-only repository permissions and does not publish it.

Firmware publication was explicitly requested by the distributor; existing
mixed-license notices remain. Publication of driver source is not a claim of
an independently verified firmware redistribution license.

HAOS updates must still follow installation of the matching kernel bundle.
This workflow does not intercept stock HAOS update buttons or reboot boards.
