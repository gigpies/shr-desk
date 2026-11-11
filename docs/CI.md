# Continuous integration

[The CI workflow](../.github/workflows/ci.yml) owns the existing offline checks.

## Failure and recovery notifications

[CI alerts](../.github/workflows/ci-alerts.yml) opens one GitHub issue when CI
fails on the current `main` commit, mentions `PaolaShultz`, and keeps repeated
failures in that thread. A passing run for the current commit posts **Recovered**,
renames and closes the issue. Ordinary successes and already-fixed historical
failures create no alerts. Cancelled or pending runs never count as recovery.

Email delivery follows GitHub's participating/@mention notification settings.
Native Actions emails remain a separate account preference; disabling those
avoids duplicate failure messages while retaining the incident thread.
The notifier uses a pinned shared action with `actions: read`, `contents: read`
and `issues: write`; it never runs the triggering code or accesses hardware.
See the [shared notification contract](https://github.com/gigpies/gigpies/blob/main/docs/development/CI_ALERTS.md)
for retry behavior, tests and manual reconciliation. Existing CI checks are unchanged.

## Dependency version monitoring

The weekly [Cargo inventory](../.github/workflows/cargo-versions.yml) records
locked and latest stable crates.io versions in its run summary. It reads the
application and frozen vendor manifests, including optional/target dependencies;
it does not alter manifests, resolve upgrades, or open hardware. Agents review
available versions against upstream migration notes and the pinned toolchain,
then validate accepted updates through the normal ARM64/x86 PR gates.

Dependabot still proposes GitHub Actions updates. Its Cargo version job is
replaced because its parser contacts `shr-panel.invalid` before invoking Cargo
and cannot use this project's directory source replacement. Ignoring the panel
dependency does not avoid that parser failure. The frozen package, exact version,
Cargo checksum verification and source ownership remain unchanged. Restore Cargo
Dependabot when upstream can parse and resolve directory-replaced registries;
verify a real update job before removing the inventory. GitHub security alerts
remain enabled; this direct-dependency version report is not a vulnerability scan.

Run the same read-only inventory with
`python3 scripts/check_dependency_versions.py`. Registry/network failures fail
the job instead of claiming the dependencies are current.

The transport pins rustls 0.23.46, including its session-secret Debug, ECH,
key-exchange hint and certificate-decompression security fixes. Ring/std feature
selection and the existing mutual-TLS transport contract remain unchanged.
