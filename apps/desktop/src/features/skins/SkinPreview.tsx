import { lazy, Suspense } from "react";

const Viewer = lazy(() => import("./SkinPreviewCanvas").then(({ SkinPreviewCanvas }) => ({ default: SkinPreviewCanvas })));

export function SkinPreview({ skinUrl, skinModel }: { skinUrl: string; skinModel: "classic" | "slim" }) {
  const placeholder = (text: string) => <div className="skin-viewer-shell"><div className="skin-preview-placeholder" role="status"><strong>{text}</strong></div></div>;
  if (!skinUrl.trim()) return placeholder("Selecione uma skin");
  return <Suspense fallback={placeholder("Abrindo prévia…")}><Viewer skinUrl={skinUrl} skinModel={skinModel} /></Suspense>;
}
