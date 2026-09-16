import { useState } from "react";
import { createRoot } from "react-dom/client";
import { OnlineSkinSearch } from "../src/features/skins/OnlineSkinSearch";
import { SkinPreviewCanvas } from "../src/features/skins/SkinPreviewCanvas";
import "../src/styles.css";

// Development example only: no login, account mutation, or equipping action.
function Example() {
  const [skin, setSkin] = useState({ skinUrl: "", model: "classic" as "classic" | "slim", name: "" });
  return <main style={{ padding: 32, maxWidth: 1050, margin: "auto" }}>
    <h1>Aurora Skins</h1><p>Exemplo de busca real: selecione para visualizar, sem alterar sua conta ou skin no jogo.</p>
    <div style={{ display: "flex", gap: 32, flexWrap: "wrap", marginTop: 24 }}>
      <div style={{ flex: "1 1 380px", minWidth: 0 }}><OnlineSkinSearch workerUrl="https://aurora-api.aurora-edge-proxy.workers.dev" onSelect={setSkin} /></div>
      <div><SkinPreviewCanvas skinModel={skin.model} skinUrl={skin.skinUrl} /><p role="status">{skin.name}</p></div>
    </div>
  </main>;
}
createRoot(document.getElementById("root")!).render(<Example />);
