# Distribution packages

Release 0.12.0 uses Flutter version `0.12.0+4016`: Android split APK version
codes are 6016 (arm64-v8a) and 8016 (x86_64). Keep the signing key already trusted
by installed clients. GitHub Actions reads the existing key from repository
secrets; it fails instead of silently publishing with a different debug key.
Never commit `key.properties`, keystores or operational access notes.

## GitHub release

The release workflow tests Rust, Flutter, PostgreSQL and deployment contracts.
It builds AppImage on Ubuntu 22.04, checks startup on Ubuntu 22.04/24.04, builds
Windows and both Android ABIs, and creates an amd64 Debian binary package.
Publishing `codex/release/v0.12.0` requires that branch to match current `main`.
The workflow publishes assets and SHA256 checksums only after required checks.
Manual Flatpak bundles and packaging kits may be attached after verification.

## Android and F-Droid

The regular APK keeps camera QR scanning. In an isolated clean checkout,
`scripts/prepare_fdroid.py` replaces the scanner with a manual pairing-link
field and removes `mobile_scanner` / Google ML Kit from the dependency graph.
`scripts/build_android_fdroid.sh` prepares that variant and compiles one ABI.
Do not run variant preparation in a checkout used to build the regular APK.

The metadata in `packaging/fdroiddata/org.fedi3.client.yml` is intentionally
**disabled pending review**. Removing ML Kit does not make the whole native
supply chain source-built: media_kit downloads native JARs directly from GitHub releases and needs an
approved provenance/source-build strategy. WebRTC uses Maven artifacts; those
may qualify under the trusted-Maven exception but still need licence and
provenance validation. Prebuilt FLOSS dependencies are not categorically banned. Run the F-Droid scanner and
an isolated offline build before removing `Disabled` or proposing inclusion.
No acceptance by F-Droid is implied by a successful local APK build or lint.

After resolving those blockers, fork fdroiddata, copy the metadata to
`metadata/org.fedi3.client.yml` and propose a merge request. Store text and
screenshots are in `fastlane/metadata/android`. F-Droid normally signs with its
own key. Plan encrypted identity/data export and restore before any signing-key
migration; never uninstall an existing account merely to bypass a signature
mismatch.

References: [inclusion policy](https://f-droid.org/en/docs/Inclusion_Policy/),
[submission guide](https://f-droid.org/en/docs/Submitting_to_F-Droid_Quick_Start_Guide/).

## Arch and derivatives

`packaging/aur/PKGBUILD` builds `fedi3-bin` from the published AppImage and checks
its exact SHA256. Its wrapper executes `/opt/fedi3/AppRun`, preserving the same
rendering/library setup as the tested AppImage. Build as an ordinary user:

```sh
cd packaging/aur
makepkg --printsrcinfo > .SRCINFO
makepkg -f
sudo pacman -U ./fedi3-bin-0.12.0-1-x86_64.pkg.tar.zst
```

The AUR accepts PKGBUILD/source metadata, not binary packages. Register a
maintainer account and SSH public key, clone
`ssh://aur@aur.archlinux.org/fedi3-bin.git`, copy PKGBUILD and .SRCINFO, commit
and push. Check for an existing package/maintainer first. The prepared recipe
is not an automatic submission to Arch's official repositories or to every
Arch derivative. See [AUR guidelines](https://wiki.archlinux.org/title/AUR_submission_guidelines).

## Debian, Ubuntu and derivatives

`bash scripts/build_deb.sh` packages the verified AppImage into
`dist/fedi3_0.12.0-1_amd64.deb`, with desktop entry, icon, runtime dependencies
and no maintainer script that removes user data. Run the builder on Debian or
Ubuntu with `dpkg-deb`; CI builds it from the Ubuntu 22.04 AppImage. Users can
install with `sudo apt install ./fedi3_0.12.0-1_amd64.deb`.

`packaging/debian` is a **source-package scaffold**, not a PPA-ready upload.
It requires a reviewed offline source archive containing vendored Rust crates,
Dart dependencies and the Flutter toolchain; the target builder must also have
Rust >=1.99. Those toolchains are not assumed to exist in stock Ubuntu series.
The rules deliberately fail when the required vendor tree is absent instead
of downloading binaries during an offline Launchpad build.

For a PPA, supply those sources and build dependencies, replace UNRELEASED with
the intended Ubuntu suite, run an isolated `sbuild`, sign the `.dsc`/`.changes`
with the maintainer's OpenPGP key, then upload using `dput`. Launchpad accepts
source packages and rejects prebuilt .deb uploads. A signed independent APT
repository can distribute the binary .deb sooner, once its hosting URL and
repository signing identity have been selected. Neither channel is silently
created or presented as an official Ubuntu repository.

References: [PPA upload](https://ubuntu.com/docs/launchpad/user/how-to/packaging/ppa-package-upload/),
[PPA requirements](https://ubuntu.com/docs/launchpad/user/reference/packaging/ppas/ppa/).

## Flatpak and the desktop menu

The menu entry launches `org.fedi3.client`. Update that installed Flatpak rather
than redirecting its desktop file to an AppImage with a different data location.
Preserve `~/.var/app/org.fedi3.client` and the OS Secret Service. AppImage and
Flatpak have separate application-data directories; switching launchers does
not migrate accounts.

The source manifest in `packaging/flatpak` builds Rust and Flutter inside the
GNOME 50 SDK, using pinned source and mpv 0.41.0. For the local verified bundle,
the same already-built Rust core and installed mpv dependency layer may be reused;
the Flutter binary must be built inside the SDK to match its ABI. A GitHub .flatpak
bundle is not publication on Flathub. Flathub submission still requires its own
manifest review and offline dependency preparation.

### Independent APT repository staging

Use `scripts/build_apt_repo.py --output /path/to/repo --unsigned-preview dist/fedi3_0.12.0-1_amd64.deb`
to prepare pool/, Packages, Packages.gz and Release for inspection. For hosting,
replace `--unsigned-preview` with `--signing-key YOUR_EXISTING_OPENPGP_KEY`;
the script creates InRelease, Release.gpg and the public archive key. Configure
clients with `signed-by=` pointing at that key. Do not disable APT signature
checks. The preparation archive is not a live repository URL.

A pacman repository preview can be generated with
`repo-add fedi3.db.tar.gz fedi3-bin-0.12.0-1-x86_64.pkg.tar.zst`.
The release preparation archive contains that index; place the separately
published package beside it. Sign the package and regenerate/sign the database
with the maintainer's OpenPGP key before enabling a public repository.
The preview does not replace an AUR submission or an official Arch repository.
