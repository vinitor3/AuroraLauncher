# Aurora Skins

Aurora Skins 0.2.4 é o módulo oficial, independente e client-side para aparência dentro do Minecraft. A fonte do Launcher instala a build correta automaticamente depois do Aurora Core 1.1.4 (API mínima 1.1.3) e antes do Companion 0.2.3. A distribuição ao usuário depende da próxima integração/compilação do instalador.

## Estado

**PARCIAL**: nove JARs compilados e inspecionados pelos testes Rust; verificações visuais isoladas em Forge 1.12.2 e Fabric 1.16.5, 1.19.2 e 1.21.1. A rodada legada também verificou a skin em um mundo novo. Isso não homologa os nove runtimes nem o IPC/multiplayer. Evidências e hashes: [reparos de runtime](../../docs/task-skins-runtime-repair.md).

Nesta versão estão implementados:

- módulo registrado pela API do Core, sem duplicar sessão ou IPC;
- biblioteca local endereçada por SHA-256, favoritos e histórico limitado;
- importação validada de PNG real 64x64 e 64x32 de até 2 MB;
- seleção manual Classic/Wide ou Slim e conversão visual correta de 64x32;
- aplicação dinâmica da skin do jogador local;
- fallback assíncrono para a skin HTTPS do perfil do Launcher;
- catálogo HTTPS oficial com pesquisa e paginação, cache local e fallback original embutido quando a rede estiver indisponível;
- página Skins nas Opções Aurora;
- botão alinhado dinamicamente e personagem 3D na tela principal;
- preview com duas camadas e cabeça/corpo suavizados em direção ao mouse;
- layout responsivo para resoluções lógicas pequenas e diferentes GUI scales.

O catálogo agora agrega MCStat e os desenhos originais Aurora, e a busca de jogadores usa mc-api com fallback MCStat. Digite `@nome`/`@UUID` em Explorar, ou `#tag` no catálogo. A prévia exibe a skin selecionada sem aplicá-la. Veja [fontes, configuração, limites e exemplos](../../docs/skin-sources.md).

Permanecem **NÃO IMPLEMENTADOS** nesta entrega: grade de thumbnails dentro do Minecraft, sincronização de favoritos/skin ativa pela API Aurora e propagação de skins entre jogadores no multiplayer. No desktop existe grade de resultados. Não há scraping de sites: os mods usam somente o Worker Aurora e este consulta as APIs oficiais.

## Matriz

| Minecraft | Loader | Java runtime | Build JDK | Core mínimo | Skins |
| --- | --- | ---: | ---: | --- | --- |
| 1.12.2 | Forge 14.23.5.2860 | 8 | 17, release 8 | 1.1.3 | 0.2.4 |
| 1.16.5 | Fabric 0.16.14 | 8 | 17, release 8 | 1.1.3 | 0.2.4 |
| 1.16.5 | Forge 36.2.42 | 8 | 17, release 8 | 1.1.3 | 0.2.4 |
| 1.19.2 | Fabric 0.16.14 | 17 | 17 | 1.1.3 | 0.2.4 |
| 1.19.2 | Forge 43.4.16 | 17 | 17 | 1.1.3 | 0.2.4 |
| 1.20.1 | Fabric 0.16.14 | 17 | 17 | 1.1.3 | 0.2.4 |
| 1.20.1 | Forge 47.4.23 | 17 | 17 | 1.1.3 | 0.2.4 |
| 1.21.1 | Fabric 0.16.14 | 21 | 21 | 1.1.3 | 0.2.4 |
| 1.21.1 | Forge 52.1.12 | 21 | 21 | 1.1.3 | 0.2.4 |

NeoForge está fora do escopo atual.

## Organização

- `core`: modelo, catálogo online/fallback, cache, validação, favoritos, histórico e geometrias responsivas;
- `minecraft/1.16.5`, `1.19.2`, `1.20.1`, `1.21.1`: adaptadores Fabric/Forge;
- `minecraft/legacy/forge-1.12.2`: adaptador Forge legado isolado;
- `releases/skins/0.2.4`: artefatos imutáveis referenciados pela fonte do Launcher.

Para compilar e testar toda a matriz sem alterar releases:

```powershell
npm run skins:build
```

O comando também gera cópias de conferência ignoradas pelo Git em `.aurora/build-matrix/skins`. Antes do instalador, `npm run release:verify-mods` recompila Core, Skins e Companion e exige correspondência SHA-256 com todos os 27 JARs publicados.
