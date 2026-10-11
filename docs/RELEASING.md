# 发布版本 / Releasing a version

## 简体中文

安装包下载入口：[GitHub Releases](https://github.com/SSToo/Squeeze-PDF/releases)。Windows 选择 `windows-x64.exe`，Apple 芯片 Mac 选择 `macos-arm64.dmg`，Intel Mac 选择 `macos-x64.dmg`。

推送 `v` 开头的版本标签时，`.github/workflows/build.yml` 会执行三平台检查和打包。全部成功后，独立发布任务下载本次构建的产物，收集三个安装包，生成 `SHA256SUMS.txt`，上传并核对 GitHub 返回的 SHA-256，最后公开 Release。新 Release 使用自动生成的发布说明；`-rc.1` 等预发布标签会标为 Pre-release。

1. 将 `package.json`、`Cargo.toml` 的 workspace 版本和 `src-tauri/tauri.conf.json` 更新为同一个版本，例如 `0.1.25`。同步 npm 和 Cargo 锁文件：

   ```sh
   npm install --package-lock-only --ignore-scripts
   cargo check -p squeeze-cli
   node scripts/release.mjs check-version v0.1.25
   node --test scripts/test-release.mjs
   ```

2. 提交版本更改并推送，然后创建对应标签：

   ```sh
   git add package.json package-lock.json Cargo.toml Cargo.lock src-tauri/tauri.conf.json
   git commit -m "Release v0.1.25"
   git push origin main
   git tag v0.1.25
   git push origin v0.1.25
   ```

3. 在 Actions 中查看 **Desktop checks and packages**。最后的 **Publish GitHub Release** 成功后，Releases 页会出现三个安装包和校验文件。也可在 Actions 的 Artifacts 下载原始构建产物。

普通分支推送、Pull Request 和手动构建只检查及打包，不发布 Release。版本标签必须与包内版本一致；构建失败时不执行发布。失败任务可在 Actions 中重试：已上传且校验一致的文件会跳过，缺失文件继续上传；不一致的同名文件会报错，应核查后使用新版本标签。首次发布过程遇到上传或校验失败时保留草稿。已有 Release 的标题、说明和预发布状态保持原样。

此流程适用于包含自动发布配置的新标签，不会移动已有标签或自动补传历史版本。当前 `v0.1.24` 已有 Release。软件内尚未接入自动检查更新；公开分发的签名与公证配置见 README。

验证发布准备流程时，可在 Actions 手动运行工作流，在 `validate_artifacts_run` 填入一个软件版本相同的成功构建运行 ID。此模式复用已有产物，执行发布测试，收集三个安装包及校验文件，输出 `release-preview` Artifact，不重新编译或发布 Release。留空则仍执行正常三平台构建。

## English

Download installers from [GitHub Releases](https://github.com/SSToo/Squeeze-PDF/releases): `windows-x64.exe` for Windows, `macos-arm64.dmg` for Apple silicon Macs, and `macos-x64.dmg` for Intel Macs.

Pushing a version tag beginning with `v` runs the three-platform checks and packaging jobs. Once all succeed, a separate job downloads this run's artifacts, collects the three installers, generates `SHA256SUMS.txt`, uploads the files, verifies GitHub's SHA-256 digests, and publishes the release. New releases use generated notes. Tags such as `v0.1.25-rc.1` are marked as prereleases.

Update `package.json`, the workspace version in `Cargo.toml`, and `src-tauri/tauri.conf.json` to the same version. Refresh both lockfiles, check the version and release tests, commit and push, then push the matching tag using the commands above.

Branch pushes, pull requests and manual builds produce checks and artifacts only. A tag/version mismatch or a failed platform build prevents publication. Retry failed jobs in Actions: matching uploaded files are kept and missing files are uploaded. A conflicting asset fails rather than overwriting published bytes. Interrupted uploads and checksum failures leave new releases as drafts. Existing release titles, notes and prerelease settings are preserved.

Only new tags containing this workflow can publish automatically; existing tags and historical releases are not changed. In-app update checks are not implemented. See the README for signing and notarization status.

For a release rehearsal, manually run the workflow with `validate_artifacts_run` set to a successful build's run ID for the same app version. This runs release tests and prepares the existing installers and checksums as a `release-preview` artifact without rebuilding or publishing. Leave the input empty for a normal three-platform build.

## 参考 / References

- [GitHub CLI: create a release](https://cli.github.com/manual/gh_release_create)
- [GitHub CLI: upload release assets](https://cli.github.com/manual/gh_release_upload)
- [GitHub release asset digests](https://github.blog/changelog/2025-06-03-releases-now-expose-digests-for-release-assets/)
