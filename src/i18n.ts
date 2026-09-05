type Dict = Record<string, string>;

const de: Dict = {
  "toolbar.settings": "Einstellungen",
  "toolbar.history": "Verlauf",
  "toolbar.theme": "Hell / Dunkel",
  "toolbar.download": "Herunterladen",
  "panel.back": "Zurück",
  "action.wallpaper": "Als Hintergrund",
  "action.lockscreen": "Als Sperrbildschirm",
  "panel.settingsTitle": "Einstellungen",
  "panel.historyTitle": "Verlauf",
  "field.provider": "Quelle",
  "field.unsplashKey": "Unsplash Access Key",
  "field.terms": "Suchbegriffe",
  "field.history": "Bilder im Verlauf",
  "field.historyMode": "Verlauf speichert",
  "historyMode.loaded": "Jedes geladene Bild",
  "historyMode.applied": "Nur als Hintergrund gesetzte",
  "field.appearance": "Erscheinungsbild",
  "field.language": "Sprache",
  "field.lockscreen": "Sperrbildschirm",
  "settings.resetLockscreen": "Auf Windows-Standard zurücksetzen",
  "theme.system": "System",
  "theme.light": "Hell",
  "theme.dark": "Dunkel",
  "placeholder.terms": "z. B. ocean; mountains, forest",
  "placeholder.unsplashKey": "von unsplash.com/developers",
  "hint.terms": "durch Komma oder Semikolon getrennt (werden kombiniert) – leer = zufällig",
  "hint.noSearch": "Diese Quelle unterstützt keine Suche.",
  "history.empty": "Noch keine Bilder im Verlauf.",
  "history.clear": "Verlauf leeren",
  "history.selMode": "Auswahl-Modus",
  "history.selectAll": "Alle auswählen",
  "history.deselect": "Auswahl aufheben",
  "history.deleteTitle": "Ausgewählte Bilder aus dem Verlauf löschen",
  "history.selected": "{n} ausgewählt",
  "history.ctxOpen": "Anzeigen",
  "history.ctxDelete": "Aus dem Verlauf löschen",
  "configPath": "Einstellungen: {path}",
  "credit.source": "Quelle: ",
  "credit.photographer": "Fotograf: ",
  "credit.originalTooltip": "Originalbild",
  "toast.loadFailed": "Bild konnte nicht geladen werden: {err}",
  "toast.saveFailed": "Einstellungen nicht gespeichert: {err}",
  "toast.startFailed": "Start fehlgeschlagen: {err}",
  "toast.soon": "Diese Funktion folgt in einem späteren Schritt.",
  "toast.wallpaperSet": "Als Hintergrund gesetzt.",
  "toast.wallpaperFailed": "Hintergrund konnte nicht gesetzt werden: {err}",
  "toast.lockscreenSet": "Sperrbildschirm gesetzt (wird beim nächsten Sperren übernommen).",
  "toast.lockscreenFailed": "Sperrbildschirm konnte nicht gesetzt werden: {err}",
  "toast.lockscreenReset": "Sperrbildschirm auf Windows-Standard zurückgesetzt.",
  "toast.lockscreenResetFailed": "Zurücksetzen fehlgeschlagen: {err}",
  "toast.downloadSaved": "Bild gespeichert.",
  "toast.downloadFailed": "Bild konnte nicht gespeichert werden: {err}",
  "dialog.saveTitle": "Bild speichern",
  "stage.flip": "Spiegeln",
  "toast.flipFailed": "Bild konnte nicht gespiegelt werden: {err}",
  "win.minimize": "Minimieren",
  "win.close": "Schließen",
  "nav.older": "Vorheriges Bild",
  "nav.newer": "Nächstes Bild",
};

const en: Dict = {
  "toolbar.settings": "Settings",
  "toolbar.history": "History",
  "toolbar.theme": "Light / dark",
  "toolbar.download": "Download",
  "panel.back": "Back",
  "action.wallpaper": "Set as wallpaper",
  "action.lockscreen": "Set as lock screen",
  "panel.settingsTitle": "Settings",
  "panel.historyTitle": "History",
  "field.provider": "Source",
  "field.unsplashKey": "Unsplash access key",
  "field.terms": "Search terms",
  "field.history": "Images in history",
  "field.historyMode": "History saves",
  "historyMode.loaded": "Every loaded image",
  "historyMode.applied": "Only images set as wallpaper",
  "field.appearance": "Appearance",
  "field.language": "Language",
  "field.lockscreen": "Lock screen",
  "settings.resetLockscreen": "Reset to Windows default",
  "theme.system": "System",
  "theme.light": "Light",
  "theme.dark": "Dark",
  "placeholder.terms": "e.g. ocean; mountains, forest",
  "placeholder.unsplashKey": "from unsplash.com/developers",
  "hint.terms": "comma or semicolon separated (combined into one search) – empty = random",
  "hint.noSearch": "This source does not support search.",
  "history.empty": "No images in history yet.",
  "history.clear": "Clear history",
  "history.selMode": "Selection mode",
  "history.selectAll": "Select all",
  "history.deselect": "Clear selection",
  "history.deleteTitle": "Remove selected images from history",
  "history.selected": "{n} selected",
  "history.ctxOpen": "Show",
  "history.ctxDelete": "Remove from history",
  "configPath": "Settings: {path}",
  "credit.source": "Source: ",
  "credit.photographer": "Photographer: ",
  "credit.originalTooltip": "Original image",
  "toast.loadFailed": "Could not load image: {err}",
  "toast.saveFailed": "Could not save settings: {err}",
  "toast.startFailed": "Startup failed: {err}",
  "toast.soon": "This feature is coming in a later step.",
  "toast.wallpaperSet": "Wallpaper set.",
  "toast.wallpaperFailed": "Could not set the wallpaper: {err}",
  "toast.lockscreenSet": "Lock screen set (applies on next lock).",
  "toast.lockscreenFailed": "Could not set the lock screen: {err}",
  "toast.lockscreenReset": "Lock screen reset to the Windows default.",
  "toast.lockscreenResetFailed": "Reset failed: {err}",
  "toast.downloadSaved": "Image saved.",
  "toast.downloadFailed": "Could not save the image: {err}",
  "dialog.saveTitle": "Save image",
  "stage.flip": "Flip",
  "toast.flipFailed": "Could not flip the image: {err}",
  "win.minimize": "Minimize",
  "win.close": "Close",
  "nav.older": "Previous image",
  "nav.newer": "Next image",
};

const dicts: Record<string, Dict> = { de, en };

let current = "en";

export function setLanguage(lang: string) {
  current = dicts[lang] ? lang : "en";
  document.documentElement.lang = current;
}

export function t(key: string, params?: Record<string, string | number>): string {
  let s = dicts[current]?.[key] ?? en[key] ?? key;
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      s = s.replace(`{${k}}`, String(v));
    }
  }
  return s;
}

/** Fill every element carrying a data-i18n* attribute with the current language. */
export function applyI18n(root: ParentNode = document) {
  root.querySelectorAll<HTMLElement>("[data-i18n]").forEach((el) => {
    el.textContent = t(el.dataset.i18n!);
  });
  root.querySelectorAll<HTMLInputElement>("[data-i18n-placeholder]").forEach((el) => {
    el.placeholder = t(el.dataset.i18nPlaceholder!);
  });
  root.querySelectorAll<HTMLElement>("[data-i18n-title]").forEach((el) => {
    const label = t(el.dataset.i18nTitle!);
    el.title = label;
    el.setAttribute("aria-label", label);
  });
}
