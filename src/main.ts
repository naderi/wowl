import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { save } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { applyI18n, setLanguage, t } from "./i18n";

const appWindow = getCurrentWindow();

/* ===== Types (mirror the Rust structs) ===== */
interface Settings {
  provider: string;
  search_terms: string;
  history_size: number;
  history_mode: string;
  theme: string;
  language: string;
  unsplash_key: string;
  last_save_dir: string;
}

interface Photo {
  id: string;
  provider: string;
  width: number;
  height: number;
  photographer: string;
  photographerUrl: string | null;
  sourceUrl: string | null;
  imageUrl: string;
  downloadTrigger: string | null;
}

interface PhotoResult {
  photo: Photo;
  dataUrl: string;
  supportsSearch: boolean;
}

interface ProviderInfo {
  id: string;
  label: string;
  supportsSearch: boolean;
  needsKey: boolean;
}

interface HistoryEntry {
  id: string;
  provider: string;
  photographer: string;
  photographerUrl: string | null;
  sourceUrl: string | null;
  imageUrl: string;
  file: string;
  savedAt: string;
}

/* ===== Element helpers ===== */
const $ = <T extends HTMLElement>(id: string): T => {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing #${id}`);
  return el as T;
};

const flip = $("flip");
const stage = $("stage");
const imgA = $<HTMLImageElement>("img-a");
const imgB = $<HTMLImageElement>("img-b");
const navLeft = $("nav-left");
const navRight = $("nav-right");
const credit = $("credit");
const creditSourceLine = $("credit-source-line");
const creditSource = $<HTMLAnchorElement>("credit-source");
const creditPhotogLine = $("credit-photog-line");
const creditPhotographer = $<HTMLAnchorElement>("credit-photographer");
const toastEl = $("toast");

const panelTitle = $("panel-title");
const settingsForm = $<HTMLFormElement>("settings-form");
const historyView = $("history-view");
const historyGrid = $("history-grid");
const historyEmpty = $("history-empty");
const historyBar = $("history-bar");
const historySel = $("history-sel");
const btnSelMode = $("btn-sel-mode");
const btnSelDelete = $("btn-sel-delete");
const btnSelAll = $("btn-sel-all");
const ctxMenu = $("ctx-menu");
const stageMenu = $("stage-menu");
const configPathEl = $("config-path");

const fProvider = $<HTMLSelectElement>("f-provider");
const fUnsplashKey = $<HTMLInputElement>("f-unsplash-key");
const fTerms = $<HTMLInputElement>("f-terms");
const fHistory = $<HTMLInputElement>("f-history");
const fHistoryMode = $<HTMLSelectElement>("f-history-mode");
const fTheme = $<HTMLSelectElement>("f-theme");
const fLanguage = $<HTMLSelectElement>("f-language");
const fieldKey = $("field-key");
const termsHint = $("terms-hint");

/* ===== State ===== */
let settings: Settings;
let providers: ProviderInfo[] = [];
let currentPhoto: Photo | null = null;
let configPath = "";
let panelMode: "settings" | "history" = "settings";
const imgs = [imgA, imgB];
let activeImg = 0;
let loading = false;

/* ===== Theme ===== */
function applyTheme(theme: string) {
  const root = document.documentElement;
  root.dataset.themeMode = theme; // system | light | dark → drives the toolbar icon
  if (theme === "light" || theme === "dark") {
    root.dataset.theme = theme;
  } else {
    delete root.dataset.theme;
  }
}

function cycleTheme() {
  const order = ["system", "light", "dark"];
  const next = order[(order.indexOf(settings.theme) + 1) % order.length];
  settings.theme = next;
  fTheme.value = next;
  applyTheme(next);
  void persist();
}

/* ===== Toast ===== */
let toastTimer: number | undefined;
function toast(message: string, isError = false) {
  toastEl.textContent = message;
  toastEl.classList.toggle("is-error", isError);
  toastEl.hidden = false;
  requestAnimationFrame(() => toastEl.classList.add("is-visible"));
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => {
    toastEl.classList.remove("is-visible");
    window.setTimeout(() => (toastEl.hidden = true), 220);
  }, 3200);
}

/* ===== Language ===== */
function applyLanguage(lang: string) {
  setLanguage(lang);
  applyI18n();
  refreshDynamicText();
}

/** Strings that are not simple static attributes and must be re-rendered on
    language change or when their data changes. */
function refreshDynamicText() {
  panelTitle.textContent =
    panelMode === "settings" ? t("panel.settingsTitle") : t("panel.historyTitle");
  syncProviderDependentUi();
  renderSelectionUi();
  if (currentPhoto) updateCredit(currentPhoto);
  if (configPath) configPathEl.textContent = t("configPath", { path: configPath });
}

/* ===== Photo pipeline ===== */
let staleObjectUrl: string | null = null;

function showImage(src: string, isObjectUrl = false) {
  const next = imgs[1 - activeImg];
  const prev = imgs[activeImg];
  next.onload = () => {
    next.classList.add("is-active");
    prev.classList.remove("is-active");
    activeImg = 1 - activeImg;
    if (staleObjectUrl) URL.revokeObjectURL(staleObjectUrl);
    staleObjectUrl = isObjectUrl ? src : null;
  };
  next.src = src;
}

/** "https://images.unsplash.com/…" → "Unsplash" */
function hostLabel(url: string | null): string {
  if (!url) return "";
  try {
    const host = new URL(url).hostname.replace(/^www\./, "");
    const base = host.split(".").slice(-2, -1)[0] || host;
    return base.charAt(0).toUpperCase() + base.slice(1);
  } catch {
    return "";
  }
}

function setCreditLine(
  line: HTMLElement,
  link: HTMLAnchorElement,
  text: string,
  href: string | null,
  tooltip: string,
) {
  if (!text) {
    line.hidden = true;
    return;
  }
  line.hidden = false;
  link.textContent = text;
  if (tooltip) link.title = tooltip;
  else link.removeAttribute("title");
  if (href) {
    link.href = href;
    link.style.pointerEvents = "";
  } else {
    link.removeAttribute("href");
    link.style.pointerEvents = "none";
  }
}

function updateCredit(p: Photo) {
  const sourceHref = p.sourceUrl ?? p.imageUrl;
  setCreditLine(
    creditSourceLine,
    creditSource,
    hostLabel(sourceHref),
    sourceHref,
    t("credit.originalTooltip"),
  );
  setCreditLine(creditPhotogLine, creditPhotographer, p.photographer, p.photographerUrl, "");
}

function suggestedFilename(p: Photo): string {
  const who = (p.photographer || p.provider || "image")
    .replace(/[<>:"/\\|?*]+/g, "")
    .trim()
    .slice(0, 60);
  return `Wowl - ${who}.jpg`;
}

async function downloadCurrent() {
  if (!currentPhoto) return;
  const name = suggestedFilename(currentPhoto);
  const defaultPath = settings.last_save_dir
    ? `${settings.last_save_dir.replace(/[/\\]$/, "")}/${name}`
    : name;
  let path: string | null;
  try {
    path = await save({
      title: t("dialog.saveTitle"),
      defaultPath,
      filters: [{ name: "JPEG", extensions: ["jpg", "jpeg"] }],
    });
  } catch (e) {
    toast(t("toast.downloadFailed", { err: String(e) }), true);
    return;
  }
  if (!path) return; // cancelled
  try {
    settings.last_save_dir = await invoke<string>("save_current_image", { path });
    toast(t("toast.downloadSaved"));
  } catch (e) {
    toast(t("toast.downloadFailed", { err: String(e) }), true);
  }
}

async function loadPhoto() {
  if (loading) return;
  loading = true;
  stage.classList.add("is-loading");
  try {
    const res = await invoke<PhotoResult>("next_photo");
    showImage(res.dataUrl);
    currentPhoto = res.photo;
    updateCredit(res.photo);
    await fetchHistory();
    historyIndex = historyEntries.findIndex((h) => h.id === currentPhoto?.id);
    updateArrows();
    if (panelMode === "history" && flip.classList.contains("is-flipped")) {
      void loadHistory();
    }
  } catch (e) {
    toast(t("toast.loadFailed", { err: String(e) }), true);
  } finally {
    loading = false;
    stage.classList.remove("is-loading");
  }
}

/* ===== History ===== */
let historyIO: IntersectionObserver | null = null;
let historyEntries: HistoryEntry[] = [];
/** index within historyEntries of the image currently shown on the photo view */
let historyIndex = 0;
const selectedIds = new Set<string>();
let lastClickedIndex = -1;
let selectionMode = false;

async function fetchHistory() {
  try {
    historyEntries = await invoke<HistoryEntry[]>("get_history");
  } catch (e) {
    historyEntries = [];
    toast(t("toast.loadFailed", { err: String(e) }), true);
  }
}

/* ===== Browse history from the photo view (‹ ›) ===== */
function updateArrows() {
  const n = historyEntries.length;
  navLeft.hidden = n === 0 || historyIndex >= n - 1 || historyIndex < 0;
  navRight.hidden = n === 0;
}

async function displayHistory(index: number) {
  const entry = historyEntries[index];
  if (!entry) return;
  historyIndex = index;
  try {
    const buf = await invoke<ArrayBuffer>("select_history_image", { id: entry.id });
    showImage(URL.createObjectURL(new Blob([buf])), true);
    currentPhoto = {
      id: entry.id,
      provider: entry.provider,
      width: 0,
      height: 0,
      photographer: entry.photographer,
      photographerUrl: entry.photographerUrl,
      sourceUrl: entry.sourceUrl,
      imageUrl: entry.imageUrl,
      downloadTrigger: null,
    };
    updateCredit(currentPhoto);
    updateArrows();
  } catch (e) {
    toast(t("toast.loadFailed", { err: String(e) }), true);
  }
}

async function navOlder() {
  if (historyIndex < 0) historyIndex = 0;
  if (historyIndex < historyEntries.length - 1) await displayHistory(historyIndex + 1);
}

async function navNewer() {
  if (historyIndex > 0) await displayHistory(historyIndex - 1);
  else void loadPhoto();
}
/** ids the currently-open context menu acts on */
let ctxTargets: string[] = [];
/** set briefly when a click dismisses the context menu, to swallow that click */
let menuDismissClick = false;

function revokeGridUrls() {
  historyGrid.querySelectorAll("img").forEach((im) => {
    if (im.src.startsWith("blob:")) URL.revokeObjectURL(im.src);
  });
}

function setSelectionMode(on: boolean) {
  selectionMode = on;
  if (!on) {
    selectedIds.clear();
    lastClickedIndex = -1;
  }
  closeMenus();
  renderSelectionUi();
}

function renderSelectionUi() {
  const present = new Set(historyEntries.map((e) => e.id));
  for (const id of [...selectedIds]) if (!present.has(id)) selectedIds.delete(id);

  const count = selectedIds.size;
  const total = historyEntries.length;

  btnSelMode.classList.toggle("is-active", selectionMode);
  btnSelMode.setAttribute("aria-pressed", String(selectionMode));

  historySel.hidden = !(selectionMode && count > 0);
  historySel.textContent = count ? t("history.selected", { n: count }) : "";

  btnSelAll.hidden = !selectionMode;
  btnSelDelete.hidden = !(selectionMode && count > 0);

  const allSelected = total > 0 && count === total;
  btnSelAll.classList.toggle("is-some", count > 0 && !allSelected);
  btnSelAll.classList.toggle("is-all", allSelected);
  const label = allSelected ? t("history.deselect") : t("history.selectAll");
  btnSelAll.title = label;
  btnSelAll.setAttribute("aria-label", label);

  historyView.classList.toggle("is-selecting", selectionMode);
  historyGrid.querySelectorAll<HTMLElement>(".tile").forEach((tile) => {
    tile.classList.toggle(
      "is-selected",
      selectionMode && selectedIds.has(tile.dataset.id!),
    );
  });
}

function clearSelection() {
  selectedIds.clear();
  lastClickedIndex = -1;
  renderSelectionUi();
}

/** Multi-function toggle: some/none → all → none. */
function cycleSelectAll() {
  const total = historyEntries.length;
  if (total > 0 && selectedIds.size === total) {
    selectedIds.clear();
    lastClickedIndex = -1;
  } else {
    for (const e of historyEntries) selectedIds.add(e.id);
  }
  renderSelectionUi();
}

function toggleSelect(id: string) {
  if (selectedIds.has(id)) selectedIds.delete(id);
  else selectedIds.add(id);
  renderSelectionUi();
}

function selectRange(toIndex: number) {
  if (lastClickedIndex < 0) {
    toggleSelect(historyEntries[toIndex].id);
    return;
  }
  const lo = Math.min(lastClickedIndex, toIndex);
  const hi = Math.max(lastClickedIndex, toIndex);
  for (let i = lo; i <= hi; i++) selectedIds.add(historyEntries[i].id);
  renderSelectionUi();
}

async function loadHistory() {
  await fetchHistory();
  if (currentPhoto) {
    const i = historyEntries.findIndex((e) => e.id === currentPhoto!.id);
    historyIndex = i >= 0 ? i : 0;
  }
  updateArrows();

  revokeGridUrls();
  historyIO?.disconnect();
  historyGrid.innerHTML = "";
  lastClickedIndex = -1;

  const empty = historyEntries.length === 0;
  historyEmpty.hidden = !empty;
  historyBar.hidden = empty;

  historyIO = new IntersectionObserver(
    (observed) => {
      for (const o of observed) {
        if (!o.isIntersecting) continue;
        const img = o.target as HTMLImageElement;
        historyIO?.unobserve(img);
        invoke<ArrayBuffer>("history_image", { id: img.dataset.id! })
          .then((buf) => {
            img.src = URL.createObjectURL(new Blob([buf]));
          })
          .catch(() => {});
      }
    },
    { root: historyGrid, rootMargin: "300px" },
  );

  historyEntries.forEach((entry, index) => {
    const tile = document.createElement("div");
    tile.className = "tile";
    tile.dataset.id = entry.id;
    tile.title = entry.photographer
      ? `${t("credit.photographer")}${entry.photographer}`
      : "";

    const img = document.createElement("img");
    img.dataset.id = entry.id;
    img.alt = entry.photographer;

    const check = document.createElement("span");
    check.className = "tile__check";
    check.textContent = "✓";

    tile.append(img, check);

    tile.addEventListener("click", (e) => {
      if (menuDismissClick) return;
      if (!selectionMode) {
        void showHistoryEntry(entry);
        return;
      }
      if (e.shiftKey) {
        e.preventDefault();
        selectRange(index);
        return;
      }
      toggleSelect(entry.id);
      lastClickedIndex = index;
    });

    tile.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      if (selectionMode) {
        if (!selectedIds.has(entry.id)) {
          selectedIds.add(entry.id);
          lastClickedIndex = index;
          renderSelectionUi();
        }
        openCtxMenu(e.clientX, e.clientY, [...selectedIds]);
      } else {
        openCtxMenu(e.clientX, e.clientY, [entry.id]);
      }
    });

    historyGrid.appendChild(tile);
    historyIO!.observe(img);
  });

  renderSelectionUi();
}

/* ===== Context menus ===== */
const menus = [ctxMenu, stageMenu];

function positionMenu(menu: HTMLElement, x: number, y: number) {
  menu.hidden = false;
  const r = menu.getBoundingClientRect();
  menu.style.left = `${Math.min(x, window.innerWidth - r.width - 8)}px`;
  menu.style.top = `${Math.min(y, window.innerHeight - r.height - 8)}px`;
}

function closeMenus() {
  for (const m of menus) m.hidden = true;
}

function anyMenuOpen() {
  return menus.some((m) => !m.hidden);
}

function openCtxMenu(x: number, y: number, targets: string[]) {
  ctxTargets = targets;
  (ctxMenu.querySelector('[data-action="open"]') as HTMLButtonElement).disabled =
    targets.length !== 1;
  const del = ctxMenu.querySelector('[data-action="delete"]') as HTMLButtonElement;
  del.textContent = `${t("history.ctxDelete")} (${targets.length})`;
  del.disabled = targets.length === 0;
  positionMenu(ctxMenu, x, y);
}

async function ctxAction(action: string) {
  const targets = ctxTargets;
  closeMenus();
  if (action === "open") {
    const entry = historyEntries.find((e) => e.id === targets[0]);
    if (entry) void showHistoryEntry(entry);
  } else if (action === "delete") {
    await deleteIds(targets);
  }
}

async function stageAction(action: string) {
  closeMenus();
  if (action === "flip") await flipCurrent();
  else if (action === "wallpaper") await setWallpaper();
  else if (action === "lockscreen") await setLockscreen();
}

async function flipCurrent() {
  if (!currentPhoto) return;
  try {
    showImage(await invoke<string>("flip_current"));
  } catch (e) {
    toast(t("toast.flipFailed", { err: String(e) }), true);
  }
}

/** history_mode "applied" adds the image on use — keep the local copy fresh. */
async function refreshAfterApply() {
  if (settings.history_mode !== "applied") return;
  await fetchHistory();
  historyIndex = historyEntries.findIndex((h) => h.id === currentPhoto?.id);
  updateArrows();
}

async function setWallpaper() {
  if (!currentPhoto) return;
  try {
    await invoke("set_wallpaper");
    toast(t("toast.wallpaperSet"));
    await refreshAfterApply();
  } catch (e) {
    toast(t("toast.wallpaperFailed", { err: String(e) }), true);
  }
}

async function setLockscreen() {
  if (!currentPhoto) return;
  try {
    await invoke("set_lockscreen");
    toast(t("toast.lockscreenSet"));
    await refreshAfterApply();
  } catch (e) {
    toast(t("toast.lockscreenFailed", { err: String(e) }), true);
  }
}

async function deleteIds(ids: string[]) {
  if (!ids.length) return;
  try {
    await invoke("delete_history", { ids });
    for (const id of ids) selectedIds.delete(id);
    await loadHistory();
  } catch (e) {
    toast(String(e), true);
  }
}

async function showHistoryEntry(entry: HistoryEntry) {
  const idx = historyEntries.findIndex((e) => e.id === entry.id);
  await displayHistory(idx >= 0 ? idx : 0);
  flipTo(null);
}

/* ===== Settings ===== */
function populateProviders() {
  fProvider.innerHTML = "";
  for (const p of providers) {
    const opt = document.createElement("option");
    opt.value = p.id;
    opt.textContent = p.label;
    fProvider.appendChild(opt);
  }
}

function providerById(id: string): ProviderInfo | undefined {
  return providers.find((p) => p.id === id);
}

function syncProviderDependentUi() {
  const p = providerById(fProvider.value);
  fieldKey.classList.toggle("is-hidden", !p?.needsKey);
  const searchable = p?.supportsSearch ?? false;
  fTerms.disabled = !searchable;
  termsHint.textContent = searchable ? t("hint.terms") : t("hint.noSearch");
}

function fillForm() {
  fProvider.value = settings.provider;
  fUnsplashKey.value = settings.unsplash_key;
  fTerms.value = settings.search_terms;
  fHistory.value = String(settings.history_size);
  fHistoryMode.value = settings.history_mode;
  fTheme.value = settings.theme;
  fLanguage.value = settings.language;
  syncProviderDependentUi();
}

function readForm(): Settings {
  return {
    provider: fProvider.value,
    unsplash_key: fUnsplashKey.value.trim(),
    search_terms: fTerms.value,
    history_size: Math.max(0, Math.min(500, parseInt(fHistory.value || "0", 10) || 0)),
    history_mode: fHistoryMode.value,
    theme: fTheme.value,
    language: fLanguage.value,
    last_save_dir: settings.last_save_dir,
  };
}

let persistTimer: number | undefined;
function persist() {
  clearTimeout(persistTimer);
  return new Promise<void>((resolve) => {
    persistTimer = window.setTimeout(async () => {
      try {
        await invoke("save_settings", { settings });
      } catch (e) {
        toast(t("toast.saveFailed", { err: String(e) }), true);
      }
      resolve();
    }, 350);
  });
}

/** Save immediately (cancel any pending debounced save) — used on Enter / closing settings. */
async function saveSettingsNow() {
  clearTimeout(persistTimer);
  settings = readForm();
  try {
    await invoke("save_settings", { settings });
  } catch (e) {
    toast(t("toast.saveFailed", { err: String(e) }), true);
  }
}

function onFormChange() {
  const prev = settings;
  settings = readForm();
  if (settings.theme !== prev.theme) applyTheme(settings.theme);
  if (settings.language !== prev.language) applyLanguage(settings.language);
  else syncProviderDependentUi();
  void persist();
  if (settings.provider !== prev.provider) void loadPhoto();
}

/* ===== Flip navigation ===== */
function flipTo(mode: "settings" | "history" | null) {
  closeMenus();
  if (mode !== "history") setSelectionMode(false);
  if (mode === null) {
    flip.classList.remove("is-flipped");
    return;
  }
  panelMode = mode;
  panelTitle.textContent =
    mode === "settings" ? t("panel.settingsTitle") : t("panel.historyTitle");
  settingsForm.hidden = mode !== "settings";
  historyView.hidden = mode !== "history";
  flip.classList.add("is-flipped");
  if (mode === "history") void loadHistory();
}

/* ===== Wiring ===== */
function wire() {
  stage.addEventListener("click", (e) => {
    if (menuDismissClick) return;
    if ((e.target as HTMLElement).closest(".toolbar, .actionbar, .nav-arrow")) return;
    void loadPhoto();
  });

  navLeft.addEventListener("click", () => void navOlder());
  navRight.addEventListener("click", () => void navNewer());

  $("btn-settings").addEventListener("click", () => flipTo("settings"));
  $("btn-history").addEventListener("click", () => flipTo("history"));
  $("btn-back").addEventListener("click", () => flipTo(null));
  $("btn-theme").addEventListener("click", cycleTheme);

  for (const id of ["btn-min", "btn-min-2"]) {
    $(id).addEventListener("click", () => void appWindow.minimize());
  }
  for (const id of ["btn-close", "btn-close-2"]) {
    $(id).addEventListener("click", () => void appWindow.close());
  }

  $("btn-clear-history").addEventListener("click", async () => {
    try {
      await invoke("clear_history");
      clearSelection();
      await loadHistory();
    } catch (e) {
      toast(String(e), true);
    }
  });

  btnSelMode.addEventListener("click", () => setSelectionMode(!selectionMode));
  btnSelDelete.addEventListener("click", () => void deleteIds([...selectedIds]));
  btnSelAll.addEventListener("click", cycleSelectAll);

  ctxMenu.addEventListener("click", (e) => {
    const btn = (e.target as HTMLElement).closest("button");
    if (btn && !btn.disabled && btn.dataset.action) void ctxAction(btn.dataset.action);
  });
  stageMenu.addEventListener("click", (e) => {
    const btn = (e.target as HTMLElement).closest("button");
    if (btn && !btn.disabled && btn.dataset.action) void stageAction(btn.dataset.action);
  });

  stage.addEventListener("contextmenu", (e) => {
    if ((e.target as HTMLElement).closest(".toolbar, .actionbar, .nav-arrow")) return;
    e.preventDefault();
    if (currentPhoto) positionMenu(stageMenu, e.clientX, e.clientY);
  });

  document.addEventListener("pointerdown", (e) => {
    if (anyMenuOpen() && !menus.some((m) => m.contains(e.target as Node))) {
      closeMenus();
      // Swallow the click that dismisses the menu so it doesn't also open an image.
      menuDismissClick = true;
      setTimeout(() => (menuDismissClick = false), 0);
    }
  });
  document.addEventListener("scroll", closeMenus, true);
  window.addEventListener("blur", closeMenus);

  // Suppress the native context menu everywhere except text fields.
  document.addEventListener("contextmenu", (e) => {
    if (!(e.target as HTMLElement).closest("input, textarea")) e.preventDefault();
  });

  $("btn-lockscreen").addEventListener("click", () => void setLockscreen());
  $("btn-download").addEventListener("click", () => void downloadCurrent());
  $("btn-wallpaper").addEventListener("click", () => void setWallpaper());

  credit.addEventListener("click", (e) => {
    const link = (e.target as HTMLElement).closest("a");
    if (!link) return;
    e.preventDefault();
    if (link.href) void openUrl(link.href);
  });

  $("btn-reset-lockscreen").addEventListener("click", async (e) => {
    const btn = e.currentTarget as HTMLButtonElement;
    btn.disabled = true;
    try {
      await invoke("reset_lockscreen");
      toast(t("toast.lockscreenReset"));
    } catch (err) {
      toast(t("toast.lockscreenResetFailed", { err: String(err) }), true);
    } finally {
      btn.disabled = false;
    }
  });

  settingsForm.addEventListener("input", onFormChange);
  settingsForm.addEventListener("change", onFormChange);

  // Enter in a settings text field: save and return to the photo view.
  settingsForm.addEventListener("submit", (e) => e.preventDefault());
  settingsForm.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && (e.target as HTMLElement).tagName === "INPUT") {
      e.preventDefault();
      void saveSettingsNow().then(() => flipTo(null));
    }
  });

  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      if (anyMenuOpen()) closeMenus();
      else if (selectionMode && selectedIds.size) clearSelection();
      else if (selectionMode) setSelectionMode(false);
      else if (flip.classList.contains("is-flipped")) flipTo(null);
      return;
    }

    // Arrow keys browse history on the photo view.
    if (!flip.classList.contains("is-flipped")) {
      if (e.key === "ArrowLeft") {
        e.preventDefault();
        void navOlder();
        return;
      }
      if (e.key === "ArrowRight") {
        e.preventDefault();
        void navNewer();
        return;
      }
    }

    const inHistory = panelMode === "history" && flip.classList.contains("is-flipped");
    if (!inHistory || !selectionMode) return;
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
      e.preventDefault();
      for (const en of historyEntries) selectedIds.add(en.id);
      renderSelectionUi();
    } else if ((e.key === "Delete" || e.key === "Backspace") && selectedIds.size) {
      e.preventDefault();
      void deleteIds([...selectedIds]);
    }
  });
}

/* ===== Init ===== */
async function init() {
  wire();
  try {
    [settings, providers] = await Promise.all([
      invoke<Settings>("load_settings"),
      invoke<ProviderInfo[]>("list_providers"),
    ]);
  } catch (e) {
    toast(t("toast.startFailed", { err: String(e) }), true);
    settings = {
      provider: "picsum",
      search_terms: "",
      history_size: 20,
      history_mode: "loaded",
      theme: "system",
      language: "en",
      unsplash_key: "",
      last_save_dir: "",
    };
    providers = [];
  }

  setLanguage(settings.language);
  applyI18n();
  applyTheme(settings.theme);
  populateProviders();
  fillForm();
  flipTo(null);
  panelTitle.textContent = t("panel.settingsTitle");

  invoke<string>("config_location")
    .then((p) => {
      configPath = p;
      configPathEl.textContent = t("configPath", { path: p });
    })
    .catch(() => {});

  void loadPhoto();
}

window.addEventListener("DOMContentLoaded", init);
