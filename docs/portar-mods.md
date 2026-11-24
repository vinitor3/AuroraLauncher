# Portar Aurora Core e Skins

1. Consulte a [versão oficial do Minecraft](https://piston-meta.mojang.com/mc/game/version_manifest_v2.json), o Java do perfil, os [loaders Fabric](https://meta.fabricmc.net/v2/versions/loader) e as [promoções Forge](https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json). Use dependências publicadas para a versão exata.
2. Reutilize `apps/aurora-core/api`, `apps/aurora-core/runtime` e `apps/aurora-skins/core`. Coloque apenas os adaptadores de tela, renderização, rede e loader em `apps/aurora-{core,skins}/minecraft/<versão>`. Confira o [guia de port do Fabric](https://docs.fabricmc.net/develop/porting/).
3. Adicione o par Minecraft/loader a `scripts/active-mod-matrix.json`, com versões de Java e loader. O campo opcional `defaultInstallLoaderVersion` seleciona um loader mais recente nas novas instâncias após teste no cliente. O empacotamento Gradle, a seleção dos JARs no Rust e a indicação de recursos na interface leem essa matriz. Os perfis experimentais de Aurora Connect têm homologação própria.
4. Compile em ordem no PowerShell. Para um par novo, substitua a versão abaixo pela sua. Sem `-Minecraft`, os scripts percorrem a matriz inteira.

```powershell
./scripts/build-aurora-core.ps1 -Minecraft 1.21.1
./scripts/build-aurora-skins.ps1 -Minecraft 1.21.1
```

5. Confira os JARs em `.aurora/build-matrix` e as licenças das bibliotecas incorporadas. Para publicar, use versões inéditas em `releases/{core,skins}`. Gere o manifesto do Core com `node scripts/create-aurora-core-release-manifest.mjs` e o de Skins com `node scripts/skins-release-manifest.mjs --create`. Execute `npm run release:verify-mods` antes do instalador.
6. Abra uma instância de teste. Registre Minecraft, loader, Java, SHA-256, handshake, tela Opções, troca de skin e capa e encerramento. Teste multiplayer com dois clientes antes de declarar compatibilidade multiplayer.
