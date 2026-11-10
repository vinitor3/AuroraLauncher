export type DownloadProgress = {
  operationId?: string;
  label: string;
  percent: number;
  totalPercent: number;
  itemPercent: number;
  itemDownloadedBytes: number;
  itemTotalBytes?: number;
  downloadedBytes: number;
  totalBytes?: number;
  completedFiles: number;
  totalFiles: number;
  activeDownloads: number;
  bytesPerSecond: number;
};

type Listen = <T>(name: string, callback: (event: { payload: T }) => void) => Promise<() => void>;
type Update = (change: (current: DownloadProgress | undefined) => DownloadProgress | undefined) => void;
type Clock = { schedule: (callback: () => void, milliseconds: number) => number; cancel: (id: number) => void };
const uuid = /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/;

/** Presentation only: completion of a Connect copy cannot dismiss a different download. */
export function observeDownloadProgress(listen: Listen, update: Update, clock: Clock = {
  schedule: (callback, milliseconds) => window.setTimeout(callback, milliseconds),
  cancel: id => window.clearTimeout(id),
}): () => void {
  let disposed = false;
  const stops: (() => void)[] = [];
  const timers = new Set<number>();
  const finished = new Set<string>();
  const remember = (id: string) => {
    finished.add(id);
    if (finished.size > 64) finished.delete(finished.values().next().value!);
  };
  const subscribe = <T>(name: string, callback: (payload: T) => void) => {
    void listen<T>(name, event => { if (!disposed) callback(event.payload); })
      .then(stop => { if (disposed) stop(); else stops.push(stop); }).catch(() => undefined);
  };
  subscribe<DownloadProgress>("aurora-download-progress", progress => {
    if (!progress || typeof progress !== "object" || Array.isArray(progress) || typeof progress.label !== "string"
      || typeof progress.percent !== "number" || !Number.isFinite(progress.percent)
      || progress.totalPercent !== undefined && (typeof progress.totalPercent !== "number" || !Number.isFinite(progress.totalPercent))) return;
    if (progress.operationId !== undefined && (!uuid.test(progress.operationId) || finished.has(progress.operationId))) return;
    update(() => progress);
    if ((progress.totalPercent ?? progress.percent) >= 100) {
      const timer = clock.schedule(() => {
        timers.delete(timer);
        if (!disposed) update(current => current === progress ? undefined : current);
      }, 1800);
      timers.add(timer);
    }
  });
  subscribe<unknown>("aurora-ready-copy-install-finished", value => {
    if (!value || typeof value !== "object" || Array.isArray(value)) return;
    const event = value as Record<string, unknown>;
    if (event.schemaVersion !== 1 || typeof event.operationId !== "string" || !uuid.test(event.operationId)
      || typeof event.outcome !== "string" || !["completed", "failed", "cancelled"].includes(event.outcome)) return;
    remember(event.operationId);
    // Terminal delivery also closes 98% when the last 100% event was lost or
    // arrives later. It never synthesizes success progress for an error.
    update(current => current?.operationId === event.operationId ? undefined : current);
  });
  return () => {
    if (disposed) return;
    disposed = true;
    for (const stop of stops) stop();
    for (const timer of timers) clock.cancel(timer);
    timers.clear(); finished.clear();
  };
}
