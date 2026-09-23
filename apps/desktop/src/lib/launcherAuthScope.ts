/** Public window identity; never contains account credentials. */
export type LauncherAuthScope = { id: string; persistent: boolean };

export function validateLauncherAuthScope(value: unknown): LauncherAuthScope {
  if (!value || typeof value !== "object") throw new Error("Sessão da janela indisponível.");
  const scope = value as Partial<LauncherAuthScope>;
  if (typeof scope.id !== "string" || !/^[a-zA-Z0-9_-]{1,128}$/.test(scope.id)
      || typeof scope.persistent !== "boolean") throw new Error("Sessão da janela inválida.");
  return { id: scope.id, persistent: scope.persistent };
}

let browserScope: LauncherAuthScope | undefined;
/** Browser previews get a tab-local session; only the native owner may remember a login. */
export function browserLauncherAuthScope(): LauncherAuthScope {
  if (browserScope) return browserScope;
  const storageKey = "aurora.browserAuthScope";
  let id: string | null = null;
  try { id = window.sessionStorage.getItem(storageKey); } catch { /* Memory-only fallback. */ }
  if (!id || !/^[a-zA-Z0-9_-]{1,128}$/.test(id)) {
    id = crypto.randomUUID();
    try { window.sessionStorage.setItem(storageKey, id); } catch { /* Memory-only fallback. */ }
  }
  browserScope = { id, persistent: false };
  return browserScope;
}

function deadline<T>(work: Promise<T>, timeoutMs: number): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error("A sessão da janela demorou para responder.")), timeoutMs);
    work.then(resolve, reject).finally(() => clearTimeout(timeout));
  });
}

/** Auth must not initialize until native ownership is known, even when configuration falls back. */
export async function prepareLauncherServices<Config, Services>(options: {
  loadScope: () => Promise<unknown>;
  loadConfig: () => Promise<Config | null>;
  bundledConfig: Config;
  create: (config: Config, scope: LauncherAuthScope) => Services;
  timeoutMs?: number;
}): Promise<Services> {
  const timeoutMs = options.timeoutMs ?? 5_000;
  const scope = deadline(Promise.resolve().then(options.loadScope).then(validateLauncherAuthScope), timeoutMs);
  const config = deadline(Promise.resolve().then(options.loadConfig), timeoutMs).catch(() => null);
  const [resolvedScope, resolvedConfig] = await Promise.all([scope, config]);
  return options.create(resolvedConfig ?? options.bundledConfig, resolvedScope);
}
