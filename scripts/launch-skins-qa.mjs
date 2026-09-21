// Local-only QA: read an installed instance's libraries/assets, never its worlds,
// accounts or launch tokens. All game writes go to a fresh .aurora/qa directory.
import { readFileSync, writeFileSync, existsSync, mkdirSync, copyFileSync, readdirSync, openSync } from 'node:fs';
import { resolve, join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';

const [baseInput, version, java, label] = process.argv.slice(2);
if (!baseInput || !version || !java || !/^[a-z0-9-]+$/.test(label || ''))
  throw new Error('Usage: node scripts/launch-skins-qa.mjs <cached instance> <version ID> <java.exe> <unique QA label>');
if (!java.toLowerCase().endsWith('javaw.exe')) throw new Error('Use javaw.exe: show only the game, no console');
const repository = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const base = resolve(baseInput);
const target = join(repository, '.aurora', 'qa', label);
if (existsSync(target)) throw new Error('Use a fresh QA label; existing test state is not overwritten');
const resolution = (process.env.AURORA_QA_RESOLUTION || '960x540').match(/^(\d{3,4})x(\d{3,4})$/);
if (!resolution || Number(resolution[1]) < 320 || Number(resolution[2]) < 240) throw new Error('Invalid QA resolution');
const child = JSON.parse(readFileSync(join(base, 'versions', version, `${version}.json`), 'utf8'));
const mc = child.inheritsFrom || child.id;
const parent = child.inheritsFrom ? JSON.parse(readFileSync(join(base, 'versions', mc, `${mc}.json`), 'utf8')) : child;
const loader = child.mainClass?.includes('fabric') ? 'fabric' : 'forge';
const matrix = JSON.parse(readFileSync(join(repository, 'scripts', 'active-mod-matrix.json'), 'utf8'));
const modPath = (name) => {
  const { version: modVersion } = matrix.modules.find(module => module.name === name);
  return join(repository, '.aurora', 'build-matrix', name, modVersion, mc, loader, `aurora-${name}-${loader}-${mc}-${modVersion}.jar`);
};
const core = modPath('core');
const skins = modPath('skins');
for (const file of [java, core, skins]) if (!existsSync(file)) throw new Error(`Required QA input missing: ${file}`);

function allowed(rules) {
  if (!rules) return true;
  let result = false;
  for (const rule of rules) {
    if (rule.features && Object.values(rule.features).some(Boolean)) continue;
    if (rule.os && (rule.os.name && rule.os.name !== 'windows' || rule.os.arch && !['x86_64', 'amd64'].includes(rule.os.arch))) continue;
    result = rule.action === 'allow';
  }
  return result;
}
function coordinatePath(coordinate) {
  const [group, artifact, ver, classifier] = coordinate.split(':');
  return `${group.replaceAll('.', '/')}/${artifact}/${ver}/${artifact}-${ver}${classifier ? `-${classifier}` : ''}.jar`;
}
const libraries = new Map();
for (const lib of [...(parent.libraries || []), ...(child === parent ? [] : child.libraries || [])]) {
  if (!allowed(lib.rules)) continue;
  const path = join(base, 'libraries', lib.downloads?.artifact?.path || coordinatePath(lib.name));
  // Legacy native-only libraries are already extracted under natives/.
  if (!existsSync(path) && lib.natives && !lib.downloads?.artifact) continue;
  if (!existsSync(path)) throw new Error(`Missing cached library: ${lib.name}`);
  const [group, artifact, , classifier] = lib.name.split(':');
  libraries.set(`${group}:${artifact}:${classifier || ''}`, path);
}
const client = join(base, 'versions', mc, `${mc}.jar`);
if (!existsSync(client)) throw new Error('Cached Minecraft client is missing');
mkdirSync(join(target, 'mods'), { recursive: true });
// Only this new QA directory receives a fixed scale for reproducible logical sizes.
if (process.env.AURORA_QA_RESOLUTION) writeFileSync(join(target, 'options.txt'), 'guiScale:2\n');
for (const file of readdirSync(join(base, 'mods'))) {
  // Fabric API supplies the mod resource loader (including translations).
  if (process.env.AURORA_QA_MINIMAL === '1' && !/^fabric-api-.*\.jar$/.test(file)) continue;
  if (file.endsWith('.jar') && !file.startsWith('aurora-')) copyFileSync(join(base, 'mods', file), join(target, 'mods', file));
}
copyFileSync(core, join(target, 'mods', `aurora-core-${mc}.jar`));
copyFileSync(skins, join(target, 'mods', `aurora-skins-${mc}.jar`));

const replacements = {
  auth_player_name: 'AuroraQA', auth_uuid: 'b8e1df26416936a79be1c08eb47936d4', auth_access_token: '0',
  auth_session: '0', user_type: 'legacy', user_properties: '{}', version_name: version, version_type: 'release',
  game_directory: target, assets_root: join(base, 'assets'), game_assets: join(base, 'assets', 'virtual', 'legacy'),
  assets_index_name: parent.assetIndex?.id || parent.assets, natives_directory: join(base, 'natives'),
  launcher_name: 'AuroraSkinsQA', launcher_version: matrix.modules.find(module => module.name === 'skins').version, clientid: '', auth_xuid: '',
  library_directory: join(base, 'libraries'), classpath_separator: ';',
  classpath: [...libraries.values(), client].join(';'), resolution_width: resolution[1], resolution_height: resolution[2],
};
function substitute(value) {
  return value.replace(/\$\{([^}]+)\}/g, (_, key) => {
    if (!(key in replacements)) throw new Error(`Unsupported launch placeholder: ${key}`);
    return replacements[key];
  });
}
function argumentsFor(values = []) {
  return values.flatMap(value => typeof value === 'string' ? [value] : allowed(value.rules) ? [value.value].flat() : []).map(substitute);
}
const jvm = ['-Xmx3G', `-Djava.library.path=${join(base, 'natives')}`, `-Daurora.skins.root=${join(target, '.aurora', 'skins')}`];
if (process.env.AURORA_QA_CATALOG_URL) {
  const fixture = new URL(process.env.AURORA_QA_CATALOG_URL);
  if (fixture.protocol !== 'http:' || fixture.hostname !== '127.0.0.1') throw new Error('QA catalog must be local');
  jvm.push(`-Daurora.skins.catalogUrl=${fixture}`);
}
jvm.push(...argumentsFor(parent.arguments?.jvm));
if (child !== parent) jvm.push(...argumentsFor(child.arguments?.jvm));
if (!jvm.includes('-cp') && !jvm.includes('-classpath')) jvm.push('-cp', replacements.classpath);
const gameArgs = parent.arguments?.game
  ? [...argumentsFor(parent.arguments.game), ...argumentsFor(child === parent ? [] : child.arguments?.game)]
  : (child.minecraftArguments || parent.minecraftArguments).split(/\s+/).map(substitute);
gameArgs.push('--width', resolution[1], '--height', resolution[2]);
const output = openSync(join(target, 'qa-process.log'), 'wx');
const processHandle = spawn(java, [...jvm, child.mainClass || parent.mainClass, ...gameArgs], {
  cwd: target, windowsHide: false, detached: true, stdio: ['ignore', output, output],
});
processHandle.unref();
console.log(JSON.stringify({ pid: processHandle.pid, minecraft: mc, loader, directory: target }));
