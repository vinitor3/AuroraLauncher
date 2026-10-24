/** One React root per generation, with a cleanup barrier carried across HMR. */
export function createPreviewOwner(options: {
  previous?: Promise<void>;
  mount: (registerCleanup: (cleanup: () => Promise<void>) => void) => { unmount(): void };
  blocked: () => void;
}) {
  let disposed = false;
  let mounted: { unmount(): void } | undefined;
  let cleanup: (() => Promise<void>) | undefined;
  let cleaning: Promise<void> | undefined;
  let disposal: Promise<void> | undefined;

  function beginCleanup(): Promise<void> | undefined {
    if (!cleanup || cleaning) return cleaning;
    try { cleaning = Promise.resolve(cleanup()); }
    catch (error) { cleaning = Promise.reject(error); }
    // The barrier remains rejected for its successor; this handler only avoids
    // an unhandled rejection while the preceding ready promise is settling.
    void cleaning.catch(() => undefined);
    return cleaning;
  }

  const ready = (async () => {
    await options.previous;
    if (disposed) return;
    mounted = options.mount(next => {
      if (cleanup) throw new Error("PREVIEW_CLEANUP_ALREADY_REGISTERED");
      cleanup = next;
      if (disposed) beginCleanup();
    });
  })().catch(error => {
    if (!disposed) options.blocked();
    throw error;
  });
  void ready.catch(() => undefined);

  function dispose(): Promise<void> {
    if (disposal) return disposal;
    disposed = true;
    beginCleanup(); // Invalidate callbacks before detaching the old view.
    let unmountFailure: unknown;
    try { mounted?.unmount(); } catch (error) { unmountFailure = error; }
    mounted = undefined;
    disposal = (async () => {
      await ready;
      await beginCleanup();
      if (unmountFailure !== undefined) throw unmountFailure;
    })();
    void disposal.catch(() => undefined);
    return disposal;
  }
  return { ready, dispose };
}
