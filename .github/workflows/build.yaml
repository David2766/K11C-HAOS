name: K11C Connectivity automatic release
on:
  workflow_dispatch:
    inputs:
      haos_release:
        description: Official HAOS release; empty selects latest stable
        default: ''
        required: false
        type: string
      publish:
        description: Publish tested image and update App catalog
        default: true
        type: boolean
  push:
    branches: [main]
    paths:
      - 'source/connectivity/**'
      - 'k11c_connectivity/app.template.yaml'
      - 'k11c_connectivity/Dockerfile'
      - 'k11c_connectivity/build/**'
      - 'k11c_connectivity/rootfs/**'
      - 'k11c_connectivity/npu-source/**'
      - 'k11c_connectivity/firmware/**'
      - 'k11c_connectivity/*-assets/**'
      - '.github/workflows/build.yaml'
  schedule:
    - cron: '17 3 * * *'
permissions:
  contents: write
  packages: write
concurrency:
  group: k11c-release
  cancel-in-progress: false
jobs:
  build:
    if: github.ref == format('refs/heads/{0}', github.event.repository.default_branch)
    runs-on: ubuntu-24.04
    timeout-minutes: 360
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683
        with:
          persist-credentials: false
      - name: Create isolated workspace
        run: echo "K11C_WORK=$(mktemp -d "$RUNNER_TEMP/k11c-ci.XXXXXX")" >> "$GITHUB_ENV"
      - name: Install planning dependencies
        run: sudo apt-get update && sudo apt-get install -y python3-yaml
      - name: Authenticate registry
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: printf '%s' "$GH_TOKEN" | docker login ghcr.io -u "$GITHUB_ACTOR" --password-stdin
      - name: Verify repository build inputs
        run: |
          sha256sum --check --strict SHA256SUMS
          python3 source/connectivity/ci/test-store-layout.py --repo "$GITHUB_WORKSPACE"
      - name: Select official release
        env:
          REQUESTED_RELEASE: ${{ inputs.haos_release }}
        run: python3 source/connectivity/ci/select-release.py "$REQUESTED_RELEASE" >> "$GITHUB_ENV"
      - name: Plan release or skip identical successful inputs
        id: plan
        run: python3 source/connectivity/ci/release.py plan --target "$HAOS_RELEASE" --plan "$K11C_WORK/plan.json"
      - name: Install build dependencies
        if: steps.plan.outputs.needed == 'true'
        run: |
          sudo apt-get install -y automake bc binutils build-essential bzip2 cpio file flex bison git help2man jq kmod libncurses-dev make patch perl rsync texinfo unzip wget zip
          df -h "$RUNNER_TEMP"
      - name: Free unused SDK space on disposable GitHub runner only
        if: steps.plan.outputs.needed == 'true'
        run: |
          test "$GITHUB_ACTIONS/$RUNNER_ENVIRONMENT/$RUNNER_OS" = true/github-hosted/Linux
          for K11C_SDK in /usr/local/lib/android /usr/share/dotnet; do
            if [ -e "$K11C_SDK" ]; then
              test "$(realpath -e -- "$K11C_SDK")" = "$K11C_SDK"
              sudo rm -rf -- "$K11C_SDK"
            fi
          done
          df -h "$RUNNER_TEMP"
      - uses: docker/setup-qemu-action@c7c53464625b32c7a7e944ae62b3e17d2b600130
        if: steps.plan.outputs.needed == 'true'
        with:
          platforms: arm64
      - uses: docker/setup-buildx-action@8d2750c68a42422c14e847fe6c8ac0403b4cbd6f
        if: steps.plan.outputs.needed == 'true'
      - name: Test release transaction and failure invariants
        if: steps.plan.outputs.needed == 'true'
        run: |
          python3 source/connectivity/ci/test-release.py
          python3 source/connectivity/ci/test-build-cache.py
      - name: Prepare every supported kernel and build tested ARM64 image
        if: steps.plan.outputs.needed == 'true'
        run: |
          K11C_CACHE_IMAGE="ghcr.io/${GITHUB_REPOSITORY,,}-buildcache"
          python3 source/connectivity/ci/pipeline.py --prepare-supported --vendor-tree "$GITHUB_WORKSPACE/source/connectivity/vendor" --output "$K11C_WORK/result" --image "local/k11c-connectivity:ci-$GITHUB_RUN_ID" --release-plan "$K11C_WORK/plan.json" --cache-dir "$K11C_WORK/build-cache" --cache-registry "$K11C_CACHE_IMAGE"
      - name: Publish exact tested image then update catalog
        if: steps.plan.outputs.needed == 'true' && (github.event_name != 'workflow_dispatch' || inputs.publish)
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          K11C_DEFAULT_BRANCH: ${{ github.event.repository.default_branch }}
        run: python3 source/connectivity/ci/release.py publish --plan "$K11C_WORK/plan.json" --result "$K11C_WORK/result"
      - uses: actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02
        if: always()
        with:
          name: k11c-release-${{ github.run_id }}
          path: |
            ${{ env.K11C_WORK }}/plan.json
            ${{ env.K11C_WORK }}/result/result.json
            ${{ env.K11C_WORK }}/result/publication.json
            ${{ env.K11C_WORK }}/result/build.log
            ${{ env.K11C_WORK }}/result/package-manifest.json
          retention-days: 7
          if-no-files-found: warn
      # No Docker image archives, OS image builds, board access or reboots.
