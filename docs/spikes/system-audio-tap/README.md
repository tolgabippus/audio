# Spike: System-Ton per Core Audio Process Tap

Minimales Testprogramm (macOS 14.2+), das 10 s System-Ton als WAV aufnimmt. Die Aufnahme beginnt beim ersten hörbaren Ton. Es dient als Vorlage für die Swift-Brücke der Meeting-Aufnahme. Ergebnisse und Stolperfallen stehen in `CLAUDE.md` unter „Ergebnis des Spikes“.

Bauen und starten (aus diesem Ordner, Ausgabe in einen beliebigen Ordner `OUT`):

```bash
OUT="$TMPDIR/system-audio-spike"
mkdir -p "$OUT/SystemAudioSpike.app/Contents/MacOS"
cp Info.plist "$OUT/SystemAudioSpike.app/Contents/Info.plist"
xcrun swiftc -swift-version 5 -O -target x86_64-apple-macos14.2 main.swift -o "$OUT/SystemAudioSpike.app/Contents/MacOS/SystemAudioSpike"
codesign --force --sign - "$OUT/SystemAudioSpike.app"
open -W --stdout "$OUT/spike.log" --stderr "$OUT/spike.log" "$OUT/SystemAudioSpike.app" --args "$OUT/system-audio-test.wav"
cat "$OUT/spike.log"
```

Auf Apple Silicon `-target arm64-apple-macos14.2` verwenden.
