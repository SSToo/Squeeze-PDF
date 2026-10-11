# 发布版本 / Releasing a version

## 简体中文

安装包下载入口：[GitHub Releases](https://github.com/SSToo/Squeeze-PDF/releases)。Windows 选择 `windows-x64.exe`，Apple 芯片 Mac 选择 `macos-arm64.dmg`，Intel Mac 选择 `macos-x64.dmg`。

推送 `v` 开头的版本标签时，`.github/workflows/build.yml` 会执行三平台检查和打包。全部成功后，独立发布任务下载本次构建的产物，收集三个安装包、两个 macOS `.app.tar.gz` 更新包及三份 `.sig` 签名，校验签名及签名中绑定的软件版本，生成更新清单 `latest.json` 和 `SHA256SUMS.txt`，上传并核对 GitHub 返回的 SHA-256，最后公开 Release。新 Release 使用自动生成的发布说明；`-rc.1` 等预发布标签会标为 Pre-release。

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

3. 在 Actions 中查看 **Desktop checks and packages**。最后的 **Publish GitHub Release** 成功后，Releases 页会出现三个安装包、更新包、签名、更新清单和校验文件。也可在 Actions 的 Artifacts 下载原始构建产物。

普通分支推送、Pull Request 和手动构建只检查及打包，不发布 Release。版本标签必须与包内版本一致；构建失败时不执行发布。失败任务可在 Actions 中重试：已上传且校验一致的文件会跳过，缺失文件继续上传；不一致的同名文件会报错，应核查后使用新版本标签。首次发布过程遇到上传或校验失败时保留草稿。已有 Release 的标题、说明和预发布状态保持原样。

此流程适用于包含自动发布配置的新标签，不会移动已有标签或自动补传历史版本。0.1.25 起已接入软件内更新检查，固定读取最新稳定版 Release 的 `latest.json`；预发布版不会进入稳定版自动更新通道。0.1.24 及更早版本需先手动安装一次支持更新的版本。更新签名与操作系统代码签名不同；正式 Windows 签名、macOS Developer ID 与公证状态见 README。

验证发布准备流程时，可在 Actions 手动运行工作流，在 `validate_artifacts_run` 填入一个软件版本相同的成功构建运行 ID。此模式复用已有产物，执行发布测试，收集安装包、更新包、签名、更新清单及校验文件，输出 `release-preview` Artifact，不重新编译或发布 Release。留空则仍执行正常三平台构建。

## English

Download installers from [GitHub Releases](https://github.com/SSToo/Squeeze-PDF/releases): `windows-x64.exe` for Windows, `macos-arm64.dmg` for Apple silicon Macs, and `macos-x64.dmg` for Intel Macs.

Pushing a version tag beginning with `v` runs the three-platform checks and packaging jobs. Once all succeed, a separate job downloads this run's artifacts, collects the three installers, two macOS `.app.tar.gz` update archives and three `.sig` signatures, verifies signatures and their bound app version, generates `latest.json` and `SHA256SUMS.txt`, uploads the files, verifies GitHub's SHA-256 digests, and publishes the release. New releases use generated notes. Tags such as `v0.1.25-rc.1` are marked as prereleases.

Update `package.json`, the workspace version in `Cargo.toml`, and `src-tauri/tauri.conf.json` to the same version. Refresh both lockfiles, check the version and release tests, commit and push, then push the matching tag using the commands above.

Branch pushes, pull requests and manual builds produce checks and artifacts only. A tag/version mismatch or a failed platform build prevents publication. Retry failed jobs in Actions: matching uploaded files are kept and missing files are uploaded. A conflicting asset fails rather than overwriting published bytes. Interrupted uploads and checksum failures leave new releases as drafts. Existing release titles, notes and prerelease settings are preserved.

Only new tags containing this workflow can publish automatically; existing tags and historical releases are not changed. Version 0.1.25 introduces in-app update checks using `latest.json` from the latest stable Release; prereleases are excluded from this stable channel. Version 0.1.24 and earlier require one manual upgrade first. Updater signatures are separate from operating-system code signing; see the README for signing and notarization status.

For a release rehearsal, manually run the workflow with `validate_artifacts_run` set to a successful build's run ID for the same app version. This runs release tests and prepares the existing installers, signed updates, manifest and checksums as a `release-preview` artifact without rebuilding or publishing. Leave the input empty for a normal three-platform build.

## 更新签名 / Update signing

GitHub 仓库的 Actions Secret `TAURI_SIGNING_PRIVATE_KEY` 保存更新签名私钥；`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 为可选密码。主项目已配置私钥 Secret，公钥固定在 `src-tauri/tauri.conf.json`。私钥保存在仓库外，需单独安全备份；不要提交私钥，也不要在后续版本重新生成或替换公钥，否则现有安装无法继续验证更新。

The repository’s Actions secret `TAURI_SIGNING_PRIVATE_KEY` holds the updater private key; `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` is optional. The main project has its signing secret configured. The public key is embedded in `src-tauri/tauri.conf.json`. Keep an independent private-key backup outside Git and retain the same key across releases. Forks must create their own key pair and configure their own repository endpoint before distributing builds.

本地生成带签名的更新包时，将 `TAURI_SIGNING_PRIVATE_KEY` 设为仓库外私钥的路径，再正常构建。开发者没有私钥时，可只打包普通安装文件：

To build signed updates locally, set `TAURI_SIGNING_PRIVATE_KEY` to your private-key path before building. Developers without the key can build ordinary installers with updater artifact generation disabled:

```sh
npm run tauri -- build --config '{"bundle":{"createUpdaterArtifacts":false}}'
```

PR / 分支构建缺少 Secret 时采用此普通打包方式；版本标签发布缺少 Secret 则失败，避免分发无法验证的更新。更新清单不包含当前时间，重试同一批产物可得到相同字节。没有可用稳定版清单或网络不可达时，自动检查安静失败；手动检查会显示可重试提示。

PR and branch builds without the secret disable updater artifact generation. Version-tag builds without a key fail. The manifest omits the current time so retries with identical artifacts remain byte-identical. Missing stable manifests and network failures remain quiet during automatic checks; manual checks show a retryable message.

## 参考 / References

- [GitHub CLI: create a release](https://cli.github.com/manual/gh_release_create)
- [GitHub CLI: upload release assets](https://cli.github.com/manual/gh_release_upload)
- [GitHub release asset digests](https://github.blog/changelog/2025-06-03-releases-now-expose-digests-for-release-assets/)
