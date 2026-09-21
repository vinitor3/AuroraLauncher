# Compatibility Matrix

## Matriz ativa após retirada do mod — 21/09/2026

**O Assistente permanece no Aurora Launcher; o mod Companion e sua integração in-game foram retirados.** Os módulos ativos são Core e Skins: 2 módulos × 9 pares = **18 JARs**. A seleção/contagem vem de `scripts/active-mod-matrix.json`; o painel desktop e suas dependências não são JARs da matriz.

Core **1.1.5** é o candidato versionado desta alteração de UI. Skins **0.2.4** mantém sua fonte e só pode ser reutilizado se passar no comparador estrito. Consulte o [registro da retirada](../task-remove-companion.md) para os resultados realmente executados; a existência de candidato ou build não prova runtime.

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
