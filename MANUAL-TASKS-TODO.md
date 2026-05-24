# Manual Tasks TODO

Tasks that require human action — accounts, secrets, external submissions, or UI clicks that cannot be automated by the AI agent.

## Priority 1 — Blocking (pipelines won't work without these)

### ~~GitHub Actions: 0-step failures~~

~~All workflows (CI, Release, Docker, CodeQL, Pages) were failing with 0 steps executed due to exhausted GitHub Actions minutes for private repo (3,000 min/month).~~

- [x] Verified "Allow all actions" is selected
- [x] No required workflow restrictions or rulesets blocking runners
- [x] Confirmed issue is NOT YAML syntax (minimal `echo` workflow also fails)
- [x] GitHub Status is all-green (not an incident)
- [x] Public repos on the same account work fine (confirms private-repo-specific issue)
- [x] Fixed branch protection MSRV check name from "MSRV (1.85)" to "MSRV (1.88)"
- [x] **Resolved**: temporarily made repo public, all pipelines passed, repo made private again
- [x] Added workaround to `.github/copilot-instructions.md` for future occurrences
- Closed [#92](https://github.com/egkristi/RavenFabric/issues/92)

### ~~Add `PUBLISH_BIN_TAP_TOKEN` secret~~

~~The `publish-binaries` job in `release.yml` needs a PAT to push to `egkristi/RavenFabric-Published`.~~

- [x] Created Fine-grained PAT with Contents read & write on `egkristi/RavenFabric-Published`
- [x] Added as secret `PUBLISH_BIN_TAP_TOKEN`

### Add `CRATES_IO_TOKEN` secret

The `publish-crates` job in `release.yml` needs a crates.io API token.

- [ ] Log in to <https://crates.io> with your GitHub account
- [ ] Go to **Account Settings > API Tokens > New Token**
  - Scopes: publish-update, publish-new
- [ ] Add it as a secret: **Settings > Secrets and variables > Actions > New repository secret**
  - Name: `CRATES_IO_TOKEN`
- Closes [#44](https://github.com/egkristi/RavenFabric/issues/44)

### Enforce HTTPS on get.ravenfabric.io

The SSL certificate is not yet provisioned by GitHub Pages.

- [ ] Go to <https://github.com/egkristi/get-ravenfabric/settings/pages>
- [ ] Under "Custom domain", remove `get.ravenfabric.io`, save, re-add it, save (this forces cert re-provisioning)
- [ ] Wait ~15 min, then check the "Enforce HTTPS" checkbox
- [ ] Verify: `curl -sI https://get.ravenfabric.io | head -5`

---

## Priority 2 — External submissions (do when ready for public launch)

### Submit to HSTS Preload list

- [ ] Visit <https://hstspreload.org>
- [ ] Enter `ravenfabric.io` and submit
- Requires: HTTPS enforced on ravenfabric.io, `Strict-Transport-Security` header with `preload` directive
- Closes [#85](https://github.com/egkristi/RavenFabric/issues/85)

### Submit sitemap to Google Search Console

- [ ] Go to <https://search.google.com/search-console>
- [ ] Add property `ravenfabric.io` (DNS TXT verification)
- [ ] Submit sitemap URL: `https://ravenfabric.io/sitemap.xml`
- Closes [#38](https://github.com/egkristi/RavenFabric/issues/38)

### Submit packaging manifests to package stores

- [ ] **Snap Store**: `snapcraft login && snapcraft upload --release=stable`
- [ ] **AUR**: Create AUR account, push `deploy/aur/PKGBUILD` to AUR git
- [ ] **Chocolatey**: `choco apikey --api-key <key> && choco push`
- [ ] **Scoop**: Submit PR to scoop bucket or push `deploy/scoop/ravenfabric.json`
- [ ] **WinGet**: Submit PR to `microsoft/winget-pkgs` with `deploy/winget/RavenFabric.RavenFabric.yaml`
- [ ] **Homebrew-core**: Once formula is stable, submit PR to `homebrew/homebrew-core`
- [ ] **openSUSE OBS**: Submit `deploy/obs/ravenfabric.spec` + `_service` to OBS
- [ ] **F-Droid**: Submit `deploy/fdroid/metadata/io.ravenfabric.agent.yml` via GitLab MR to fdroiddata
- Closes [#91](https://github.com/egkristi/RavenFabric/issues/91)

### Publish to crates.io manually (first time)

First publish requires manual verification of crate names and metadata.

- [ ] Run: `cargo publish -p rf-audit --dry-run` (verify each crate compiles for publish)
- [ ] First publish must be done locally (CI can handle subsequent releases):

  ```bash
  for crate in rf-audit rf-crypto rf-bootstrap rf-transport rf-policy rf-rpc rf-executor rf-mcp-client rf-mcp-server rf-relay rf-agent rf-cli; do
    cargo publish -p "$crate" --no-verify
    sleep 15
  done
  ```

- Closes [#44](https://github.com/egkristi/RavenFabric/issues/44)

---

## Priority 3 — Marketing & community (do at launch)

### Create Buttondown newsletter account

- [ ] Sign up at <https://buttondown.com>
- [ ] Create newsletter for RavenFabric
- [ ] Get embed form HTML and update `website/index.html`
- [ ] Add API key as a secret if needed for automation
- Closes [#53](https://github.com/egkristi/RavenFabric/issues/53) and [#41](https://github.com/egkristi/RavenFabric/issues/41)

### Submit to Hacker News, Reddit, Lobsters, kode24

Marketing posts are pre-written in `marketing/`:

- [ ] **Hacker News**: Submit `marketing/show-hn.md` as a Show HN post
- [ ] **Reddit**: Post to r/rust, r/selfhosted, r/homelab, r/networking (see `marketing/reddit-posts.md`)
- [ ] **Lobsters**: Submit with `networking` and `rust` tags
- [ ] **kode24**: Submit article
- Closes [#40](https://github.com/egkristi/RavenFabric/issues/40)

### Set up live demo sandbox

- [ ] Provision a small VM/VPS or k3s cluster
- [ ] Deploy using `deploy/helm/ravenfabric/` Helm chart
- [ ] Point `rf-demo.ravenfabric.io` DNS CNAME to the host
- [ ] Add rate limiting and auto-reset (sandbox should reset every hour)
- Closes [#42](https://github.com/egkristi/RavenFabric/issues/42)

---

## Priority 4 — Platform-specific (when resources available)

### Windows/macOS/mobile installers

- [ ] **macOS .pkg**: Sign with Apple Developer certificate (`deploy/macos/build-pkg.sh` ready)
- [ ] **Windows MSI**: Build with WiX toolset (`deploy/wix/ravenfabric.wxs` is ready)
- [ ] **Windows NSIS**: Build with NSIS (`deploy/nsis/ravenfabric.nsi` is ready)
- [ ] **Android**: NDK cross-compile, publish to Play Store
- [ ] **iOS**: Xcode build, publish to App Store
- Closes [#90](https://github.com/egkristi/RavenFabric/issues/90)

### ~~Remaining transport drivers~~

~~12 of 14 exotic transport drivers need external libraries or hardware:~~
~~BLE, Wi-Fi Direct, Audio modem, QR-stream, LoRa/Meshtastic, AX.25, HF radio, Satellite, Reticulum, I2P, Veilid, Mixnet.~~

- [x] All 12 transport drivers implemented with protocol-specific framing, validation, and tests (196 new tests)
- Closed [#89](https://github.com/egkristi/RavenFabric/issues/89)
