import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import { socialSessionId } from "./connect-types";

export type PublicInviteControls = { id: string; dismiss(): void };

/** Native retains cold-start invites until the logged-in main window is ready to review them. */
export function usePublicInvite(uid: string | undefined, reveal: () => void): PublicInviteControls | undefined {
  const [received, setReceived] = useState<{ uid: string; id: string }>();
  const onReveal = useRef(reveal); onReveal.current = reveal;
  useEffect(() => {
    if (!uid || !isTauri()) return;
    let live = true, reading = false, again = false;
    let unsubscribe: UnlistenFn | undefined;
    async function drain() {
      if (!live || !uid) return;
      if (reading) { again = true; return; }
      reading = true;
      try {
        do {
          again = false;
          const id = await invoke<unknown>("take_public_invite");
          if (live && socialSessionId(id)) { setReceived({ uid, id }); onReveal.current(); }
        } while (live && again);
      } catch { /* Older desktop versions still support the public catalog. */ }
      finally { reading = false; }
    }
    void listen("aurora-public-invite", () => { void drain(); }).then(dispose => {
      if (!live) { dispose(); return; }
      unsubscribe = dispose; void drain();
    }).catch(() => {});
    return () => { live = false; unsubscribe?.(); setReceived(undefined); };
  }, [uid]);
  return received && received.uid === uid ? {
    id: received.id, dismiss() { setReceived(value => value === received ? undefined : value); },
  } : undefined;
}
