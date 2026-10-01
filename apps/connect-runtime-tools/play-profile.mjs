import { createHash } from 'node:crypto';
import { artifactPins, EULA_URL, SERVER_PROPERTIES } from './pins.mjs';
import { SVC_ARTIFACT, SVC_CONFIG_PATH, SVC_BASELINE_BYTES } from './svc-profile.mjs';
import { PLASMO_ARTIFACT, FABRIC_API_ARTIFACT, PLASMO_CONFIG_PATH, PLASMO_BASELINE_BYTES } from './plasmo-profile.mjs';
import { guiWhitelist } from './gui-profile.mjs';

// Fixed synthetic identities, never a Microsoft/Mojang account assertion. MD5
// here reproduces Minecraft's offline UUID naming, not transport cryptography.
export const PLAY_NAMES = Object.freeze(['AuroraLabA', 'AuroraLabB']);
export function offlineUuid(name) {
  if (!PLAY_NAMES.includes(name)) throw new Error('runtime_play_identity');
  const bytes = createHash('md5').update(`OfflinePlayer:${name}`, 'utf8').digest();
  bytes[6] = (bytes[6] & 0x0f) | 0x30;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = bytes.toString('hex');
  return [hex.slice(0, 8), hex.slice(8, 12), hex.slice(12, 16), hex.slice(16, 20), hex.slice(20)].join('-');
}
export function runtimeFiles(profile, guiNames) {
  if (!['status', 'play', 'svc', 'plasmo', 'gui'].includes(profile)) throw new Error('runtime_profile');
  if (profile !== 'gui' && guiNames !== undefined) throw new Error('runtime_gui_names');
  const files = { 'server.properties': SERVER_PROPERTIES, 'eula.txt': `# ${EULA_URL}\neula=false\n` };
  if (profile === 'play' || profile === 'gui') {
    // Separate reviewed plan. Keep whitelist active; no personal instance,
    // arbitrary name, compression or signed account/chat authentication.
    files['server.properties'] += 'network-compression-threshold=-1\nenforce-secure-profile=false\n';
    files['whitelist.json'] = profile === 'gui' ? guiWhitelist(guiNames)
      : `${JSON.stringify(PLAY_NAMES.map(name => ({ uuid: offlineUuid(name), name })), null, 2)}\n`;
  }
  if (profile === 'svc') files[SVC_CONFIG_PATH] = SVC_BASELINE_BYTES.toString('utf8');
  if (profile === 'plasmo') files[PLASMO_CONFIG_PATH] = PLASMO_BASELINE_BYTES.toString('utf8');
  return files;
}
export function runtimeArtifacts(profile) {
  if (!['status', 'play', 'svc', 'plasmo', 'gui'].includes(profile)) throw new Error('runtime_profile');
  return [...artifactPins(), ...(profile === 'svc' ? [{ ...SVC_ARTIFACT }] : []),
    ...(profile === 'plasmo' ? [{ ...PLASMO_ARTIFACT }, { ...FABRIC_API_ARTIFACT }] : [])];
}
