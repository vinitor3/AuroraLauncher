import { useEffect, useRef, useState } from "react";
import { useVoiceAnnouncement } from "./VoiceAnnouncementProvider";
import { MODS, type VoiceMod, type VoiceReceiptSummary } from "./types";

type Props = { instanceId: string | null; voiceMod: VoiceMod; disabled: boolean; onResume: (receipt: VoiceReceiptSummary) => Promise<void>; onRetire?: (receipt: VoiceReceiptSummary) => Promise<void> };
/** Explicit read-only listing. A row never contains executable restoration authority. */
export function DurableReceipts({ instanceId, voiceMod, disabled, onResume, onRetire }: Props) {
  const voice = useVoiceAnnouncement();
  const [receipts, setReceipts] = useState<VoiceReceiptSummary[] | null>(null);
  const epoch = useRef(0);
  const mounted = useRef(false);
  const context = useRef({ instanceId, voiceMod, disabled });
  context.current = { instanceId, voiceMod, disabled };
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; epoch.current++; }; }, []);
  useEffect(() => { epoch.current++; setReceipts(null); }, [instanceId, voiceMod]);
  async function list() {
    if (!voice || disabled || !instanceId) return;
    const request = ++epoch.current;
    setReceipts(null);
    try {
      const result = await voice.listReceipts(instanceId);
      if (mounted.current && request === epoch.current && context.current.instanceId === instanceId && context.current.voiceMod === voiceMod) setReceipts(result.receipts);
    } catch { /* Provider exposes redacted native errors; no automatic retry. */ }
  }
  async function select(receipt: VoiceReceiptSummary, action: (receipt: VoiceReceiptSummary) => Promise<void>) {
    if (context.current.disabled || context.current.instanceId !== instanceId || context.current.voiceMod !== voiceMod
      || !instanceId || receipt.state !== "ready" || receipt.generation !== 0 || receipt.voiceMod !== voiceMod
      || !receipts?.some(row => row.receiptId === receipt.receiptId && row.state === "ready")) return;
    epoch.current++; setReceipts(null);
    await action(receipt);
  }
  return <section aria-label="Recibos salvos do anúncio">
    <p>Consulte o histórico desta cópia para retomar uma restauração. Cada retomada exige verificação e nova aprovação; não altera arquivos ao selecionar.</p>
    <button type="button" className="secondary" disabled={disabled || !instanceId} onClick={() => void list()}>Consultar recibos salvos</button>
    {receipts && !receipts.some(row => row.voiceMod === voiceMod) && <p>Nenhum recibo salvo para {MODS[voiceMod].name} nesta cópia.</p>}
    {receipts?.filter(row => row.voiceMod === voiceMod).map(receipt => <div key={receipt.receiptId}>
      <p>{MODS[receipt.voiceMod].name} · {receipt.state === "ready" ? "Disponível para revisar restauração" : receipt.state === "restored" ? "Restauração já concluída" : "Recibo aposentado; não pode ser retomado"}</p>
      <details><summary>Identificador do recibo</summary><code>{receipt.receiptId}</code></details>
      {receipt.state === "ready" && <><button type="button" disabled={disabled} onClick={() => void select(receipt, onResume)}>Retomar restauração</button>
        {onRetire && voice?.capabilities?.durableReceipts?.retirement && <button type="button" className="secondary" disabled={disabled} onClick={() => void select(receipt, onRetire)}>Revisar aposentadoria do recibo</button>}</>}
    </div>)}
  </section>;
}
