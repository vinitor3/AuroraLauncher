import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { SupportContextButton } from "./SupportContextButton";

type LogFile = { filename: string; lines: string[] };
type Result = { file: LogFile } | { error: string };

function lineClass(line: string) {
  if (/\b(error|exception|failed|fatal)\b/i.test(line)) return "log-error";
  if (/\b(warn|warning)\b/i.test(line)) return "log-warning";
  return "";
}

export function InstanceLogsPanel({ instanceId }: { instanceId: string }) {
  const [crash, setCrash] = useState(false);
  const [revision, setRevision] = useState(0);
  const [result, setResult] = useState<{ request: string; value: Result }>();
  const request = JSON.stringify([instanceId, crash, revision]);
  useEffect(() => {
    let active = true;
    invoke<LogFile>("read_instance_log", { id: instanceId, crash }).then(
      file => { if (active) setResult({ request, value: { file } }); },
      error => { if (active) setResult({ request, value: { error: String(error) } }); },
    );
    return () => { active = false; };
  }, [instanceId, crash, request]);
  const current = result?.request === request ? result.value : undefined;
  const file = current && "file" in current ? current.file : undefined;
  return <section className="instance-logs">
    <h3 className="editor-content-heading">Logs e crashes</h3>
    <div className="instance-log-controls">
      <div role="group" aria-label="Arquivo de registro">
        <button className={crash ? "secondary compact" : "selected compact"} aria-pressed={!crash} onClick={() => setCrash(false)} type="button">Log do jogo</button>
        <button className={crash ? "selected compact" : "secondary compact"} aria-pressed={crash} onClick={() => setCrash(true)} type="button">Último crash</button>
      </div>
      <button className="secondary compact" disabled={!current} onClick={() => setRevision(value => value + 1)} type="button">Recarregar</button>
      {!!file?.lines.length && <SupportContextButton key={request} label={crash ? "Copiar crash" : "Copiar log"} source="Minecraft" log={file.lines.join("\n")} />}
    </div>
    {!current ? <p role="status">Lendo o arquivo…</p> : "error" in current ? <p role="alert">Não foi possível ler o arquivo. {current.error}</p> : !current.file.lines.length
      ? <p className="muted">{crash ? "Esta instância ainda não tem um relatório de crash." : "Esta instância ainda não gerou um log."}</p>
      : <><p className="log-filename">{current.file.filename}</p>
        <pre aria-label="Últimas linhas do arquivo" className="log-viewer">{current.file.lines.map((line, i) =>
          <code className={lineClass(line)} key={i}>{line}{"\n"}</code>)}</pre></>}
  </section>;
}
