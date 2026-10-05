# Local compatibility validation

This directory contains developer-side compatibility and acceptance tools.
The customer installer uses [Windows-native preparation](../native/README.md),
downloads official HAOS and the pinned GHCR image, and creates its initial data
area on the PC. Only the small compatibility catalog and U-Boot are published
for this workflow; no prebuilt data partition is downloaded.

## Local reference state

`build.py` creates reference state with the stock Docker and Supervisor shipped
for the selected official HAOS image. Run it on Linux with Docker, ARM64 binfmt,
e2fsprogs and root access for loop mounting. This is a local developer check,
not part of the Windows build or customer installation.

```sh
python3 build.py --image /build/haos_generic-aarch64-18.3.img \
  --image-xz /build/haos_generic-aarch64-18.3.img.xz \
  --haos 18.3 --docker-version 29.7.2 --output /build/factory-18.3
```

The output directory must not exist. The official release checksum is fetched
from GitHub. The resolved repository commit supplies RELEASE.json and config.yaml.
The builder fetches the published OCI image by digest, verifies its manifest,
config and every layer, imports it into the stock Docker store and reads the
image filesystem. It uses the stock ARM64 Supervisor's own validation schema.
No customer disk, HA token, account, Bluetooth capture or camera media is used.

Validate reference state for each supported official image. Docker and Supervisor
must match that image, not the developer PC's versions. A Connectivity release
must contain the actual target-kernel modules. `hardware_tested` is false until
the complete install/first-boot path has been accepted on K11C.

## Compatibility catalog

Generate small compatibility metadata from the locally verified state.
`--apps` is the normalized `apps.json` produced with the matching stock
Supervisor; `--config` is the exact public app configuration from the pinned
repository commit.

```sh
python3 catalog.py --seed /build/factory-18.3/seed.json \
  --apps /build/apps-normalized.json --config /build/config.yaml --containerd 2.3.4 \
  --firmware /build/u-boot-k11c-dfi-r24.bin \
  --firmware-manifest /source/boot-release.json \
  --installer /source/installer
```

This writes the installer's `catalog.json`, exported as the repository root's
`installer-catalog.json`. Schema 2 includes the exact tested OS/app/kernel,
normalized Supervisor state/hash and U-Boot hash. It contains no prebuilt
data-partition URL. Only validated combinations belong in the list.

Installer 0.5.0 downloads the official OS, GHCR app content and matching normal
repository checkout and creates the initial data area using native Windows
utilities. No `.ext4.xz` release asset, remote CI seed
generation, customer Docker/WSL or ARM emulation is used. Existing users still
update HAOS and the app normally. New combinations are verified locally before
adding them to the compatibility list.

Export the catalog and its referenced U-Boot together. The image digest and
repository commit must already be public. Check the public URLs and a clean-cache
installation after publication; see [release setup](../../docs/RELEASE.md#installer-구성요소-배포).

## Acceptance

1. Validate the generated `apps.json` with the bundled Supervisor schema; no
   shared generated credentials or customer state.
   Create the root-owned 0755 `apps/data/157e89e9_k11c_connectivity` directory.
   Run the stock Supervisor's actual `App.write_options` against it, then remove
   the generated options and verify that factory app data is empty. Catalog
   staging requires both checks. Run `sudo python3 test_state.py -v` on Linux
   for layout, ownership, mode, empty-state and symlink regression tests.
   `python3 test_catalog.py -v` checks the production staging gate. Both test
   programs accept `--mutation` to remove/reverse their critical conditions.
2. Use `installation-import` and the production simulated eMMC transaction test
   to compare the seven stock OS partitions, seeded data, U-Boot and both GPTs.
3. Boot a test K11C with wired network: initial expansion, onboarding, Connectivity
   installed/auto/enabled, real modules loaded, clean GPT and official OTA.
4. Exercise advanced backup/removal/reinstall on a test installation. Confirm the
   old app data is removed and the partial backup remains recoverable.
