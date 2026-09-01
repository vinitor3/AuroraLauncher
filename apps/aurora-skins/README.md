# Aurora Skins

Aurora Skins 0.1.0 é o módulo oficial, independente e client-side para aparência dentro do Minecraft. O Launcher instala a build correta automaticamente depois do Aurora Core 1.1.0 e antes do Companion 0.2.1.

## Estado

**IMPLEMENTADO MAS NÃO HOMOLOGADO** nas nove combinações suportadas. Todos os JARs compilam, têm bytecode compatível com a versão de Java e são inspecionados pelos testes Rust do Launcher. Build não equivale à homologação visual dentro do jogo.

Nesta versão estão implementados:

- módulo registrado pela API do Core, sem duplicar sessão ou IPC;
- biblioteca local endereçada por SHA-256, favoritos e histórico limitado;
- importação validada de PNG real 64x64 e 64x32 de até 2 MB;
- seleção manual Classic/Wide ou Slim e conversão visual correta de 64x32;
- aplicação dinâmica da skin do jogador local;
- fallback assíncrono para a skin HTTPS do perfil do Launcher;
- página Skins nas Opções Aurora;
- botão alinhado dinamicamente e personagem 3D na tela principal;
- preview com duas camadas e cabeça/corpo suavizados em direção ao mouse;
- layout responsivo para resoluções lógicas pequenas e diferentes GUI scales.

Permanecem **NÃO IMPLEMENTADOS** nesta entrega: catálogo online com provedor autorizado, grade de thumbnails remotos, sincronização de favoritos/skin ativa pela API Aurora e propagação de skins entre jogadores no multiplayer. A interface Explorar mantém fallback offline explícito; não é feito scraping de sites sem API permitida.

## Matriz

| Minecraft | Loader | Java runtime | Build JDK | Core mínimo | Skins |
| --- | --- | ---: | ---: | --- | --- |
| 1.12.2 | Forge 14.23.5.2860 | 8 | 17, release 8 | 1.1.0 | 0.1.0 |
| 1.16.5 | Fabric 0.16.14 | 8 | 17, release 8 | 1.1.0 | 0.1.0 |
| 1.16.5 | Forge 36.2.42 | 8 | 17, release 8 | 1.1.0 | 0.1.0 |
| 1.19.2 | Fabric 0.16.14 | 17 | 17 | 1.1.0 | 0.1.0 |
| 1.19.2 | Forge 43.4.16 | 17 | 17 | 1.1.0 | 0.1.0 |
| 1.20.1 | Fabric 0.16.14 | 17 | 17 | 1.1.0 | 0.1.0 |
| 1.20.1 | Forge 47.4.23 | 17 | 17 | 1.1.0 | 0.1.0 |
| 1.21.1 | Fabric 0.16.14 | 21 | 21 | 1.1.0 | 0.1.0 |
| 1.21.1 | Forge 52.1.12 | 21 | 21 | 1.1.0 | 0.1.0 |

NeoForge está fora do escopo atual.

## Organização

- `core`: modelo, provider abstrato, cache, validação, favoritos, histórico e geometrias responsivas;
- `minecraft/1.16.5`, `1.19.2`, `1.20.1`, `1.21.1`: adaptadores Fabric/Forge;
- `minecraft/legacy/forge-1.12.2`: adaptador Forge legado isolado;
- `releases/skins/0.1.0`: artefatos imutáveis embarcados no Launcher.

Para compilar e testar toda a matriz sem alterar releases:

```powershell
npm run skins:build
```

O comando também gera cópias de conferência ignoradas pelo Git em `.aurora/build-matrix/skins`. Antes do instalador, `npm run release:verify-mods` recompila Core, Skins e Companion e exige correspondência SHA-256 com todos os 27 JARs publicados.
