const maximumSegmentLength = 3_500;
const maximumBatchLength = 7_000;
const maximumBatchSegments = 96;
const preservedSelector = "code, pre, kbd, samp, script, style, textarea, svg, math, [translate=no], .notranslate, [data-translation-preserve]";

export function secureProjectLinks(element: HTMLElement) {
  for (const anchor of element.querySelectorAll("a[href]")) {
    anchor.setAttribute("target", "_blank");
    anchor.setAttribute("rel", "noopener noreferrer");
  }
}

export function splitTranslationText(text: string): string[] {
  const pieces: string[] = [];
  while (text.length > maximumSegmentLength) {
    const head = text.slice(0, maximumSegmentLength);
    // Prefer a sentence/word boundary; never split a UTF-16 surrogate pair.
    const boundary = Math.max(head.lastIndexOf(". "), head.lastIndexOf("! "), head.lastIndexOf("? "));
    let end = boundary > maximumSegmentLength / 2 ? boundary + 1 : head.lastIndexOf(" ");
    if (end < maximumSegmentLength / 2) end = maximumSegmentLength;
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
    throw new Error("A tradução veio incompleta. O texto original foi mantido.");
  }
  return value.map((item: unknown, index) => {
    if (!item || typeof item !== "object" || !("id" in item) || item.id !== index
      || !("text" in item) || typeof item.text !== "string" || !item.text.trim()
      || item.text.length > originals[index].length * 6 + 200) {
      throw new Error("Não foi possível conferir a tradução. O texto original foi mantido.");
    }
    return item.text.trim();
  });
}

export function prepareProjectTranslation(element: HTMLElement) {
  // Never replace markup with model output: only text nodes in a detached clone.
  const clone = element.cloneNode(true) as HTMLElement;
  const walker = clone.ownerDocument.createTreeWalker(clone, 4 /* SHOW_TEXT */);
  const segments: string[] = [];
  const nodes: Array<{ node: Node; pieces: Array<{ before: string; after: string; index?: number; original: string }> }> = [];
  while (walker.nextNode()) {
    const node = walker.currentNode;
    const original = node.nodeValue ?? "";
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
  return {
    segments,
    apply(translations: string[]) {
      if (translations.length !== segments.length || translations.some((text) => typeof text !== "string" || !text.trim())) {
        throw new Error("A tradução veio incompleta. O texto original foi mantido.");
      }
      for (const { node, pieces } of nodes) {
        node.nodeValue = pieces.map(({ before, after, index, original }) => index === undefined
          ? original : before + translations[index].trim() + after).join("");
      }
      return clone.innerHTML;
    },
  };
}
