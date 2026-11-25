import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  actionName,
  combo,
  encoded,
  editableModifier,
  eventKey,
  eventMouse,
  forgeModifiers,
  modifierApplies,
  modifierChoices,
  modifierLabel,
  extras,
  groups,
  keyLabel,
  keyDescription,
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
  const panel = useRef<HTMLElement>(null);
  const heldLabel = useRef<HTMLSpanElement>(null);
  const capture = useRef<HTMLButtonElement>(null);
  const editTrigger = useRef<HTMLButtonElement | null>(null);
  const pendingModifier = useRef<string | null>(null);
  useEffect(() => {
    pendingModifier.current = null;
    if (editing) {
      capture.current?.focus();
    } else if (editTrigger.current) {
      editTrigger.current.focus({ preventScroll: true });
      editTrigger.current = null;
    }
  }, [editing]);
  useEffect(() => {
    const held = new Map<string, Element | null>();
    function labelHeld() {
      if (heldLabel.current) heldLabel.current.textContent = held.size
        ? [...held.keys()].map((key) => keyDescription(key, layout)).join(" + ")
        : "Pressionada";
    }
    function mark(id: string | undefined, down: boolean, target: EventTarget | null) {
      if (!id) return;
      if (!down) {
        held.get(id)?.removeAttribute("data-held");
        if (held.delete(id)) labelHeld();
        return;
      }
      if (held.has(id) || !panel.current) return;
      if (target instanceof HTMLElement && target !== document.body &&
        (!panel.current.contains(target) || target.closest("input,select,textarea,[contenteditable=true]"))) return;
      const keycap = panel.current.querySelector('[data-key="' + id + '"]');
      keycap?.setAttribute("data-held", "");
      held.set(id, keycap);
      labelHeld();
    }
    const press = (ev: KeyboardEvent) => { if (!ev.isComposing) mark(eventKey(ev.code), true, ev.target); };
    const release = (ev: KeyboardEvent) => mark(eventKey(ev.code), false, ev.target);
    const pressMouse = (ev: MouseEvent) => mark(eventMouse(ev.button), true, ev.target);
    const releaseMouse = (ev: MouseEvent) => mark(eventMouse(ev.button), false, ev.target);
    function reset() {
      for (const keycap of held.values()) keycap?.removeAttribute("data-held");
      held.clear();
      labelHeld();
      pendingModifier.current = null;
    }
    const visibility = () => { if (document.hidden) reset(); };
    window.addEventListener("keydown", press, true);
    window.addEventListener("keyup", release, true);
    window.addEventListener("mousedown", pressMouse, true);
    window.addEventListener("mouseup", releaseMouse, true);
    window.addEventListener("pointercancel", reset);
    window.addEventListener("blur", reset);
    document.addEventListener("visibilitychange", visibility);
    return () => {
      window.removeEventListener("keydown", press, true);
      window.removeEventListener("keyup", release, true);
      window.removeEventListener("mousedown", pressMouse, true);
      window.removeEventListener("mouseup", releaseMouse, true);
      window.removeEventListener("pointercancel", reset);
      window.removeEventListener("blur", reset);
      document.removeEventListener("visibilitychange", visibility);
      reset();
    };
  }, [instanceId, layout, refresh]);
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
  const backend = map?.inputBackend;
  const analysis = useMemo(() => {
    const list = bindings ?? [];
    const conflicts = new Set(
      [...groups(list, backend)]
        .filter(([, actions]) => actions.length > 1)
        .map(([code]) => code),
    );
    const keys = new Map<string, Binding[]>();
    for (const b of list) {
      const id = physical(b.input, backend);
      const arr = keys.get(id) ?? [];
      arr.push(b);
      keys.set(id, arr);
    }
    return { keys, conflicts };
  }, [bindings, backend]);
  const query = filter.trim().toLocaleLowerCase();
  const visible = (bindings ?? [])
    .filter(
      (b) =>
        (!selected || physical(b.input, backend) === selected) &&
        (!conflictsOnly || analysis.conflicts.has(combo(b, backend))) &&
        (!query ||
          [
            b.action,
            actionName(b.action),
            keyLabel(physical(b.input, backend), layout),
            b.modifier,
          ].some((s) => s.toLocaleLowerCase().includes(query))),
    )
    .sort(
      (a, b) =>
        Number(analysis.conflicts.has(combo(b, backend))) -
          Number(analysis.conflicts.has(combo(a, backend))) ||
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
      candidate === physical(editing.input, backend)
        ? editing.input
        : encoded(candidate, editing.input, backend);
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
    (candidate !== physical(editing.input, backend) || editingModifierChanged);
  function recordInput(id: string | undefined, event?: Pick<KeyboardEvent, "altKey" | "ctrlKey" | "shiftKey" | "metaKey"> & { getModifierState?(name: "AltGraph"): boolean }) {
    if (!editing || !map || running || busy) return;
    const modifiers: ForgeModifier[] = [];
    if (event?.altKey) modifiers.push("ALT");
    if (event?.ctrlKey) modifiers.push("CONTROL");
    if (event?.shiftKey) modifiers.push("SHIFT");
    let error = "";
    if (!id || !encoded(id, editing.input, backend)) error = "Esta entrada não tem um código confirmado para o perfil da instância.";
    else if (event?.metaKey || event?.getModifierState?.("AltGraph") || modifiers.length > 1) error = "Escolha uma tecla simples ou uma combinação com um único modificador disponível.";
    else if (modifiers.length && (!choices.includes(modifiers[0]) || map.modifierSupport === "brokenForgeReader")) {
      error = "Este perfil não aceita essa combinação. Pressione uma tecla simples.";
    }
    setMessage(error);
    setCandidate(error ? "" : (id ?? ""));
    if (!error && choices.includes("NONE")) setModifier(modifiers[0] ?? "NONE");
  }
  function cancelEdit() {
    setEditing(null);
    setCandidate("");
    setMessage("");
  }
  function key(id: string, width = 1) {
    const assigned = analysis.keys.get(id) ?? [];
    const clash = assigned.some((b) => analysis.conflicts.has(combo(b, backend)));
    const chosen = editing ? candidate === id : selected === id;
    return (
      <button
        type="button"
        key={id}
        data-key={id}
        className={
          "secondary km-key " +
          (clash ? "km-conflict" : assigned.length ? "km-used" : "") +
          (chosen ? " km-current" : "")
        }
        style={{ flexGrow: width }}
        disabled={busy || (!!editing && (running || !encoded(id, editing.input, backend)))}
        aria-pressed={chosen}
        onClick={() => choose(id)}
        aria-label={
          keyDescription(id, layout) +
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
            combo(b, backend) ===
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
        combo(b, backend) !==
          combo({ ...editing, input: candidate, modifier: effectiveModifier }),
    );
  return (
    <section className="km-panel" ref={panel} aria-label="Mapa de teclas da instância"
      onKeyDown={(event) => {
        if (!editing || event.key !== "Escape") return;
        event.stopPropagation();
        if ((event.target as HTMLElement).closest("input,select,textarea") || busy) return;
        event.preventDefault();
        cancelEdit();
      }}>
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
            aria-label="Layout do teclado"
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
              <i /> Em uso <b>!</b> Sobreposição <i className="km-held-sample" /> <span ref={heldLabel}>Pressionada</span>
            </span>
          </div>
          {editing && (
            <div
              className="km-edit"
              role="group"
              aria-label="Reatribuir comando"
            >
              <strong>{actionName(editing.action)}</strong>
              <span role="status">
                {candidate
                  ? [
                      modifierLabel(effectiveModifier),
                      keyLabel(candidate, layout),
                    ]
                      .filter(Boolean)
                      .join(" + ")
                  : "Escolha uma entrada."}
                {overlaps.length > 0
                  ? " Já usada por " +
                    overlaps.map((b) => actionName(b.action)).join(", ") +
                    "."
                  : ""}
                {sharedBase
                  ? " Outra ação usa essa tecla com um modificador diferente. O contexto definido pelo mod pode permitir que as duas respondam juntas."
                  : ""}
              </span>
              <div className="km-capture-row">
                <button type="button" className="secondary km-capture" ref={capture} disabled={busy || running}
                  aria-describedby="km-capture-help"
                  onBlur={() => { pendingModifier.current = null; }}
                  onKeyDown={(event) => {
                    if (event.key === "Escape" || event.key === "Tab" || event.nativeEvent.isComposing) return;
                    event.preventDefault();
                    event.stopPropagation();
                    if (event.repeat) return;
                    if (/^(?:Shift|Control|Alt|Meta)(?:Left|Right)$/.test(event.code)) {
                      if (pendingModifier.current && pendingModifier.current !== event.code && event.key !== "AltGraph") {
                        pendingModifier.current = null;
                        setCandidate("");
                        setMessage("Solte os modificadores e pressione a combinação desejada.");
                        return;
                      }
                      pendingModifier.current = event.code;
                      return;
                    }
                    pendingModifier.current = null;
                    recordInput(eventKey(event.code), event);
                  }}
                  onKeyUp={(event) => {
                    if (event.code !== pendingModifier.current) return;
                    pendingModifier.current = null;
                    event.preventDefault();
                    recordInput(eventKey(event.code));
                  }}
                  onMouseDown={(event) => {
                    event.preventDefault();
                    event.currentTarget.focus();
                    pendingModifier.current = null;
                    recordInput(eventMouse(event.button), event);
                  }}
                  onMouseUp={(event) => event.preventDefault()}
                  onAuxClick={(event) => event.preventDefault()}
                  onContextMenu={(event) => event.preventDefault()}>
                  Capturar tecla ou botão do mouse
                </button>
                <small id="km-capture-help">Com a captura em foco, pressione a tecla ou clique aqui. Tab navega e Esc cancela. Para atribuir Tab ou Esc, escolha a tecla no mapa.</small>
              </div>
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
                onClick={cancelEdit}
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
                  {keyLabel(physical(b.input, backend), layout)}
                </kbd>
                <div>
                  <strong>{actionName(b.action)}</strong>
                  <small>{b.action}</small>
                </div>
                {analysis.conflicts.has(combo(b, backend)) && (
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
                  onClick={(event) => {
                    editTrigger.current = event.currentTarget;
                    setEditing(b);
                    setCandidate(physical(b.input, backend));
                    setModifier(editableModifier(b) ?? "NONE");
                    setMessage("");
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
