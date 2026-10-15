import type { SessionStartupState } from "../../lib/sessionStartup";
import { snapshotFrom } from "./types";

export type AccountTransitionBarrier = "idle" | "closing" | "blocked";

type Options<Profile> = {
  publish: (state: SessionStartupState<Profile>) => void;
  closeConnect: () => Promise<void>;
  onBarrier: (state: AccountTransitionBarrier) => void;
};

/** Keep this controller across auth observer retries. A WebView reload creates a
 * new controller, but may retain native Connect workers: its first state must
 * cross cleanup too. This closes only Connect, never Java or voice receipts. */
export function createAccountTransition<Profile extends { uid: string }>(options: Options<Profile>) {
  let initialCleanupNeeded = true;
  let publishedUid: string | undefined;
  let pending: SessionStartupState<Profile> | undefined;
  let barrier: AccountTransitionBarrier = "idle";
  let running: Promise<void> | undefined;

  function publish(state: SessionStartupState<Profile>) {
    publishedUid = state.phase === "ready" ? state.profile.uid : undefined;
    options.publish(state);
  }

  function close(): Promise<void> {
    if (running) return running;
    barrier = "closing";
    options.onBarrier(barrier);
    // Hide the old account immediately, including loading for the same UID.
    // No auth/profile data is forwarded to the native cleanup function.
    options.publish({ phase: "loading" });
    running = Promise.resolve().then(options.closeConnect).then(() => {
      running = undefined;
      publishedUid = undefined;
      barrier = "idle";
      const latest = pending;
      pending = undefined;
      if (latest) publish(latest);
      options.onBarrier(barrier);
    }, () => {
      running = undefined;
      barrier = "blocked";
      // Keep the latest state, without exposing it or the native error. Only
      // an explicit retry may attempt the barrier again.
      options.onBarrier(barrier);
    });
    return running;
  }

  return {
    accept(state: SessionStartupState<Profile>): void {
      if (initialCleanupNeeded) {
        initialCleanupNeeded = false;
        pending = state;
        void close();
        return;
      }
      if (barrier !== "idle") {
        pending = state;
        return;
      }
      if (publishedUid !== undefined && (state.phase !== "ready" || state.profile.uid !== publishedUid)) {
        pending = state;
        void close();
        return;
      }
      publish(state);
    },
    retry(): Promise<void> {
      if (barrier === "closing") return running ?? Promise.resolve();
      return barrier === "blocked" ? close() : Promise.resolve();
    },
  };
}

type Invoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>;

/** Called only in Tauri. Capability availability never substitutes for cleanup. */
export async function stopConnectForAccountTransition(invoke: Invoke): Promise<void> {
  const current = snapshotFrom(await invoke("connect_snapshot"));
  if (current.operationId === null) {
    if (current.phase !== "idle" || !["notStarted", "complete"].includes(current.cleanupState)) {
      throw new Error("connect_account_cleanup_incomplete");
    }
    return;
  }
  const stopped = snapshotFrom(await invoke("connect_stop", { operationId: current.operationId }));
  if (stopped.operationId !== current.operationId || stopped.generation < current.generation
    || !["closed", "failed"].includes(stopped.phase)
    || stopped.cleanupState !== "complete") throw new Error("connect_account_cleanup_incomplete");
}
