import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { SkinPreviewCanvas } from "../src/features/skins/SkinPreviewCanvas";
import { SkinThumbnail } from "../src/features/skins/SkinThumbnail";
import { cacheLocalSkinImage, listLocalSkins, saveLocalSkin, updateLocalSkinSource, type LocalSkin } from "../src/lib/localSkins";
import "../src/styles.css";

// Local development fixture only. Never authenticates or fetches remote images.
// The dedicated owner/key cannot replace any real user's wardrobe records.
const OWNER = "qa-wardrobe";
const SKIN_ID = "synthetic-recovery-64x64";
const METADATA_URL = "https://wardrobe-fixture.invalid/synthetic-64x64.png";

function syntheticSkin() {
  return new Promise<Blob>((resolve, reject) => {
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 64;
    const context = canvas.getContext("2d");
    if (!context) { reject(new Error("Canvas 2D indisponível.")); return; }
    // Opaque base regions, transparent hat/jacket overlays, obvious magenta eyes.
    context.fillStyle = "#8570dc";
    context.fillRect(0, 0, 32, 16);
    context.fillStyle = "#49b7ba";
    context.fillRect(0, 16, 56, 16);
    context.fillRect(16, 48, 32, 16);
    context.fillStyle = "#f0c79b";
    context.fillRect(8, 8, 8, 8);
    context.fillStyle = "#b12df4";
    context.fillRect(9, 10, 2, 2);
    context.fillRect(13, 10, 2, 2);
    context.fillStyle = "#312051";
    context.fillRect(11, 14, 2, 1);
    canvas.toBlob((blob) => blob ? resolve(blob) : reject(new Error("PNG sintético não pôde ser criado.")), "image/png");
  });
}

async function readFixture() {
  return (await listLocalSkins(OWNER)).find((skin) => skin.id === SKIN_ID);
}

async function initializeFixture() {
  const saved = await readFixture();
  if (saved?.image?.size) return { skin: saved, restored: true };
  const image = await syntheticSkin();
  const temporaryUrl = URL.createObjectURL(image);
  try {
    await saveLocalSkin({ ownerId: OWNER, id: SKIN_ID, name: "Skin sintética QA", skinModel: "classic", image, sourceUrl: temporaryUrl });
  } finally {
    URL.revokeObjectURL(temporaryUrl);
  }
  // Exercise both production merge helpers; the HTTPS metadata URL is never fetched.
  await cacheLocalSkinImage(OWNER, SKIN_ID, image);
  await updateLocalSkinSource(OWNER, SKIN_ID, METADATA_URL);
  const skin = await readFixture();
  if (!skin?.image?.size) throw new Error("A imagem não foi persistida no IndexedDB.");
  return { skin, restored: false };
}

function Example() {
  const [skin, setSkin] = useState<LocalSkin>();
  const [status, setStatus] = useState("Lendo biblioteca de teste…");
  const [unavailable, setUnavailable] = useState(false);
  const [imageUrl, setImageUrl] = useState("");
  const [busy, setBusy] = useState(true);

  useEffect(() => {
    let active = true;
    void initializeFixture().then((result) => {
      if (!active) return;
      setSkin(result.skin);
      setStatus(result.restored ? "PNG restaurado do IndexedDB após reabrir a página." : "PNG sintético salvo e relido do IndexedDB. Recarregue para verificar a persistência.");
    }).catch(() => { if (active) setStatus("Falha ao abrir a biblioteca local de teste."); })
      .finally(() => { if (active) setBusy(false); });
    return () => { active = false; };
  }, []);

  useEffect(() => {
    if (!skin?.image) return;
    const image = unavailable ? new Blob(["invalid synthetic PNG"], { type: "image/png" }) : skin.image;
    const url = URL.createObjectURL(image);
    setImageUrl(url);
    return () => URL.revokeObjectURL(url);
  }, [skin, unavailable]);

  async function restore() {
    setBusy(true);
    try {
      const stored = await readFixture();
      if (!stored?.image?.size) throw new Error("Imagem ausente.");
      setSkin(stored);
      setUnavailable(false);
      setStatus("Imagem salva restaurada pelo Blob do IndexedDB, sem download.");
    } catch {
      setStatus("O PNG de teste não está disponível no IndexedDB.");
    } finally {
      setBusy(false);
    }
  }

  return <main style={{ padding: 32, maxWidth: 980, margin: "auto" }}>
    <p className="eyebrow">FIXTURE LOCAL · SEM CONTA OU REDE EXTERNA</p>
    <h1>Recuperação do guarda-roupa</h1>
    <p>Componentes de produção e PNG sintético de 64×64. Este teste não simula nem comprova a navegação “Equipada” do launcher.</p>
    <p role="status" aria-live="polite">{status}</p>
    <p className="muted">Proprietário: {OWNER} · Registro: {SKIN_ID} · Blob persistido: {skin?.image?.size ?? 0} bytes · URL temporária persistida: {skin?.sourceUrl?.startsWith("blob:") ? "sim (erro)" : "não"}</p>
    <div style={{ display: "flex", flexWrap: "wrap", gap: 12, margin: "24px 0" }}>
      <button disabled={busy || !skin} onClick={() => { setUnavailable(true); setStatus("Erro de imagem simulado apenas na prévia; o PNG salvo permanece intacto."); }} type="button">Simular imagem indisponível</button>
      <button disabled={busy || !skin} onClick={() => void restore()} type="button">Restaurar imagem salva</button>
      <button className="secondary" disabled={busy} onClick={() => window.location.reload()} type="button">Reabrir página</button>
    </div>
    <div style={{ display: "flex", alignItems: "flex-start", gap: 32, flexWrap: "wrap" }}>
      <div className="skin-card" style={{ width: 160 }}>
        <SkinThumbnail src={imageUrl} name={skin?.name ?? "Skin sintética QA"} />
        <strong>{skin?.name ?? "Skin sintética QA"}</strong>
        <span>Classic · 64×64</span>
      </div>
      <SkinPreviewCanvas skinModel="classic" skinUrl={imageUrl} />
    </div>
  </main>;
}

createRoot(document.getElementById("root")!).render(<Example />);
