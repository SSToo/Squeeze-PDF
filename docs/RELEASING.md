# Releasing a version

Download installers from [GitHub Releases](https://github.com/SSToo/Squeeze-PDF/releases): `windows-x64.exe` for Windows, `macos-arm64.dmg` for Apple silicon Macs, and `macos-x64.dmg` for Intel Macs.

Pushing a version tag beginning with `v` runs the three-platform checks and packaging jobs. Once all succeed, a separate job downloads this run's artifacts, collects the three installers, two macOS `.app.tar.gz` update archives and three `.sig` signatures, verifies signatures and their bound app version, generates `latest.json` and `SHA256SUMS.txt`, uploads the files, verifies GitHub's SHA-256 digests, and publishes the release. New releases use generated notes. Tags such as `v0.1.25-rc.1` are marked as prereleases.

1. Update `package.json`, the workspace version in `Cargo.toml`, and `src-tauri/tauri.conf.json` to the same version, for example `0.1.25`. Refresh both lockfiles and check the release configuration:

   ```sh
   npm install --package-lock-only --ignore-scripts
   cargo check -p squeeze-cli
   node scripts/release.mjs check-version v0.1.25
   node --test scripts/test-release.mjs
   ```

2. Commit and push the version changes, then create and push the matching tag:

   ```sh
   git add package.json package-lock.json Cargo.toml Cargo.lock src-tauri/tauri.conf.json
   git commit -m "Release v0.1.25"
   git push origin main
   git tag v0.1.25
   git push origin v0.1.25
   ```

3. Open **Desktop checks and packages** in Actions. After **Publish GitHub Release** succeeds, the Releases page contains the three installers, update archives, signatures, manifest and checksum file. Raw build outputs are also available under the run's Artifacts.

Branch pushes, pull requests and manual builds produce checks and artifacts only. A tag/version mismatch or a failed platform build prevents publication. Retry failed jobs in Actions: matching uploaded files are kept and missing files are uploaded. A conflicting asset fails rather than overwriting published bytes. Interrupted uploads and checksum failures leave new releases as drafts. Existing release titles, notes and prerelease settings are preserved.

Only new tags containing this workflow can publish automatically; existing tags and historical releases are not changed. Version 0.1.25 introduces in-app update checks using `latest.json` from the latest stable Release; prereleases are excluded from this stable channel. Version 0.1.24 and earlier require one manual upgrade first. Updater signatures are separate from operating-system code signing; see the README for signing and notarization status.

For a release rehearsal, manually run the workflow with `validate_artifacts_run` set to a successful build's run ID for the same app version. This runs release tests and prepares the existing installers, signed updates, manifest and checksums as a `release-preview` artifact without rebuilding or publishing. Leave the input empty for a normal three-platform build.

## Update signing

The repository's Actions secret `TAURI_SIGNING_PRIVATE_KEY` holds the updater private key; `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` is optional. The main project has its signing secret configured. The public key is embedded in `src-tauri/tauri.conf.json`. Keep an independent private-key backup outside Git. Do not commit the private key or regenerate/replace the public key for subsequent releases: existing installations need the same key to verify updates. Forks must create their own key pair and configure their own repository endpoint before distributing builds.

To build signed updates locally, set `TAURI_SIGNING_PRIVATE_KEY` to your private-key path before building. Developers without the key can build ordinary installers with updater artifact generation disabled:

```sh
npm run tauri -- build --config '{"bundle":{"createUpdaterArtifacts":false}}'
```

PR and branch builds without the secret disable updater artifact generation. Version-tag builds without a key fail rather than distributing unverifiable updates. The manifest omits the current time so retries with identical artifacts remain byte-identical. Missing stable manifests and network failures remain quiet during automatic checks; manual checks show a retryable message.

## References

- [GitHub CLI: create a release](https://cli.github.com/manual/gh_release_create)
- [GitHub CLI: upload release assets](https://cli.github.com/manual/gh_release_upload)
- [GitHub release asset digests](https://github.blog/changelog/2025-06-03-releases-now-expose-digests-for-release-assets/)
