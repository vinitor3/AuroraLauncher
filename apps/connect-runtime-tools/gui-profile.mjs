import { createHash } from 'node:crypto';

// Public names are explicitly reviewed input, never a desktop UUID or an
// authenticated Microsoft/Mojang account assertion. Do not normalize casing.
export function validateGuiNames(names) {
  const length = Array.isArray(names) ? names.length : 0;
  if (length < 1 || length > 2) throw new Error('runtime_gui_names');
  // Copy the reviewed indices, not a caller-controlled array iterator.
  const copy = length === 1 ? [names[0]] : [names[0], names[1]];
  if (copy.some(name => typeof name !== 'string' || name.length < 3 || name.length > 16
    || /[^A-Za-z0-9_]/u.test(name))
    || new Set(copy.map(name => name.toLowerCase())).size !== copy.length) throw new Error('runtime_gui_names');
  return Object.freeze(copy);
}

export function guiOfflineUuid(name) {
  validateGuiNames([name]);
  // Minecraft's UUID.nameUUIDFromBytes uses MD5 with version/variant bits.
  // This is offline game identity compatibility, not transport cryptography.
  const bytes = createHash('md5').update(`OfflinePlayer:${name}`, 'utf8').digest();
  bytes[6] = (bytes[6] & 0x0f) | 0x30;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = bytes.toString('hex');
  return [hex.slice(0, 8), hex.slice(8, 12), hex.slice(12, 16), hex.slice(16, 20), hex.slice(20)].join('-');
}

export function guiWhitelist(names) {
  return `${JSON.stringify(validateGuiNames(names).map(name => ({ uuid: guiOfflineUuid(name), name })), null, 2)}\n`;
}
