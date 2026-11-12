# Pacchetti di distribuzione

Fedi3 è pubblicato su GitHub Releases come AppImage, ZIP per Windows e due APK
Android. Questa pagina copre i canali di distribuzione: F-Droid per Android,
Flatpak per i desktop Linux e il pacchetto AUR `fedi3-bin`.

I file di packaging sono nel repository:

- `fastlane/metadata/android/`: metadati dello store F-Droid e screenshot.
- `packaging/fdroiddata/org.fedi3.client.yml`: ricetta di build F-Droid.
- `packaging/flatpak/`: manifest Flatpak, modulo mpv, desktop file e metadati
  AppStream.
- `packaging/aur/`: PKGBUILD `fedi3-bin` e `.SRCINFO`.

## Versioni e version code

`app/pubspec.yaml` contiene il build number Flutter `4015`. Con
`--split-per-abi` Flutter aggiunge l'offset dell'ABI, quindi gli APK della
release usano:

| ABI | Version code |
| --- | --- |
| arm64-v8a | 6015 (4015 + 2000) |
| x86_64 | 8015 (4015 + 4000) |

La ricetta F-Droid codifica la stessa mappatura con `VercodeOperation` e
`UpdateCheckData`; `CurrentVersionCode` è il codice più alto, 8015. A ogni
release vanno aggiornati `CurrentVersion`, `CurrentVersionCode`, `commit` e le
due voci `Builds`.

## Android: F-Droid

### Build locale

La ricetta usa Flutter 3.50.0, Rust 1.99.1, NDK r28c e JDK 21. Lo script di
supporto compila una sola ABI per volta:

```bash
export JAVA_HOME=/usr/lib/jvm/java-21-openjdk
export FEDI3_ANDROID_NDK=$HOME/Android/Sdk/ndk/28.2.13676358
bash scripts/build_android_fdroid.sh arm64-v8a
bash scripts/build_android_fdroid.sh x86_64
```

Compila `fedi3_core` per il target NDK, copia `libfedi3_core.so` in
`app/android/app/src/main/jniLibs/<abi>/` ed esegue
`flutter build apk --release --split-per-abi`. L'output è
`app/build/app/outputs/flutter-apk/app-<abi>-release.apk`.

### Invio a F-Droid

1. Fork di <https://gitlab.com/fdroid/fdroiddata>.
2. Copia `packaging/fdroiddata/org.fedi3.client.yml` in
   `metadata/org.fedi3.client.yml`.
3. Push del branch e merge request verso `master`.

La ricetta compila entrambe le ABI dai sorgenti e F-Droid firma il risultato con
la propria chiave. Gli APK su GitHub Releases sono firmati con la chiave di debug
locale (in CI manca `app/android/key.properties`), quindi le firme sono diverse:
chi passa da un APK sideload alla build F-Droid deve disinstallare una volta la
vecchia app. Per le release GitHub future va mantenuta la stessa chiave di firma,
così gli aggiornamenti sideload restano possibili.

Note per revisori e release future:

- `mobile_scanner` usa l'AAR precompilato di Google MLKit da Maven; non viene
  ricompilato nella ricetta. Il resto del codice nativo (core Rust e plugin
  Flutter) è compilato dai sorgenti o arriva dalle dipendenze Gradle.
- `flutter_webrtc` include le librerie WebRTC precompilate nel proprio AAR
  Android; la ricetta non le copia nel repository.
- La ricetta fissa l'SDK Flutter tramite lo srclib `flutter@stable` e fa
  checkout del tag `3.50.0`; `.flutter` e `.pub-cache` sono esclusi dallo
  scanner, mentre `.flutter/bin/cache` è ignorato come cache binaria.

## Linux: Flatpak

Il manifest usa il runtime e l'SDK GNOME 50, che forniscono GTK3, libsecret,
GStreamer e `webkit2gtk-4.1` (necessario alla web view integrata). `libmpv` non
fa parte del runtime, quindi il manifest compila mpv 0.41.0 dai sorgenti.
Flutter 3.50.0 viene scaricato dall'archivio stable ufficiale con SHA256 fisso;
il core Rust è compilato con l'estensione SDK `rust-stable`.

### Build e test locale

```bash
flatpak-builder --user --force-clean --repo=/tmp/fedi3-flatpak-repo \
  /tmp/fedi3-flatpak-build packaging/flatpak/org.fedi3.client.yml
flatpak build-export /tmp/fedi3-flatpak-repo /tmp/fedi3-flatpak-build
flatpak install --user /tmp/fedi3-flatpak-repo org.fedi3.client
flatpak run org.fedi3.client
```

Servono `org.gnome.Platform//50`, `org.gnome.Sdk//50` e
`org.freedesktop.Sdk.Extension.rust-stable//25.08` installati da Flathub.

### Flathub

Flathub ospita ogni app in un repository dedicato nell'organizzazione GitHub
`flathub`. L'app va prima proposta con il flusso new-app su
<https://github.com/flathub/flathub/issues/new?template=new-app.yml>; i revisori
creano `flathub/org.fedi3.client`. Poi si apre una pull request con:

- `org.fedi3.client.yml`
- `mpv.json`
- `org.fedi3.client.desktop`
- `org.fedi3.client.metainfo.xml`

L'app ID `org.fedi3.client` coincide con l'application ID GTK in
`app/linux/CMakeLists.txt`. Il manifest installa icona e metadati AppStream e
l'app non ha dipendenze runtime proprietarie.

## Linux: AUR

`packaging/aur/PKGBUILD` crea il pacchetto binario `fedi3-bin` dall'AppImage
pubblicata. Estrae l'AppImage in `/opt/fedi3`, installa un wrapper in
`/usr/bin/fedi3` e aggiunge desktop file e icona. `fedi3-bin` fornisce `fedi3`
ed entra in conflitto con un pacchetto `fedi3` compilato dai sorgenti.

Build locale:

```bash
cd packaging/aur
makepkg -f
sudo pacman -U fedi3-bin-0.11.11-1-x86_64.pkg.tar.zst
```

Pubblicazione sull'AUR:

```bash
git clone ssh://aur@aur.archlinux.org/fedi3-bin.git
cp packaging/aur/PKGBUILD packaging/aur/.SRCINFO fedi3-bin/
cd fedi3-bin
git add PKGBUILD .SRCINFO
git commit -m "fedi3-bin 0.11.11-1"
git push
```

Gli utenti lo installano con `yay -S fedi3-bin` o `paru -S fedi3-bin`.

## Checklist di release

1. Aggiorna `app/pubspec.yaml` (`version: X.Y.Z+NNNN`).
2. Compila e pubblica gli asset della release GitHub con
   `scripts/release_build_linux.sh` e `scripts/build_android.sh release`.
3. Aggiorna `packaging/fdroiddata/org.fedi3.client.yml` (`commit`,
   `versionName`, `versionCode`, `CurrentVersion`, `CurrentVersionCode`).
4. Aggiorna `packaging/aur/PKGBUILD` (`pkgver`, `sha256sums` dell'AppImage) e
   rigenera `.SRCINFO` con `makepkg --printsrcinfo > .SRCINFO`.
5. Aggiorna il commit sorgente nel manifest Flatpak ed esegui la build Flatpak
   locale.
6. Aggiorna i changelog fastlane in
   `fastlane/metadata/android/<locale>/changelogs/<versionCode>.txt`.
