import type { LocalSkin } from "./localSkins";

export type EquippedSkinSelection = {
  id: string;
  skinModel: "classic" | "slim";
};

export function parseEquippedSkinSelection(raw: string | null): EquippedSkinSelection | undefined {
  if (!raw) return undefined;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return undefined;
    const { id, skinModel } = parsed as Record<string, unknown>;
    if (typeof id !== "string" || !id.trim() || id.length > 128
      || /^blob:/i.test(id.trim()) || /[\u0000-\u001f\u007f]/.test(id)
      || (skinModel !== "classic" && skinModel !== "slim")) return undefined;
    return { id, skinModel };
  } catch {
    return undefined;
  }
}

export async function findMatchingLocalSkin(
  records: LocalSkin[],
  image: Blob,
  preferredId?: string,
): Promise<LocalSkin | undefined> {
  const expected = new Uint8Array(await image.arrayBuffer());
  const preferred = records.find((record) => record.id === preferredId);
  const candidates = preferred ? [preferred, ...records.filter((record) => record !== preferred)] : records;
  for (const record of candidates) {
    if (!(record.image instanceof Blob) || record.image.size !== expected.length) continue;
    try {
      const actual = new Uint8Array(await record.image.arrayBuffer());
      // A saved ID or matching file length is not proof that these are the
      // equipped PNG bytes. In particular, different skins often share a size.
      if (actual.length === expected.length && actual.every((byte, index) => byte === expected[index])) return record;
    } catch {
      // An unreadable legacy Blob must not prevent checking healthy records.
    }
  }
  return undefined;
}
