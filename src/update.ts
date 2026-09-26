// The update section of the About dialog, and the dot on the About button.
//
// The backend (update.rs) does the work: it looks up the latest GitHub release,
// downloads the matching file, checks its signature and finally swaps it in.
// This module only drives the UI through those steps. A silent check runs at
// startup (at most once a day, if enabled) and, when it finds something, lights
// the dot so the user can open About and decide.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";

import { t } from "./i18n";

export interface UpdateInfo {
  currentVersion: string;
  latestVersion: string;
  updateAvailable: boolean;
  mode: string;
  releaseUrl: string;
  notes: string;
  canDownload: boolean;
  assetSize: number | null;
  reason: string;
}

interface Prepared {
  version: string;
  mode: string;
}

type State =
  | { k: "idle" }
  | { k: "checking" }
  | { k: "current"; info: UpdateInfo }
  | { k: "available"; info: UpdateInfo }
  | { k: "downloading"; info: UpdateInfo; done: number; total: number }
  | { k: "ready"; info: UpdateInfo; prepared: Prepared }
  | { k: "error"; message: string; info: UpdateInfo | null };

/** What the controller needs from the app. */
export interface UpdateHost {
  autoCheck(): boolean;
  lastCheck(): number;
  /** Remember (and persist) when a check last ran. */
  setLastCheck(unixSeconds: number): void;
  /** Save app state before the app exits for an update. */
  prepareExit(): Promise<void>;
}

const DAY = 24 * 60 * 60;
const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

export class UpdateController {
  #host: UpdateHost;
  #state: State = { k: "idle" };
  #supported = false;

  readonly #section = el<HTMLElement>("about-update");
  readonly #dot = el<HTMLElement>("update-dot");
  readonly #aboutBtn = el<HTMLElement>("btn-about");
  readonly #checkBtn = el<HTMLButtonElement>("about-check-update");
  readonly #status = el<HTMLElement>("about-update-status");
  readonly #notes = el<HTMLElement>("about-update-notes");
  readonly #progress = el<HTMLElement>("about-update-progress");
  readonly #bar = el<HTMLElement>("about-update-progress-bar");
  readonly #percent = el<HTMLElement>("about-update-progress-text");
  readonly #actions = el<HTMLElement>("about-update-actions");
  readonly #secondary = el<HTMLButtonElement>("about-update-secondary");
  readonly #primary = el<HTMLButtonElement>("about-update-primary");
  #onPrimary: (() => void) | null = null;
  #onSecondary: (() => void) | null = null;

  constructor(host: UpdateHost) {
    this.#host = host;
    this.#checkBtn.addEventListener("click", () => void this.check(false));
    this.#primary.addEventListener("click", () => this.#onPrimary?.());
    this.#secondary.addEventListener("click", () => this.#onSecondary?.());
    void listen<{ downloaded: number; total: number }>("update-progress", (e) => {
      if (this.#state.k !== "downloading") return;
      this.#state = { ...this.#state, done: e.payload.downloaded, total: e.payload.total };
      this.render();
    });
  }

  /** Ask the backend whether this build can update, then maybe check quietly. */
  async init(): Promise<void> {
    this.#supported = await invoke<boolean>("update_supported").catch(() => false);
    this.render();
    if (!this.#supported || !this.#host.autoCheck()) return;
    const now = Math.floor(Date.now() / 1000);
    if (now - this.#host.lastCheck() < DAY) return;
    window.setTimeout(() => void this.check(true), 4000); // never compete with startup
  }

  async check(silent: boolean): Promise<void> {
    const s = this.#state.k;
    if (!this.#supported || s === "checking" || s === "downloading") return;
    const previous = this.#state;
    if (!silent) {
      this.#state = { k: "checking" };
      this.render();
    }
    this.#host.setLastCheck(Math.floor(Date.now() / 1000));
    try {
      const info = await invoke<UpdateInfo>("check_for_update");
      this.#state = info.updateAvailable ? { k: "available", info } : { k: "current", info };
    } catch (e) {
      // a quiet check that fails (offline, rate limit) should not nag
      this.#state = silent ? previous : { k: "error", message: String(e), info: null };
    }
    this.render();
  }

  async #download(): Promise<void> {
    if (this.#state.k !== "available") return;
    const info = this.#state.info;
    this.#state = { k: "downloading", info, done: 0, total: info.assetSize ?? 0 };
    this.render();
    try {
      const prepared = await invoke<Prepared>("download_update", { version: info.latestVersion });
      this.#state = { k: "ready", info, prepared };
    } catch (e) {
      this.#state = { k: "error", message: String(e), info };
    }
    this.render();
  }

  async #install(): Promise<void> {
    if (this.#state.k !== "ready") return;
    const { info, prepared } = this.#state;
    await this.#host.prepareExit();
    try {
      await invoke("install_update", { version: prepared.version });
    } catch (e) {
      this.#state = { k: "error", message: String(e), info };
      this.render();
    }
  }

  /** Re-label after a language change, too. */
  render(): void {
    this.#section.hidden = !this.#supported;
    const s = this.#state;
    const busy = s.k === "checking" || s.k === "downloading";
    this.#checkBtn.disabled = busy;
    this.#checkBtn.textContent = t(s.k === "checking" ? "update.checking" : "update.check");

    const pending = s.k === "available" || s.k === "downloading" || s.k === "ready";
    this.#dot.hidden = !pending;
    const base = t("toolbar.about");
    const label = pending ? `${base} — ${t("update.dot")}` : base;
    this.#aboutBtn.title = label;
    this.#aboutBtn.setAttribute("aria-label", label);

    let status = "";
    let notes = "";
    let primary: { label: string; run: () => void } | null = null;
    let secondary: { label: string; run: () => void } | null = null;
    const releasePage = (url: string) => ({
      label: "update.releasePage",
      run: () => void openUrl(url),
    });

    switch (s.k) {
      case "current":
        status = t("update.latest");
        break;
      case "available": {
        status = t("update.available", { version: s.info.latestVersion });
        if (s.info.reason) status += ` ${t(`update.reason.${s.info.reason}`)}`;
        notes = s.info.notes.trim();
        secondary = releasePage(s.info.releaseUrl);
        if (s.info.canDownload) primary = { label: "update.download", run: () => void this.#download() };
        break;
      }
      case "downloading":
        status = t("update.downloading");
        break;
      case "ready":
        status = t("update.ready", { version: s.prepared.version });
        secondary = releasePage(s.info.releaseUrl);
        primary = {
          label: s.prepared.mode === "installed" ? "update.install" : "update.restart",
          run: () => void this.#install(),
        };
        break;
      case "error":
        status = t("update.failed", { err: s.message });
        if (s.info) secondary = releasePage(s.info.releaseUrl);
        break;
    }

    this.#status.hidden = !status;
    this.#status.textContent = status;
    this.#status.classList.toggle("is-error", s.k === "error");
    this.#notes.hidden = !notes;
    this.#notes.textContent = notes;

    const showProgress = s.k === "downloading" && s.total > 0;
    this.#progress.hidden = !showProgress;
    if (s.k === "downloading" && s.total > 0) {
      const pct = Math.round(Math.min(1, s.done / s.total) * 100);
      this.#bar.style.width = `${pct}%`;
      this.#percent.textContent = `${pct}%`;
    }

    this.#onPrimary = primary?.run ?? null;
    this.#onSecondary = secondary?.run ?? null;
    this.#primary.hidden = !primary;
    this.#secondary.hidden = !secondary;
    if (primary) this.#primary.textContent = t(primary.label);
    if (secondary) this.#secondary.textContent = t(secondary.label);
    this.#actions.hidden = !primary && !secondary;
  }
}
