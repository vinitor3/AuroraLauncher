import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { loadAuroraCape, saveAuroraCape, uploadFirebaseAppearanceImage, type AuroraUserProfile, type FirebaseServices } from "../../lib/firebase";
import { uploadAuroraAppearanceImage } from "../../lib/edge";

type StoredCape = { path: string; url: string | null; pending: boolean };
type Cape = StoredCape & { owner: string; png: string };
const original: StoredCape = { path: "", url: null, pending: false };
const storageKey = (uid: string) => `aurora.equippedCape.${uid}`;

function readCape(uid: string): StoredCape {
  try {
    const value = JSON.parse(localStorage.getItem(storageKey(uid)) ?? "null");
    if (value && typeof value.path === "string" && (value.url === null || typeof value.url === "string")
        && typeof value.pending === "boolean") return value;
  } catch { /* A new session can recover the equipped cape from the account. */ }
  return original;
}

async function pngFile(data: string) {
  const blob = await (await fetch(data)).blob();
  return new File([blob], "capa.png", { type: "image/png" });
}

async function validateCape(file: File) {
  if (file.type !== "image/png" || !file.size || file.size > 5 * 1024 * 1024) throw new Error("Escolha um PNG de até 5 MB.");
  const bitmap = await createImageBitmap(file);
  const { width, height } = bitmap;
  bitmap.close();
  if (width < 64 || width > 1024 || width % 64 !== 0 || height !== width / 2) {
    throw new Error("Use a textura completa de uma capa 64×32, ou um múltiplo até 1024×512.");
  }
}

export function useWardrobeCape(services: FirebaseServices, profile: AuroraUserProfile | undefined,
    published: (uid: string, url: string | null) => void) {
  const owner = profile?.uid ?? "";
  const remote = profile?.capeUrl ?? null;
  const [cape, setCape] = useState<Cape>();
  const [draft, setDraft] = useState<{ owner: string; png: string; file: File }>();
  const [message, setMessage] = useState("");
  const [working, setWorking] = useState(false);
  const [sharing, setSharing] = useState({ owner: "", value: false });
  const [retry, setRetry] = useState(0);
  const operation = useRef(0);
  const actionBusy = useRef(false);
  const fresh = useRef(false);
  const active = cape?.owner === owner ? cape : undefined;
  const selected = draft?.owner === owner ? draft : undefined;
  function current(id: number) { return operation.current === id && services.auth.currentUser?.uid === owner; }
  function remember(state: Cape) {
    const { path, url, pending } = state;
    localStorage.setItem(storageKey(owner), JSON.stringify({ path, url, pending }));
    setCape(state);
  }
  useEffect(() => {
    const id = ++operation.current;
    actionBusy.current = false;
    setWorking(false);
    setDraft(undefined);
    setMessage("");
    setCape(undefined);
    if (!owner) return;
    setSharing({ owner, value: localStorage.getItem(`aurora.shareCape.${owner}`) === "1" });
    const stored = readCape(owner);
    const reloadSource = fresh.current;
    fresh.current = false;
    void (async () => {
      try {
        if (stored.path && (stored.pending || (!reloadSource && stored.url === remote))) {
          try {
            const cached = await invoke<{ dataBase64: string; localPath: string }>("load_local_appearance", { userId: owner, kind: "cape" });
            if (current(id)) setCape({ ...stored, owner, path: cached.localPath, png: cached.dataBase64 });
            return;
          } catch (error) { if (stored.pending) throw error; }
        }
        if (stored.pending || !remote) {
          if (current(id)) setCape({ ...original, pending: stored.pending, owner, png: "" });
          return;
        }
        const loaded = await invoke<{dataBase64: string}>("load_appearance_url", { url: remote, kind: "cape" });
        await validateCape(await pngFile(loaded.dataBase64));
        if (!current(id)) return;
        const path = await invoke<string>("save_local_appearance", { userId: owner, kind: "cape", dataBase64: loaded.dataBase64 });
        if (current(id)) remember({ owner, path, url: remote, pending: false, png: loaded.dataBase64 });
      } catch {
        if (current(id)) {
          setCape({ ...original, url: remote, owner, png: "" });
          setMessage("Não foi possível recuperar a capa. Você pode tentar novamente ou escolher outro PNG.");
        }
      }
    })();
    return () => { ++operation.current; };
  }, [owner, remote, services, retry]);

  async function choose(file?: File) {
    if (!file || !owner || actionBusy.current) return;
    const id = ++operation.current;
    try {
      await validateCape(file);
      const png = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result));
        reader.onerror = () => reject(new Error("Não foi possível abrir o PNG."));
        reader.readAsDataURL(file);
      });
      if (current(id)) { setDraft({ owner, file, png }); setMessage("Confira a prévia e aplique a capa."); }
    } catch (error) {
      if (current(id)) { setDraft(undefined); setMessage(error instanceof Error ? error.message : "Não foi possível abrir a capa."); }
    }
  }

  async function equip(remove = false) {
    if (!owner || !profile || !active || actionBusy.current) return;
    const id = ++operation.current;
    actionBusy.current = true;
    setWorking(true);
    let local = false;
    try {
      const png = remove ? "" : selected?.png ?? active.png;
      const file = png ? selected?.file ?? await pngFile(png) : undefined;
      if (!current(id)) return;
      const path = png ? await invoke<string>("save_local_appearance", { userId: owner, kind: "cape", dataBase64: png }) : "";
      if (!current(id)) return;
      const next = { path, owner, png, url: null, pending: true };
      remember(next);
      local = true;
      setDraft(undefined);
      let url: string | null = null;
      if (file) {
        try { url = await uploadAuroraAppearanceImage(services, "cape", file); }
        catch {
          if (!current(id)) return;
          url = await uploadFirebaseAppearanceImage(services, profile, "cape", file);
        }
      }
      if (!current(id)) return;
      await saveAuroraCape(services, owner, url);
      if (!current(id)) return;
      remember({ ...next, url, pending: false });
      published(owner, url);
      setMessage(url ? "Capa equipada e sincronizada." : "Capa do Minecraft restaurada.");
    } catch {
      if (current(id)) setMessage(local ? "Alteração aplicada neste computador. Sincronize novamente quando houver conexão." : "Não foi possível salvar a capa neste computador.");
    } finally {
      if (current(id)) { actionBusy.current = false; setWorking(false); }
    }
  }
  const share = owner !== "" && sharing.owner === owner && sharing.value;
  async function reload() {
    if (!owner || actionBusy.current) return;
    const id = ++operation.current;
    actionBusy.current = true;
    setWorking(true);
    try {
      const url = await loadAuroraCape(services, owner);
      if (!current(id)) return;
      fresh.current = true;
      published(owner, url);
      setRetry(value => value + 1);
    } catch {
      if (current(id)) setMessage("Não foi possível consultar a conta. A seleção local continua disponível.");
    } finally {
      if (current(id)) { actionBusy.current = false; setWorking(false); }
    }
  }
  function setShare(value: boolean) {
    if (!owner || services.auth.currentUser?.uid !== owner) return;
    localStorage.setItem(`aurora.shareCape.${owner}`, value ? "1" : "0");
    setSharing({ owner, value });
  }
  return { active, selected, share, setShare, ready: !!active, message, working, choose, equip,
    reload, discard: () => { setDraft(undefined); setMessage(""); } };
}

function CapePreview({ png }: { png: string }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const image = new Image();
    const context = canvas.current?.getContext("2d");
    if (!context) return;
    context.clearRect(0, 0, 100, 160);
    image.onload = () => {
      const unit = image.naturalWidth / 64;
      context.imageSmoothingEnabled = false;
      context.drawImage(image, unit, unit, 10 * unit, 16 * unit, 0, 0, 100, 160);
    };
    image.src = png;
    return () => { image.onload = null; };
  }, [png]);
  return <canvas ref={canvas} width={100} height={160} role="img" aria-label="Prévia das costas da capa" />;
}

export function CapePanel({ wardrobe }: { wardrobe: ReturnType<typeof useWardrobeCape> }) {
  const { active, selected, working, ready, message } = wardrobe;
  const png = selected?.png || active?.png;
  return <section className="skin-import" aria-label="Capa do personagem">
    <h2>Capa</h2>
    {png ? <CapePreview png={png} /> : <p>Usando a capa do Minecraft, se a sua conta tiver uma.</p>}
    <p className="muted">Escolha uma textura PNG de 64×32 ou um múltiplo, até 1024×512. A alteração aparece no seu personagem na próxima inicialização com Aurora Skins.</p>
    <label className="file-input">Escolher PNG da capa<input type="file" accept="image/png" disabled={working || !ready} onChange={event => {
      const file = event.target.files?.[0]; event.target.value = ""; void wardrobe.choose(file);
    }} /></label>
    <label><input type="checkbox" checked={wardrobe.share} disabled={working || !ready} onChange={event => wardrobe.setShare(event.target.checked)} /> Mostrar minha capa a outros jogadores em servidores com Aurora Skins compatível</label>
    {selected && <button type="button" disabled={working} onClick={() => void wardrobe.equip()}>Aplicar capa</button>}
    {selected && <button type="button" className="secondary" disabled={working} onClick={wardrobe.discard}>Descartar seleção</button>}
    {active?.pending && !selected && <button type="button" disabled={working} onClick={() => void wardrobe.equip()}>Sincronizar alteração</button>}
    {(active?.path || active?.url) && <button type="button" className="secondary" disabled={working} onClick={() => void wardrobe.equip(true)}>Restaurar capa do Minecraft</button>}
    {message && <p role="status">{message}</p>}
    {ready && !active?.pending && !selected && <button type="button" className="secondary" disabled={working} onClick={wardrobe.reload}>Recarregar capa da conta</button>}
  </section>;
}
