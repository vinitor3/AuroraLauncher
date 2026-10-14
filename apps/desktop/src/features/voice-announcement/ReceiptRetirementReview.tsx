import { useEffect, useState } from "react";
import { MODS, voiceDate } from "./types";
import type { RetirementReview } from "./retirement-types";

type Props = {
  instanceId: string; operationId: string; review: RetirementReview; disabled: boolean;
  onConfirm: (request: { operationId: string; planId: string; digest: string }) => Promise<void>;
  onCancel: () => Promise<void>;
};
export function ReceiptRetirementReview({ instanceId, operationId, review, disabled, onConfirm, onCancel }: Props) {
  const [consent, setConsent] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => { setConsent(false); setNow(Date.now()); }, [instanceId, operationId, review.planId, review.digest, review.receiptId, review.voiceMod]);
  useEffect(() => {
    const clock = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(clock);
  }, []);
  useEffect(() => { if (now >= review.expiresAtMs) setConsent(false); }, [now, review.expiresAtMs]);
  return <section aria-label="Revisão da aposentadoria do recibo">
    <h4>Abandonar a restauração seletiva deste recibo</h4>
    <p>Cópia <strong>{instanceId}</strong> · {MODS[review.voiceMod].name}</p>
    <p>{review.warning}</p>
    <p>Antes: disponível para restauração seletiva. Depois: aposentado, somente histórico. A configuração continuará como está.</p>
    {review.currentConfigSha256 === null ? <p>O arquivo de configuração estava ausente na revisão. Aposentar não cria nem restaura esse arquivo.</p>
      : <p>Arquivo observado: {review.currentConfigBytes} bytes. Aposentar não altera esses bytes.</p>}
    <details><summary>Identificadores da revisão</summary><p>Recibo: <code>{review.receiptId}</code></p>
      {review.currentConfigSha256 !== null && <p>Arquivo observado: <code>{review.currentConfigSha256}</code></p>}
      <p>Aprovação: <code>{review.digest}</code></p></details>
    <p>Válida até {voiceDate(review.expiresAtMs)}.</p>
    {now >= review.expiresAtMs && <p role="alert">A revisão venceu. Cancele e prepare outra.</p>}
    <label><input type="checkbox" aria-label="Autorizo aposentar somente este recibo" checked={consent}
      disabled={disabled || now >= review.expiresAtMs} onChange={event => setConsent(event.target.checked)} />Entendi que perderei a restauração seletiva deste recibo e que a configuração não será alterada.</label>
    <button type="button" disabled={disabled || !consent || now >= review.expiresAtMs} onClick={() => {
      if (disabled || !consent) return;
      setConsent(false);
      if (Date.now() >= review.expiresAtMs) return;
      void onConfirm({ operationId, planId: review.planId, digest: review.digest });
    }}>Confirmar aposentadoria do recibo</button>
    <button type="button" className="secondary" disabled={disabled} onClick={() => { setConsent(false); void onCancel(); }}>Cancelar revisão e manter recibo</button>
  </section>;
}
