# App guide

## Login / relay

- Seleziona il relay dalla lista.
- Inserisci il token o crea account se abilitato.

## Timeline

- Filtri rapidi, popup reazioni, azioni inline.
- Media viewer con swipe e toggle audio.

## Chat

- 1:1 e gruppi P2P.
- Reazioni, GIF, file e stato lettura.

## Impostazioni

- Lingua IT/EN.
- Telemetria opzionale (consenso).
- Traduzione dei post (Deepl/Giphy).

# linux deploy app
sudo pacman -Syu --needed \
  base-devel git curl unzip xz zip \
  clang cmake ninja pkgconf \
  gtk3 gsettings-desktop-schemas \
  libxkbcommon libxss libxrandr libxcursor libxinerama libxi \
  mesa libglvnd \
  pango cairo gdk-pixbuf2 glib2 \
  alsa-lib pulseaudio \
  mpv ffmpeg

sudo pacman -S --needed flutter
flutter config --enable-linux-desktop
flutter doctor

cd ~
git clone https://github.com/flutter/flutter.git -b stable
export PATH="$HOME/flutter/bin:$PATH"
flutter config --enable-linux-desktop
flutter doctor

cd /path/to/Fedi3/app
flutter pub get
flutter build linux --release

yay -S --needed linuxdeploy linuxdeploy-plugin-gtk appimagetool

paru -S --needed linuxdeploy linuxdeploy-plugin-gtk appimagetool

# appdir
cd /path/to/Fedi3/app
APPDIR=AppDir
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr"

cp -r build/linux/x64/release/bundle/* "$APPDIR/usr/"

# Desktop entry
cat > fedi3.desktop <<'EOF'
[Desktop Entry]
Type=Application
Name=Fedi3
Exec=fedi3
Icon=fedi3
Categories=Network;
Terminal=false
EOF

# Icona (usa un png 256x256 reale del progetto)
mkdir -p "$APPDIR/usr/share/applications"
mkdir -p "$APPDIR/usr/share/icons/hicolor/256x256/apps"
cp fedi3.desktop "$APPDIR/usr/share/applications/"

# Sostituisci con la tua icona reale
cp /path/to/icon.png "$APPDIR/usr/share/icons/hicolor/256x256/apps/fedi3.png"
# fine appdir

linuxdeploy \
  --appdir "$APPDIR" \
  --desktop-file fedi3.desktop \
  --icon-file /path/to/icon.png \
  --output appimage


ldd build/linux/x64/release/bundle/fedi3 | rg "not found"

cp /usr/lib/libmpv.so* "$APPDIR/usr/lib/"

------------------

# pulizia completa dei file generati
rm -rf .dart_tool build linux/flutter/ephemeral linux/build
find linux -name CMakeCache.txt -delete
find linux -name CMakeFiles -type d -prune -exec rm -rf {} +

# rigenera i file di piattaforma Linux
flutter config --enable-linux-desktop
flutter create --platforms=linux .

# ricrea i symlink dei plugin
flutter pub get

# build
flutter build linux --release

Se ancora dà lo stesso errore, vuol dire che i plugin non hanno la cartella linux/ (o non sono risolti). In quel caso, prova:

flutter pub cache repair
flutter pub get
e poi ricompila.

Se vuoi, incolla l’output di:

ls -la linux/flutter/ephemeral/.plugin_symlinks
flutter doctor -v
e ti dico il punto esatto che manca.