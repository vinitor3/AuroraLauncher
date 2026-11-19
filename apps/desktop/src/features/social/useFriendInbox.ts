import { useEffect, useState } from "react";
import type { FriendInbox, SocialService } from "../../shared/social";

export function useFriendInbox(service: SocialService | null) {
  const [value, setValue] = useState<{ service: SocialService; inbox?: FriendInbox }>();
  useEffect(() => {
    const watch = service?.watchFriendInbox;
    if (!service || !watch) return;
    let mounted = true;
    let timer: number | undefined;
    let unsubscribe: (() => void) | undefined;
    let generation = 0;
    const subscribe = () => {
      const revision = ++generation;
      unsubscribe?.();
      const update = (inbox: FriendInbox | undefined) => {
        if (!mounted || revision !== generation) return;
        window.clearTimeout(timer);
        setValue({ service, inbox });
        if (!inbox) timer = window.setTimeout(subscribe, 60_000);
      };
      try { unsubscribe = watch(update); }
      catch { update(undefined); }
    };
    subscribe();
    return () => { mounted = false; window.clearTimeout(timer); unsubscribe?.(); };
  }, [service]);
  return value?.service === service ? value?.inbox : undefined;
}
