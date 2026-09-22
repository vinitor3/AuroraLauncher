# Aurora Skins 0.3.0 — Multiplayer e compatibilidade de aparência

Data: 22/09/2026. Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**.

## Escopo e integração

Branch `codex/aur-skins-multiplayer-compat`, worktree exclusiva, base `269d282`. A entrega implementa propagação de skins pelo servidor e melhora as consultas de aparência usadas por outros mods. Não adiciona cabeças ao chat, não inclui Chat Heads e não gera instalador.

Skins 0.3.0 é uma nova versão; releases anteriores são imutáveis. Esta branch foi iniciada antes das alterações posteriores de Skins 0.2.5. A integração deve preservar explicitamente essas alterações, incluindo a grade de resultados no Minecraft, e revisar os conflitos de UI, versões e artefatos antes de distribuir o conjunto. O estado desta tarefa não declara que 0.2.5 já esteja integrado.

Decisão arquitetural: [ADR-018 — Retransmissão pelo servidor Minecraft](engineering/ADR/ADR-018-skins-server-relay.md).

## Comportamento implementado

- O cliente anuncia suporte, espera confirmação e publica o PNG/modelo selecionado. Mudanças rápidas são agrupadas de forma que a última seleção seja publicada.
- O servidor usa o UUID da conexão; o pacote de publicação não permite escolher o UUID de outro jogador. Retransmite as aparências e envia o estado atual aos novos participantes.
- Até 256 participantes, uma publicação por jogador a cada dois segundos, PNG canônico 64x64 de até 24 KiB. Validação de estrutura/CRC/tamanho/dimensões precede a decodificação; a recodificação remove metadados.
- O estado compartilhado fica em memória. Saída, troca de servidor, desconexão e encerramento descartam o estado correspondente. Não há novo serviço de nuvem, persistência de skins de terceiros no servidor ou envio de credenciais.
- Clientes usam filas limitadas e verificação de sessão para descartar respostas/envios atrasados; texturas antigas são liberadas sem interferir nas substitutas.
- A aparência moderna é consultada por UUID no personagem e em `PlayerInfo`, cobrindo os caminhos de chat/tab. `SkinManager` recebe consultas complementares em 1.19.2/1.20.1 e consulta dinâmica em 1.21.1. O trilho 1.12.2 usa seu adaptador legado. Capas/elytra e fallbacks vanilla são preservados.
- Servidores sem Aurora Skins mantêm o funcionamento local. Clientes sem o mod podem entrar conforme as demais regras do servidor, mas não veem as skins Aurora dos outros jogadores.

## Requisitos e limites

Instalar Aurora Skins 0.3.0 no servidor e nos clientes que participarão, usando o JAR da mesma combinação Minecraft/loader. O cliente exige Aurora Core 1.1.3 ou superior; servidor dedicado não exige Core nem Companion. A matriz abrange Forge 1.12.2 e Fabric/Forge 1.16.5, 1.19.2, 1.20.1 e 1.21.1. Java: 8 para 1.12.2/1.16.5, 17 para 1.19.2/1.20.1 e 21 para 1.21.1.

Proxy/ViaVersion e comunicação entre versões de Minecraft não são suportados. Comunicação cruzada Fabric/Forge não está homologada. Nenhuma mudança altera a skin oficial da conta, regras de autenticação do servidor ou propriedades oficiais dos perfis.

Chat Heads é uma dependência externa opcional para testar a integração, não um recurso ou dependência incluída pelo Aurora. Mods que buscam imagens diretamente na Mojang, conservam caches próprios ou usam cabeças geradas apenas pelo servidor podem continuar mostrando a aparência oficial. Compatibilidade universal com mods de skin não é declarada.

## Evidências disponíveis

| Verificação | Resultado nesta tarefa | Limite da evidência |
| --- | --- | --- |
| `SkinSyncServerTest` | 7 testes aprovados | Retransmissor sem Minecraft: identidade da conexão, entrada tardia, modelo, limite de envio, reset/saída, pacote/PNG inválido e capacidade |
| `SkinSyncClientTest` | 4 testes aprovados | Dois clientes simulados, seleção final, descarte de callbacks/envios de sessão antiga, PNG inválido/deduplicação e servidor sem protocolo |
| Compilação das nove combinações | Aprovada | Não demonstra boot, aplicação de Mixins, comportamento visual ou comunicação em jogo |
| Matriz final dos JARs 0.3.0 e hashes | 9/9 compilados; 2 testes Rust de contratos aprovados | Bytecode, metadados, dependências declaradas, classes e recursos; [manifesto SHA-256](../releases/skins/0.3.0/manifest.json) |
| Núcleo completo | 27/27 testes aprovados | Inclui os 11 casos de sincronização e regressões de biblioteca/catálogo/UI lógica |
| Rust: formatação e Clippy | Aprovados, incluindo `--all-targets -- -D warnings` | Fonte do launcher aponta para os novos artefatos; não equivale a gerar instalador |
| Fabric 1.21.1 dedicado sem Core | Inicialização do adaptador/retransmissor aprovada; exit 0 | Loader 0.16.14, Java Temurin 21.0.12.1+1-LTS; parou antes da EULA, sem mundo/jogadores |
| Forge 1.21.1 dedicado sem Core | Descoberta do mod/mixins aprovada; exit 0 | Forge 52.1.12, mesmo Java; EULA é verificada antes dos entrypoints, portanto execução do adaptador NÃO VERIFICADO |
| Dois clientes reais, chat/tab e Chat Heads | PRECISA DE TESTE | Nenhuma homologação visual desta feature foi realizada |
| Instalador do Launcher | NÃO IMPLEMENTADO nesta tarefa | Nenhum instalador gerado ou substituído |

O teste focado `:core:test --tests com.aurora.skins.sync.SkinSyncClientTest` aprovou 4/4 casos, sem falhas; o grupo completo de sincronização inclui ainda os 7 casos do servidor. Houve também compilação direta dos managers/mixins contra os binários mapeados das quatro versões modernas. A revisão independente identificou uma corrida na confirmação HELLO: a alteração de `ready` passou a ser sincronizada e a conferir novamente a geração da sessão.

Evidência local reproduzível: `.aurora/skins-030-final-matrix.log` e `.aurora/qa/{fabric,forge}-1211-server-no-core/{evidence.json,qa-process.log,classloading.log}`. Os smokes finais usaram exatamente os hashes do manifesto: Fabric `fa98c8972b9ce4def0ef2dea0ce0e0d92ab1f878d3df159024b756728111549f`; Forge `295a8933a96aae8c24e95af1aabb3f576ebcf91801d7959c949170eba2d1346f`. `eula=false` foi preservado; nenhum servidor ficou em execução. Isso não constitui homologação multiplayer.

Os resultados históricos de Skins 0.2.4 e do Launcher não substituem essa validação. Atualizar esta seção apenas com evidência produzida pela execução correspondente.

## Roteiro manual de homologação

Repetir para cada combinação suportada em instâncias isoladas. Registrar Minecraft, loader, Java, versão/hash dos JARs, resultado e evidência de cada etapa; não registrar contas, credenciais ou dados pessoais.

1. Iniciar servidor dedicado com apenas Aurora Skins e suas dependências de loader; confirmar boot sem Core, Companion ou acesso a classes de interface/renderização.
2. Abrir dois clientes com Core compatível e Skins 0.3.0; equipar skins diferentes, uma slim e outra wide. Entrar no mesmo servidor e confirmar que cada jogador enxerga o outro corretamente.
3. Conferir cabeça na lista de jogadores e no chat com uma versão compatível de Chat Heads instalada separadamente. Confirmar também a cabeça própria quando o mod a exibir, sem atribuir ao Aurora uma interface de chat nova.
4. Alterar a skin durante a sessão, alternar rapidamente entre três escolhas e confirmar que a última chega ao outro cliente. Repetir usando o mesmo PNG com modelo diferente e usando uma skin legada 64x32.
5. Entrar com o segundo cliente depois da publicação inicial e confirmar recebimento do estado atual. Sair, reconectar e trocar de servidor; nenhuma textura da sessão anterior deve permanecer ou reaparecer depois.
6. Conferir capas/elytra oficiais e os fallbacks com jogador sem skin Aurora. Entrar com cliente sem Aurora Skins e confirmar acesso permitido pelas regras normais e aparência vanilla.
7. Entrar com cliente Aurora em servidor sem o mod: a skin local permanece funcional, não há compartilhamento e não ocorre desconexão causada pelo protocolo opcional.
8. Encerrar clientes e servidor, repetir o boot e confirmar estado remoto vazio, ausência de falhas de textura e encerramento normal. Executar também abertura/fechamento do catálogo, seleção e persistência da biblioteca para detectar regressões locais.

Até cumprir e registrar o roteiro, manter **IMPLEMENTADO MAS NÃO HOMOLOGADO**.
