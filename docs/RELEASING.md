# Building and releasing PolarEffects

The release workflow builds five targets from one pinned commit. A manual
run with **release_tag left blank** uploads workflow artifacts only. A `v*`
tag, or a manual run naming an existing unpublished tag, creates or resumes
a draft beta and publishes it only after all selected builds and the
all-platform installer check pass. It refuses to replace a published release.

## Validate first

Use the toolchain and prerequisites in [CLAUDE.md](../CLAUDE.md). From the root:

```sh
CARGO_INCREMENTAL=0 cargo fmt --all --check
CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets -- -D warnings
CARGO_INCREMENTAL=0 cargo test --workspace
npm run ui:typecheck
npm run ui:test
npm run ui:perf
npm run ui:build
npm run check:offline
npm run tools:test
node tools/check-release.mjs
node --test tools/configure-macos-signing.test.mjs
python3 -m unittest discover -s tools -p 'test_release*.py'
npm run ux
```

The Windows frontend CI job also exercises certificate import with a temporary
self-signed certificate. It requires no release secrets and does not contact a
timestamp service. Real issuer signing and Apple notarization require credentials
and cannot be verified by those fixture tests.

## First builds and artifacts

Connect the checkout to the repository, push the working branch, then dispatch
**Release builds**, choosing that branch, `platform: all`, and no release tag.
CLI equivalent, for this project's build branch:

```sh
gh workflow run release.yml --ref build/v1 -f platform=all
gh run list --workflow release.yml
gh run watch RUN_ID --exit-status
gh run download RUN_ID --dir target/release-downloads
```

The workflow must exist on the repository's default branch for GitHub to accept
a dispatch. Do not create a release tag just to test bundling: tags trigger
publication. Individual-platform dispatches are useful for packaging failures.
For a tagged build, the installer guard still requires the complete set already
present in the draft before publishing.

| Target | Runner | Installers |
| --- | --- | --- |
| Intel macOS | `macos-15-intel` | DMG and app archive |
| Apple Silicon macOS | `macos-15` | DMG and app archive |
| Windows x64 | `windows-2022` | MSI and NSIS setup |
| Windows ARM64 | x64 cross-build on `windows-2022` | NSIS setup (WiX does not build ARM64 MSI) |
| Linux x64 | `ubuntu-22.04` | AppImage, Debian and RPM |

The user guide, data notices and ORC MIT notice are resources in every bundle.
Artifact-only runs also upload them separately. Tagged releases include
`PolarEffects-documentation.zip` and `SHA256SUMS` covering the installers, app
archives and documentation archive. The installer check rejects missing, empty
and mixed-version payloads.

## Optional macOS signing and notarization

Without secrets, the workflow explicitly uses ad-hoc signing (`-`). That is a
local/test build, with no Developer ID or notarization ticket. Configure these
GitHub Actions repository secrets for Developer ID signing:

| Secret | Value |
| --- | --- |
| `APPLE_CERTIFICATE` | Base64-encoded Developer ID Application certificate and private key exported as a `.p12` |
| `APPLE_CERTIFICATE_PASSWORD` | Export password; may be empty for an unencrypted export |
| `APPLE_SIGNING_IDENTITY` | Full Developer ID Application identity matching the certificate |
| `APPLE_ID` | Apple account used for notarization |
| `APPLE_PASSWORD` | App-specific password, not the account password |
| `APPLE_TEAM_ID` | Developer team identifier |

The last three must be supplied together. A certificate without a signing
identity, orphaned credentials, or a partial notarization configuration fails
before bundling. Certificate plus identity alone signs but does not notarize.
Tauri imports the certificate and submits the app for notarization; the workflow
verifies the app signature and, when requested, its stapled ticket. The DMG
contains that app and has its own signature checked when a certificate is used.
No credentials are placed in repository files or artifacts.

See [Tauri's macOS signing instructions](https://v2.tauri.app/distribute/sign/macos/).
To make a local ad-hoc Intel Mac package:

```sh
CARGO_INCREMENTAL=0 APPLE_SIGNING_IDENTITY=- npm run build -- --bundles app,dmg -- --locked
codesign --verify --deep --strict target/release/bundle/macos/PolarEffects.app
hdiutil verify target/release/bundle/dmg/PolarEffects_0.1.0_x64.dmg
```

## Optional Windows signing

Without secrets, Windows installers are unsigned. For an exportable PFX
certificate, configure these repository secrets:

| Secret | Value |
| --- | --- |
| `WINDOWS_CERTIFICATE` | Base64-encoded PFX containing the code-signing certificate and private key |
| `WINDOWS_CERTIFICATE_PASSWORD` | PFX password; may be empty |
| `WINDOWS_TIMESTAMP_URL` | Issuer's HTTP(S) RFC 3161 timestamp endpoint |

The runner imports the PFX into the current user's certificate store, selects
the code-signing certificate and writes a temporary Tauri configuration with
its thumbprint, SHA-256 digest and timestamp service. The decoded PFX is removed
even if import fails. After bundling, Authenticode checks require valid
signatures, the expected certificate and timestamps on the executable and all
installers. Hosted runners discard their certificate store after the job.

Certificates whose private keys must remain in a hardware token or managed
signing service need a provider-specific Tauri `signCommand` integration. The
PFX path does not implement those providers. See
[Tauri's Windows signing instructions](https://v2.tauri.app/distribute/sign/windows/).

## Publish a beta

1. Complete an artifact-only build and install/open its packages on the target
   systems. Windows ARM64 tests are cross-compiled, so that laptop needs a real
   launch check. Record the reference-machine performance checks from the plan
   and the native-speaker translation review separately from CI.
2. Confirm signing status and release readiness. Signing-ready configuration
   without credentials does not mean the artifacts are signed or notarized.
3. Set the same version in the workspace `Cargo.toml`, package manifests,
   lockfiles and Tauri configuration. Run `node tools/check-release.mjs`.
4. Commit and push, then create and push the matching `vVERSION` tag when
   publication is intended. The workflow prepares a draft beta, builds and
   verifies the packages, uploads checksums and publishes the beta.
5. If a build fails, repair the failure before retrying. The manual `release_tag`
   input rebuilds the existing tag's commit, not newer branch changes. A source
   fix needs a new version/tag; do not move a published tag.

Production builds use the default Rust features. Never enable the development
WebDriver endpoint in distributed packages. Packaging, CI success and human
launch/signing review are separate evidence; record which actually happened.
