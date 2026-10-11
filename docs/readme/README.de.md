# Squeeze PDF

[English](README.en.md) · [简体中文](../../README.md) · [Français](README.fr.md) · [Español](README.es.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · **Deutsch** · [Português](README.pt.md)

Ein quelloffener, vollständig offline arbeitender PDF-Kompressor für Windows und macOS, derzeit im MVP-Stadium. Die Oberfläche nutzt Tauri 2 und React. Rust implementiert Kompressionsstrategien, Bildverarbeitung, Seitenrekonstruktion und Dateischutz. lopdf liest und schreibt PDFs; die Desktop-App rendert mit lokalem PDF.js, die CLI standardmäßig mit hayro. MozJPEG wird über Rust statisch eingebunden. Zur Laufzeit sind weder qpdf noch Ghostscript oder Python erforderlich.

Installationspakete für Windows x64, macOS Apple silicon und macOS Intel stehen unter [GitHub Releases](https://github.com/SSToo/Squeeze-PDF/releases) bereit. Ein `v`-Tag, das der App-Version entspricht, veröffentlicht nach drei erfolgreichen Builds automatisch die Installationspakete und Prüfsummen. Siehe [Veröffentlichungsanleitung](../RELEASING.md) (Chinesisch/Englisch).

## Lizenz

Der eigene Projektcode steht unter [Apache-2.0](../../LICENSE), mit Urheberhinweisen in [NOTICE](../../NOTICE). Kommerzielle Nutzung und abgeleitete Produkte mit geschlossenem Quellcode sind erlaubt, sofern Lizenzbedingungen wie die Beibehaltung von Hinweisen und Kennzeichnung von Änderungen erfüllt werden. Umfang und Bedingungen der Patentlizenz ergeben sich aus dem Lizenztext.

Bibliotheken, Schriften und Ressourcen Dritter behalten ihre eigenen Lizenzen. Distributionen enthalten `third-party/PROJECT-LICENSE.txt`, `PROJECT-NOTICE.txt`, Hinweise Dritter und erforderliche Quellmaterialien. Siehe [Mitwirken](../../CONTRIBUTING.md) und die [Dokumentation zur Open-Source-Vorbereitung](../OPEN-SOURCE.md).

## Zwei Ausgabemodi

| Modus                      | Ergebnis                                                                               | Typischer Einsatz                                                      |
| -------------------------- | -------------------------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| Originalinhalt erhalten    | Erhält Text, Schriften und Vektoren; optimiert Pfade, Struktur und unterstützte Bilder | Berichte, Verträge, Zeichnungen, auswählbarer Text und Vergrößerung    |
| Seiten in Bilder umwandeln | Rendert jede Seite als eigenes Bild und erstellt daraus ein PDF                        | Einheitliche Bildseiten ohne Bedarf an auswählbarem Text oder Vektoren |

Ausgabemodus und Qualitätsstufe werden unabhängig gewählt:

| Stufe                      | Originalinhalt                                                                        | Bildseiten                                              |
| -------------------------- | ------------------------------------------------------------------------------------- | ------------------------------------------------------- |
| Hohe Qualität (beide Modi) | Ursprüngliche Bildauflösung, JPEG 95, Pfadpräzision und Wiederverwendung von Konturen | 300 DPI, verlustfreie Flate-Kodierung gerenderter Pixel |
| Ausgewogen                 | Pfadoptimierung, JPEG 85, bis 200 DPI                                                 | 1400 px breit, JPEG-Qualität 72                         |
| Kleiner                    | Pfadoptimierung, JPEG 50, bis 100 DPI                                                 | 1000 px breit, JPEG-Qualität 60                         |

Hohe Qualität erhält die Bildabmessungen, kann aber Pixel verändern und ist **nicht streng verlustfrei**. Das Runden von Vektorkoordinaten begrenzt den Fehler unter bekannten Transformationen auf etwa 0,0005 Punkt je Achse. Textpositionen und Farbwerte werden nicht gerundet; sehr feine Kanten können sich dennoch ändern. Geeignete geschlossene, einfarbig gefüllte Konturen werden als Vektor-Forms wiederverwendet.

Hohe Qualität kodiert bereits gerenderte Pixel verlustfrei; Text und Vektoren wurden zuvor gerastert. Der Bildmodus erhält keinen auswählbaren Text, keine Vektorpfade, Links oder Lesezeichen. Die Desktop-App zeichnet renderbare sichtbare Formulare und Anmerkungen in die Bilder. Versteckte Felder und Text aus Anmerkungs-Pop-ups bleiben nicht erhalten. Daten eingebetteter Anhänge werden beibehalten.

Der Erhaltungsmodus exportiert nur ein validiertes PDF, das kleiner als das Original ist. Kandidaten werden als vollständige Dateien verglichen; bei Wachstum wird zurückgefallen. Der Bildmodus erzeugt das gewählte Format auch bei größerer Datei und zeigt den Zuwachs an. Originale werden nur gelesen, bestehende Ausgaben niemals überschrieben. Änderungen an der Eingabe während der Verarbeitung verhindern das Abschließen der Ausgabe.

Die Engine vergleicht je nach Unterstützung JPEG, exaktes Flate, Indexed-Paletten, 1-Bit- und CCITT-G4-Kandidaten. Hinzu kommen JPEG-Koeffizientenoptimierung, gemeinsame Ressourcen, eingeschränkte CID-Schriftuntergruppen und Dateilayouts. Die Desktop-App kann innerhalb von Budgets und Qualitätsschwellen direktes Rendern mit 2×-Rendern und anschließendem Verkleinern vergleichen. Siehe [Implementierungs- und Validierungsgrenzen](../VALIDATION.md).

## Desktop-Funktionen

- Mehrere PDFs hineinziehen oder auswählen, Duplikate entfernen und nacheinander verarbeiten.
- Zwei Modi, drei Qualitätsstufen, Verarbeitungsphasen und Anzahl abgeschlossener Dateien.
- Vorher-/Nachher-Größe, Einsparung oder Zuwachs, Details und Fehler direkt in der Dateizeile.
- Original und Ergebnis nebeneinander mit Seitenwechsel und Zoom; sämtliche Vorschauressourcen sind lokal.
- Ausgabeordner, automatische Nummerierung, Abbrechen, Wiederholen sowie Ergebnis oder Ordner öffnen.
- Modus, Stufe, Ordner und Design werden gespeichert; die Dateiwarteschlange gilt nur für die aktuelle Sitzung.
- Das Sprachsymbol rechts vom Designschalter öffnet Englisch, vereinfachtes Chinesisch, Französisch, Spanisch, Koreanisch, Japanisch, Deutsch und Portugiesisch. Anfangs gilt eine unterstützte Systemsprache, sonst Englisch; spätere Auswahl wird gespeichert.
- Sprachwechsel aktualisiert Oberfläche, Phasen, Fehler und Vorschau ohne Aufgaben zu unterbrechen. Dateinamen, Pfade und unbekannte technische Fehler bleiben unverändert.
- Transparentes Logo, rote Akzente und ein dunkles Design mit anthrazitfarbenem Hintergrund.

## Entwicklung

Benötigt werden Node.js 22.13+, Rust 1.92.0 gemäß `rust-toolchain.toml` und ein C-Compiler der Plattform. macOS benötigt Xcode Command Line Tools, Windows MSVC C++ Build Tools und WebView2. MozJPEG wird in den Worker kompiliert. Optionales Assembler-SIMD ist deaktiviert; NASM und CMake sind nicht erforderlich.

Alle Befehle im Repository-Stamm ausführen:

```sh
npm ci
npm run tauri -- dev
```

`npm run preview` startet nur das Frontend; native Dateifunktionen benötigen `tauri dev`. Skripte bereiten Worker, lokales PDF.js und Lizenzmaterialien vor. `private: true` verhindert versehentliches Veröffentlichen auf npm und ändert die Open-Source-Lizenz nicht.

## Prüfungen und Pakete

```sh
cargo test --workspace --locked
node scripts/prepare-worker.mjs
npm run notices
npm run test:licenses
npm run check
npm run test:render
npm run test:i18n
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings

# macOS-App
npm run tauri -- build --bundles app
# macOS-App und einfaches DMG
npm run package:mac
# EXE-Installer; unter Windows ausführen
npm run tauri -- build --bundles nsis
```

Pakete liegen unter `target/release/bundle/` für die Host-Architektur. Bei `--target` zusätzlich `CARGO_BUILD_TARGET` setzen, damit der Worker übereinstimmt. Der Windows-Installer enthält den Offline-Installer für WebView2.

CI ist für Windows x64, macOS arm64 und Intel eingerichtet. Lokal wurde auf macOS arm64 geprüft; Installationstests für Windows/Intel stehen aus. macOS verwendet ad-hoc-Signaturen für lokale Tests. Produktive Windows-Signierung, macOS Developer ID und Notarisierung sind noch nicht eingerichtet. Zur Veröffentlichung `APPLE_SIGNING_IDENTITY` setzen und die [Tauri-Dokumentation](https://v2.tauri.app/distribute/sign/macos/) beachten.

## CLI und Engine

```sh
cargo run -p squeeze-cli -- inspect "input.pdf"
cargo run -p squeeze-cli -- compress "input.pdf" -o "fidelity.pdf" --output-mode preserve --preset fidelity --compact-paths --reuse-paths --json
cargo run -p squeeze-cli -- compress "input.pdf" -o "balanced.pdf" --output-mode preserve --preset balanced --json
cargo run -p squeeze-cli -- compress "input.pdf" -o "raster.pdf" --output-mode raster --preset small --json
```

`--output-mode`: standardmäßig `preserve`, alternativ `raster`. Stufen: `lossless`, `fidelity`, `balanced`, `small`; unterstützte Stufen erlauben `--quality 85 --dpi 200`. `--compact-paths` und `--reuse-paths` aktivieren Pfadpräzision und Konturwiederverwendung. In der CLI standardmäßig aus, in allen drei Erhaltungsstufen der Desktop-App an.

`lossless` quantisiert Bilder nicht neu, verkleinert sie nicht und ignoriert die beiden Vektoroptionen. Exakte Kodierungen, JPEG-Koeffizientenoptimierung und äquivalente Ressourcenteilung bleiben möglich. `fidelity` erhält Abmessungen, nutzt standardmäßig JPEG 95 und akzeptiert keine Ziel-DPI. Hohe Qualität im Bildmodus kodiert Pixel verlustfrei. Strg+C fordert den Abbruch an. Die Desktop-App beendet dann das Rendern oder den separaten Worker. Seitenrendering und Worker-Kodierung haben jeweils ein Limit von 120 Sekunden.

Kern-APIs: `squeeze_core::inspect_file`, `squeeze_core::audit_file`, `squeeze_core::compress`. `squeeze audit input.pdf` liefert eine nur lesende Größenaufschlüsselung. Kompressions-JSON beschreibt Größen, Phasen, Bildkandidaten, Laufzeiten, Cache und Schreibauswahl. Stream-/Kandidatenkosten sind nicht die vollständige PDF-Größe und bedeuten keine automatisch entfernbaren Daten.

## Unterstützung und Grenzen

- Keine feste PDF-Größengrenze; mehr als 256 MiB sind je nach verfügbarem Speicher möglich. Dekomprimierte Streams maximal 64 MiB, Bilder maximal 24 Millionen Pixel; sequenzielle Verarbeitung.
- Optimierung von RGB/Gray sowie eingeschränkten ICCBased-, Indexed-, G4- und Rohfarbkomponenten-Eingaben. Unbekannte Kodierungen/Abbildungen werden vorsichtig erhalten. Einige CMYK/YCCK-JPEGs erlauben Koeffizientenoptimierung ohne Pixel-Neukodierung. Abmessungen und Farb-/Decode-/Maskensemantik werden bei Bedarf geschützt; keine universelle Codec-Unterstützung.
- Unbekannte Zeichnungsskalierung oder unvollständige Analyse deaktiviert das Verkleinern betroffener Bilder. Hohe Qualität erhält stets die ursprünglichen Bildabmessungen.
- Der Erhaltungsmodus bewahrt Formulare, Anmerkungen und Anhänge und schützt Darstellungsressourcen sowie gemeinsam verwendete Bilder. Unabhängige normale Seitenbilder bleiben optimierbar. Formulare/Anmerkungen mit Darstellungen deaktivieren konservativ die beiden verlustbehafteten Vektoroptimierungen. Verschlüsselte, digital signierte, PDF/A-, XFA- und Portfolio-PDFs werden übersprungen.
- Der Desktop-Bildmodus akzeptiert renderbare Formulare/Anmerkungen, normale Links, leere Formularcontainer und Anhänge. Fehlende zeichnbare Darstellungen oder beschädigte Anmerkungen führen zu Seitenfehlern. CLI-hayro lehnt zu zeichnende Formulare/Anmerkungen und `BrotliDecode`-Streams ab; dafür Desktop-PDF.js verwenden.
- Rotation, sichtbarer Ausschnitt und physische Seitengröße werden berücksichtigt. Fehlende eingebettete Nichtstandardschriften verhindern die Konvertierung. Komplexe Seiten visuell prüfen; hayro entwickelt sich weiter und garantiert keine vollständige Kompatibilität mit kommerziellen PDFs.
- Der Parser kann Querverweise reparieren; nicht jedes fehlerhafte PDF wird strikt abgelehnt. Vorhandene Grenzen decken nicht sämtliche Speicherzuweisungen ab.
- Reguläres Abbrechen entfernt temporäre Verzeichnisse beendeter Worker. Startbereinigung nach erzwungenem Beenden oder Stromausfall ist nicht implementiert.
- Keine OCR, keine PDF-Bearbeitung, keine garantierte Zieldateigröße und kein kommerzielles Aktivierungssystem.

## Benchmarks und technische Aufzeichnungen

```sh
node scripts/benchmark.mjs
node scripts/benchmark-round1.mjs tmp/round1-benchmark /path/to/baseline-worker
cargo run -p squeeze-core --example make_sample -- tmp/samples --large
```

Die große Probe erzeugt zusätzlich ein PDF mit etwa 138 MB und 24 Seiten für Abbruch- und Großdateitests. Synthetische Kompressionsraten sagen reale Dokumente nicht voraus. Poppler, pypdf, Pillow und NumPy dienen unabhängigen Entwicklungsprüfungen und sind keine Laufzeitabhängigkeiten.

Ausführliche Aufzeichnungen derzeit auf Chinesisch: [Validierung](../VALIDATION.md), [Plan](../../PLAN.md). Jpegli und Brotli-Ausgabe sind Experimente, keine Produktabhängigkeiten oder Ausgabeoptionen.

## Materialien Dritter

Lockfiles fixieren Abhängigkeiten. `npm run notices` sammelt Lizenztexte, hayro NOTICE, PDF.js-Schrift-/WASM-Hinweise, QuickJS/Mozilla-MIT-Hinweise, unveränderte MPL-Quellen und Liberation-1.07.4-Quellen sowie editierbare TTX-Quellen der tatsächlich ausgelieferten Schriften. Alles liegt unter `third-party/`; das Inventar enthält vorsorglich Build-Abhängigkeiten. Siehe [Quellen und Wiederherstellung](../../licenses/SOURCE-CODE.md) und die [Lizenzaufzeichnung](../LICENSING-0.1.22.md).

`npm run test:licenses` prüft Texte, Asset-/Quellhashes und MPL-Vollständigkeit in Windows/macOS-CI. Änderungen an PDF.js oder Ressourcen erfordern erneute Prüfung. `scripts/make-font-sources.py` benötigt FontTools 4.66.1 nur als Entwicklungswerkzeug. Liberation behält die GPL mit Schriftausnahmen; andere Komponenten behalten ihre jeweiligen Lizenzen.

PDF: [lopdf](https://docs.rs/lopdf/0.45.0/lopdf/); Rendering: [PDF.js](https://mozilla.github.io/pdf.js/getting_started/) oder [hayro](https://github.com/LaurenzV/hayro); Dekodierung: image; JPEG: MozJPEG; G4: MIT-lizenziertes `fax 0.3.0`; eingeschränktes CID-TrueType-Subsetting: MIT/Apache-2.0 `subsetter 0.2.6`. Testschriften werden nicht verteilt. Eigener Code ist seit 0.1.23 Apache-2.0; das ändert keine Drittanbieterlizenz.
