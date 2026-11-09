import { useCallback, useEffect, useRef, useState } from "react";
import type { SocialAvailability } from "./availability";
import type { SocialConnectController } from "./connect-controller";
import { socialSessionId } from "./connect-types";

type Observe = (id: string, element: Element) => () => void;
function observationError(cause: unknown): string {
  const code = typeof cause === "string" ? cause : cause && typeof cause === "object" && "code" in cause ? cause.code : "";
  if (code === "connect_social_quota") return "O limite de consultas de disponibilidade foi atingido. Tente novamente mais tarde.";
  if (code === "connect_social_auth_failed") return "Sua conta não pôde ser confirmada para consultar a disponibilidade. Entre novamente no Aurora.";
  return "Não foi possível consultar a disponibilidade das sessões neste momento.";
}
export function useSocialAvailability(controller: SocialConnectController | undefined, enabled: boolean) {
  const [values, setValues] = useState<Map<string, SocialAvailability>>(new Map());
  const [error, setError] = useState("");
  const tracked = useRef(new Map<Element, { id: string; visible: boolean }>());
  const observer = useRef<IntersectionObserver | null>(null);
  const poll = useRef<() => void>(() => {});
  const observe: Observe = useCallback((id, element) => {
    if (!socialSessionId(id)) return () => {};
    const supported = typeof IntersectionObserver !== "undefined";
    if (supported && !observer.current) observer.current = new IntersectionObserver(entries => {
      for (const entry of entries) { const item = tracked.current.get(entry.target); if (item) item.visible = entry.isIntersecting; }
      poll.current();
    });
    tracked.current.set(element, { id, visible: !supported }); observer.current?.observe(element); poll.current();
    return () => { observer.current?.unobserve(element); tracked.current.delete(element); };
  }, []);
  useEffect(() => {
    let alive = true, busy = false, next = 0;
    setValues(new Map()); setError("");
    const run = async () => {
      if (!alive || busy || !enabled || typeof controller?.observeSessions !== "function" || document.hidden || performance.now() < next) return;
      const ids = [...new Set([...tracked.current.values()].filter(item => item.visible).map(item => item.id))].slice(0, 8);
      if (!ids.length) return;
      busy = true; next = performance.now() + 2500;
      try {
        const result = await controller.observeSessions(ids);
        if (alive) { setValues(new Map(result.map(item => [item.sessionId, item]))); setError(""); }
      } catch (cause) { if (alive) { setValues(new Map()); setError(observationError(cause)); } }
      finally { busy = false; }
    };
    poll.current = () => { void run(); };
    const resume = () => {
      const now = performance.now();
      setValues(previous => {
        const next = new Map([...previous].filter(([, value]) => !document.hidden && value.validUntil > now));
        return next.size === previous.size ? previous : next;
      });
      if (!document.hidden) void run();
    };
    const timer = window.setInterval(resume, 500);
    window.addEventListener("focus", resume); document.addEventListener("visibilitychange", resume); void run();
    return () => { alive = false; poll.current = () => {}; window.clearInterval(timer); window.removeEventListener("focus", resume); document.removeEventListener("visibilitychange", resume); };
  }, [controller, enabled]);
  useEffect(() => () => { observer.current?.disconnect(); observer.current = null; tracked.current.clear(); }, []);
  return { values, observe, error };
}

export function SocialAvailabilityBadge({ sessionId, value, observe }: { sessionId: string; value?: SocialAvailability; observe: Observe }) {
  const element = useRef<HTMLSpanElement>(null);
  useEffect(() => element.current ? observe(sessionId, element.current) : undefined, [sessionId, observe]);
  const fresh = value && value.validUntil > performance.now() ? value : undefined;
  return <span ref={element} className="social-availability" data-state={fresh?.state ?? "unknown"}>
    {fresh?.state === "ready" ? <><strong>Host disponível</strong><span>{fresh.connectedGuests} / {fresh.maxGuests} conexões remotas</span>
      <small>{fresh.admittedGuests} admissões ativas{fresh.hostPlayerConnected ? " · anfitrião conectado" : ""}</small></>
      : fresh?.state === "unavailable" ? <strong>Host indisponível</strong> : <span>Disponibilidade não verificada</span>}
  </span>;
}
