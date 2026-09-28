# Gemini e ações confirmadas no Aurora

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. Worker atualizado em produção e instalador alpha.13 gerado, com gates nativos aprovados. Assistente, Doctor e Tools responderam usando Gemini real em preview remoto com entradas sintéticas; a operação completa numa instância real do jogador permanece sem homologação.

## Base e escopo

Trabalho em `codex/aur-ai-tools`, worktree exclusiva, sobre `ab1e1b5afda45d55ec3cb6aa12b1f11bf69d8a7c` do repositório ativo AuroraLauncher. Base: launcher `0.1.1-alpha.12`, Core `1.1.5` e Skins `0.3.1`, conferidos no checkout e na matriz ativa. O launcher foi atualizado para alpha.13. As versões Core/Skins e a matriz Minecraft/loaders/Java permanecem inalteradas; o build oficial reconstruiu e verificou os 18 JARs.

O incremento centraliza o acesso Gemini, distingue erros operacionais e oferece propostas estreitas de remoção reversível de mod e ajuste de opções permitidas. Assistente e Doctor compartilham revisão, confirmação, histórico e restauração locais. A análise determinística do Doctor continua utilizável sem Gemini. Preserva Assistente desktop, TTS, microfone, sessões independentes e integrações Core/Skins; não reintroduz Companion.

## Diagnóstico real do serviço

A inspeção inicial foi somente leitura, usando a autenticação Wrangler existente e imprimindo apenas metadados permitidos. O Worker de produção `aurora-api` respondeu a `/health` com HTTP 200, revisão `2026-09-17-integrated-catalog-skins-updates-v1`. A implantação mais recente era a versão 47, publicada em 22/09/2026 às 14:51:49 UTC, sem uma versão posterior. O projeto Firebase público era `auroralauncher`; o nome do secret `GEMINI_API_KEY` estava presente. Valores de secrets e tokens não foram exibidos.

A rota Gemini do Doctor entrou no código em 23/09, commit `0ed345f`, depois dessa implantação. Isso sustenta a hipótese de serviço antigo sem a rota nova; HTTP 401 anônimo não comprova a existência de uma rota, porque a autenticação antecede o despacho. A presença do nome de um secret também não comprova sua validade.

Em preview remoto temporário, usando o secret existente, uma consulta real do Assistente retornou HTTP 200 e `modelVersion=gemini-3.8-flash`. A consulta de metadados desse modelo retornou HTTP 200. Doctor também retornou HTTP 200 com JSON completo validado para a entrada sintética `memory_failure`, evidência correspondente e passos permitidos. Tools retornou HTTP 200, `model=gemini-3.8-flash` e nenhuma proposta para um contexto sem evidência de falha. As tentativas também encontraram HTTP 503 `UNAVAILABLE` intermitente, inclusive numa solicitação sintética explícita de ajuste de configuração. A estrutura e o acesso real foram demonstrados; isso não comprova disponibilidade contínua nem a qualidade de uma sugestão de alteração. O preview foi encerrado e seu helper e credencial temporários foram removidos após os testes.

O Worker foi publicado com `npm --prefix apps/edge-proxy run deploy`, versão `f4847c3e-9f6e-465c-86e3-670b819820f8`. A saúde de produção respondeu HTTP 200 com revisão `2026-09-28-gemini-tools-v1`; as três rotas de IA retornaram HTTP 401 `session` para consultas anônimas. A presença do secret Gemini foi conferida novamente sem ler seu valor. Não foi extraído token do jogador para testar uma consulta autenticada em produção: as respostas reais acima pertencem ao preview, e os testes autenticados de rota usam chaves e sessões sintéticas.

## Implementação e limites

- `gemini.ts` centraliza `gemini-flash-latest`, fallback limitado para `gemini-3.8-flash`, leitura limitada e erros públicos fixos. O fallback ocorre quando o alias não existe ou retorna 503; quota e recusa de autorização não disparam repetição. Os fluxos autenticados compartilham prazo total de 35 segundos no servidor e 40 segundos no cliente, incluindo obtenção de token e leitura.
- `/v1/ai/tools` aceita pergunta explícita, metadados reduzidos dos alvos locais, categorias do Doctor e histórico canônico. A resposta contém resumo e até três propostas. IDs são capacidades temporárias do contexto; o modelo não escolhe caminhos nem fornece comandos executáveis.
- O broker reconstrói a alteração a partir do contexto nativo, exige revisão e confirmação de um plano com digest, validade e janela proprietária. Revalida conteúdo, disponibilidade da instância e estado do jogo sob o bloqueio de mutação antes de aplicar.
- Remoção preserva o JAR original; configuração preserva os bytes anteriores. Journals autenticados e hashes sustentam a restauração confirmada. Conteúdo protegido, links, caminhos fora do escopo e alterações posteriores são rejeitados. Recuperação interrompida e integridade não dependem da confiança na resposta Gemini.
- O histórico local limita a 64 entradas e 128 KiB, removendo as mais antigas também pelo custo UTF-8. Guarda categorias, orientações aceitas e ações; nunca guarda logs brutos. A fronteira para o modelo reconstrói títulos e resumos a partir de categorias e alvos canônicos, sem reaproveitar a prosa anterior da IA.
- A comparação exige outra execução iniciada depois da ação. Falha posterior com a mesma categoria conhecida gera `unresolved`; encerramento normal gera apenas `normal_exit`; cancelamento, causa desconhecida e evidência insuficiente permanecem `inconclusive`.

Detalhes de formatos, capacidades e limites: [Aurora AI Tools](aurora-ai-tools.md). Decisão: [ADR-025](engineering/ADR/ADR-025-confirmed-local-ai-tools.md).

## Evidência executada

| Verificação | Resultado |
| --- | --- |
| Worker, suíte completa e dry-run | **CONCLUÍDO**: 116 testes e `npm run check` aprovados no estado integrado final. |
| Build TypeScript/Vite do desktop | **CONCLUÍDO** para alpha.13. |
| Smoke DOM de Tools | **CONCLUÍDO**: 18 grupos, incluindo confirmação, cancelamento, contexto obsoleto e recibo pendente sem falso sucesso. |
| Smoke DOM do Doctor e preservação do Assistente | **CONCLUÍDO**: 21 grupos Doctor e 10 testes Assistente. |
| Transporte IA do desktop | **CONCLUÍDO**: 17 testes, incluindo cinco novos casos de fronteira/erro. |
| Histórico Rust independente | **CONCLUÍDO**: 13 testes no gate integrado, incluindo junction real no Windows, exclusão de logs/prosa no contexto do modelo, concorrência, recuperação de snapshot, orçamento UTF-8 e identidade clone/rename. |
| Rust completo, clippy, fmt e broker de mutação final | **CONCLUÍDO**: 190 testes aprovados, cinco opt-in ignorados; `cargo fmt --all -- --check` e `cargo clippy --all-targets -- -D warnings` aprovados. |
| Assistente Gemini real em preview | **CONCLUÍDO** para uma resposta HTTP 200 que identificou `gemini-3.8-flash`; disponibilidade contínua não foi demonstrada. |
| Doctor/Tools com resposta estruturada Gemini real | **CONCLUÍDO** para entradas sintéticas controladas: HTTP 200 e validação dos contratos nos dois fluxos. Também houve 503 intermitente do provedor. |
| Nova versão publicada no Worker de produção | **CONCLUÍDO**: versão e revisão acima, saúde 200, proteção de sessão 401 e secret preservado. |
| Instalador alpha.13 | **CONCLUÍDO**: build oficial, preflight inicial/final e matriz 18/18 aprovados; instalação e execução nativas pelo jogador ainda **NÃO VERIFICADO**. |
| UI nativa autenticada com uma ação real e restauração | **PRECISA DE TESTE**; testes DOM e fixtures Rust não são essa homologação. |

A revisão independente encontrou correções incorporadas e cobertas pelos gates: vazamento de basename de JAR sem metadados; recuperação de configuração quando a interrupção deixa o destino ausente; vínculo do journal à identidade da instância preservando renomeação e recusando cópia; histórico genérico ou journal corrompido ocultando outros recibos de recuperação; e UI tratando recibo pendente como sucesso. Testes com arquivos sintéticos exercitam escrita/backup/restauração reais no filesystem, sem modificar instâncias do jogador.

## Entrega alpha.13

Fonte do instalador: commit `db74fb5ee2353ee4f729100d1adab4614d8ad270`, checkout limpo, `npm run release:package` executando `npm run desktop:build`. Artefato em `releases/Aurora Smart Launcher_0.1.1-alpha.13_x64-setup.exe`, 8.048.800 bytes, SHA-256 `918167c1d5e45fb8a8bc7d686098abd1f917d9976b7ef912e4b1d4afc8f406b4`. Manifesto e arquivo SHA-256 acompanham o instalador; releases anteriores foram preservadas. O manifesto declara runtime e assinatura digital **NÃO VERIFICADO**.

Não foram lidos tokens pessoais do navegador. As verificações descritas não instalam JAR numa instância do jogador nem iniciam jogo para provocar crash. O instalador foi gerado, não executado automaticamente.
