import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  actionName,
  combo,
  encoded,
  editableModifier,
  forgeModifiers,
  modifierApplies,
  modifierChoices,
  modifierLabel,
  extras,
  groups,
  keyLabel,
  physical,
  rows,
  type Binding,
  type ForgeModifier,
  type KeyboardLayout,
  type KeyMap,
} from "./keyboard";
import "./keymap.css";

export default function KeymapPanel({
  instanceId,
  running,
}: {
  instanceId: string;
  running: boolean;
}) {
  const [map, setMap] = useState<KeyMap | null>(null);
  const [layout, setLayout] = useState<KeyboardLayout>(() => {
    try {
      return localStorage.getItem("aurora.keymap.layout") === "ANSI"
        ? "ANSI"
        : "ABNT2";
    } catch {
      return "ABNT2";
    }
  });
  const [filter, setFilter] = useState("");
  const [conflictsOnly, setConflictsOnly] = useState(false);
  const [selected, setSelected] = useState("");
  const [editing, setEditing] = useState<Binding | null>(null);
  const [candidate, setCandidate] = useState("");
  const [modifier, setModifier] = useState<ForgeModifier>("NONE");
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const [refresh, setRefresh] = useState(0);
  const [shownCount, setShownCount] = useState(60);
  const active = useRef(0);
  useEffect(() => {
    const generation = ++active.current;
    setBusy(true);
    setMap(null);
    setMessage("");
    setEditing(null);
    setCandidate("");
    invoke<KeyMap>("instance_keymap", { id: instanceId })
      .then((data) => {
        if (active.current === generation) setMap(data);
      })
      .catch((error) => {
        if (active.current === generation) setMessage(String(error));
      })
      .finally(() => {
        if (active.current === generation) setBusy(false);
      });
    return () => {
      active.current++;
    };
  }, [instanceId, refresh]);
  const bindings = map?.bindings;
  const analysis = useMemo(() => {
    const list = bindings ?? [];
    const conflicts = new Set(
      [...groups(list)]
        .filter(([, actions]) => actions.length > 1)
        .map(([code]) => code),
    );
    const keys = new Map<string, Binding[]>();
    for (const b of list) {
      const id = physical(b.input);
      const arr = keys.get(id) ?? [];
      arr.push(b);
      keys.set(id, arr);
    }
    return { keys, conflicts };
  }, [bindings]);
  const query = filter.trim().toLocaleLowerCase();
  const visible = (bindings ?? [])
    .filter(
      (b) =>
        (!selected || physical(b.input) === selected) &&
        (!conflictsOnly || analysis.conflicts.has(combo(b))) &&
        (!query ||
          [
            b.action,
            actionName(b.action),
            keyLabel(physical(b.input), layout),
            b.modifier,
          ].some((s) => s.toLocaleLowerCase().includes(query))),
    )
    .sort(
      (a, b) =>
        Number(analysis.conflicts.has(combo(b))) -
          Number(analysis.conflicts.has(combo(a))) ||
        actionName(a.action).localeCompare(actionName(b.action), "pt-BR"),
    );
  const mainRows = rows(layout);
  const named = new Set(
    mainRows
      .flat()
      .map((k) => k.id)
      .concat(extras),
  );
  const extraKeys = extras.concat(
    [...analysis.keys.keys()].filter(
      (k) => !named.has(k) && k !== "key.keyboard.unknown",
    ),
  );
  function choose(id: string) {
    if (editing) {
      setCandidate(id);
      setMessage("");
      return;
    }
    setSelected((prev) => (prev === id ? "" : id));
    setConflictsOnly(false);
    setFilter("");
  }
  async function apply() {
    if (!editing || !candidate || !map?.revision || busy || running) return;
    const input =
      candidate === physical(editing.input)
        ? editing.input
        : encoded(candidate, editing.input);
    if (!input) return;
    const generation = active.current;
    setBusy(true);
    setMessage("");
    try {
      const next = await invoke<KeyMap>("rebind_instance_key", {
        id: instanceId,
        revision: map.revision,
        action: editing.action,
        input,
        modifier: editingModifierChanged ? modifier : null,
      });
      if (generation !== active.current) return;
      setMap(next);
      setEditing(null);
      setCandidate("");
      setSelected(candidate);
      setConflictsOnly(false);
      setFilter("");
      setMessage("Tecla salva para esta instância.");
    } catch (e) {
      if (generation === active.current) setMessage(String(e));
    } finally {
      if (generation === active.current) setBusy(false);
    }
  }
  const choices =
    map && editing ? modifierChoices(map.modifierSupport, editing) : [];
  const editingModifierChanged =
    !!editing && choices.length > 0 && modifier !== editableModifier(editing);
  const effectiveModifier = editingModifierChanged
    ? modifier
    : (editing?.modifier ?? "");
  const changed =
    !!editing &&
    (candidate !== physical(editing.input) || editingModifierChanged);
  function key(id: string, width = 1) {
    const assigned = analysis.keys.get(id) ?? [];
    const clash = assigned.some((b) => analysis.conflicts.has(combo(b)));
    const chosen = editing ? candidate === id : selected === id;
    return (
      <button
        type="button"
        key={id}
        className={
          "secondary km-key " +
          (clash ? "km-conflict" : assigned.length ? "km-used" : "") +
          (chosen ? " km-current" : "")
        }
        style={{ flexGrow: width }}
        disabled={busy || (!!editing && !encoded(id, editing.input))}
        aria-pressed={chosen}
        onClick={() => choose(id)}
        aria-label={
          keyLabel(id, layout) +
          " · " +
          assigned.length +
          " comandos" +
          (clash ? " · conflito" : "")
        }
        title={assigned.map((b) => actionName(b.action)).join(" · ") || "Livre"}
      >
        {keyLabel(id, layout)}
        {clash && <sup>!</sup>}
      </button>
    );
  }
  const overlaps =
    editing && candidate && candidate !== "key.keyboard.unknown"
      ? (analysis.keys.get(candidate) ?? []).filter(
          (b) =>
            b.action !== editing.action &&
            combo(b) ===
              combo({
                ...editing,
                input: candidate,
                modifier: effectiveModifier,
              }),
        )
      : [];
  const sharedBase =
    !!editing &&
    candidate !== "key.keyboard.unknown" &&
    (analysis.keys.get(candidate) ?? []).some(
      (b) =>
        b.action !== editing.action &&
        combo(b) !==
          combo({ ...editing, input: candidate, modifier: effectiveModifier }),
    );
  return (
    <section className="km-panel" aria-label="Mapa de teclas da instância">
      <div className="km-heading">
        <div>
          <h3>Mapa de teclas</h3>
          <p>
            {instanceId}
            {map?.source === "defaults"
              ? " · Configuração inicial do modpack"
              : map?.source === "saved"
                ? " · Controles salvos no jogo"
                : ""}
          </p>
        </div>
        <label>
          Teclado{" "}
          <select
            value={layout}
            onChange={(e) => {
              const v = e.target.value as KeyboardLayout;
              setLayout(v);
              try {
                localStorage.setItem("aurora.keymap.layout", v);
              } catch {
                /* opção só nesta sessão */
              }
            }}
          >
            <option>ABNT2</option>
            <option>ANSI</option>
          </select>
        </label>
        <button
          className="secondary compact"
          type="button"
          disabled={busy}
          onClick={() => setRefresh((n) => n + 1)}
        >
          Recarregar
        </button>
      </div>
      {message && (
        <p role="status" className="km-message">
          {message}
        </p>
      )}
      {busy && !map && <p role="status">Lendo controles…</p>}
      {map && !map.bindings.length && (
        <p className="km-note">
          Nenhum comando salvo nesta instância. Abra o Minecraft, confira os
          controles e recarregue o mapa depois de fechar o jogo.
        </p>
      )}
      {map && !!map.bindings.length && (
        <>
          <div className="km-tools">
            <button
              className={
                "secondary compact" + (conflictsOnly ? " km-active" : "")
              }
              type="button"
              aria-pressed={conflictsOnly}
              onClick={() => {
                setConflictsOnly(!conflictsOnly);
                setSelected("");
                setFilter("");
              }}
            >
              {analysis.conflicts.size}{" "}
              {analysis.conflicts.size === 1
                ? "conflito possível"
                : "conflitos possíveis"}
            </button>
            <span className="km-legend">
              <i /> Em uso <b>!</b> Sobreposição
            </span>
          </div>
          {editing && (
            <div
              className="km-edit"
              role="group"
              aria-label="Reatribuir comando"
            >
              <strong>{actionName(editing.action)}</strong>
              <span>
                {candidate
                  ? [
                      modifierLabel(effectiveModifier),
                      keyLabel(candidate, layout),
                    ]
                      .filter(Boolean)
                      .join(" + ")
                  : "Escolha a nova tecla no teclado abaixo."}
                {overlaps.length > 0
                  ? " Já usada por " +
                    overlaps.map((b) => actionName(b.action)).join(", ") +
                    "."
                  : ""}
                {sharedBase
                  ? " Outra ação usa essa tecla com um modificador diferente. O contexto definido pelo mod pode permitir que as duas respondam juntas."
                  : ""}
              </span>
              {choices.length > 0 && (
                <label>
                  Modificador{" "}
                  <select
                    aria-label="Modificador"
                    value={modifier}
                    disabled={busy || running}
                    onChange={(e) => {
                      const next = choices.find(
                        (item) => item === e.target.value,
                      );
                      if (next) setModifier(next);
                    }}
                  >
                    {choices.map((item) => (
                      <option key={item} value={item}>
                        {forgeModifiers[item]}
                      </option>
                    ))}
                  </select>
                </label>
              )}
              {map.modifierSupport === "brokenForgeReader" ? (
                <small>
                  O Forge 1.21.1 verificado não relê novas combinações. É
                  possível remover um modificador salvo.
                </small>
              ) : (
                choices.length === 0 && (
                  <small>
                    {editableModifier(editing) === null
                      ? "O formato de modificador deste mod será preservado."
                      : "Neste perfil, modificadores dependem do suporte do jogo ou do mod."}
                  </small>
                )
              )}
              <button
                type="button"
                className="secondary compact"
                disabled={busy || running}
                onClick={() => {
                  setCandidate("key.keyboard.unknown");
                  if (choices.includes("NONE")) setModifier("NONE");
                  setMessage("");
                }}
              >
                Sem tecla
              </button>
              <button
                type="button"
                className="compact"
                disabled={
                  busy ||
                  running ||
                  !candidate ||
                  !changed ||
                  (choices.length > 0 &&
                    !modifierApplies(candidate, effectiveModifier)) ||
                  (map.modifierSupport === "brokenForgeReader" &&
                    !!effectiveModifier &&
                    effectiveModifier !== "NONE")
                }
                onClick={() => void apply()}
              >
                Salvar tecla
              </button>
              <button
                type="button"
                className="secondary compact"
                disabled={busy}
                onClick={() => {
                  setEditing(null);
                  setCandidate("");
                }}
              >
                Cancelar
              </button>
            </div>
          )}
          <div className="km-board" aria-label="Teclado interativo">
            {mainRows.map((row, i) => (
              <div className="km-row" key={i}>
                {row.map((k) => key(k.id, k.width))}
              </div>
            ))}
          </div>
          <details className="km-extra">
            <summary>Mouse, navegação e teclado numérico</summary>
            <div>{extraKeys.map((id) => key(id))}</div>
          </details>
          <div className="km-find">
            <label>
              <span>Buscar comando, mod ou tecla</span>
              <input
                type="search"
                value={filter}
                placeholder="Ex.: mapa, pular, espaço"
                onChange={(e) => {
                  setFilter(e.target.value);
                  setSelected("");
                }}
              />
            </label>
            {selected && (
              <button
                type="button"
                className="secondary compact"
                onClick={() => setSelected("")}
              >
                {keyLabel(selected, layout)} ×
              </button>
            )}
          </div>
          <p className="km-count">
            {visible.length} {visible.length === 1 ? "comando" : "comandos"}
            {map.source === "defaults"
              ? " · Abra o jogo uma vez para editar."
              : running
                ? " · Feche o jogo para editar."
                : ""}
          </p>
          <div className="km-list">
            {visible.slice(0, shownCount).map((b) => (
              <div key={b.action} className="km-action">
                <kbd>
                  {modifierLabel(b.modifier)
                    ? modifierLabel(b.modifier) + " + "
                    : ""}
                  {keyLabel(physical(b.input), layout)}
                </kbd>
                <div>
                  <strong>{actionName(b.action)}</strong>
                  <small>{b.action}</small>
                </div>
                {analysis.conflicts.has(combo(b)) && (
                  <span
                    className="km-warning"
                    title="Mais de um comando usa a mesma combinação. Alguns mods só a utilizam em telas diferentes."
                  >
                    Sobreposição
                  </span>
                )}
                <button
                  className="secondary compact"
                  type="button"
                  disabled={busy || running || !map.revision}
                  onClick={() => {
                    setEditing(b);
                    setCandidate(physical(b.input));
                    setModifier(editableModifier(b) ?? "NONE");
                    setMessage("");
                    document
                      .querySelector(".km-edit, .km-tools")
                      ?.scrollIntoView({ block: "nearest" });
                  }}
                >
                  Alterar
                </button>
              </div>
            ))}
          </div>
          {visible.length > shownCount && (
            <button
              type="button"
              className="secondary compact"
              onClick={() => setShownCount((count) => count + 60)}
            >
              Mostrar mais comandos
            </button>
          )}
          {!visible.length && (
            <p className="km-note">Nenhum comando corresponde à seleção.</p>
          )}
        </>
      )}
    </section>
  );
}
