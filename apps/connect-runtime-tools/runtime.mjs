import { mkdir, lstat, mkdtemp, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { artifactPins, EULA_URL } from './pins.mjs';
import { runtimeFiles } from './play-profile.mjs';
import { hash, fail, readLocal, absent, downloadVerified } from './files.mjs';
import { rejectLinkedParents, resolveExplicitLocalPath } from '../connect-voice-tools/local-path.mjs';

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const json = value => `${JSON.stringify(value, null, 2)}\n`;
export const planDigest = plan => hash(json(plan));

export async function makePlan(javaHome, profile = 'status') {
  const files = runtimeFiles(profile);
  if (process.platform !== 'win32') fail('runtime_windows_only');
  const home = resolveExplicitLocalPath(javaHome);
  const release = await readLocal(join(home, 'release'), 64 * 1024);
  let text;
  try { text = new TextDecoder('utf-8', { fatal: true }).decode(release); } catch { fail('runtime_java_release'); }
  const field = key => {
    const matches = [...text.matchAll(new RegExp(`^${key}="([^"\\r\\n]+)"\\r?$`, 'gm'))];
    if (matches.length !== 1) fail('runtime_java_release');
    return matches[0][1];
  };
  const version = field('JAVA_VERSION');
  if (!/^21(?:\.\d+){1,3}(?:\+\d+)?$/.test(version) || field('OS_NAME') !== 'Windows' || field('OS_ARCH') !== 'x86_64') fail('runtime_java_target');
  const executable = join(home, 'bin', 'java.exe');
  const binary = await readLocal(executable, 4 * 1024 * 1024);
  if (binary.length < 2 || binary.toString('ascii', 0, 2) !== 'MZ') fail('runtime_java_binary');
  const artifacts = artifactPins();
  return {
    schemaVersion: 1, purpose: `aurora_connect_disposable_${profile}_lab`,
    target: { minecraft: '1.21.1', loader: 'fabric', loaderVersion: '0.16.14', javaMajor: 21, protocol: 767 },
    java: { home, executable, version, releaseSha256: hash(release), executableSha256: hash(binary) },
    artifacts, downloadBytes: artifacts.reduce((sum, item) => sum + item.bytes, 0),
    files,
    launch: { executable, args: ['-Xms256M', '-Xmx1536M', '-Dfabric.gameJarPath=server.jar', '-cp',
      artifacts.filter(item => item.file.startsWith('libraries/')).map(item => item.file).join(';'),
      'net.fabricmc.loader.impl.launch.knot.KnotServer', 'nogui'],
      cwd: 'fresh_lab_directory', unsetChildEnvironment: ['JAVA_TOOL_OPTIONS', '_JAVA_OPTIONS', 'JDK_JAVA_OPTIONS', 'CLASSPATH'],
      executableNotRunByThisTool: true },
    consent: { minecraftEula: EULA_URL, downloadRequiresUserAcceptance: true, eulaFileRemainsFalse: true },
    limits: { totalDownloadSeconds: 180, perArtifactSeconds: 90, retries: 0, redirects: false,
      personalInstanceAccess: false, relay: false, voiceMods: false, coreSkins: false, runtimeHomologated: false },
  };
}

async function ensurePlainDirectory(path) {
  await rejectLinkedParents(path);
  try { await mkdir(path); } catch (error) { if (error.code !== 'EEXIST') throw error; }
  const stat = await lstat(path);
  if (!stat.isDirectory() || stat.isSymbolicLink()) fail('runtime_unsafe_directory');
}

// The only production entry accepts a fresh plan derived from fixed code and the
// explicitly selected JDK. Never load URLs, paths or commands from a plan file.
export async function stageRuntime(javaHome, approval, acceptedEula, profile = 'status') {
  if (acceptedEula !== true) fail('runtime_eula_acceptance_required');
  const plan = await makePlan(javaHome, profile);
  if (typeof approval !== 'string' || !/^[a-f0-9]{64}$/.test(approval) || approval !== planDigest(plan)) fail('runtime_plan_approval');
  const aurora = resolveExplicitLocalPath(join(REPO, '.aurora'));
  await ensurePlainDirectory(aurora);
  const parent = join(aurora, 'connect-runtime');
  await ensurePlainDirectory(parent);
  const root = await mkdtemp(join(parent, 'prepared-'));
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 180000);
  const writeNew = async (file, value) => { await absent(join(root, file)); await writeFile(join(root, file), value, { flag: 'wx' }); };
  try {
    await mkdir(join(root, 'libraries'));
    await writeNew('plan.json', json(plan));
    await writeNew('consent.json', json({ planSha256: approval, downloadEulaAccepted: true, javaExecuted: false }));
    const verified = [];
    for (const pin of plan.artifacts) {
      verified.push(await downloadVerified(pin, join(root, pin.file), { signal: controller.signal }));
    }
    // Recheck the selected files before declaring the staging complete.
    if (planDigest(await makePlan(javaHome, profile)) !== approval) fail('runtime_java_changed');
    controller.signal.throwIfAborted();
    for (const [file, bytes] of Object.entries(plan.files)) await writeNew(file, bytes);
    await writeNew('ready.json', json({ planSha256: approval, verified, javaExecuted: false, runtimeHomologated: false }));
    return { event: 'runtime_staged', directory: root, planSha256: approval, artifacts: verified.length,
      bytes: plan.downloadBytes, javaExecuted: false, eulaFile: false, runtimeHomologated: false };
  } catch (error) {
    // No recursive deletion or retries. Preserve only this new lab for inspection.
    await writeNew('failed.json', json({ event: 'runtime_staging_failed', ready: false })).catch(() => {});
    throw error;
  } finally { clearTimeout(timer); controller.abort(); }
}

export async function main(args) {
  if (args.length === 2 && ['plan', 'plan-play'].includes(args[0])) {
    const plan = await makePlan(args[1], args[0] === 'plan-play' ? 'play' : 'status');
    return { event: 'runtime_plan', planSha256: planDigest(plan), plan };
  }
  if (args.length === 4 && ['stage', 'stage-play'].includes(args[0]) && args[3] === '--user-accepted-minecraft-eula') {
    return stageRuntime(args[1], args[2], true, args[0] === 'stage-play' ? 'play' : 'status');
  }
  if (args.length === 3 && ['stage', 'stage-play'].includes(args[0])) fail('runtime_eula_acceptance_required');
  fail('runtime_usage');
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try { process.stdout.write(json(await main(process.argv.slice(2)))); }
  catch (error) {
    const code = /^runtime_[a-z_]+$/.test(error.message) ? error.message : 'runtime_io_or_network';
    process.stdout.write(json({ event: 'failed', code })); process.exitCode = 1;
  }
}
