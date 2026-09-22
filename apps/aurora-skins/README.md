# Aurora Skins

Aurora Skins 0.3.0 é o módulo de aparência do Aurora para Minecraft. O cliente oferece biblioteca, catálogo e aplicação de skins; instalado também no servidor, o mesmo JAR da combinação Minecraft/loader retransmite as skins entre os clientes participantes. A fonte do Launcher seleciona a build correspondente, com Aurora Core 1.1.4 (API mínima 1.1.3) no cliente. A distribuição pelo instalador depende da integração e do fluxo de release.

## Estado

**IMPLEMENTADO MAS NÃO HOMOLOGADO** em 22/09/2026. Nove JARs finais compilados e inspecionados; 27 testes do núcleo aprovados, incluindo sete do servidor e quatro do cliente de sincronização. Fabric 1.21.1 iniciou o retransmissor dedicado sem Core; Forge 1.21.1 aprovou a descoberta dos metadados sem Core. Ambos encerraram antes da aceitação da EULA, sem mundo ou jogadores. A prova visual com dois clientes, incluindo Chat Heads, **PRECISA DE TESTE**. Nenhum instalador foi gerado nesta tarefa. Hashes: [manifesto 0.3.0](../../releases/skins/0.3.0/manifest.json).

As evidências visuais anteriores de Skins 0.2.4 são históricas e não homologam 0.3.0. Veja [tarefa multiplayer](../../docs/task-skins-multiplayer.md), [matriz de compatibilidade](../../docs/engineering/COMPATIBILITY_MATRIX.md) e [reparos de runtime anteriores](../../docs/task-skins-runtime-repair.md).

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
- layout responsivo para resoluções lógicas pequenas e diferentes GUI scales.

O catálogo agora agrega MCStat e os desenhos originais Aurora, e a busca de jogadores usa mc-api com fallback MCStat. Digite `@nome`/`@UUID` em Explorar, ou `#tag` no catálogo. A prévia exibe a skin selecionada sem aplicá-la. Veja [fontes, configuração, limites e exemplos](../../docs/skin-sources.md).

A sincronização de favoritos/skin ativa pela API Aurora permanece **NÃO IMPLEMENTADO** nesta entrega. A retransmissão multiplayer é independente do catálogo: usa a conexão do Minecraft e não depende do Worker. O catálogo continua usando o Worker Aurora e suas APIs, sem scraping de sites. Esta worktree parte de `269d282`; as alterações posteriores de Skins 0.2.5, incluindo a grade de resultados no Minecraft, devem ser preservadas na integração antes da distribuição.

## Multiplayer e instalação

1. Em cada cliente participante, instalar Aurora Core 1.1.3 ou superior e Aurora Skins 0.3.0 para a versão do Minecraft e o loader da instância.
2. No servidor Fabric/Forge correspondente, instalar o JAR Aurora Skins 0.3.0 dessa mesma combinação. Aurora Core e Companion não são necessários no servidor.
3. Entrar no servidor e escolher uma skin no cliente. Após a confirmação do protocolo, o servidor distribui a skin aos participantes; novos participantes recebem as skins já anunciadas. Mudanças sucessivas convergem para a última seleção.

Clientes sem Aurora Skins podem entrar, sujeitos às demais exigências do servidor, mas continuam vendo as skins que o Minecraft normalmente fornece. Em servidor sem Aurora Skins, a skin local continua funcionando e não é compartilhada. A skin oficial da conta Mojang/Microsoft não é alterada.

O servidor identifica cada publicação pelo UUID da conexão, mantém apenas estado em memória e aceita até 256 participantes do protocolo. Há no máximo uma publicação por jogador a cada dois segundos; o cliente espaça os envios para respeitar esse limite. PNGs são validados, convertidos para 64x64 e recodificados sem metadados, com limite de 24 KiB no protocolo. Saída do jogador, desconexão e encerramento descartam o estado correspondente. A sincronização não envia credenciais e não usa armazenamento em nuvem; a biblioteca local existente continua persistente.

Usar cliente e servidor da mesma versão do Minecraft e do mesmo loader. Proxy/ViaVersion e travessia entre versões não são suportados; comunicação cruzada Fabric/Forge não está homologada. A decisão está em [ADR-018](../../docs/engineering/ADR/ADR-018-skins-server-relay.md).

## Compatibilidade com mods de aparência

Nos Minecrafts modernos, a skin é resolvida por UUID tanto no personagem quanto em `PlayerInfo`, consultado por chat/tab e outros mods. Em 1.19.2/1.20.1 há também substituição da consulta direta de skin em `SkinManager`; em 1.21.1 são atendidas a consulta e a função de consulta reutilizável. O adaptador 1.12.2 atualiza o perfil de renderização legado. Modelos slim/wide acompanham a textura, preservando capas/elytra vanilla e o fallback quando nenhuma skin Aurora está disponível.

Isso amplia a compatibilidade com mods como Chat Heads, mas **não adiciona cabeças ao chat e não inclui Chat Heads no JAR**. O teste com o mod externo ainda **PRECISA DE TESTE**. Mods que baixam diretamente da Mojang, mantêm uma cópia própria permanente da textura ou usam dados de cabeça gerados exclusivamente no servidor podem continuar mostrando a skin oficial. As propriedades oficiais do perfil e texturas de itens/blocos de cabeça não são reescritas.

## Matriz

| Minecraft | Loader | Java runtime | Build JDK | Core mínimo no cliente | Skins |
| --- | --- | ---: | ---: | --- | --- |
| 1.12.2 | Forge 14.23.5.2860 | 8 | 17, release 8 | 1.1.3 | 0.3.0 |
| 1.16.5 | Fabric 0.16.14 | 8 | 17, release 8 | 1.1.3 | 0.3.0 |
| 1.16.5 | Forge 36.2.42 | 8 | 17, release 8 | 1.1.3 | 0.3.0 |
| 1.19.2 | Fabric 0.16.14 | 17 | 17 | 1.1.3 | 0.3.0 |
| 1.19.2 | Forge 43.4.16 | 17 | 17 | 1.1.3 | 0.3.0 |
| 1.20.1 | Fabric 0.16.14 | 17 | 17 | 1.1.3 | 0.3.0 |
| 1.20.1 | Forge 47.4.23 | 17 | 17 | 1.1.3 | 0.3.0 |
| 1.21.1 | Fabric 0.16.14 | 21 | 21 | 1.1.3 | 0.3.0 |
| 1.21.1 | Forge 52.1.12 | 21 | 21 | 1.1.3 | 0.3.0 |

NeoForge está fora do escopo atual.

## Organização

- `core`: modelo, catálogo online/fallback, cache, validação, favoritos, histórico, geometrias responsivas e protocolo/cliente/servidor de sincronização;
- `minecraft/1.16.5`, `1.19.2`, `1.20.1`, `1.21.1`: adaptadores Fabric/Forge;
- `minecraft/legacy/forge-1.12.2`: adaptador Forge legado isolado;
- `releases/skins/0.3.0`: nove artefatos versionados e manifesto SHA-256; versões anteriores permanecem imutáveis.

Para compilar e testar toda a matriz sem alterar releases:

```powershell
npm run skins:build
```

O comando também gera cópias de conferência ignoradas pelo Git em `.aurora/build-matrix/skins`. Antes do instalador, `npm run release:verify-mods` recompila Core, Skins e Companion e exige correspondência SHA-256 com todos os 27 JARs publicados.
