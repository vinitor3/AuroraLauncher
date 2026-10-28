# Compatibility Matrix

## Doctor, recuperação e cópias de teste — 29/09/2026

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. Launcher alpha.14 integra integralmente a recuperação manual de `be3a1c4`, preserva a base alpha.13 com Doctor, Gemini, Assistente e sessões, e acrescenta observações verificadas por tentativa e cópias independentes com aplicação seletiva, proteção e desfazer. Não amplia Minecraft/loaders/Java e conserva Core 1.1.5/Skins 0.3.1. Mundos são excluídos por padrão e nunca transferidos ao aplicar; cópia explícita de mundo serve somente ao teste. Testes automatizados usam dados descartáveis; partida real, skins/capa, multiplayer, autenticação e voz não recebem nova homologação por build. Evidências: [AUR-1167](../task-aur-1167-doctor-lab.md).

## Recuperação manual de instâncias — 29/09/2026

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. Base `dc2ec5a`, launcher alpha.13, Core 1.1.5 e Skins 0.3.1 preservados. Pontos manuais protegem o escopo local declarado com prévia, confirmação e desfazer, sem mundos, contas ou instalação compartilhada. Core/Skins administrados ficam fora da reposição de artefatos antigos; Companion não é reinstalado. Não amplia Minecraft/loaders/Java nem altera os 18 JARs. Os testes de filesystem e interface não homologam runtime Minecraft ou a UI nativa autenticada. Contrato e evidência: [AUR-1166](../task-aur-1166-instance-recovery.md).

## Failover Gemini no Worker — 29/09/2026

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. Base `a3325b5`, cliente alpha.13 existente, Core 1.1.5 e Skins 0.3.1 preservados. O Worker limita tentativas entre chaves e oferece Flash-Lite explicitamente autorizado como último recurso, com prazo e orçamento compartilhados. Tradução usa o mesmo transporte; contratos desktop e confirmação local permanecem iguais. Não exige novo instalador nem altera os 18 JARs ou a matriz Minecraft/loader/Java. Publicado na revisão `2026-09-29-gemini-key-failover-v1`, com saúde e proteção de sessão verificadas. Preview respondeu com Gemini real; chamada autenticada em produção permanece **NÃO VERIFICADO**, pois Firebase recusou sessão anônima de teste. Evidências em [task-gemini-failover.md](../task-gemini-failover.md).

## Gemini e ações locais confirmadas — 28/09/2026

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. O incremento parte de `ab1e1b5`, launcher alpha.12, Core 1.1.5 e Skins 0.3.1; o instalador alpha.13 foi gerado e o Worker foi publicado. A matriz ativa e as versões dos módulos permanecem iguais, com os 18 JARs reconstruídos e verificados. Assistente e Doctor compartilham transporte Gemini e revisão de remoção reversível de mod/ajuste de opções JSON permitidas, com histórico e restauração locais. Assistente e respostas estruturadas Doctor/Tools foram comprovados em preview com Gemini real e entradas sintéticas; também houve 503 intermitente. Testes de broker, DOM e Worker não homologam os pares Minecraft/loader abaixo. Evidências em [task-ai-tools.md](../task-ai-tools.md).

## Instalador integrado alpha.12 — 28/09/2026

Empacotamento: **CONCLUÍDO**. O build oficial de 23/09 foi conferido na retomada de 28/09, com 18/18 JARs Core 1.1.5/Skins 0.3.1, sessões independentes e Doctor com orientação Gemini opcional. Não altera a matriz de Minecraft/loaders/Java. Consulta Gemini real e homologação completa de Minecraft: **NÃO VERIFICADO**. Artefato e hashes em [task-release-alpha12.md](../task-release-alpha12.md).

## Aurora Doctor — orientação Gemini opcional — 23/09/2026

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. O incremento parte de `6f3a13b`, launcher alpha.11, Core 1.1.5 e Skins 0.3.1. Amplia regras locais e acrescenta explicação Gemini por ação explícita, com contrato reduzido, timeout e passos manuais permitidos. Não altera os JARs, a matriz de Minecraft/loaders/Java ou as sessões independentes. A análise determinística continua offline; testes com respostas sintéticas não homologam Gemini real nem uma nova implantação do Worker. Evidências em [task-doctor-gemini.md](../task-doctor-gemini.md).

## Skins 0.3.1 e sessões independentes — 23/09/2026

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. A integração parte do launcher alpha.10 atual (`4c58a3b`) e reúne a grade/cache da 0.2.5 com o multiplayer/compatibilidade da 0.3.0 nos nove pares existentes. Core permanece 1.1.5; o servidor recebe Skins da mesma combinação Minecraft/loader e não exige Core. O contrato cumulativo de features passa a bloquear artefatos sem grade, cache ou sincronização. Não há mudança da matriz Java, inclusão de Chat Heads ou retorno de Companion.

O launcher preserva o login lembrado na primeira janela e isola as contas das adicionais. Dois jogos usam pastas de instância separadas. Relato do usuário: multiplayer e Chat Heads funcionaram na 0.3.0 em Minecraft 1.20.1 Forge e Fabric; o relato não homologa os novos hashes nem os nove pares. Execução gráfica completa da 0.3.1 e duas contas reais simultâneas: **PRECISA DE TESTE**. Evidências em [integração](../task-skins-session-integration.md).

## Aurora Skins 0.3.0 — Multiplayer e aparência — 22/09/2026

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. As nove combinações (Forge 1.12.2 e Fabric/Forge 1.16.5, 1.19.2, 1.20.1, 1.21.1) geraram JARs finais e passaram pelos contratos Rust de empacotamento; manifesto em `releases/skins/0.3.0/manifest.json`. Foram aprovados 27 testes do núcleo, incluindo sete do servidor e quatro do cliente de sincronização. Fabric 1.21.1 inicializou o retransmissor dedicado sem Core; Forge 1.21.1 aprovou descoberta de metadados sem Core, mas não executou entrypoints antes da EULA. Ambos encerraram pré-EULA sem mundo/jogadores. Dois clientes reais e Chat Heads continuam com estado **PRECISA DE TESTE**; não há homologação visual ou instalador novo nesta tarefa.

O servidor usa Aurora Skins 0.3.0 da mesma combinação Minecraft/loader, sem exigir Aurora Core/Companion; clientes participantes exigem Core 1.1.3+. Clientes sem Aurora Skins podem entrar, conforme as outras regras do servidor, e continuam vendo a aparência vanilla. O retransmissor limita a 256 participantes e uma publicação por jogador a cada dois segundos, com PNG canônico de até 24 KiB e estado apenas em memória, sem nuvem ou credenciais.

`PlayerInfo` amplia a compatibilidade moderna com chat/tab; consultas adicionais de `SkinManager` atendem 1.19.2/1.20.1/1.21.1, e 1.12.2 mantém o adaptador legado. Não há recurso novo de cabeças no chat nem Chat Heads incluído. Mods que consultam diretamente a Mojang, mantêm caches próprios ou usam cabeças geradas exclusivamente no servidor podem continuar exibindo a skin oficial. Proxy/ViaVersion e travessia entre versões não são suportados; combinação Fabric/Forge não está homologada.

Branch `codex/aur-skins-multiplayer-compat`, base `269d282`: preservar as alterações posteriores de Skins 0.2.5 na integração. Evidência e roteiro: [tarefa multiplayer](../task-skins-multiplayer.md); decisão: [ADR-022](ADR/ADR-022-skins-server-relay.md). O histórico abaixo permanece válido somente para as versões indicadas.

## Aurora Doctor local — 22/09/2026

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. O launcher passa a registrar e analisar tentativas de inicialização localmente, com regras determinísticas, associação limitada de logs e relatório sanitizado. Esse incremento não altera os JARs Core/Skins nem amplia Minecraft/loaders/Java suportados. Fixtures sintéticas e testes do diagnóstico não promovem os estados de homologação runtime abaixo. Comportamento e evidências específicas em [aurora-doctor.md](../aurora-doctor.md) e [task-aurora-doctor.md](../task-aurora-doctor.md).

## Launcher alpha.9 — integração de 22/09/2026

Integra Core 1.1.5 e Skins 0.2.5 das entregas anteriores, sem alterar os JARs ou ampliar a matriz Minecraft. A retirada do Companion mantém o Assistente desktop. A busca de jogadores ganha fallback oficial Mojang no Worker, preservando o contrato TSV V1 dos módulos existentes. A homologação da grade continua **PARCIAL**, conforme a tabela abaixo.

O desktop adiciona tradução de descrições, armazenamento local da seleção online, face baseada no PNG aplicado, retorno às instâncias após instalação e controles de janela/bandeja. **CONCLUÍDO** nos testes nativos: maximizar, restaurar, minimizar e fechar ocultando a janela com IPC/processo ativos. Menu físico da bandeja e comportamento no computador do destinatário: **PRECISA DE TESTE**. Evidências e limitações em [task-release-alpha9.md](../task-release-alpha9.md).

## Grade Explorar — Skins 0.2.5 / Core 1.1.5

A grade de 12 slots está integrada nos nove pares abaixo, com lógica comum Java 8, seleção por identidade e paginação pelo provider. Estado: **PARCIAL**: builds e contratos aprovados nos nove pares, verificação gráfica focada em três e runtime pendente nos outros seis. Resultados desta entrega, hashes e limites no [registro da grade](../task-skins-grid.md). A fonte do launcher seleciona os novos candidatos; o histórico abaixo mantém as versões realmente testadas.

| Minecraft | Loader | Java runtime | Skins | Evidência da grade |
| --- | --- | ---: | --- | --- |
| 1.12.2 | Forge 14.23.5.2860 | 8 | 0.2.5 | PARCIAL: execução gráfica focada |
| 1.16.5 | Fabric | 8 | 0.2.5 | Build/contratos; runtime PRECISA DE TESTE |
| 1.16.5 | Forge 36.2.42 | 8 | 0.2.5 | Build/contratos; runtime PRECISA DE TESTE |
| 1.19.2 | Fabric | 17 | 0.2.5 | Build/contratos; runtime PRECISA DE TESTE |
| 1.19.2 | Forge 43.4.16 | 17 | 0.2.5 | Build/contratos; runtime PRECISA DE TESTE |
| 1.20.1 | Fabric 0.14.23 | 17 | 0.2.5 | PARCIAL: execução gráfica focada e 320×240 |
| 1.20.1 | Forge 47.4.23 | 17 | 0.2.5 | Build/contratos; runtime PRECISA DE TESTE |
| 1.21.1 | Fabric 0.19.3 | 21 | 0.2.5 | PARCIAL: execução gráfica focada e 427×240/fullscreen |
| 1.21.1 | Forge 52.1.12 | 21 | 0.2.5 | Build/contratos; runtime PRECISA DE TESTE |

## Histórico: matriz após retirada do mod, anterior à grade — 21/09/2026

**O Assistente permanece no Aurora Launcher; o mod Companion e sua integração in-game foram retirados.** Os módulos ativos são Core e Skins: 2 módulos × 9 pares = **18 JARs**. A seleção/contagem vem de `scripts/active-mod-matrix.json`; o painel desktop e suas dependências não são JARs da matriz.

Core **1.1.5** é o candidato versionado desta alteração de UI. Skins **0.2.4** mantém sua fonte e foi reutilizado após rebuild e aprovação no comparador estrito. Os **18/18 JARs** passaram na comparação de fontes/conteúdo; os nove Core passaram em tamanho/hash/assinatura Ed25519 e as suítes Java Core (9) e Skins (16) passaram. Consulte o [registro da retirada](../task-remove-companion.md) para os resultados realmente executados; a existência de candidato ou build não prova runtime.

| Minecraft | Loader | Java runtime | Core | Skins | Runtime completo após retirada |
| --- | --- | ---: | --- | --- | --- |
| 1.12.2 | Forge | 8 | 1.1.5 candidato | 0.2.4 | PRECISA DE TESTE |
| 1.16.5 | Fabric | 8 | 1.1.5 candidato | 0.2.4 | PRECISA DE TESTE |
| 1.16.5 | Forge | 8 | 1.1.5 candidato | 0.2.4 | PRECISA DE TESTE |
| 1.19.2 | Fabric | 17 | 1.1.5 candidato | 0.2.4 | PRECISA DE TESTE |
| 1.19.2 | Forge | 17 | 1.1.5 candidato | 0.2.4 | PRECISA DE TESTE |
| 1.20.1 | Fabric | 17 | 1.1.5 candidato | 0.2.4 | PRECISA DE TESTE |
| 1.20.1 | Forge | 17 | 1.1.5 candidato | 0.2.4 | PRECISA DE TESTE |
| 1.21.1 | Fabric | 21 | 1.1.5 candidato | 0.2.4 | PRECISA DE TESTE |
| 1.21.1 | Forge | 21 | 1.1.5 candidato | 0.2.4 | PRECISA DE TESTE |

Os scripts preservam Gradle 8.4 em `tools/gradle/8.4` com JVM de build 17, e o wrapper 9.7.1/JVM 21 de Core 1.21.1 utilizado também por Skins. Rodar builds sequencialmente para evitar colisões Loom. Versões mínimas/exatas de loader e assinaturas seguem os manifestos de compatibilidade ativos.

### Gates vigentes

1. Recompilar Core e Skins nos nove pares fora da pasta Companion; validar metadata, bytecode, dependências, recursos, hashes e assinatura Core.
2. Comparar todas as entradas normalizadas dos 18 JARs contra as fontes; divergência bloqueia release. A normalização limitada existente não autoriza ignorar classes/recursos diferentes.
3. Em QA isolada, migrar Companion conhecido de `mods` para backup/journal fora da pasta carregada; repetir, simular falha, recusar link e preservar identidade incerta/terceiros/dados desktop.
4. Iniciar Core → Skins sem Companion, com uma única conexão IPC, handshake/nonce/protocolo 1, sessão pública e desconexão.
5. Verificar menu, preview 3D, catálogo, aplicação/persistência de skin e capa onde implementada, foco e encerramento sem impedir Minecraft.
6. Com Minecraft fechado, validar separadamente botão/atalho, conversa Gemini, microfone/transcrição, mute/TTS e erros/timeouts do Assistente desktop. Sem ponte de pedido antigo do mod.
7. Preparar a próxima release sem modificar artefatos históricos; gerar instalador somente mediante tarefa explícita e pelo fluxo oficial. Alteração do Worker não significa deploy.

## Evidências históricas anteriores à retirada

> As seções abaixo conservam resultados das versões/release indicadas. Companion, 27 JARs e procedimentos de instalação abaixo são históricos e foram substituídos pelos gates acima; não são requisitos atuais. Evidências gráficas antigas Core/Skins continuam úteis somente para os hashes/cenários realmente testados.

## Reparos de runtime — 19/09/2026

Fonte: Core **1.1.4**, Skins **0.2.4**, Companion **0.2.3**. Os 27 JARs
recompilados correspondem aos artefatos versionados; nove pares Core/Skins
passam pelos contratos de empacotamento, bytecode e dependências. Os novos
artefatos ainda dependem da integração no instalador; alpha.7 é imutável.

| Minecraft / loader testado | Java | Evidência desta entrega | Estado |
| --- | ---: | --- | --- |
| 1.12.2 / Forge 14.23.5.2860 | 8u504 | Menu, modelo completo, catálogo, aplicação e skin em mundo novo; Core → Skins → F11 → Escape; mundo repetido com Core 1.1.4 + Skins 0.2.4 finais | PARCIAL |
| 1.16.5 / Fabric 0.18.4 | 8u504 | Catálogo online, textura visível, aplicação e retorno ao menu sem ficar branco | PARCIAL |
| 1.19.2 / Fabric 0.14.19 | 17 | Inicialização sem erro de Mixin, menu, campo de busca e catálogo | PARCIAL |
| 1.21.1 / Fabric 0.19.3 | 21 | Menu, Core e Skins sem blur sobre conteúdo; catálogo e aplicação | PARCIAL |
| 1.16.5, 1.19.2, 1.20.1, 1.21.1 / Forge; 1.20.1 / Fabric | 8/17/21 | Build e inspeção dos JARs; sem nova execução gráfica | IMPLEMENTADO MAS NÃO HOMOLOGADO |

As quatro rodadas da tabela usaram Core 1.1.3; Core 1.1.4 mantém a UI e corrige
somente a versão dos metadados legados, além do próprio versionamento. O
candidato 1.1.3 foi preservado e não é selecionado pelo launcher. A rodada
legada final com Core 1.1.4 repetiu catálogo, aplicação e skin dentro do mundo.

Instâncias isoladas, sem contas, mundos ou servidores existentes; Companion
e handshake IPC não fizeram parte destes lançamentos. Os hashes das rodadas
anteriores e do mundo legado final estão em [task-skins-runtime-repair](../task-skins-runtime-repair.md).
Não extrapolar os testes para multiplayer, capas, todos os modpacks ou todas
as GUI scales. O histórico abaixo conserva os resultados das versões antigas.

## Histórico anterior

Atualizado em 2026-09-02. `BUILD` significa JAR compilado e inspecionado; `RUNTIME` exige o roteiro completo em uma instância real. Core 1.1.2, Skins 0.2.1 e Companion 0.2.3 formam a matriz embarcada no Launcher 0.1.1-alpha.6.

| Minecraft | Loader | Java | Core build | Skins build | Companion build | Runtime do conjunto | Estado |
| --- | --- | ---: | :---: | :---: | :---: | :---: | --- |
| 1.12.2 | Forge 14.23.5.2860 | 8 | sim | sim | sim | PRECISA DE TESTE | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.16.5 | Fabric 0.16.14 | 8 | sim | sim | sim | PRECISA DE TESTE | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.16.5 | Forge 36.2.42 | 8 | sim | sim | sim | PRECISA DE TESTE | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.19.2 | Fabric 0.16.14 | 17 | sim | sim | sim | PRECISA DE TESTE | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.19.2 | Forge 43.4.16 | 17 | sim | sim | sim | PRECISA DE TESTE | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.20.1 | Fabric 0.16.14 | 17 | sim | sim | sim | PRECISA DE TESTE | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.20.1 | Forge 47.4.23 | 17 | sim | sim | sim | Cave Horror: launch/menu/IPC/skin/catálogo PASS; mundo PRECISA DE TESTE | PARCIAL |
| 1.21.1 | Fabric 0.16.14 | 21 | sim | sim | sim | PRECISA DE TESTE | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.21.1 | Forge 52.1.12 | 21 | sim | sim | sim | PRECISA DE TESTE | IMPLEMENTADO MAS NÃO HOMOLOGADO |

Evidências runtime anteriores do Companion 1.12.2 Forge e 1.20.1 Fabric não homologam automaticamente os novos Core/Skins. O conjunto precisa ser reexecutado porque a aparência deixou de pertencer ao Companion e passou para o módulo Aurora Skins.

Em 1º de setembro de 2026, o Cave Horror Project 1.20.1 Forge 47.4.21 iniciou
com Java 17 usando exatamente os JARs embarcados Core 1.1.2, Skins 0.2.1 e
Companion 0.2.3. Não houve conflito JPMS de SLF4J/Minecraft nem rejeição de
resource pack; os três módulos registraram, o IPC conectou, a skin apareceu em
pé, o botão permaneceu sob o avatar no FancyMenu e abriu o catálogo online. Um
mundo/servidor não foi aberto nessa execução, portanto a linha continua parcial.

## Instalação pelo Launcher

Atualização de 16/09/2026: **Skins 0.2.2** tem nove JARs compilados (1.12.2 Forge e Fabric/Forge 1.16.5, 1.19.2, 1.20.1, 1.21.1), busca mc-api/MCStat, debounce e prévia do resultado. Os hashes estão em `releases/skins/0.2.2/manifest.json`; o código do launcher passou a embarcar essa matriz. **IMPLEMENTADO MAS NÃO HOMOLOGADO**: nenhum runtime/gameplay de 0.2.2 está comprovado. A tabela e a evidência Cave Horror acima são históricas do conjunto 0.2.1/alpha.5, não homologam 0.2.2. Não foi substituído o instalador alpha.5.

Para cada instância Fabric/Forge suportada, o Launcher instala por troca atômica:

1. Aurora Core 1.1.2, após validar tamanho, SHA-256 e assinatura Ed25519;
2. Aurora Skins 0.2.1;
3. Aurora Companion 0.2.3.

O release preflight recompila as 27 combinações e compara o conteúdo normalizado dos JARs de staging com os artefatos embarcados. A normalização ignora apenas o namespace aleatório de injeção gerado pelo Architectury e metadados ZIP; qualquer outra divergência bloqueia a criação do instalador. O mesmo gate exige `pack.mcmeta` em todos os módulos e abre cada Core para rejeitar `org/slf4j/**` sem relocação ou qualquer `net/minecraft/**`, evitando os conflitos e avisos observados no Cave Horror.

## Compatibilidade do launcher

Release alpha.7 (17/09/2026): instalador recompilado com Core 1.1.2,
Skins 0.2.3 e Companion 0.2.3; matriz de fontes 27/27, 50 testes desktop e
88 testes Rust aprovados. As correções abaixo agora estão no instalador,
mas os testes de runtime real continuam pendentes. Evidência:
`../task-release-alpha7.md`.

- Guarda-roupa, correção de 17/09/2026: recuperação da skin local por PNG,
  seleção equipada persistente e espera da recuperação antes de iniciar o jogo.
  O launch prioriza o modelo Slim/Clássico equipado local, mesmo quando o perfil
  remoto está desatualizado. **IMPLEMENTADO MAS NÃO HOMOLOGADO** no executável
  empacotado; fixture visual e testes do IPC simulado não homologam gameplay.
  Nenhum JAR ou instalador foi alterado. Ver `../task-wardrobe-recovery.md`.

- Inicialização Windows sem console, incluindo probes Java/Forge/jogo:
  **IMPLEMENTADO MAS NÃO HOMOLOGADO** na fonte posterior à alpha.6. Testes de
  subprocesso confirmam console ausente e stdout/stderr preservados; executar
  a próxima build empacotada e iniciar Minecraft ainda **PRECISA DE TESTE**.
  Sessão restaurada usa tela animada sem antecipar o login; detalhes em
  `../startup-session.md`.

- Minecraft 1.16.5 em sessão offline: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. O
  launcher isola somente o endpoint de privilégios do `authlib 2.1.28`, fazendo
  o cliente usar o fallback `OfflineSocialInteractions` e preservar Multiplayer
  LAN/offline. Há teste unitário do escopo por versão e prova direta com o
  `authlib` da instância; a confirmação visual do botão ainda **PRECISA DE
  TESTE** em uma nova build desktop.

## Roteiro runtime obrigatório

Skins **0.2.3** substitui 0.2.2 na próxima build do launcher: a prévia guarda somente o PNG e não altera metadados da biblioteca/skin ativa, inclusive quando a textura é idêntica e o modelo é diferente. 0.2.2 permanece imutável como histórico. Estado das nove combinações: **IMPLEMENTADO MAS NÃO HOMOLOGADO**, até repetição do roteiro abaixo.

1. Instância limpa e hashes de instalador/JAR.
2. Java/loader corretos e launch sem argumento sensível.
3. Handshake IPC e rejeição de nonce inválido.
4. Core → Skins → Companion; somente uma conexão WebSocket.
5. Options → Aurora Options; botão sem sobreposição em diferentes GUI scales.
6. Painel roxo, textos truncados sem colisão e abrir/fechar repetido por botão/ESC.
7. Avatar 3D na tela principal e na Central Aurora, classic/slim, duas camadas e fallback.
8. Mouse para cima/baixo e esquerda/direita com cabeça/corpo na direção correta e suavização.
9. Importar/aplicar PNG 64x64 classic, slim e 64x32 convertida; reiniciar e confirmar persistência.
10. Mundo não pausa; teclado/mouse voltam ao jogo sem travar.
11. Em 1.16.5, Multiplayer não deve ser bloqueado pela conta Microsoft fictícia; servidor `online-mode` continua exigindo autenticação oficial.
12. Falha de módulo, skin HTTPS, IPC e encerramento sem impedir o Minecraft de iniciar.

## Estratégia

- Referência moderna: Fabric/Forge 1.20.1.
- 1.21.1 usa Java 21 e adaptadores próprios de `PlayerSkin`.
- 1.16.5 preserva bytecode Java 8 e identidade de launch `legacy`/offline.
- 1.12.2 permanece em trilho Forge legado Java 8/LWJGL2.
- NeoForge: fora do escopo por decisão atual.

## Java e engine por instância — 28/10/2026

**IMPLEMENTADO MAS NÃO HOMOLOGADO**: RAM manual/inicial opcional, RAM automática pela fórmula existente e argumentos JVM de desempenho com allowlist. GC opcional G1 no Java >=17, padrão abaixo de 17; nenhuma promessa de desempenho. O launch verifica Java também em vanilla e registra a memória efetiva no Doctor. Matriz Core 1.1.5/Skins 0.3.1 e os nove alvos por módulo preservados, sem novo JAR ou instalador.

Inventário de RAM física: Windows. Outros sistemas continuam no modo manual; automático exige um inventário futuro. O painel independente usa tokens oficiais e aguarda o patch do dono de `App.tsx`. Testes unitários, smoke DOM e build não homologam Minecraft/loader/Java, Assistente ou temas no aplicativo nativo. Contrato, testes e roteiro restante em [Java/engine](../java-engine.md) e [tarefa](../task-java-engine.md).
