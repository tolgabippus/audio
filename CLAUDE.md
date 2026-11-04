Read @AGENTS.md

# Fork-Notizen (tolgabippus/audio)

Dieser Abschnitt hält fest, was in einer früheren Claude-Code-Sitzung (Cloud, 2026-11-04) besprochen und geprüft wurde. Er ergänzt `AGENTS.md`, die Projektregeln dort gelten weiter.

## Ausgangslage

- Dieses Repo ist ein Fork von [cjpais/Handy](https://github.com/cjpais/Handy), Stand **v0.10.0**. Am 2026-11-04 war `HEAD` (`82dfab9`) vollständig in `cjpais/Handy@main` enthalten, es gab keine eigenen Änderungen.
- Upstream nachziehen:
  ```bash
  git remote add upstream https://github.com/cjpais/Handy.git   # einmalig
  git fetch upstream && git merge upstream/main
  ```
- Upstream ist im Feature-Freeze (siehe `AGENTS.md`). Eigene Features wie die Meeting-Aufnahme sind deshalb nur für diesen Fork gedacht, nicht als PR für Upstream.

## Rechner des Besitzers

- **MacBook Pro 2019, Intel i9-9980HK, 64 GB RAM, macOS 26.6.2** (geprüft am 2026-11-04).
- Toolchain (2026-11-04 eingerichtet): Xcode (Swift 6.2, SDK 26.2), Rust 1.99 per rustup, Bun 1.4 per bun.sh-Installer, cmake 4.3, Homebrew. In Befehlen aus Claude Code ggf. `export PATH="$HOME/.cargo/bin:$HOME/.bun/bin:$PATH"` voranstellen.
- `CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev` läuft. Der erste Build dauerte gut 6 Minuten. transcribe-cpp meldet nur ein Compute-Device: CPU.
- Handy 0.10.0 ist über Homebrew installiert (`brew install --cask handy`), also die offizielle, signierte Upstream-Version unter `/Applications/Handy.app`.
- Homebrew unterstützt Intel-Macs nur noch als „Tier 3“ (keine Bottles mehr). Casks installieren trotzdem normal. Ein alter Tap `adntgv/tap` wird als „nicht vertrauenswürdig“ gemeldet und hat nichts mit Handy zu tun.
- Standard-Shortcuts auf macOS (`src-tauri/src/settings.rs`): `option+space` diktieren, `option+shift+space` diktieren mit LLM-Nachbearbeitung, `escape` abbrechen.

### Modellempfehlung für diesen Rechner

Ohne Apple Silicon sind Modelle, die auf der CPU schnell sind, die beste Wahl. Die Grafikkarte im Intel-Mac hilft vermutlich wenig. Die Empfehlung stützt sich nur auf `src-tauri/src/catalog/catalog.json` und ist **auf dem Gerät nicht gemessen**.

1. **Parakeet Ultra** (627M): laut Katalog die beste Genauigkeit für 25 europäische Sprachen inkl. Deutsch
2. **Parakeet TDT 0.6B v3** (627M): bewährter Standard, schnell, erkennt die Sprache automatisch
3. **Canary 1B v2** (980M): Alternative für europäische Sprachen

- Eher meiden: Whisper Large v3 / Turbo, Qwen3-ASR 1.7B, Voxtral, Granite 2B, Cohere Transcribe. Sie sind laut Katalog „Best on Apple Silicon or a GPU“ bzw. „Slow without a GPU“.
- Unter Einstellungen → Erweitert → Acceleration CPU und GPU vergleichen. Sprache fest auf Deutsch stellen, wenn fast nur Deutsch gesprochen wird.

## Sicherheits-Check (2026-11-04)

Der Code wurde stichprobenartig geprüft. Das war kein vollständiges Audit. **Es gab keine Hinweise auf bösartigen Code.**

- **Netzwerk:** Netzwerkzugriffe gibt es nur an drei Stellen:
  - Modell-Downloads von `https://blob.handy.computer`, per SHA256 geprüft (`src-tauri/src/managers/model/download.rs`)
  - Update-Check über GitHub-Releases `cjpais/Handy`, signiert per minisign (`src-tauri/tauri.conf.json`, abschaltbar)
  - LLM-Nachbearbeitung (`src-tauri/src/llm_client.rs`), **nur wenn aktiviert**. Dann geht der transkribierte Text, nicht das Audio, an den gewählten Anbieter.
- Keine Telemetrie oder Analytics gefunden. Audio verlässt den Rechner nicht.
- **Externe Befehle:** alle plausibel. `osascript` zum Stummschalten während der Aufnahme, `ioreg`/`pmset` für die Erkennung des zugeklappten Laptops, `ioreg`/`ps` für die Secure-Input-Warnung, `open` zum Öffnen eines Ordners, außerdem der Nutzer-Hook und Linux-Tipp-Tools.
- `package.json` `postinstall` (`scripts/check-nix-deps.ts`) und `src-tauri/build.rs` sind unauffällig.
- **Härtungsmangel, keine Bösartigkeit:** In `tauri.conf.json` ist `"csp": null` gesetzt und `assetProtocol.scope` erlaubt `**`.
- **Nicht geprüft:** die Abhängigkeiten (Rust-Crates, npm, `transcribe-cpp`) und ob die Homebrew-Binary exakt aus diesem Code gebaut ist.
- Die Bedienungshilfen-Freigabe ist mächtig: Damit kann Handy systemweit Tasten mitlesen und in andere Apps tippen. Das ist für die Funktion nötig.

## Bekannte Grenzen von Handy (Stand v0.10.0)

- Nur Desktop (macOS, Windows, Linux). Es gibt keine iOS- oder Android-App und kein Sync. Globale Shortcuts und Einfügen in fremde Apps sind auf Mobilgeräten technisch nicht möglich, dort bräuchte man eine eigene Tastatur-App.
- Daten liegen nur lokal im App-Datenordner (`~/Library/Application Support/com.pais.handy/`), der Verlauf in `history.db` (SQLite). Den Ordner nicht per iCloud oder Syncthing synchronisieren, sonst droht eine kaputte Datenbank.
- **Nimmt nur das Mikrofon auf, keinen System-Ton.** Sprecher werden nicht getrennt: Die Diarisierungs-Modelle im Katalog (`Streaming Sortformer`, `MOSS Transcribe Diarize`) liefern keinen Text und werden von der App nicht genutzt.
- Datei-Transkription per Drag & Drop. Unterstützt werden `mp3 m4a aac wav flac aif aiff mp4 m4v mov`. **Opus wird nicht unterstützt**, also auch keine WhatsApp-Sprachnachrichten (`src/components/settings/history/transcribeFiles.ts`).
- Meetings gehen derzeit nur über einen Umweg: mit Zoom oder Teams aufnehmen und die Datei in Handy ziehen. Bei Meetings im selben Raum geht auch der Toggle-Modus mit dem Mikrofon.
- **Rechtlich (DE):** Nicht-öffentliche Gespräche ohne Zustimmung aufzunehmen ist strafbar (§ 201 StGB). Teilnehmer also immer vorher informieren.

## Optional: Diktate nach iCloud spiegeln (Transcription Hook)

Handy ruft `<App-Daten>/hooks/transcription` mit dem Text auf stdin auf und fügt stdout ein (`src-tauri/src/transcription_hook.rs`, 30 s Timeout). Ob der Besitzer dieses Script installiert hat, ist offen. **Es ist ungetestet.**

```bash
mkdir -p ~/Library/Application\ Support/com.pais.handy/hooks
cat > ~/Library/Application\ Support/com.pais.handy/hooks/transcription << 'EOF'
#!/bin/bash
text=$(cat)
dir="$HOME/Library/Mobile Documents/com~apple~CloudDocs/Handy"
/bin/mkdir -p "$dir"
printf -- '- %s: %s\n' "$(/bin/date '+%Y-%m-%d %H:%M')" "$text" >> "$dir/Transkripte.md"
printf '%s' "$text"
EOF
chmod +x ~/Library/Application\ Support/com.pais.handy/hooks/transcription
```

Der Hook bekommt nicht den Shell-`PATH`, deshalb absolute Pfade verwenden. Er muss den Text wieder auf stdout ausgeben, sonst wird nichts eingefügt.

## Feature: Meeting-Aufnahme mit System-Ton

**Ziel:** Handy soll Meetings (Zoom, Teams, Meet usw.) mitschneiden, mit **System-Ton und Mikrofon gleichzeitig**. Danach wird automatisch transkribiert und das Ergebnis landet im Verlauf. Das normale Diktieren muss unabhängig davon weiter funktionieren.

**Status:** Stufe 1 ist umgesetzt (2026-11-04), aber **noch nicht mit einem echten Meeting getestet**. Stufe 2 ist offen. Die Umsetzung passiert **lokal auf dem Mac**, weil die macOS-Teile in der Cloud (Linux) weder gebaut noch getestet werden können.

### Umsetzung Stufe 1 (so ist es gebaut)

- **System-Ton über `cpal`, nicht über eine eigene Swift-Brücke.** `cpal` 0.18.2, das über `rodio::cpal` schon in Handy steckt, nimmt auf macOS mit `build_input_stream` auf einem _Ausgabegerät_ per Process Tap auf (`cpal/src/host/coreaudio/macos/loopback.rs`). Der Spike `CpalLoopbackSpike` hat das auf diesem Mac bestätigt. Vorteil: kein Swift, kein Eingriff in `build.rs`. Nachteil: Der Tap hängt am aktuellen Standard-Ausgabegerät. Wechselt man es, baut Handy den Stream neu auf.
- `src-tauri/src/managers/meeting/`:
  - `mod.rs` (`MeetingManager`): Start, Stopp, Mischen, Übergabe an den Verlauf, Wiederherstellung nach Absturz, `remove_tracks` beim Löschen.
  - `system_audio.rs`: eigener Thread mit dem Loopback-Stream, Downmix und Resampling auf 16 kHz. Neuaufbau bei Fehler, bei Wechsel des Ausgabegeräts (alle 2 s geprüft) und **solange nur exakte Nullen kommen** (alle 4 s, höchstens 15-mal). Das löst Stolperfalle 1.
  - `track.rs`: `TrackWriter`, 16-kHz-Mono-WAV, Header alle 5 s aktualisiert, Lücken mit Stille aufgefüllt, damit beide Spuren zeitgleich zum Meeting-Start bleiben. Dazu `mix_tracks`.
- Mikrofon: eine zweite `handy_recorder::Recorder`-Instanz (`RecorderConfig::speech()`) auf dem Gerät, das auch das Diktieren nutzt (`AudioRecordingManager::selected_microphone_device`).
- Dateien: Spuren in `<App-Daten>/meetings/<Start-ms>/mic.wav` und `system.wav`. Beim Stoppen entsteht `recordings/handy-meeting-<Start-ms>.wav`. Erst diese Datei markiert das Meeting als importiert. Danach läuft `TranscriptionJobManager::transcribe_recording` (neu), und der Verlauf nennt den Eintrag `Meeting JJJJ-MM-TT HH:MM`.
- Resampler: von `audio_toolkit/audio/decode.rs` nach `audio_toolkit/audio/resample.rs` (`MonoResampler`, jetzt mit `take_output`) verschoben.
- Bedienung: Shortcut-Binding `meeting_toggle` (macOS `option+shift+m`, sonst `ctrl+alt+m`) in `settings.rs` und `actions.rs`, Tray-Eintrag `meeting_toggle` in `tray.rs` und `lib.rs`, Befehle in `commands/meeting.rs`, Event `MeetingState`, Seite `src/components/settings/meeting/`.
- Meldungen: `NoticeSource::Meeting` mit `MeetingRecordingFailed` (Error), `MeetingNoSystemAudio` und `MeetingNoMicrophone` (Warning).
- Texte: Deutsch und Englisch übersetzt, die anderen 24 Sprachen vorerst mit englischem Text.
- `Info.plist`: `NSAudioCaptureUsageDescription` ergänzt.
- Bekannte Grenzen: Mit „Stummschalten während der Aufnahme“ ist auch die System-Spur stumm, solange man nebenher diktiert. Echo ohne Kopfhörer wird nicht entfernt. Mehrere Ausgabegeräte gleichzeitig (Multi-Output) sind nicht getestet. Auf Linux kann `cpal` (ALSA) keinen Loopback, dort kommt die Warnung „ohne System-Ton“.
- **Testen nur als echte App:** `tauri dev` startet keine `.app`, dann fragt macOS die Berechtigung für den falschen Prozess ab. Bauen mit `CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri build --bundles app -c '{"bundle":{"createUpdaterArtifacts":false}}'` und `src-tauri/target/release/bundle/macos/Handy.app` per `open` starten. Ein Debug-Bundle scheitert, weil es beim Start `../src/bindings.ts` relativ zum Arbeitsverzeichnis schreiben will.

### Ergebnis des Spikes (2026-11-04, macOS 26.6.2)

- Code und Bauanleitung liegen in `docs/spikes/system-audio-tap/`.
- Eine kleine Swift-App mit **Core Audio Process Tap** (`CATapDescription(stereoGlobalTapButExcludeProcesses: [])`, `isPrivate`, `muteBehavior = .unmuted`, privates Aggregate Device mit dem Standard-Ausgabegerät als Main-Sub-Device und dem Tap in `kAudioAggregateDeviceTapListKey`, `AudioDeviceCreateIOProcIDWithBlock`) hat 10 s YouTube-Ton sauber aufgenommen. Pegel durchgehend etwa -18 bis -25 dBFS RMS.
- Tap-Format hier: **48 kHz, 2 Kanäle, Float32 interleaved**. Für Handy muss das auf 16 kHz mono gebracht werden.
- Die App braucht ein eigenes Bundle mit `NSAudioCaptureUsageDescription` in der `Info.plist`. Gestartet wurde sie per `open`, damit macOS sie selbst und nicht Claude oder Terminal um Erlaubnis fragt. Berechtigung: TCC-Dienst `kTCCServiceAudioCapture`, in den Einstellungen unter „Bildschirm- & Systemaudioaufnahme → Nur Systemaudioaufnahme“.
- **Stolperfalle 1:** Der Dialog erscheint erst beim Start der Aufnahme. Wer dann erlaubt, bekommt **für diese Sitzung nur Nullen**. Es kommt kein Fehler, nur digitale Stille. Erst ab dem nächsten Start kommt Ton. Handy sollte also vorab um Erlaubnis bitten oder den Tap nach dem Erlauben neu aufbauen und anhaltende exakte Stille als möglichen Hinweis auf eine fehlende Berechtigung melden.
- **Stolperfalle 2:** Jeder Neubau mit Ad-hoc-Signatur ändert die Code-Signatur, und macOS fragt erneut.
- Für die Analyse von Logs: Die TCC-Entscheidung steht in `log show --predicate 'process == "tccd"'` als `AUTHREQ_RESULT … authValue=2` (erlaubt).

### Bausteine, die es schon gibt

- `src-tauri/src/managers/transcription_jobs.rs` → `TranscriptionJobManager::transcribe_file`: dekodiert eine Datei, speichert sie als WAV bei den Verlaufs-Aufnahmen, transkribiert sie über `transcribe_in_chunks` (`src-tauri/src/managers/transcription/job.rs`) in Stücken, die dem Diktieren Vorrang lassen, und legt einen Verlaufseintrag an, inklusive Retry und Abbrechen. **Den fertigen Mitschnitt hier einspeisen**, eventuell mit einer Variante, die Samples statt eines Pfads nimmt.
- `src-tauri/src/managers/audio.rs` → `AudioRecordingManager` nimmt das Mikrofon über das Crate `handy-recorder` auf. Zu prüfen ist, ob eine zweite, parallele `Recorder`-Instanz für das Meeting-Mikrofon geht.
- `src-tauri/swift/` und `src-tauri/build.rs`: Swift-Brücke inklusive Stub (`apple_intelligence.swift`, `apple_intelligence_bridge.h`, `apple_intelligence_stub.swift`). Nach diesem Muster eine Swift-Brücke für die System-Audio-Aufnahme bauen.
- `src-tauri/Info.plist` enthält bereits `NSMicrophoneUsageDescription`, dort kommt der neue Usage-Key dazu.
- Shortcuts: Bindings in `settings.rs` (`transcribe`, `transcribe_with_post_process`, `cancel`), Logik in `actions.rs` und `src-tauri/src/shortcut/`. Tray-Menü: `src-tauri/src/tray.rs`.
- Fehler über `notices::raise` melden (Regeln in `AGENTS.md`). Alle UI-Texte über i18n in allen Locales anlegen, `bun run check:translations` prüft das. `bindings.ts` neu erzeugen, indem die App im Debug-Modus läuft.

### Welche Apple-Schnittstelle (hängt von `sw_vers` ab, vor dem Bauen gegen die Apple-Doku prüfen)

- **macOS ≥ 14.2: Core Audio Process Taps** (`CATapDescription`, `AudioHardwareCreateProcessTap` plus Aggregate Device). Nur Audio, eigene Berechtigung „Systemaudioaufnahme“, Usage-Key `NSAudioCaptureUsageDescription`. **Bevorzugt.**
- **macOS 13–14.1: ScreenCaptureKit** (`SCStream` mit `SCStreamConfiguration.capturesAudio = true`, `excludesCurrentProcessAudio = true`). Braucht die Berechtigung „Bildschirmaufnahme“.

### Stufe 1: Aufnahme (zuerst umsetzen)

- Neuer Bereich „Meeting“ in den Einstellungen mit Start/Stopp und Laufzeitanzeige, eigener Shortcut und Tray-Eintrag
- Gleichzeitig System-Ton und Mikrofon aufnehmen, auf 16 kHz mono mischen und **fortlaufend auf die Festplatte schreiben**, damit bei einem Absturz nicht alles verloren ist. Die beiden Spuren zusätzlich getrennt speichern, die braucht Stufe 2.
- Beim Stoppen an den `TranscriptionJobManager` übergeben, das Ergebnis erscheint im Verlauf.
- **Echo:** Ohne Kopfhörer nimmt das Mikrofon den Lautsprecher-Ton mit, dann steht Text doppelt im Transkript. Kopfhörer empfehlen oder das später behandeln.

### Stufe 2: „Du“ und „Andere“

- Die Mikrofon-Spur („Du“) und die System-Spur („Andere“) getrennt transkribieren und nach Zeit zusammenführen, zum Beispiel `[00:01:12] Du: …` / `[00:01:15] Andere: …`.
- Zu prüfen ist, ob `transcribe-cpp` Segment-Zeitstempel liefert. Der Katalog hat `capabilities.timestamps`, aber `managers/transcription*` nutzt sie derzeit nicht. Notfalls über die Chunk-Grenzen annähern.
- Das ist keine echte Sprechererkennung: Mehrere Personen auf der Gegenseite bleiben „Andere“.

### Ergebnis des ersten Tests (2026-11-04)

- 87 s Meeting mit YouTube-Video und AirPods: Der System-Ton kam durchgehend an, die Transkription brauchte 14 s (Parakeet TDT 0.6B v3, CPU). Der Neuaufbau bei Stille hat nach 4 s gegriffen.
- **Die eigene Stimme fehlte im Text**, weil der Besitzer gleichzeitig mit dem Video sprach. In der Mischung gewinnt eine Stimme. Das ist das stärkste Argument für Stufe 2.
- Mit AirPods als Mikrofon wechselt macOS in den Headset-Modus: Der System-Ton kommt dann mit 24 kHz mono statt 48 kHz stereo. Für die Spracherkennung reicht das.

### Ideen des Besitzers für später (Reihenfolge offen)

- **Stufe 2:** getrennte Spuren „Du“ und „Andere“ (siehe oben).
- **Meetings im Verlauf einheitlich kategorisieren:** eigene Ansicht oder einen Filter „Meetings“, Titel umbenennen, eventuell Tags oder Ordner. Heute erkennt man Meetings nur am `file_name`-Präfix `handy-meeting-` bzw. am `source_name` „Meeting …“. Für echte Kategorien braucht es eine eigene Spalte oder Tabelle in `history.db` (Migration in `managers/history.rs`).
- **Zusammenfassung mit den wichtigsten Punkten und Aufgaben** über die vorhandene LLM-Nachbearbeitung (`llm_client.rs`, Anbieter in `settings.rs`). Der Anbieter `custom` zeigt standardmäßig auf `http://localhost:11434/v1` (Ollama), damit bliebe auch der Text lokal. Apple Intelligence gibt es nur auf Apple Silicon. Ein eigener Prompt für Meetings, getrennt vom Diktier-Prompt, und ein langes Transkript müssen eventuell in Teilen zusammengefasst werden.
- **Bedienung allgemein ausbauen**, zum Beispiel eine Meeting-Detailansicht mit Transkript, Zusammenfassung und Audio.

### Erwartungen und Stolperfallen

- Eine Stunde Meeting braucht auf dem Intel-i9 voraussichtlich einige Minuten Transkription. Das ist geschätzt, nicht gemessen. Mit Stufe 2 ungefähr doppelt so lange, weil zwei Spuren transkribiert werden.
- Eine selbst gebaute App hat dieselbe Bundle-ID (`com.pais.handy`) wie die Homebrew-Version. Vor der Installation `brew uninstall --cask handy`.
- Lokale Builds sind ad-hoc signiert. Nach einem Rebuild bleibt die Bedienungshilfen-Freigabe eventuell hängen, dann mit `tccutil reset Accessibility com.pais.handy` zurücksetzen (siehe `BUILD.md` → Troubleshooting). Dasselbe kann für die neue Audio-Berechtigung gelten.
- Berechtigungsdialoge und die Installation der Xcode Command Line Tools muss der Besitzer selbst bestätigen.

### Erste Schritte für die lokale Sitzung

1. ~~Mit `sw_vers` die Version prüfen und danach die API wählen.~~ Erledigt: 26.6.2, also Process Taps.
2. ~~Toolchain prüfen bzw. installieren.~~ Erledigt.
3. ~~`bun install`, VAD-Modell, `bun run tauri dev` als Baseline.~~ Erledigt.
4. ~~**Spike:** 10 s System-Ton als WAV.~~ Erledigt, Ton kommt an (siehe oben).
5. ~~Stufe 1 umsetzen.~~ Erledigt. **Offen:** vom Besitzer mit einem echten Meeting testen lassen, danach Stufe 2.
6. Vor jedem Commit `bun run lint`, `bun run format`, `cargo clippy` und `bun run check:translations` laufen lassen. Commits mit Conventional-Commit-Präfix (siehe `AGENTS.md`).
