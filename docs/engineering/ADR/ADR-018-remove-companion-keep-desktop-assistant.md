# ADR-018 — Retirar o mod Companion e preservar o Assistente desktop

- Data: 21/09/2026.
- Estado: Aceita por instrução explícita do responsável.
- Substitui: ADR-004 e ADR-005 quanto à implementação/expansão do mod Companion. Atualiza somente essa fronteira das ADR-011 e ADR-015.

## Contexto

O Assistente do launcher e o mod Companion compartilhavam nomes, TTS, transportes e IPC, mas tinham consumidores diferentes. Remover indiscriminadamente arquivos ou dependências de “assistant”, “companion” ou “voz” destruiria recursos desktop ainda necessários. Core e Skins também utilizavam o wrapper Gradle armazenado dentro da pasta do mod.

## Decisão

**O Assistente permanece no Aurora Launcher; o mod Companion e sua integração in-game foram retirados.** A mudança não porta versões novas nem cria um novo Assistente/HUD no Core.

1. Retirar o módulo Java, UI Swing, variantes Fabric/Forge/legado, registro `aurora_companion`, página `aurora_assistant`, instalação/seleção do JAR e ponte de perguntas, voz, capturas, legendas e telemetria exclusiva do mod.
2. Preservar painel, botão, conversa/estado, controles, microfone, reconhecimento, mute, reprodução e TTS desktop. `Ctrl+Alt+/` alterna diretamente o painel. Preservar `askAuroraAssistant`, `/v1/assistant`, `askGemini`, autorização, limites, configuração e modelos/fallbacks usados pelo launcher.
3. Preservar `synthesize_speech`, `engine/tts.rs`, `edge-tts-rust`, plugins/permissões do atalho e dependências com consumidores mantidos. Não excluir `functions/`, Firebase, CurseForge, Supabase ou APIs de catálogo/aparência.
4. Preservar Core/Skins, menu/preview, registro genérico de módulos, configurações e IPC público: loopback, porta efêmera, nonce, handshake, protocolo 1, sessão sem credenciais, limites, eventos e desconexão. Mensagens antigas do Companion não geram consulta Gemini nem reativam a ponte.
5. Transferir o wrapper Gradle 8.4, inclusive scripts/JAR/properties, para `tools/gradle/8.4`. Preservar o wrapper 9.7.1 da geração 1.21.1 e JDKs compatíveis. A matriz ativa contém 9 Core + 9 Skins = 18 JARs. Assistente desktop fica no aplicativo e não integra essa contagem.
6. Core recebe candidato versionado 1.1.5 pelas mudanças de interface. Skins 0.2.4 só pode ser reutilizado se contratos e comparador estrito de conteúdo permanecerem válidos. Releases históricas e assinaturas não são reescritas para ocultar o mod.

## Migração de instâncias

Antes do launch, sob o bloqueio de mutações da instância e com o jogo encerrado, `engine/companion_migration.rs` identifica os antigos JARs por metadata exata e hashes do catálogo histórico `known-companion-artifacts.json`. Nome semelhante sozinho não prova propriedade. Metadata Companion com origem/hash incertos bloqueia o launch com aviso e mantém o arquivo para decisão do usuário; mods de terceiros não são removidos por substring.

O JAR confirmado sai de `mods` por movimentação para `.aurora/removed-companion/<transação>/`, acompanhado por journal e hash. Caminhos são confinados à instância e links/traversal recusados. Retentativas concluem a transação recuperável de forma idempotente. Repor o JAR no diretório carregado reativa a detecção na próxima tentativa; backup não é seleção ativa do produto. Conversas, preferências de voz, mundos, bibliotecas, contas, configuração/aparência e Core/Skins permanecem intactos. Dados antigos exclusivos do mod podem permanecer inertes.

## Validação e consequências

A retirada intencional do mod é mudança de produto. Perder o Assistente desktop ou qualquer consumidor compartilhado é regressão. Builds e contratos verificam fontes/artefatos; não homologam Minecraft ou serviços online. O [registro da tarefa](../../task-remove-companion.md) reúne o mapa de consumidores, testes realmente executados e pendências; a [matriz](../COMPATIBILITY_MATRIX.md) separa estado atual e evidência histórica.

Alterações do Worker exigem implantação posterior autorizada; esta tarefa não faz deploy, revoga chaves, lê valores de secrets ou gera instalador. A expansão multiversão permanece posterior à retirada. A política de colaboração, qualidade, segurança e imutabilidade de releases continua válida.
