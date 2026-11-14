# Pacchetti di distribuzione

La versione 0.12.0 usa `0.12.0+4016`: gli APK separati hanno codice 6016
(arm64-v8a) e 8016 (x86_64). Gli aggiornamenti mantengono la firma già fidata dai
dispositivi. Le chiavi sono conservate privatamente e nei secret di GitHub Actions;
non vanno inserite in Git. La CI richiede la firma configurata, verifica Rust,
Flutter e PostgreSQL e produce AppImage, Windows, APK e .deb con checksum.

## F-Droid

La variante preparata con `scripts/prepare_fdroid.py` sostituisce la fotocamera
ML Kit con l’inserimento del collegamento di abbinamento e rimuove
`mobile_scanner`. Usare un checkout separato: lo script modifica i sorgenti.

La ricetta `packaging/fdroiddata/org.fedi3.client.yml` resta intenzionalmente
**disabilitata**: occorre ancora verificare e predisporre la compilazione dai
sorgenti degli artefatti Android WebRTC e media_kit. Un APK compilato o un lint
pulito non dimostrano l’idoneità a F-Droid. Prima della proposta a fdroiddata
servono scanner, verifica licenze e build isolata offline. Metadati, screenshot
e changelog sono in `fastlane/metadata/android`.

F-Droid normalmente firma con una propria chiave. Prima di cambiare canale
esportare identità e dati cifrati e verificare il ripristino: non disinstallare
per aggirare una firma diversa. Vedere la [politica di inclusione](https://f-droid.org/en/docs/Inclusion_Policy/).

## Arch/AUR e derivate

`packaging/aur/PKGBUILD` e `.SRCINFO` preparano `fedi3-bin`, verificando il
checksum dell’AppImage pubblicata. Il launcher usa AppRun, conservandone la
configurazione di librerie e rendering. Per compilare: `makepkg -f` nella
cartella della ricetta; per installare: `sudo pacman -U ./fedi3-bin-0.12.0-1-x86_64.pkg.tar.zst`.

AUR riceve ricette, non pacchetti binari. Per inviare la ricetta occorrono
account e chiave SSH registrati, verifica di eventuali manutentori esistenti,
commit e push nel repository `aur@aur.archlinux.org:fedi3-bin.git`.
Non equivale all’inclusione nei repository ufficiali Arch o delle derivate.
[Regole AUR](https://wiki.archlinux.org/title/AUR_submission_guidelines).

## Debian/Ubuntu e derivate

`scripts/build_deb.sh` genera un .deb amd64 dall’AppImage verificata; la CI lo
costruisce su Ubuntu 22.04. Include dipendenze, icona e voce del menu, senza
script che cancellino dati utente. Installazione: `sudo apt install ./fedi3_0.12.0-1_amd64.deb`.

`packaging/debian` è una **base per il pacchetto sorgente**, non un caricamento
PPA pronto. Mancano un archivio sorgente offline revisionato con dipendenze
Rust/Dart e SDK Flutter, e la disponibilità di Rust >=1.99 nel builder della
suite scelta. Le regole falliscono esplicitamente se quei sorgenti mancano.

Un PPA Launchpad accetta pacchetti sorgente, non .deb precompilati: dopo aver
risolto i prerequisiti occorrono test sbuild, suite Ubuntu nel changelog,
firma OpenPGP e caricamento dput. Un repository APT indipendente può distribuire
prima i .deb, ma richiede hosting e identità di firma scelti dal manutentore.
[Requisiti PPA](https://ubuntu.com/docs/launchpad/user/reference/packaging/ppas/ppa/).

## Flatpak e menu del PC

La voce del menu avvia `org.fedi3.client`: bisogna aggiornare quel Flatpak,
conservando `~/.var/app/org.fedi3.client` e Secret Service. Flatpak e AppImage
usano directory dati separate; sostituire il collegamento non migra gli account.

Il manifest sorgente usa GNOME SDK 50 e compila core Rust, Flutter e mpv.
Il bundle locale può riutilizzare core già verificato e dipendenze mpv già
installate, compilando Flutter nello SDK per compatibilità ABI. Un bundle
su GitHub non significa che il programma sia stato pubblicato su Flathub.

I dettagli completi, le procedure e i limiti della preparazione si trovano in
[Distribution packages](../distribution.md).
