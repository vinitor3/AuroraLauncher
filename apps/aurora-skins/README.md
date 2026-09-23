# Aurora Skins

Aurora Skins 0.3.1 integra a grade e os caches da 0.2.5 com o multiplayer e a compatibilidade da 0.3.0. A fonte do Launcher instala a build correta depois do Aurora Core 1.1.5 (API mínima 1.1.3) no cliente. O mesmo JAR, instalado no servidor correspondente, retransmite as skins sem precisar do Core no servidor. O Assistente desktop permanece no Launcher, sem Companion.

## Estado

**IMPLEMENTADO MAS NÃO HOMOLOGADO**: integração dos nove alvos. Evidências e limites em [registro da integração](../../docs/task-skins-session-integration.md). O usuário relatou sucesso de multiplayer e Chat Heads na 0.3.0 em Minecraft 1.20.1 Forge e Fabric; isso não homologa toda a matriz nem os novos hashes da 0.3.1. Os registros da [grade 0.2.5](../../docs/task-skins-grid.md) e do [multiplayer 0.3.0](../../docs/task-skins-multiplayer.md) preservam as evidências anteriores.

Nesta versão estão implementados:

- módulo registrado pela API do Core, sem duplicar sessão ou IPC;
- biblioteca local endereçada por SHA-256, favoritos e histórico limitado;
- importação validada de PNG real 64x64 e 64x32 de até 2 MB;
- seleção manual Classic/Wide ou Slim e conversão visual correta de 64x32;
- aplicação dinâmica da skin do jogador local e recebimento das skins dos demais participantes pelo servidor;
- importação do arquivo de skin do perfil do Launcher e fallback HTTPS nos adaptadores que já o suportam;
- catálogo HTTPS oficial com pesquisa e paginação, cache local e fallback original embutido quando a rede estiver indisponível;
- página Skins nas Opções Aurora;
- botão alinhado dinamicamente e personagem 3D na tela principal;
- preview com duas camadas e cabeça/corpo suavizados em direção ao mouse;
- layout responsivo para resoluções lógicas pequenas e diferentes GUI scales;
- grade Explorar de 4 × 3, ou 6 × 2 em áreas baixas, sem texto dentro dos cards;
- seleção por identidade, loading/falha por slot e descarte de respostas antigas;
- prévias deduplicadas, cache limitado em memória e reaproveitamento do PNG validado em disco.

O catálogo agora agrega MCStat e os desenhos originais Aurora, e a busca de jogadores usa mc-api com fallback MCStat. Digite `@nome`/`@UUID` em Explorar, ou `#tag` no catálogo. Explorar mostra 12 slots com modelos de corpo inteiro. Clique numa prévia pronta para selecionar; **Usar skin** aplica explicitamente. As setas trocam lotes inteiros de 12; slots sem resultado ficam vazios e inativos. Veja [fontes, configuração, limites e exemplos](../../docs/skin-sources.md).

A sincronização de favoritos/skin ativa pela API Aurora permanece **NÃO IMPLEMENTADO**. A retransmissão multiplayer usa a conexão do Minecraft e não depende do Worker. O catálogo mantém o Worker e suas APIs, sem scraping.

## Multiplayer e compatibilidade

Instale Core 1.1.3+ e Skins 0.3.1 em cada cliente participante; no servidor, instale somente o JAR Skins 0.3.1 da mesma combinação Minecraft/loader. Ao entrar, os participantes recebem as aparências já anunciadas e as mudanças seguintes. Clientes sem Skins continuam vendo a aparência fornecida pelo Minecraft; em servidor sem Skins, a aplicação local continua disponível.

O servidor vincula cada publicação ao UUID da conexão e mantém somente estado em memória, limitado a 256 participantes e uma publicação por jogador a cada dois segundos. PNGs são validados e normalizados para 64x64, sem metadados, com limite de 24 KiB. Não são enviadas credenciais nem alterada a skin oficial da conta. A biblioteca local continua persistente.

A resolução por UUID em `PlayerInfo` e nas consultas disponíveis de `SkinManager` amplia a compatibilidade com Chat Heads e outros mods. O adaptador 1.12.2 mantém a integração de perfil legado. Modelos slim/wide, capas/elytra oficiais e fallback vanilla são preservados. Aurora não adiciona cabeças ao chat nem inclui o mod Chat Heads. Mods que baixam diretamente da Mojang, mantêm caches próprios ou geram cabeças exclusivamente no servidor podem continuar exibindo a skin oficial.

Proxy/ViaVersion e travessia entre versões não são suportados; comunicação cruzada Fabric/Forge não está homologada. Decisão: [ADR-022](../../docs/engineering/ADR/ADR-022-skins-server-relay.md).

## Matriz

| Minecraft | Loader de build | Java runtime | Build JDK | Core mínimo | Skins |
| --- | --- | ---: | ---: | --- | --- |
| 1.12.2 | Forge 14.23.5.2860 | 8 | 17, release 8 | 1.1.3 | 0.3.1 |
| 1.16.5 | Fabric 0.16.14 | 8 | 17, release 8 | 1.1.3 | 0.3.1 |
| 1.16.5 | Forge 36.2.42 | 8 | 17, release 8 | 1.1.3 | 0.3.1 |
| 1.19.2 | Fabric 0.16.14 | 17 | 17 | 1.1.3 | 0.3.1 |
| 1.19.2 | Forge 43.4.16 | 17 | 17 | 1.1.3 | 0.3.1 |
| 1.20.1 | Fabric 0.16.14 | 17 | 17 | 1.1.3 | 0.3.1 |
| 1.20.1 | Forge 47.4.23 | 17 | 17 | 1.1.3 | 0.3.1 |
| 1.21.1 | Fabric 0.16.14 | 21 | 21 | 1.1.3 | 0.3.1 |
| 1.21.1 | Forge 52.1.12 | 21 | 21 | 1.1.3 | 0.3.1 |

As versões de loader acima são as dependências de compilação. Versões mínimas selecionadas ficam em `scripts/active-mod-matrix.json`; os ambientes realmente executados estão no registro da grade. NeoForge está fora do escopo atual.

## Organização

- `core`: modelo, catálogo online/fallback, cache, validação, favoritos, histórico, geometrias responsivas e protocolo/cliente/servidor de sincronização;
- `minecraft/1.16.5`, `1.19.2`, `1.20.1`, `1.21.1`: adaptadores Fabric/Forge;
- `minecraft/legacy/forge-1.12.2`: adaptador Forge legado isolado;
- `releases/skins/0.3.1`: artefatos imutáveis referenciados pela fonte do Launcher.

Para compilar e testar toda a matriz sem alterar releases:

```powershell
npm run skins:build
```

O comando também gera cópias de conferência ignoradas pelo Git em `.aurora/build-matrix/skins`. Antes do instalador, `npm run release:verify-mods` recompila Core e Skins e exige correspondência normalizada com os 18 JARs selecionados, além de tamanho/hash, metadata e assinatura aplicáveis. Releases históricas não são sobrescritas.


O wrapper Gradle 8.4 compartilhado fica em `tools/gradle/8.4`; a geração 1.21.1 preserva o wrapper 9.7.1 próprio do Core, usado também pelo Skins. Execute builds sequencialmente para evitar concorrência no cache Loom.
