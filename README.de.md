[English](README.md) · **Deutsch**

![Wowl logo](docs/logo.png)

# Wowl

**Desktop- und Sperrbildschirm-Wallpaper von Unsplash, Bing & Co. – suchen, blättern, setzen.**

Modern · minimalistisch · leichtgewichtig · portable

***

![Wowl Screenshot](docs/screenshot.png)

## Download

Windows ist bereits verfügbar — siehe [Releases](https://github.com/naderi/wowl/releases) für die portable `.exe` und den Installer. macOS- und Linux-Versionen sind geplant.

## Funktionen

- **Bildquellen**: Unsplash (mit Suche), Bing – Bild des Tages, Lorem Picsum
- **Ein Klick aufs Bild** lädt das nächste; **Pfeile / ← →** blättern durch den Verlauf
- **Als Hintergrund** und **Als Sperrbildschirm** setzen (Windows)
- **Bild spiegeln** (horizontal) per Rechtsklick
- **Download** über Speichern-unter-Dialog (merkt sich den Ordner)
- **Verlauf** mit Kachel-Raster, Mehrfachauswahl und Kontextmenü; FIFO-begrenzt, ohne Doubletten
- **Suchbegriffe** frei definierbar (Komma/Semikolon-getrennt, werden kombiniert)
- **Rahmenloses Fenster** mit eigener Titelleiste, festes 16:9-Format
- **Hell / Dunkel / System** und **Deutsch / Englisch**, umschaltbar zur Laufzeit
- **Portable**: Einstellungen liegen als `wowl.toml` neben der `.exe`

## Bildquellen

| Quelle                    | Suche |      API-Key       | Auflösung                      |
| ------------------------- | :---: | :----------------: | ------------------------------ |
| **Unsplash**              |   ✅   | eigener Access Key | Monitor-Auflösung              |
| **Bing – Bild des Tages** |   –   |         –          | UHD (3840×2160), letzte 8 Tage |
| **Lorem Picsum**          |   –   |         –          | Monitor-Auflösung              |

### Warum Unsplash einen eigenen API-Schlüssel braucht

Unsplash begrenzt die API-Nutzung pro Anwendung, nicht pro Endnutzer – ein in Wowl fest eingebauter Schlüssel würde von allen, die die App heruntergeladen haben, gemeinsam genutzt und das Anfragelimit von Unsplash für alle innerhalb von Minuten erreichen. Einen privaten Schlüssel in einer öffentlich verteilten App auszuliefern verstößt außerdem gegen die Nutzungsbedingungen von Unsplash. Ein eigener, kostenloser Schlüssel hält die eigene Nutzung von der aller anderen getrennt und ist in einer Minute erstellt:

1. Auf [unsplash.com/developers](https://unsplash.com/developers) eine kostenlose App anlegen
2. Den **Access Key** kopieren
3. In Wowl: Einstellungen → Quelle „Unsplash" → Key eintragen

Demo-Apps von Unsplash sind auf **50 Anfragen/Stunde** begrenzt; für mehr in der
Unsplash-App „Production"-Zugang beantragen.

## Nutzung

Wowl ist portabel – einfach `Wowl.exe` ausführen. Die Datei `wowl.toml` mit den
Einstellungen wird daneben angelegt (bzw. in `%APPDATA%\Wowl`, falls der
Programmordner schreibgeschützt ist). Der Bild-Cache liegt in `history/`.

### Tastenkürzel

| Taste                                         | Aktion                                                                 |
| ---------------------------------------------- | ------------------------------------------------------------------------ |
| `←` / `→`                                     | vorheriges / nächstes Bild aus dem Verlauf                             |
| `Esc`                                         | Menü schließen · Auswahl aufheben · Auswahl-Modus verlassen · Seite zu |
| `Enter` (in Einstellungen)                    | speichern und zurück                                                   |
| `Strg`+`A` / `Entf` (Verlauf, Auswahl-Modus)  | alle auswählen / löschen                                               |

### Sperrbildschirm (Windows)

„Als Sperrbildschirm" setzt das Bild über die `PersonalizationCSP`-Registry und
benötigt **eine UAC-Bestätigung** sowie **Windows Pro/Enterprise**. Windows
markiert die Sperrbildschirm-Einstellung danach als „von Ihrer Organisation
verwaltet" – mit **Einstellungen → Sperrbildschirm → „Auf Windows-Standard
zurücksetzen"** wird das rückgängig gemacht.

## Aus dem Quellcode bauen

Voraussetzungen: [Rust](https://rustup.rs), [Node.js](https://nodejs.org),
[pnpm](https://pnpm.io), und die
[Tauri-Systemabhängigkeiten](https://tauri.app/start/prerequisites/) (unter
Windows: WebView2, in Windows 10/11 vorinstalliert).

```bash
pnpm install
pnpm tauri dev      # Entwicklung mit Hot-Reload
pnpm tauri build    # portable Wowl.exe nach src-tauri/target/release/
```

## Technik

- **Backend**: Rust + [Tauri 2](https://tauri.app)
- **UI**: Vanilla TypeScript + [Vite](https://vitejs.dev), kein Framework
- **Bildverarbeitung**: [`image`](https://crates.io/crates/image) (Spiegeln)
- **Wallpaper**: [`wallpaper`](https://crates.io/crates/wallpaper) (Modus „Ausfüllen")

```
src/            UI (index.html, main.ts, i18n.ts, styles.css)
src-tauri/src/  config.rs · providers.rs · history.rs · lib.rs
```
