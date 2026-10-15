# Canal IPC do Aurora Core

O Assistente permanece no Aurora Launcher; o mod Companion e sua integração in-game foram retirados. O IPC continua sendo a conexão local do Aurora Core e de seus módulos, independente da API/Gemini e da voz do painel desktop.

## Endpoint e handshake preservados

O Launcher abre o WebSocket exclusivamente em `127.0.0.1`, em porta efêmera, e passa à JVM somente `-Daurora.ipc.port=<porta>` e `-Daurora.session.nonce=<nonce-efêmero>`, além das propriedades públicas necessárias ao launch. O nonce muda por execução, não é persistido e não é senha/JWT/token Firebase. O jogo não recebe credenciais Firebase ou chaves Gemini, CurseForge e Cloudflare.

O Core inicia com `hello`, incluindo nonce, loader, versão Minecraft, versão Core e `protocol: 1`. O Launcher encerra nonce/protocolo inválidos e só responde `accepted` após autenticação. A mensagem `session` transmite apenas id Aurora, UUID Minecraft, username, estado e scopes. A sessão offline existente permanece compatível; não houve mudança de versão do protocolo público para retirar mensagens de um módulo.

## Contratos ativos

| Direção | Mensagem/evento | Consumidor |
| --- | --- | --- |
| Core → Launcher | `hello` | autenticação do canal |
| Launcher → Core | `accepted`, `session` | runtime Core, sessão pública e módulos |
| Core/módulos → Launcher | `kind` genérico com nome/payload limitado | eventos genéricos; não dispara consultas ao Assistente |
| Launcher → Core/módulos | envelope genérico autorizado | API IPC pública do Core |
| Local no launcher | conexão e desconexão | acompanhamento da instância/processo |

As filas do servidor permanecem limitadas a 64 elementos. Nomes de eventos são limitados a 64 caracteres; payloads são validados e campos com aparência de credencial recusados. O Core limita mensagens a 1 MiB; o servidor nativo também limita mensagens e frames recebidos a 1 MiB. O protocolo não foi substituído por uma nova ponte de Assistente.

O incremento Connect73 corrige uma espera anterior sem limite no upgrade HTTP/WebSocket: um cliente local que abria TCP sem completar o pedido podia impedir o encerramento do servidor. O upgrade agora usa socket não bloqueante, retoma `MidHandshake` e verifica shutdown e prazo monotônico de cinco segundos. Bytes parciais não renovam esse prazo. A dependência fixada Tungstenite 0.24 impõe ainda seu teto de 64 KiB/512 leituras bem-sucedidas no upgrade. Falhas dessa etapa não imprimem o pedido ou erro bruto. Depois do upgrade, leituras e escritas têm timeout de 75 ms; a autenticação `hello` mantém seu prazo de cinco segundos, nonce e protocolo 1. Um wrapper de IO verifica cancelamento e prazo absoluto antes de cada leitura/escrita/flush: continuações WebSocket vazias sem FIN não podem manter o leitor interno ativo indefinidamente. Só a autenticação válida remove o prazo de `hello`; o cancelamento permanece. O fechamento aguarda o worker próprio; esses limites não prometem um prazo de tempo real para o sistema operacional.

Foram acrescentados testes locais com sockets TCP reais para cancelamento durante upgrade incompleto, progresso parcial sem renovação, recusa de cabeçalho excessivo e continuações vazias sem FIN durante cancelamento e prazo de `hello`, além da regressão do protocolo autenticado. Os testes de fragmentação limitam os bytes enviados e mantêm o peer aberto até observar o resultado. A execução e os resultados ficam no [registro Connect](task-aur-r4-connect.md). Esses testes não iniciam Java nem homologam Core/Skins dentro do Minecraft. O caller de lançamento mantém a destruição do servidor IPC fora do mutex de confirmação Connect, inclusive quando o processo não pode ser criado.

Mensagens antigas `overlay`, `assistantRequest`, `assistantListen` e `telemetry` do Companion não ativam Gemini, microfone, captura ou painel in-game. Os retornos `toggleAssistant`, `assistantResponse`, `assistantCaption` e `assistantTranscript` e seus comandos Tauri exclusivos foram retirados. A coleta de FPS/MSPT/memória/dimensão produzida pelo mod também foi retirada; diagnósticos reais do launcher/Core permanecem, sem promessa de métricas cujo produtor foi removido.

## Assistente desktop preservado

O botão **Assistente** e `Ctrl+Alt+/` abrem/fecham diretamente o painel desktop, mesmo com Minecraft fechado. Não há encaminhamento prioritário ao jogo. O fluxo é `AssistantPanel → askAuroraAssistant → Worker /v1/assistant → askGemini → resposta no painel`; a fala usa `playAuroraSpeech → synthesize_speech → edge-tts-rust`. Reconhecimento de fala, microfone, mute, histórico/estado e tratamento de erro continuam pertencendo ao desktop.

A captura/voz/legenda exclusiva do Companion foi retirada. Esta mudança não cria captura desktop ou nova HUD para substituir o mod. Serviços compartilhados, plugin/permissões do atalho e TTS desktop permanecem. Para a matriz, migração e validação, veja [retirada do mod](task-remove-companion.md) e [ADR-018](engineering/ADR/ADR-018-remove-companion-keep-desktop-assistant.md).
