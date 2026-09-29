# Gemini: chaves alternativas e Flash-Lite como último recurso

Data: 29/09/2026. Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**.

Base: `a3325b5` (launcher alpha.13), branch `codex/aur-gemini-failover`, na worktree reutilizada `codex-aur-ai-tools`. Escopo restrito ao Worker e documentação. O cliente alpha.13 existente permanece compatível; não há novo instalador. Core 1.1.5, Skins 0.3.1 e os 18 JARs da matriz ativa permanecem inalterados.

## Problema e decisão

Consultas reais apresentaram indisponibilidade intermitente do Flash e recusa de uma credencial. Adicionar chaves não garante capacidade nem corrige uma autorização recusada. O usuário autorizou explicitamente Flash-Lite como último recurso, mantendo Flash como primeira opção.

Assistente, Doctor, Tools e tradução passam pelo mesmo transporte. A seleção usa até três secrets opcionais (`GEMINI_API_KEY`, `GEMINI_API_KEY_2`, `GEMINI_API_KEY_3`), com remoção de espaços nas extremidades e deduplicação de valores. `GEMINI_KEY_ORDER` aceita somente uma sequência de slots 1–3, sem repetição; configuração inválida falha antes do provedor. A configuração publicada em produção é `1,2`. O slot 3 permanece excluído após 403 e só deve ser reativado depois de corrigida e validada sua autorização.

`gemini-flash-latest` é a primeira opção; ausência do alias permite `gemini-3.8-flash`. `GEMINI_FALLBACK_MODEL=gemini-flash-lite-latest` habilita a alternativa explicitamente aprovada. Ausência do alias Lite permite `gemini-3.5-flash-lite`. O modelo efetivamente retornado continua validado. Decisão: [ADR-026](engineering/ADR/ADR-026-bounded-gemini-failover.md).

## Limites e falhas

- Uma consulta admite no máximo quatro chamadas ao provedor. Com Lite habilitado, a fase Flash tem até três chamadas e 20 segundos, reservando orçamento para Lite dentro do prazo global de 35 segundos. O prazo do cliente de IA continua em 40 segundos, incluindo obtenção de token.
- Chaves alternativas atendem apenas falhas transitórias: rede, HTTP 408/5xx ou prazo da tentativa. A espera entre tentativas é limitada, cancelável e inclui variação curta. Com Lite desabilitado, cada tentativa tem teto de 34 segundos; o prazo global sempre prevalece.
- Quota, cobrança recusada e autorização recusada encerram o fluxo. HTTP 401/403/402/429 conhecidos cancelam imediatamente o corpo. Não há troca de chave ou modelo para contornar esses erros. HTTP 400 genérico também encerra; apenas ausência identificada de modelo permite a alternativa de modelo.
- O corpo de erro é limitado a 8 KiB e um segundo. Logs operacionais incluem somente slot, modelo, tentativa, status, código público e duração. Chaves, cabeçalhos, perguntas, respostas e corpos de erro não são registrados nem apresentados ao jogador.
- Lite só atende indisponibilidade transitória, modelo ausente ou expiração da fase Flash, enquanto houver prazo e orçamento. Resposta inválida não autoriza troca para Lite. A validação dos contratos e a confirmação humana de ações locais permanecem obrigatórias.
- Tradução mantém até duas validações semânticas, compartilhando o limite de quatro chamadas. JSON inválido, resposta incompleta ou prosa CJK copiada podem receber a segunda validação; quota/autorização/configuração recusadas não. IDs dos trechos, ordem, completude, expansão textual e limite próprio de resposta continuam validados. O prazo inclui a leitura do corpo de entrada.

## Evidência obtida

As medições abaixo são pontuais e usam entradas sintéticas. Não demonstram estabilidade contínua ou qualidade de diagnóstico.

| Momento / consulta isolada | Resultado observado |
| --- | --- |
| Slots originais, slot 1 | 503 em aproximadamente 13 s |
| Slots originais, slot 2 | 200, Gemini 3.8 Flash, em 33.518 ms (aproximadamente 33,5 s) |
| Slots originais, slot 3 | 403 |
| Após o usuário informar a troca dos slots 1 e 3; preview renovado, slot 1 | 200, Gemini 3.8 Flash, em 6.408 ms |
| Após a troca informada, slot 2 | 503 |
| Após a troca informada, slot 3 | 403; permanece fora da ordem preparada |
| Preview isolado Lite, Assistente | 200, Gemini 3.5 Flash-Lite, em 1.636 ms |
| Preview isolado Lite, tradução | 200 em 746 ms |
| Preview final, ordem `1,2`, Assistente | 200 via Flash-Lite: Flash/slot 1 retornou 503 em 984 ms; Flash/slot 2, 503 em 1.192 ms; Lite/slot 1, 200 em 1.263 ms |
| Preview final, ordem `1,2`, Doctor | 200 com JSON validado para `memory_failure`: Flash/slot 1 retornou 503 em 1.306 ms; Flash/slot 2, 503 em 1.404 ms; Lite/slot 1, 200 em 997 ms |
| Preview final, ordem `1,2`, Tools | 200 via Gemini 3.8: Flash/slot 1 retornou 503 em 7.585 ms; Flash/slot 2, 200 em 3.606 ms; proposta sintética `set_setting`, alvo `setting-1`, valor 8 |

O preview remoto isolado usado para sondar o provedor contornou a autenticação somente nesse ambiente de teste, com entrada sintética. Isso não altera a autenticação Firebase das rotas de produção e não comprova uma sessão real do jogador. Nenhum segredo ou conteúdo privado faz parte deste registro.

Os tempos por tentativa não incluem toda a espera entre chamadas. A proposta retornada por Tools foi validada; nenhuma ação nativa foi executada.

## Validação e publicação

| Verificação | Estado / evidência |
| --- | --- |
| Testes da tradução após migração central | **CONCLUÍDO**: 21 testes passaram, incluindo chave secundária, orçamento combinado, cancelamento, leitura de corpo e prazo total |
| Suíte Worker anterior ao wrapper final Flash/Lite | **CONCLUÍDO**: 145 testes passaram naquela revisão; não equivale à validação da revisão final |
| QA do failover Flash/Lite | **CONCLUÍDO**: 32 testes passaram |
| Suíte final Worker | **CONCLUÍDO**: 155 testes passaram |
| `npm --prefix apps/edge-proxy run check` | **CONCLUÍDO**: dry-run passou |
| Preview do fluxo final com ordem `1,2` e fallback Lite | **CONCLUÍDO** no escopo sintético: Assistente, Doctor e Tools responderam conforme as tentativas acima; não comprova produção autenticada |
| Deploy desta alteração em produção | **CONCLUÍDO**: fonte `cab7bb1`, versão Worker `37d71a4c-1253-4950-bb2f-477209194f02`; ordem `1,2`, Lite habilitado |
| Saúde e proteção das rotas após deploy | **CONCLUÍDO**: `/health` retornou 200 e revisão `2026-09-29-gemini-key-failover-v1`; as quatro rotas POST recusaram chamadas sem sessão com 401 e código `session`; os três nomes de secrets continuam presentes |
| Chamada autenticada em produção | **NÃO VERIFICADO**: a tentativa de criar sessão anônima temporária no Firebase retornou `ADMIN_ONLY_OPERATION` (HTTP 400). Nenhuma conta de teste foi criada; configurações de autenticação não foram alteradas |
| Estabilidade continuada, conta real no desktop e resposta de cada superfície | **NÃO VERIFICADO** |

Falha de Gemini continua preservando o Doctor local e o texto original da tradução. A mudança não executa propostas, não modifica instâncias e não altera o broker de confirmação. O histórico da entrega alpha.13 permanece em [task-ai-tools.md](task-ai-tools.md).

## Referências oficiais consultadas em 29/09/2026

Os limites da Gemini API são por projeto; ter várias chaves não multiplica a quota do mesmo projeto. [Rate limits](https://ai.google.dev/gemini-api/docs/rate-limits).

A distinção entre problemas de autorização, quota e indisponibilidade orienta as falhas terminais e transitórias. [Troubleshooting](https://ai.google.dev/gemini-api/docs/troubleshooting). Modelos e aliases disponíveis devem ser conferidos novamente ao alterar a configuração. [Models](https://ai.google.dev/gemini-api/docs/models).
