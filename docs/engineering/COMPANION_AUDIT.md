# Dossiê técnico do Aurora Companion

Atualizado em 31 de agosto de 2026. Este documento separa intenção histórica,
artefato publicado, fonte atual e evidência runtime. Código/teste/log observado
tem precedência sobre documentação antiga.

## Resultado central

O Aurora Companion é o módulo cliente que liga recursos do Assistente e de
aparência ao Minecraft. Desde a versão 0.2.0 ele depende do Aurora Core 1.0.0 e
não deve abrir um WebSocket próprio. O Core mantém o único canal autenticado com
o launcher; o Companion registra uma página de Assistente, produz eventos e
recebe respostas pela API do Core.

O estado correto é **IMPLEMENTADO MAS NÃO HOMOLOGADO**:

- existem nove JARs 0.2.0, todos legíveis como ZIP/JAR;
- nenhuma combinação 0.2.0 concluiu o roteiro runtime completo;
- Forge 1.16.5 e 1.19.2 possuem metadata incompatível no artefato publicado;
- os quatro runtimes Fabric observados expuseram uma corrida de inicialização
  que impediu o Companion de se anexar ao Core;
- o Assistente continua sendo uma janela Swing externa, não uma tela/HUD nativa;
- a fonte posterior aos artefatos corrige os bloqueios acima, recuperação de
  pedido e lifecycle, mas ainda não foi publicada em JAR versionado.

Consequência prática: o instalador `0.1.1-alpha.3` ainda embute os JARs 0.2.0
imutáveis. As correções deste branch só chegam ao produto depois de uma nova
versão, novos hashes e nova homologação. Não sobrescrever 0.2.0.

## O que cada componente é

| Nome | Responsabilidade | Não confundir com |
| --- | --- | --- |
| Aurora Smart Launcher | instala, inicia, acompanha o jogo e processa Gemini/voz/TTS | mod dentro do Minecraft |
| Aurora Core | fundação in-game, sessão pública, módulos, configurações, menu e único WebSocket | engine Rust do launcher |
| Aurora Companion | módulo do Core para Assistente, atalho, telemetria e aparência | Core ou backend Gemini |
| Assistente Aurora | experiência distribuída entre UI, Companion, launcher e Worker | um modelo executando dentro do JAR |

## Tudo que o Companion foi

### Especificação V4.0

A proposta original descrevia um mod autônomo Fabric/Forge que:

- possuía cliente WebSocket em `127.0.0.1:45882`;
- capturava `AltGr + /` dentro do loop do jogo;
- acionava uma janela Tauri transparente como overlay;
- injetava skin/capa obtidas do Firebase;
- enviava FPS, MSPT, RAM e dimensão;
- serviria no futuro a presença, telemetria social e sincronização ao vivo.

Parte disso foi substituída por decisões posteriores: porta fixa virou porta
efêmera; Firebase Storage principal virou Supabase/arquivo local; a janela
Tauri problemática virou Swing temporária; o WebSocket saiu do Companion e foi
centralizado no Core; social e sync nunca foram implementados.

### Companion 0.1.0

O primeiro código versionado, no commit `67ec897` de 29 de agosto de 2026,
implementou o Companion como cliente WebSocket autônomo. Cada JAR sombreava
`Java-WebSocket`, autenticava com o nonce, enviava telemetria e pedidos do
Assistente e mostrava a janela Swing. Há onze artefatos 0.1.0 preservados: dois
genéricos de referência e nove por versão/loader.

Documentos anteriores registram homologação de Forge 1.12.2 e Fabric 1.20.1.
Essas provas são históricas. Os anexos disponíveis também preservam tentativas
anteriores com `module-info.class`, conflito SLF4J/classloader e JAR duplicado;
elas explicam correções do legado, mas não substituem a evidência final citada.

### Companion 0.2.0 com Aurora Core

O commit `97bf0a7` de 31 de agosto de 2026 removeu o cliente WebSocket sombreado,
reduziu muito os JARs, tornou Core 1.0.0 obrigatório e registrou o Companion
como módulo `aurora_companion`. O launcher passou a instalar Core antes do
Companion e os testes Rust passaram a rejeitar JAR que ainda empacote um segundo
cliente WebSocket.

A migração arquitetural é correta, mas a evidência local revelou que a ordem de
entrypoints Fabric não garantiu que `Aurora.isAvailable()` já fosse verdadeira.
O Companion desistia após a primeira tentativa e nunca registrava seu módulo.

### Fonte posterior ao 0.2.0

Esta auditoria aplicou na fonte, sem alterar releases históricas:

- nova tentativa periódica de anexação ao Core, cobrindo a ordem Fabric;
- encerramento do IPC e dos agendadores no shutdown do módulo;
- threads daemon também no legado 1.12.2;
- retorno de falha ao enviar texto/voz, timeout de 60/30 segundos e recuperação
  dos controles da janela;
- JPEG limitado a 700.000 bytes antes de Base64, compatível com o frame de 1 MiB
  do Core;
- `mods.toml` Forge usando a faixa de loader configurada por versão, em vez de
  `[47,)` fixo.

## O que a fonte atual faz

| Capacidade | Comportamento verificável | Estado |
| --- | --- | --- |
| Instalação | launcher instala Core e Companion em `mods` somente para perfis Fabric/Forge reconhecidos | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| Inicialização | entrypoints Fabric/Forge chamam `AuroraCompanion.initialize`; fonte nova aguarda Core quando necessário | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| Módulo Core | registra “Assistente” 0.2.0 e uma página que abre a interface | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| IPC | usa `Aurora.services().ipc()`; não abre socket próprio | CONCLUÍDO no desenho; runtime PRECISA DE TESTE |
| Atalho moderno | lê Right Alt + `/` via GLFW no tick; launcher mantém atalho global `Ctrl+Alt+/` como fallback | PARCIAL |
| Atalho 1.12.2 | consulta LWJGL2 a cada 75 ms | PARCIAL |
| Interface | `JWindow` 520×390, always-on-top, conversa, campo, Enviar, Falar, Analisar tela, ESC e fechar | PROTÓTIPO |
| Texto | envia `assistantRequest` com UUID e pergunta; exibe resposta/erro | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| Voz | pede transcrição ao launcher; o JAR não grava nem reconhece áudio sozinho | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| Legendas | exibe `assistantCaption` associado ao pedido em andamento | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| TTS | é executado no launcher; o Companion só mostra legendas/estado | PARCIAL |
| Screenshot | pede confirmação por ocorrência, captura a área útil da tela, reduz e envia JPEG no pedido | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| Telemetria | no sampler atual, FPS e memória usada são significativos; MSPT é sempre `0` e dimensão é vazia | PROTÓTIPO |
| Skin moderna | 1.16.5/1.19.2/1.20.1 baixam HTTPS ou leem arquivo local, registram textura dinâmica e alteram somente o jogador local por Mixin | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| Skin 64×32 | caminho de textura dinâmica converte o layout legado para 64×64 | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| Skin/capa 1.12.2 e 1.21.1 | injeta propriedade `textures` no `GameProfile` por reflexão | RISCO |
| Capa | URL HTTPS; textura dinâmica nas linhas comuns ou propriedade de perfil nas extremas | PRECISA DE TESTE |
| Encerramento | fonte nova fecha inscrição IPC e agendadores; JAR 0.2.0 não contém a correção | IMPLEMENTADO MAS NÃO HOMOLOGADO |

## O que ele não faz

- Não contém Gemini, credencial de IA, Firebase, Supabase ou Edge TTS.
- Não recebe JWT, senha, cookie, chave de serviço ou token Firebase.
- Não é uma HUD nativa e não desenha widgets no render do Minecraft.
- Não garante mundo sem pausa, foco correto ou cursor restaurado; isso pertence
  ao futuro adaptador `Screen` + HUD.
- Não implementa social, amigos, presença, P2P, live sync ou analytics remoto.
- Não calcula MSPT nem dimensão de forma útil no sampler atual.
- Não faz diagnóstico, Tool Calling, alteração de mods/configs ou execução de
  comandos.
- Não possui suporte NeoForge.
- Não está homologado para distribuição pública ou para 9/9 combinações.

## Dados e fronteiras de segurança

Do jogo para o launcher podem sair somente eventos tipados: pedido do
Assistente, pedido de microfone, telemetria e eventos de módulo limitados. Do
launcher para o jogo entram sessão pública sem tokens, toggle, resposta,
legenda e transcrição. O Core limita kind, nomes, payload, fila e tamanho e
recusa chaves com aparência de credencial.

O screenshot não faz parte da telemetria: exige botão e confirmação a cada
captura. O risco residual é que `Robot` captura a área útil da tela, não apenas
a janela do Minecraft; a confirmação precisa continuar dizendo “tela atual”.

O nonce é efêmero e o listener é loopback, mas a propriedade JVM pode ser vista
por outro processo do mesmo usuário. Ele reduz conexões acidentais; não é um
segredo equivalente a senha e não deve autorizar capacidades remotas.

## Defeitos confirmados e melhorias

| ID | Defeito/evidência | Impacto | Correção | Estado |
| --- | --- | --- | --- | --- |
| COMP-001 | logs Fabric 1.16.5/1.19.2/1.20.1/1.21.1: Companion inicia antes do Core | Assistente/módulo/telemetria sem IPC | retry de anexação no sampler | fonte corrigida; release PRECISA DE TESTE |
| COMP-002 | JARs Forge 1.16.5 e 1.19.2 declaram `[47,)` | loader 36/43 pode recusar o mod | faixa vem de `forge_loader_range` | fonte corrigida; JAR histórico permanece |
| COMP-003 | retorno de `AuroraIpc.send` era ignorado | janela podia ficar bloqueada sem pedido enviado | retorno booleano + recuperação imediata | fonte corrigida |
| COMP-004 | pergunta/voz não tinham timeout | controles podiam ficar desabilitados indefinidamente | 60 s texto, 30 s voz | fonte corrigida |
| COMP-005 | JPEG não tinha teto compatível com Core de 1 MiB | screenshot complexo podia ser recusado silenciosamente | recompressão/escala até 700 kB | fonte corrigida |
| COMP-006 | executor legado usava threads não daemon e não havia shutdown global | encerramento/recarga poderia manter trabalho vivo | daemon + `shutdownNow` pelo módulo | fonte corrigida |
| COMP-007 | log 1.21.1 não localizou `GameProfile` | aparência não aplicada | adaptador real por versão ou reflexão revisada | NÃO IMPLEMENTADO |
| COMP-008 | build raiz gera só 1.20.1 | “build do Companion” pode ser confundido com matriz | criar orquestrador imutável por alvo | NÃO IMPLEMENTADO |
| COMP-009 | telemetria anuncia MSPT/dimensão, mas envia 0/vazio | documentação/produto superestimam dados | adaptadores reais ou remover campos da promessa | NÃO IMPLEMENTADO |
| COMP-010 | não há testes Java do estado/UI/limites | regressão só aparece em build/runtime | extrair view-model e testes sem Minecraft | NÃO IMPLEMENTADO |

## Validação executada nesta auditoria

- o build de referência 1.20.1 (`common`, Fabric e Forge) concluiu com sucesso;
- os builds parametrizados 1.16.5 e 1.19.2 concluíram com sucesso e os JARs
  Forge gerados passaram a declarar, respectivamente, `[36,)` e `[43,)`;
- o build legado Forge 1.12.2 e o build dedicado 1.21.1 Fabric/Forge
  concluíram com sucesso;
- o teste Rust direcionado ao Companion passou e verificou os nove JARs
  históricos instalados pelo launcher, incluindo metadata, dependência do Core
  e ausência de um segundo cliente WebSocket;
- os links Markdown relativos e a integridade de whitespace do diff foram
  verificados.

Esses resultados provam compilação, empacotamento e invariantes estáticos. Não
há testes Java automatizados e a fonte corrigida ainda não passou pelo roteiro
runtime dentro do Minecraft. Portanto, nenhum item foi promovido a homologado
e nenhum JAR em `releases/` foi substituído.

## Evidência runtime 0.2.0 observada

| Alvo | Evidência local | Classificação |
| --- | --- | --- |
| 1.12.2 Forge | Core e Companion carregaram; módulo registrado; aparência/atalho observados; mundo iniciou e encerrou | PARCIAL; handshake e Assistente completos não provados |
| 1.16.5 Fabric, pack A | Fabric Loader 0.13.3 abaixo do mínimo 0.14; outro mod também exigiu Java 16 | incompatível com a instância observada |
| 1.16.5 Fabric, pack B | Core conectou; Companion desistiu antes do Core; skin/atalho locais apareceram | RISCO COMP-001 |
| 1.19.2 Fabric | Core conectou; Companion desistiu antes do Core; skin/atalho apareceram | RISCO COMP-001 |
| 1.20.1 Fabric | Core conectou; Companion desistiu antes do Core; skin/atalho apareceram | RISCO COMP-001 |
| 1.21.1 Fabric | mesma corrida; perfil de aparência não localizado | RISCO COMP-001/007 |
| 1.16.5 Forge | metadata `[47,)` no JAR | RISCO COMP-002 |
| 1.19.2 Forge | metadata `[47,)` no JAR | RISCO COMP-002 |
| 1.20.1 Forge / 1.21.1 Forge | nenhum roteiro recente localizado | NÃO VERIFICADO |

A matriz operacional detalhada e o roteiro de doze passos permanecem em
[`COMPATIBILITY_MATRIX.md`](COMPATIBILITY_MATRIX.md).

## Inventário dos artefatos preservados

### Arquitetura por versão do Companion

| Versão | Quantidade | Característica |
| --- | ---: | --- |
| 0.1.0 | 11 | cliente WebSocket e classes sombreadas dentro de cada JAR; 9 alvos + 2 genéricos |
| 0.2.0 | 9 | sem cliente WebSocket embarcado; módulo dependente do Aurora Core 1.0.0 |

### JARs 0.2.0 usados pelo launcher

| Alvo | Bytes | SHA-256 |
| --- | ---: | --- |
| 1.12.2 Forge | 25.712 | `FCFA89AEF332BCE031CB36063A1F50786197AEC3B990C6BC7E4ECE344037073E` |
| 1.16.5 Fabric | 29.675 | `461EAAEA2FF580766409BA6BE74C923F64E9E4DC3DD4D0AF887A17B98E5F0800` |
| 1.16.5 Forge | 29.714 | `E68F9F2BB76BF13716858F19CEEFA3188461FC4686098422EEC08B9260F14FB2` |
| 1.19.2 Fabric | 29.693 | `B8B0D9D187DA7DDA7DDE8B1B1D7EF21AB0764D07455FF06DFE5BC9545A978102` |
| 1.19.2 Forge | 29.694 | `4B27C29F7D27770D5E3EF36A13C208D46BC1DC2296D5C802BE9821289D3C2C66` |
| 1.20.1 Fabric | 29.693 | `B2F15823B4CE1FA1D8A327285470DC421755C0BD5604731A86B97C71E6FFC505` |
| 1.20.1 Forge | 29.692 | `723F7CC17DB7CED220A4B4AEBA44F6DBD1E1532AB226263E18FFF09CC8C93F05` |
| 1.21.1 Fabric | 28.597 | `9C8EAF8653E79E9C1F7F42E95DAF1492050F5876586A85376C6E4B95FFCE6547` |
| 1.21.1 Forge | 28.621 | `3C0661B0A672766CD20AE4B9493FC3307FF2CF5C59C946CAB5343F316978DFA1` |

### JARs 0.1.0 históricos

| Artefato | Bytes | SHA-256 |
| --- | ---: | --- |
| genérico Fabric | 211.616 | `0B902777942832D75869D1F045512BB5502AA29B9972683919DCDF7FDB6A6670` |
| genérico Forge | 211.512 | `CB7B2978393DB7DDC3A5D14AECDA62F92EDE81DF55649D852C8FF92F144897A6` |
| 1.12.2 Forge | 227.950 | `01F23B336AD1F6648768759284BA10F3B3A0C8AF98BF0DD6CFB4B53AB5A3C339` |
| 1.16.5 Fabric | 231.633 | `DF1EBE11E2CA731D7FAE44453BC81726458A9C8300E60AEB99FDCA13903BA643` |
| 1.16.5 Forge | 231.663 | `C372B20669822E8C495B6F2B915CF2FAC6A21D3D4B2313AC805D4A9272C8FFB8` |
| 1.19.2 Fabric | 231.770 | `C09C62F15B65B75F3369B04C8A336BF60C799FAEFB4DE624B2C0D9FC2E2B0959` |
| 1.19.2 Forge | 231.769 | `561C47E36685293EBCB57BADA8C34E127C790173BB80D523D7404BEA7DF86349` |
| 1.20.1 Fabric | 232.379 | `BF4E721FD9AFB603E85CD953C62903F0636109DC9DFEDC082C447649FB3346A5` |
| 1.20.1 Forge | 232.377 | `6C9A3F3051BE2FBAB86DE5CCED65F5168ECAC42B1283DBFDF7106A822563C45D` |
| 1.21.1 Fabric | 223.417 | `133B21E012F44785FDE6A3D1EF25FE76121FF78C905EE24D163A421506B88FEC` |
| 1.21.1 Forge | 223.429 | `E4C00901FBFE44F55148380C0D4FBFF547E22F7BDAED840A7B5C5BF2A013B27C` |

As classes dos JARs 0.2.0 usam bytecode Java 8 (major 52) nas linhas 1.12.2 e
1.16.5, Java 17 (major 61, com partes compartilhadas major 52) em 1.19.2 e
1.20.1, e Java 17/major 61 em 1.21.1, cujo runtime exigido pelo metadata é Java
21. Isso é compatibilidade de bytecode, não homologação do jogo.

## Catálogo de documentos do repositório

### Fontes centrais

- [`apps/companion-mod/README.md`](../../apps/companion-mod/README.md): build,
  arquitetura, aparência e status do módulo.
- [`docs/companion-ipc.md`](../companion-ipc.md): protocolo, mensagens, limites,
  atalho e screenshot.
- [`COMPATIBILITY_MATRIX.md`](COMPATIBILITY_MATRIX.md): estado por alvo e roteiro runtime.
- [`ADR-004`](ADR/ADR-004-companion-architecture.md): core compartilhado e adaptadores.
- [`ADR-005`](ADR/ADR-005-native-hud.md): substituição de Swing por Screen + HUD.
- [`ADR-011`](ADR/ADR-011-legacy-1122.md): 1.12.2 como tier legado.
- [`ADR-015`](ADR/ADR-015-aurora-core.md): Core obrigatório, modular e IPC único.
- [`ARCHITECTURE_REVIEW.md`](ARCHITECTURE_REVIEW.md): fronteiras atuais e alvo incremental.
- [`TEST_STRATEGY.md`](TEST_STRATEGY.md): gate runtime e testes adversariais.
- [`RISK_REGISTER.md`](RISK_REGISTER.md): riscos que bloqueiam promessa/release.
- [`EXECUTIVE_SUMMARY.md`](EXECUTIVE_SUMMARY.md) e
  [`MASTER_ROADMAP.md`](MASTER_ROADMAP.md): estado e sequência de evolução.
- [`docs/phase-0-status.md`](../phase-0-status.md) e
  [`docs/ROADMAP.md`](../ROADMAP.md): status público e roadmap curto.
- [`docs/aurora-core.md`](../aurora-core.md) e
  [`apps/aurora-core/README.md`](../../apps/aurora-core/README.md): serviço do qual o Companion depende.
- [`README.md`](../../README.md), [`SECURITY.md`](../../SECURITY.md) e
  [`releases/README.md`](../../releases/README.md): comunicação pública, segurança e imutabilidade.
- [`Aurora_Status_e_Reorganizacao_2026-08-29.docx`](../Aurora_Status_e_Reorganizacao_2026-08-29.docx):
  relatório histórico com 327 parágrafos, 20 tabelas e 248 linhas de tabela.
  O conteúdo foi extraído; o layout não pôde ser renderizado porque LibreOffice
  não está instalado neste ambiente.

### Menções e contratos auxiliares lidos

- [`AGENTS.md`](../../AGENTS.md), [`CONTRIBUTING.md`](../../CONTRIBUTING.md),
  [`docs/module-a.md`](../module-a.md), [`docs/supabase-storage.md`](../supabase-storage.md)
  e [`docs/task-profile-instances-java.md`](../task-profile-instances-java.md).
- [`DEPENDENCY_GRAPH.md`](DEPENDENCY_GRAPH.md),
  [`RELEASE_STRATEGY.md`](RELEASE_STRATEGY.md),
  [`RESEARCH_REPORT.md`](RESEARCH_REPORT.md) e [`ADR/README.md`](ADR/README.md).
- [`CODEX_INTEGRATION_PLAN.md`](CODEX_INTEGRATION_PLAN.md) e
  [`CODEX_PARALLEL_PLAN.md`](CODEX_PARALLEL_PLAN.md).
- [`CODEX_TASKS/README.md`](CODEX_TASKS/README.md),
  [`AUR-R0-002`](CODEX_TASKS/AUR-R0-002-license.md),
  [`AUR-R0-003`](CODEX_TASKS/AUR-R0-003-ci.md),
  [`AUR-R0-004`](CODEX_TASKS/AUR-R0-004-release-provenance.md),
  [`AUR-R1-001`](CODEX_TASKS/AUR-R1-001-network-limits.md) e
  [`AUR-R1-002`](CODEX_TASKS/AUR-R1-002-runtime-harness.md).
- [`CODEX_PROMPTS/AUR-R0-003-ci.md`](CODEX_PROMPTS/AUR-R0-003-ci.md) e
  [`CODEX_PROMPTS/AUR-R0-004-release-provenance.md`](CODEX_PROMPTS/AUR-R0-004-release-provenance.md).
- [`package.json`](../../package.json) e os metadados
  [`Fabric comum`](../../apps/companion-mod/fabric/src/main/resources/fabric.mod.json),
  [`Forge comum`](../../apps/companion-mod/forge/src/main/resources/META-INF/mods.toml),
  [`Forge 1.12.2`](../../apps/companion-mod/legacy/forge-1.12.2/src/main/resources/mcmod.info),
  [`Fabric 1.21.1`](../../apps/companion-mod/modern/1.21.1/src/fabric/resources/fabric.mod.json)
  e [`Forge 1.21.1`](../../apps/companion-mod/modern/1.21.1/src/forge/resources/META-INF/mods.toml),
  que definem comandos, versão, dependências e lado cliente.

Documentos auxiliares podem ser históricos ou instruções de tarefa; este dossiê
não promove suas promessas acima do código/log atual.

## Fontes históricas externas ao Git

Os anexos locais continham seis conteúdos distintos relevantes, deduplicados por
SHA-256:

| Conteúdo | Cópias | SHA-256 | Uso |
| --- | ---: | --- | --- |
| Especificação Técnica V4.0 | 3 | `1879DB8B83E5E42F5FD0E2CC696727EFF6271946F735A8447155C3A3D0169E0A` | intenção original do Companion autônomo |
| Especificação “Desenvolvimento do Aurora Core” | 2 | `CD28E0F35A44C8739C5C1700B8F4219A1D73439FE09280856F595BAD6851F12A` | decisão de módulos independentes |
| versão textual do relatório de 29/08 | 1 | `AE7224B93B3813EDE506FB7C78A0313D50E750EC8B00F410A3DA8CCA58273815` | auditoria histórica |
| prompt de arquitetura/coordenação | 1 | `40C8108FC10EF224E6751300CB116F2D2D179551AC77EA7FA51201910ABFB164` | critérios de pesquisa e HUD |
| log 1.12.2 com JAR duplicado | 1 | `4DD5AAACB2FE64FAE40A5062DD8BE12336E34ED6EE543824D823E8E74F619460` | falha histórica |
| log 1.12.2 com module-info/SLF4J | 1 | `120F37D3CA16B8112FD79F507A920640A4DA54A28684DE192D9F568A3013650E` | falha histórica e causa da isolação |

Um HTML capturado do CurseForge apareceu na busca por palavra, mas é falso
positivo de página de terceiros e não é documento do Companion. O script
`build_aurora_status_doc.py` no repositório Prism de referência apenas gerou o
relatório Word; não é implementação do produto.

## Próxima ordem de trabalho

1. Criar versão inédita do Companion e um construtor de matriz que gere todos
   os alvos sem mutar 0.2.0.
2. Validar metadata antes de qualquer instalação: Minecraft, loader, Java,
   dependência Core, tamanho, SHA-256 e ausência de segundo WebSocket.
3. Executar primeiro 1.20.1 Fabric e Forge com a fonte corrigida: Core antes do
   módulo, handshake, página, texto, voz, legenda, screenshot, aparência e shutdown.
4. Corrigir o adaptador de aparência 1.21.1 com API real da versão.
5. Extrair view-model do Swing, adicionar testes Java e então implementar a
   `Screen` + HUD nativa prevista no ADR-005.
6. Portar e homologar 1.19.2, 1.16.5, 1.21.1 e por último o tier 1.12.2.
7. Só marcar suporte quando cada célula tiver hash, Java, loader, log redigido,
   evidência visual e os doze passos da matriz aprovados.
