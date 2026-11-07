# FEDI3 Flutter client

Flutter UI for FEDI3. It starts the local Rust core through `dart:ffi` and
handles timelines, chat, media and settings.

## Run

```bash
flutter pub get
flutter run
```

## Checks

```bash
flutter analyze --fatal-infos
dart format --output=none --set-exit-if-changed lib test
flutter test
```

## Documentation

See [the app guide](../docs/app_guide.md) (Italian: [guida dell'app](../docs/it/app_guide.md)).
