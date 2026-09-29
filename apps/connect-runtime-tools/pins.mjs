// Official metadata inspected 2026-09-29. No executable is bundled or fetched on import.
const maven = 'https://maven.fabricmc.net/';
const library = (path, bytes, digest) => ({
  file: `libraries/${path.split('/').at(-1)}`, url: maven + path,
  bytes, algorithm: 'sha256', digest,
});
export function artifactPins() {
  return [
    { file: 'server.jar', url: 'https://piston-data.mojang.com/v1/objects/59353fb40c36d304f2035d51e7d6e6baa98dc05c/server.jar',
      bytes: 51627615, algorithm: 'sha1', digest: '59353fb40c36d304f2035d51e7d6e6baa98dc05c' },
    library('org/ow2/asm/asm/9.8/asm-9.8.jar', 126113, '876eab6a83daecad5ca67eb9fcabb063c97b5aeb8cf1fca7a989ecde17522051'),
    library('org/ow2/asm/asm-analysis/9.8/asm-analysis-9.8.jar', 35151, 'e640732fbcd3c6271925a504f125e38384688f4dfbbf92c8622dfcee0d09edb9'),
    library('org/ow2/asm/asm-commons/9.8/asm-commons-9.8.jar', 73498, '3301a1c1cb4c59fcc5292648dac1d7c5aed4c0f067dfbe88873b8cdfe77404f4'),
    library('org/ow2/asm/asm-tree/9.8/asm-tree-9.8.jar', 51934, '14b7880cb7c85eed101e2710432fc3ffb83275532a6a894dc4c4095d49ad59f1'),
    library('org/ow2/asm/asm-util/9.8/asm-util-9.8.jar', 94559, '8ba0460ecb28fd0e2980e5f3ef3433a513a457bc077f81a53bdc75b587a08d15'),
    library('net/fabricmc/sponge-mixin/0.15.5+mixin.0.8.7/sponge-mixin-0.15.5+mixin.0.8.7.jar', 1499194, '01dd2b778ed5283bce6b6b07d9690d86d956b17a7103efafad47073db1599584'),
    library('net/fabricmc/intermediary/1.21.1/intermediary-1.21.1.jar', 657725, '6059157dfb4a536ec151697004f74f6004b7d346b289c2f89849e0e585aa36fa'),
    library('net/fabricmc/fabric-loader/0.16.14/fabric-loader-0.16.14.jar', 1416842, '9ed7b7f4197153ad97d71b4b9cd9148989e8f0c600934b035ce45f3ef003efc0'),
  ];
}
export const EULA_URL = 'https://www.minecraft.net/en-us/eula';
export const SERVER_PROPERTIES = [
  '# Disposable Aurora Connect STATUS laboratory; no personal world.',
  'server-ip=127.0.0.1', 'server-port=43001', 'online-mode=false',
  'white-list=true', 'enforce-whitelist=true', 'enable-rcon=false',
  'enable-query=false', 'enable-jmx-monitoring=false', 'enable-status=true',
  'max-players=2', 'view-distance=2', 'simulation-distance=5',
  'level-name=connect-lab-world', 'level-seed=0', 'generate-structures=false',
  'motd=Aurora Connect disposable status lab', '',
].join('\n');
