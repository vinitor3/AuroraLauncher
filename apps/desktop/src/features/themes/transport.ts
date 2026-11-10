import { invoke, isTauri } from "@tauri-apps/api/core";
import { parseTheme, type ThemeLibrary, type ThemePreview } from "./model";

type Command = "themes_load" | "themes_save" | "themes_delete" | "themes_preview" | "themes_confirm" | "themes_reset" | "themes_heartbeat" | "themes_set_automatic" | "themes_unbind";

// Browser development has an explicitly temporary library. Production storage,
// deadlines and crash recovery belong to Rust, never localStorage or a theme.
let browserLibrary: ThemeLibrary = { themes: [], selectedThemeId: null, bindings: {}, automatic: true, recovered: false, persistent: false };
let browserPreview: (ThemePreview & { id: string; instanceId: string | null }) | null = null;
let browserTimer: ReturnType<typeof setTimeout> | undefined;

function resetBrowser() {
  clearTimeout(browserTimer);
  browserPreview = null;
  browserLibrary = { ...browserLibrary, selectedThemeId: null, automatic: false };
  return browserLibrary;
}

export async function themeCommand<T>(command: Command, args: Record<string, unknown> = {}): Promise<T> {
  if (isTauri()) return invoke<T>(command, args);
  let result: unknown;
  switch (command) {
    case "themes_load": result = browserLibrary; break;
    case "themes_heartbeat": return undefined as T;
    case "themes_reset": result = resetBrowser(); break;
    case "themes_save": {
      const theme = parseTheme(String(args.json));
      if (!browserLibrary.themes.some((item) => item.id === theme.id) && browserLibrary.themes.length >= 32) throw new Error("A biblioteca aceita até 32 temas.");
      browserLibrary = { ...browserLibrary, themes: [...browserLibrary.themes.filter((item) => item.id !== theme.id), theme],
        selectedThemeId: browserLibrary.selectedThemeId === theme.id ? null : browserLibrary.selectedThemeId,
        bindings: Object.fromEntries(Object.entries(browserLibrary.bindings).filter(([, id]) => id !== theme.id)) };
      result = browserLibrary;
      break;
    }
    case "themes_delete":
      browserLibrary = { ...browserLibrary, themes: browserLibrary.themes.filter((item) => item.id !== args.id),
        selectedThemeId: browserLibrary.selectedThemeId === args.id ? null : browserLibrary.selectedThemeId,
        bindings: Object.fromEntries(Object.entries(browserLibrary.bindings).filter(([, id]) => id !== args.id)) };
      result = browserLibrary;
      break;
    case "themes_unbind":
      browserLibrary = { ...browserLibrary, bindings: Object.fromEntries(Object.entries(browserLibrary.bindings).filter(([id]) => id !== args.instanceId)) };
      result = browserLibrary;
      break;
    case "themes_set_automatic":
      browserLibrary = { ...browserLibrary, automatic: args.enabled === true };
      result = browserLibrary;
      break;
    case "themes_preview": {
      if (!browserLibrary.themes.some((theme) => theme.id === args.id)) throw new Error("Salve o tema antes de visualizar.");
      clearTimeout(browserTimer);
      browserPreview = { token: crypto.randomUUID(), deadlineMs: Date.now() + 10_000, id: String(args.id), instanceId: typeof args.instanceId === "string" ? args.instanceId : null };
      browserTimer = setTimeout(() => { resetBrowser(); window.dispatchEvent(new Event("aurora-theme-reset")); }, 10_000);
      result = { token: browserPreview.token, deadlineMs: browserPreview.deadlineMs };
      break;
    }
    case "themes_confirm": {
      if (!browserPreview || browserPreview.token !== args.token || Date.now() >= browserPreview.deadlineMs) {
        resetBrowser();
        throw new Error("A prévia terminou. A aparência padrão foi restaurada.");
      }
      const pending = browserPreview;
      clearTimeout(browserTimer);
      browserPreview = null;
      browserLibrary = pending.instanceId
        ? { ...browserLibrary, bindings: { ...browserLibrary.bindings, [pending.instanceId]: pending.id }, automatic: true }
        : { ...browserLibrary, selectedThemeId: pending.id };
      result = browserLibrary;
      break;
    }
  }
  return result as T;
}
