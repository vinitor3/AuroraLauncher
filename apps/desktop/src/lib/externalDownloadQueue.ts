export type ExternalDownloadRequest = {
  requestId: string;
  id: string;
  pageUrl: string;
  filename: string;
  contentType: "mod" | "resourcepack" | "shader" | "modpack";
  sha1: string;
  projectId?: number;
  fileId?: number;
};

export type ExternalDownloadEvent = {
  requestId: string;
  instanceId: string;
  filename: string;
  contentType: ExternalDownloadRequest["contentType"];
  status: "resolving" | "downloading" | "completed" | "failed";
  error?: string;
};

type QueueOptions = {
  open: (request: ExternalDownloadRequest) => Promise<void>;
  cancel: (requestId: string) => Promise<void>;
  onStart?: (request: ExternalDownloadRequest) => void;
  timeoutMs?: number;
};

type PendingDownload = {
  request: ExternalDownloadRequest;
  finish: (error?: Error) => void;
  settled: boolean;
  started: boolean;
  timeout?: ReturnType<typeof setTimeout>;
  cancellation?: Promise<void>;
};

/** Serializes external pages and correlates every response with its own request. */
export class ExternalDownloadQueue {
  private readonly options: QueueOptions;
  private readonly pending = new Map<string, PendingDownload>();
  private readonly targets = new Set<string>();
  private tail: Promise<void> = Promise.resolve();

  constructor(options: QueueOptions) {
    this.options = options;
  }

  enqueue(request: ExternalDownloadRequest): Promise<void> {
    const target = JSON.stringify([request.id, request.filename]);
    if (this.targets.has(target) || this.pending.has(request.requestId)) {
      return Promise.reject(new Error(`“${request.filename}” já está na fila de downloads.`));
    }
    let finish!: PendingDownload["finish"];
    // A fulfilled outcome also observes cancellation while the open command is pending.
    const completion = new Promise<Error | undefined>((resolve) => { finish = resolve; });
    const entry: PendingDownload = { request, finish, settled: false, started: false };
    this.pending.set(request.requestId, entry);
    this.targets.add(target);

    const operation = this.tail.then(async () => {
      try {
        if (entry.settled) {
          const error = await completion;
          if (error) throw error;
          return;
        }
        entry.started = true;
        entry.timeout = setTimeout(() => {
          this.settle(entry, new Error("o download não foi concluído dentro de 10 minutos"));
        }, this.options.timeoutMs ?? 10 * 60 * 1000);
        this.options.onStart?.(request);
        await Promise.all([
          this.options.open(request),
          completion.then((error) => { if (error) throw error; }),
        ]);
      } catch (error) {
        this.settle(entry, error instanceof Error ? error : new Error(String(error)));
        await this.cancelEntry(entry);
        throw error;
      } finally {
        clearTimeout(entry.timeout);
        this.pending.delete(request.requestId);
        this.targets.delete(target);
      }
    });
    // A failed or cancelled item must not block the next queued request.
    this.tail = operation.catch(() => undefined);
    return operation;
  }

  accept(event: ExternalDownloadEvent): boolean {
    const entry = this.pending.get(event.requestId);
    if (!entry || !entry.started || entry.settled
      || entry.request.id !== event.instanceId
      || entry.request.filename !== event.filename
      || entry.request.contentType !== event.contentType) return false;
    if (event.status === "completed") this.settle(entry);
    else if (event.status === "failed") {
      this.settle(entry, new Error(event.error ?? "arquivo não validado"));
    }
    return true;
  }

  cancelAll(error = new Error("o download foi interrompido")): Promise<void> {
    return Promise.all([...this.pending.values()].map(async (entry) => {
      this.settle(entry, error);
      await this.cancelEntry(entry);
    })).then(() => undefined);
  }

  private settle(entry: PendingDownload, error?: Error) {
    if (entry.settled) return;
    entry.settled = true;
    clearTimeout(entry.timeout);
    entry.finish(error);
  }

  private cancelEntry(entry: PendingDownload): Promise<void> {
    if (!entry.started) return Promise.resolve();
    // Timeout, unmount and invoke failures can race; close the page only once.
    entry.cancellation ??= Promise.resolve()
      .then(() => this.options.cancel(entry.request.requestId))
      .catch(() => undefined);
    return entry.cancellation;
  }
}
