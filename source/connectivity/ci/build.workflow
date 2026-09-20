name: K11C local-equivalent candidate build
on:
  workflow_dispatch:
    inputs:
      haos_release:
        description: Official HAOS release; empty selects latest stable
        default: ''
        required: false
        type: string
  schedule:
    - cron: '17 3 * * *'
permissions:
  contents: read
concurrency:
  group: k11c-candidate
  cancel-in-progress: false
jobs:
  build:
    runs-on: ubuntu-24.04
    timeout-minutes: 360
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683
        with:
          persist-credentials: false
      - name: Install build dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y automake bc binutils build-essential bzip2 cpio file flex bison git help2man jq kmod libncurses-dev make patch perl python3 python3-yaml rsync texinfo unzip wget zip
          df -h "$RUNNER_TEMP"
      - uses: docker/setup-qemu-action@c7c53464625b32c7a7e944ae62b3e17d2b600130
        with:
          platforms: arm64
      - uses: docker/setup-buildx-action@8d2750c68a42422c14e847fe6c8ac0403b4cbd6f
      - name: Create isolated workspace
        run: echo "K11C_WORK=$(mktemp -d "$RUNNER_TEMP/k11c-ci.XXXXXX")" >> "$GITHUB_ENV"
      - name: Select official release
        env:
          REQUESTED_RELEASE: ${{ inputs.haos_release }}
        run: |
          python3 source/connectivity/ci/select-release.py "$REQUESTED_RELEASE" >> "$GITHUB_ENV"
      - name: Verify repository build inputs
        run: |
          sha256sum --check --strict SHA256SUMS
      - name: Prepare rollback and target kernels
        run: |
          bash source/connectivity/ci/prepare-haos.sh 18.2 "$K11C_WORK/haos-18.2"
          if [ "$HAOS_RELEASE" != 18.2 ]; then
            bash source/connectivity/ci/prepare-haos.sh "$HAOS_RELEASE" "$K11C_WORK/haos-$HAOS_RELEASE"
          fi
      - name: Build all modules and test actual ARM64 App image
        run: |
          args=(--haos-tree "$K11C_WORK/haos-18.2")
          if [ "$HAOS_RELEASE" != 18.2 ]; then args+=(--haos-tree "$K11C_WORK/haos-$HAOS_RELEASE"); fi
          python3 source/connectivity/ci/pipeline.py "${args[@]}" --vendor-tree "$GITHUB_WORKSPACE/source/connectivity/vendor" --output "$K11C_WORK/result" --image "local/k11c-connectivity:ci-$GITHUB_RUN_ID"
      - uses: actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02
        if: always()
        with:
          name: k11c-candidate-${{ github.run_id }}
          path: |
            ${{ env.K11C_WORK }}/result/result.json
            ${{ env.K11C_WORK }}/result/build.log
            ${{ env.K11C_WORK }}/result/package-manifest.json
          retention-days: 7
          if-no-files-found: warn
      # Deliberately no push, git commit, board access, OS update or reboot.
      # Keep only reports in Actions; optional image archives are local-only.
      # Candidates require board acceptance and an explicit release operation.
