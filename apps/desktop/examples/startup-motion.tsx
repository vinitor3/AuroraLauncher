import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { StartupScreen } from "../src/features/auth/StartupScreen";
import "../src/styles.css";

type MotionMode = "system" | "normal" | "reduced";
type Phase = "loading" | "error" | "signed-out";
const REDUCED_QUERY = "(prefers-reduced-motion: reduce)";

// Reuse the actual production rules, without copying keyframes or modifying the OS.
// The original CSSOM media queries are restored when the mode changes/unmounts.
function applyMotionMode(mode: MotionMode) {
  const originals: { rule: CSSMediaRule; media: string }[] = [];
  function visit(rules: CSSRuleList) {
    for (const rule of Array.from(rules)) {
      if (rule instanceof CSSMediaRule && rule.conditionText.replace(/\s+/g, " ").trim() === REDUCED_QUERY) {
        originals.push({ rule, media: rule.media.mediaText });
        rule.media.mediaText = mode === "reduced" ? "all" : mode === "normal" ? "not all" : rule.media.mediaText;
      }
      if ("cssRules" in rule) visit((rule as CSSGroupingRule).cssRules);
    }
  }
  for (const sheet of Array.from(document.styleSheets)) {
    try { visit(sheet.cssRules); } catch { /* Only this page's readable local CSS is relevant. */ }
  }
  return { count: originals.length, restore: () => originals.forEach(({ rule, media }) => { rule.media.mediaText = media; }) };
}

function animationDetails(animation: Animation) {
  const effect = animation.effect instanceof KeyframeEffect ? animation.effect : undefined;
  const properties = new Set(effect?.getKeyframes().flatMap((frame) => Object.keys(frame)) ?? []);
  for (const metadata of ["offset", "computedOffset", "easing", "composite"]) properties.delete(metadata);
  return {
    name: animation instanceof CSSAnimation ? animation.animationName : animation.id || "Web Animation",
    playState: animation.playState,
    time: typeof animation.currentTime === "number" ? Math.round(animation.currentTime) : String(animation.currentTime),
    infinite: effect?.getTiming().iterations === Infinity,
    properties: [...properties].sort(),
  };
}

function sampleMotion(stage: HTMLElement) {
  const elements: { label: string; element: Element; pseudo?: string }[] = [];
  for (const [label, selector] of [["Anel", ".startup-orbit"], ["Marca", ".startup-mark"], ["Progresso", ".startup-progress span"], ["Ponto", ".startup-dot"]]) {
    stage.querySelectorAll(selector).forEach((element, index) => elements.push({ label: label === "Ponto" || label === "Anel" ? `${label} ${index + 1}` : label, element }));
  }
  const emblem = stage.querySelector(".startup-emblem");
  if (emblem) elements.push({ label: "Brilho ::before", element: emblem, pseudo: "::before" });
  return {
    timestamp: Math.round(performance.now()),
    elements: elements.map(({ label, element, pseudo }) => {
      const style = getComputedStyle(element, pseudo);
      const visible = element.getClientRects().length > 0 && style.visibility !== "hidden" && style.display !== "none";
      return { label, visible, transform: style.transform, opacity: style.opacity, animations: element.getAnimations().map(animationDetails) };
    }),
    animations: stage.getAnimations({ subtree: true }).map(animationDetails),
  };
}

type Sample = ReturnType<typeof sampleMotion>;
type Measurement = { first: Sample; second: Sample; summary: string; passed: boolean };

function compareSamples(first: Sample, second: Sample, reduced: boolean, phase: Phase): Measurement {
  const pairs = second.elements.map((after) => ({ after, before: first.elements.find((before) => before.label === after.label) }));
  const transformChanges = pairs.filter(({ before, after }) => before && before.transform !== after.transform).length;
  const opacityChanges = pairs.filter(({ before, after }) => before && before.opacity !== after.opacity).length;
  const running = second.animations.filter((animation) => animation.playState === "running");
  const loops = running.filter((animation) => animation.infinite);
  const animatedTransform = loops.some((animation) => animation.properties.includes("transform"));
  const opacityOnly = loops.length > 0 && loops.every((animation) => animation.properties.length > 0 && animation.properties.every((property) => property === "opacity"));
  const elapsed = second.timestamp - first.timestamp;
  const dots = second.elements.filter((element) => element.label.startsWith("Ponto ")).length;
  const visibleDots = second.elements.filter((element) => element.label.startsWith("Ponto ") && element.visible).length;
  const progress = second.elements.find((element) => element.label === "Progresso");
  const progressVisible = progress?.visible === true;
  const fullLoading = dots === 3 && visibleDots === 3 && ["Anel", "Marca"].every((label) => second.elements.some((element) => element.label.startsWith(label) && element.visible));
  const passed = phase === "error" ? !progress && dots === 0 && loops.length === 0 && transformChanges === 0 && opacityChanges === 0
    : reduced ? fullLoading && !progressVisible && transformChanges === 0 && !animatedTransform && opacityChanges > 0 && opacityOnly
      : fullLoading && progressVisible && transformChanges > 0 && animatedTransform && loops.length > 0;
  const expected = phase === "error" ? "erro estático sem loops" : reduced ? "reduzido com opacity, sem transform" : "normal com movimento";
  return { first, second, passed, summary: `${passed ? "APROVADO" : "PRECISA REVISAR"}: ${expected}. ${elapsed} ms; transform mudou em ${transformChanges} elemento(s); opacity em ${opacityChanges}; loops ativos: ${loops.length}; pontos visíveis: ${visibleDots}/${dots}; barra: ${progress ? progressVisible ? "visível" : "oculta" : "ausente"}.` };
}

function Example() {
  const [mode, setMode] = useState<MotionMode>("system");
  const [phase, setPhase] = useState<Phase>("loading");
  const [systemReduced, setSystemReduced] = useState(() => window.matchMedia(REDUCED_QUERY).matches);
  const [ruleCount, setRuleCount] = useState(0);
  const [callback, setCallback] = useState("Nenhum callback acionado.");
  const [measurement, setMeasurement] = useState<Measurement>();
  const [measuring, setMeasuring] = useState(false);
  const stageRef = useRef<HTMLDivElement>(null);
  const generation = useRef(0);

  useEffect(() => {
    const preference = window.matchMedia(REDUCED_QUERY);
    const changed = () => setSystemReduced(preference.matches);
    preference.addEventListener("change", changed);
    return () => preference.removeEventListener("change", changed);
  }, []);
  useEffect(() => {
    const applied = applyMotionMode(mode);
    setRuleCount(applied.count);
    return applied.restore;
  }, [mode]);
  useEffect(() => {
    generation.current += 1;
    setMeasurement(undefined);
    setMeasuring(false);
    return () => { generation.current += 1; };
  }, [mode, phase, systemReduced]);

  async function verifyMotion() {
    const stage = stageRef.current;
    if (!stage || phase === "signed-out") return;
    const run = ++generation.current;
    setMeasuring(true);
    setMeasurement(undefined);
    const first = sampleMotion(stage);
    await new Promise<void>((resolve) => window.setTimeout(resolve, 350));
    if (generation.current !== run) return;
    const second = sampleMotion(stage);
    setMeasurement(compareSamples(first, second, mode === "reduced" || (mode === "system" && systemReduced), phase));
    setMeasuring(false);
  }

  return <div style={{ display: "flex", flexWrap: "wrap", minHeight: "100vh" }}>
    <aside style={{ flex: "0 1 340px", minWidth: 280, padding: 22, borderRight: "1px solid #ffffff20", overflowWrap: "anywhere" }}>
      <p className="eyebrow">FIXTURE LOCAL · SEM AUTENTICAÇÃO</p>
      <h1 style={{ fontSize: "1.25rem" }}>Inicialização em movimento</h1>
      <p>Usa StartupScreen e CSS de produção. Mudar o modo afeta somente esta página; nenhuma configuração do sistema é alterada.</p>
      <p>Preferência do sistema: <strong>{systemReduced ? "movimento reduzido" : "movimento normal"}</strong>.</p>
      <label style={{ display: "grid", gap: 8 }}>Modo de movimento
        <select value={mode} onChange={(event) => setMode(event.target.value as MotionMode)} disabled={measuring}>
          <option value="system">Sistema</option><option value="normal">Normal</option><option value="reduced">Reduzido</option>
        </select>
      </label>
      <p className="muted">Regras reduced-motion de produção encontradas: {ruleCount}.</p>
      <div style={{ display: "grid", gap: 10, margin: "20px 0" }}>
        <button disabled={measuring} onClick={() => setPhase("error")} type="button">Simular erro</button>
        <button className="secondary" disabled={measuring} onClick={() => { setPhase("loading"); setCallback("Carregamento reiniciado pelo fixture."); }} type="button">Reiniciar carregamento</button>
        <button disabled={measuring || phase === "signed-out" || ruleCount === 0} onClick={() => void verifyMotion()} type="button">{measuring ? "Medindo movimento…" : "Verificar movimento"}</button>
      </div>
      <p role="status">{callback}</p>
      <p className="muted">No erro, use os botões do componente à direita para testar onRetry e onSignOut. A saída é apenas simulada e não abre login real.</p>
      {measurement && <section aria-label="Resultado da verificação">
        <p role="status" style={{ color: measurement.passed ? "#a8efbe" : "#ffc1ca" }}>{measurement.summary}</p>
        <ul style={{ paddingLeft: 18 }}>{measurement.second.elements.map((after) => {
          const before = measurement.first.elements.find((item) => item.label === after.label);
          return <li key={after.label} style={{ marginBottom: 12 }}><strong>{after.label}</strong> · {after.visible ? "visível" : "oculto"}<br />transform: {before?.transform === after.transform ? "estável" : "mudou"}; opacity: {before?.opacity} → {after.opacity}<br />Animações: {after.animations.map((animation) => `${animation.name} (${animation.playState}, ${animation.properties.join("/")})`).join(", ") || "nenhuma"}</li>;
        })}</ul>
        <details><summary>Amostras completas: computedStyle e getAnimations</summary><pre style={{ fontSize: 10, whiteSpace: "pre-wrap", maxHeight: 400, overflowY: "auto" }}>{JSON.stringify(measurement, null, 2)}</pre></details>
      </section>}
    </aside>
    <div ref={stageRef} style={{ flex: "1 1 600px", minWidth: 0 }}>
      {phase === "signed-out" ? <section style={{ padding: 48 }}><h2>Saída simulada</h2><p>O callback onSignOut foi acionado. Nenhum dado ou sessão real foi alterado.</p></section> : <StartupScreen
        error={phase === "error" ? "Falha de conexão simulada para validar recuperação e ausência de loops." : undefined}
        onRetry={() => { setPhase("loading"); setCallback("onRetry acionado pelo botão Tentar novamente."); }}
        onSignOut={() => { setPhase("signed-out"); setCallback("onSignOut acionado pelo botão Voltar para entrar."); }}
      />}
    </div>
  </div>;
}

createRoot(document.getElementById("root")!).render(<Example />);
