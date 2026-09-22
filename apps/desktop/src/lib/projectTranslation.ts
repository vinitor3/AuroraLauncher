const maximumSegmentLength = 2_400;
const maximumBatchLength = 2_500;
const maximumBatchSegments = 48;
const cjk = /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}]/u;
const cjkBoundary = /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}，。！？、：；]/u;
const preservedSelector = "code, pre, kbd, samp, script, style, textarea, svg, math, [translate=no], .notranslate, [data-translation-preserve]";
const blockSelector = "p, div, h1, h2, h3, h4, h5, h6, li, td, th, pre, blockquote, section, article, figcaption, summary";

export function secureProjectLinks(element: HTMLElement) {
  for (const anchor of element.querySelectorAll("a[href]")) {
    anchor.setAttribute("target", "_blank");
    anchor.setAttribute("rel", "noopener noreferrer");
  }
}

export function splitTranslationText(text: string): string[] {
  const pieces: string[] = [];
  // CJK prose expands significantly in Portuguese and has no word spaces.
  const limit = cjk.test(text) ? 800 : maximumSegmentLength;
  while (text.length > limit) {
    const head = text.slice(0, limit);
    // Prefer a sentence/word boundary; never split a UTF-16 surrogate pair.
    const boundary = Math.max(...[". ", "! ", "? ", "。", "！", "？", "\n"].map(mark => head.lastIndexOf(mark)));
    let end = boundary > limit / 2 ? boundary + 1 : head.lastIndexOf(" ");
    if (end < limit / 2) end = limit;
    if (/[\uD800-\uDBFF]/.test(text[end - 1])) end -= 1;
    pieces.push(text.slice(0, end));
    text = text.slice(end);
  }
  if (text) pieces.push(text);
  return pieces;
}

export function translationBatches(segments: string[]): string[][] {
  if (segments.length > 1_600 || segments.reduce((total, text) => total + text.length, 0) > 60_000) {
    throw new Error("Esta página é muito longa para traduzir de uma vez. Consulte a página original.");
  }
  const batches: string[][] = [];
  let current: string[] = [];
  let length = 0;
  for (const segment of segments) {
    if (!segment.trim() || segment.length > maximumSegmentLength) throw new Error("Trecho de tradução inválido.");
    if (current.length >= maximumBatchSegments || length + segment.length > maximumBatchLength) {
      batches.push(current);
      current = [];
      length = 0;
    }
    current.push(segment);
    length += segment.length;
  }
  if (current.length) batches.push(current);
  return batches;
}

export function validateProjectTranslations(value: unknown, originals: string[]): string[] {
  if (!Array.isArray(value) || value.length !== originals.length) {
    throw new Error("A tradução deste trecho veio incompleta.");
  }
  const ordered = new Map<number, string>();
  for (const item of value as unknown[]) {
    if (!item || typeof item !== "object" || !("id" in item) || typeof item.id !== "number"
      || !Number.isInteger(item.id) || item.id < 0 || item.id >= originals.length || ordered.has(item.id)
      || !("text" in item) || typeof item.text !== "string" || !item.text.trim()
      || item.text.length > originals[item.id].length * (cjk.test(originals[item.id]) ? 12 : 6) + 200) {
      throw new Error("Não foi possível conferir a tradução deste trecho.");
    }
    ordered.set(item.id, item.text.trim());
  }
  return originals.map((_, index) => ordered.get(index)!);
}

export function prepareProjectTranslation(element: HTMLElement) {
  // Never replace markup with model output: only text nodes in a detached clone.
  const clone = element.cloneNode(true) as HTMLElement;
  const walker = clone.ownerDocument.createTreeWalker(clone, 4 /* SHOW_TEXT */);
  const segments: string[] = [];
  const textNodes: Array<{ node: Node; original: string }> = [];
  const nodes: Array<{ node: Node; pieces: Array<{ before: string; after: string; index?: number; original: string }> }> = [];
  while (walker.nextNode()) {
    const node = walker.currentNode;
    const original = node.nodeValue ?? "";
    textNodes.push({ node, original });
    if (node.parentElement?.closest(preservedSelector) || !/\p{L}/u.test(original)
      || /^(?:https?:\/\/|www\.)\S+$/i.test(original.trim())) continue;
    const pieces = splitTranslationText(original).map((part) => {
      const before = part.match(/^\s*/)?.[0] ?? "";
      const after = part.match(/\s*$/)?.[0] ?? "";
      const text = part.trim();
      const index = text ? segments.push(text) - 1 : undefined;
      return { before, after, index, original: part };
    });
    nodes.push({ node, pieces });
  }
  const translatedNodes = new Set(nodes.map(({ node }) => node));
  return {
    segments,
    apply(translations: string[]) {
      if (translations.length !== segments.length || translations.some((text) => typeof text !== "string" || !text.trim())) {
        throw new Error("A tradução veio incompleta. O texto original foi mantido.");
      }
      for (const { node, pieces } of nodes) {
        node.nodeValue = pieces.map(({ before, after, index, original }, position) => {
          if (index === undefined) return original;
          const previous = pieces[position - 1];
          const text = translations[index].trim();
          // A no-space CJK boundary needs a word separator once translated into
          // Portuguese. Identity application must still restore the exact source.
          const separate = previous?.index !== undefined && !previous.after && !before
            && (text !== original.trim() || translations[previous.index] !== previous.original.trim())
            && /\p{L}/u.test(text[0] ?? "") && cjk.test(previous.original + original);
          return (separate ? " " : before) + text + after;
        }).join("");
      }
      // CJK does not require spaces around inline emphasis/links. Portuguese
      // does: insert separators in translated text, never in protected code or
      // markup. Original whitespace and punctuation remain authoritative.
      for (let index = 1; index < textNodes.length; index += 1) {
        const left = textNodes[index - 1];
        const right = textNodes[index];
        if (!cjkBoundary.test(Array.from(left.original).at(-1) ?? "") && !cjkBoundary.test(Array.from(right.original)[0] ?? "")) continue;
        if (left.node.parentElement?.closest(blockSelector) !== right.node.parentElement?.closest(blockSelector)) continue;
        const before = left.node.nodeValue ?? "";
        const after = right.node.nodeValue ?? "";
        if ((before === left.original && after === right.original) || !/[\p{L}\p{N}.,!?:;…]$/u.test(before) || !/^[\p{L}\p{N}]/u.test(after)) continue;
        if (translatedNodes.has(left.node) && before !== left.original) left.node.nodeValue = before + " ";
        else if (translatedNodes.has(right.node) && after !== right.original) right.node.nodeValue = " " + after;
      }
      return clone.innerHTML;
    },
  };
}

export type ProjectTranslationProgress = {
  html: string;
  completed: number;
  total: number;
  changed: boolean;
  complete: boolean;
};

export function createProjectTranslationJob(element: HTMLElement) {
  const plan = prepareProjectTranslation(element);
  const batches = translationBatches(plan.segments);
  const translations = [...plan.segments];
  let nextBatch = 0;
  let completed = 0;
  const progress = (): ProjectTranslationProgress => ({
    html: plan.apply(translations), completed, total: plan.segments.length,
    changed: translations.some((text, index) => text !== plan.segments[index]),
    complete: nextBatch === batches.length,
  });
  return {
    total: plan.segments.length,
    hasImages: Boolean(element.querySelector("img")),
    async run(
      translate: (segments: string[], signal: AbortSignal) => Promise<unknown>,
      signal: AbortSignal,
      onProgress: (progress: ProjectTranslationProgress) => void,
    ) {
      while (nextBatch < batches.length) {
        signal.throwIfAborted();
        const batch = batches[nextBatch];
        const result = await translate(batch, signal);
        // A late response to a cancelled/replaced page must never change it.
        signal.throwIfAborted();
        const checked = validateProjectTranslations(result, batch);
        translations.splice(completed, checked.length, ...checked);
        completed += checked.length;
        nextBatch += 1;
        onProgress(progress());
      }
      signal.throwIfAborted();
      return progress();
    },
  };
}
