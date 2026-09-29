# Aurora Connect — reconciliação e autoridade documental

Data: **29/09/2026**. Esta reconciliação reúne as duas entregas documentais sem substituir código alpha.14 por arquivos da alpha.13. O [registro de execução](../task-aur-r4-connect.md) concentra comandos, testes e estado do incremento; uma revisão documental não homologa transporte, Minecraft ou voz.

## Base cumulativa conferida

| Pacote | HEAD conferido, limpo na leitura | Base e alcance |
| --- | --- | --- |
| `codex/aur-r4-connect-v2-docs` | `d6f31db6bd1bd0e9b9e2f8bbc2ebf1a6e653af5d` | Alpha.13 `dc2ec5ac36a88f54b1130bac4789f8ecd56641d2`; especificação extensa, sete IDs e prompt consolidado posterior |
| `codex/aur-r4-connect-v2-review` | `5ac2f7e288e77fbb0d5c0d7adbe057f6f76c52a6` | Alpha.14 `6a107b87adc4882bd464909e22650185f862ea5e`; oito IDs, recovery/CAS/Doctor/cópias preservados |
| Implementação isolada `codex/aur-r4-connect` | Base `5ac2f7e288e77fbb0d5c0d7adbe057f6f76c52a6` | Worktree própria; nenhuma edição nas duas fontes ou no checkout ativo |

O `git merge-base` das duas entregas é `dc2ec5a`. A linha cumulativa posterior contém `be3a1c4` (recuperação manual), `89211c7` (Doctor e cópias), `6a107b8` (registro alpha.14) e `5ac2f7e` (revisão documental). A escolha não decorre apenas do maior número de versão. Core **1.1.5**, Skins **0.3.1**, Assistente desktop e os nove pares existentes permanecem; nenhuma matriz foi ampliada. A base histórica `TESTE-main/aurora-skins-runtime` não foi utilizada.

## Uma autoridade por responsabilidade

| Responsabilidade | Documento canônico |
| --- | --- |
| Fases, prioridades e gates completos | [MASTER_ROADMAP.md](MASTER_ROADMAP.md) |
| Contrato de produto e contrato experimental delimitado | [AURORA_CONNECT_SPEC.md](AURORA_CONNECT_SPEC.md) |
| Decisão de transporte e alternativas | [ADR/PROPOSAL-aurora-connect.md](ADR/PROPOSAL-aurora-connect.md) |
| IDs, critérios de aceite, testes e matriz candidata | [CODEX_TASKS/AUR-R4-CONNECT-BACKLOG.md](CODEX_TASKS/AUR-R4-CONNECT-BACKLOG.md) |
| Operadores, custo, termos e decisões de habilitação | [AURORA_CONNECT_DEPENDENCIES.md](AURORA_CONNECT_DEPENDENCIES.md) |
| Correspondência histórica e requisito → prova | [AURORA_CONNECT_TRACEABILITY.md](AURORA_CONNECT_TRACEABILITY.md) |
| Instrução operacional consolidada | [CODEX_PROMPTS/AUR-R4-CONNECT-V2.md](CODEX_PROMPTS/AUR-R4-CONNECT-V2.md) |
| Comandos/resultados/rollback desta execução | [task-aur-r4-connect.md](../task-aur-r4-connect.md) |

Não se cria outro prompt com variação somente de caixa: Windows colide esses nomes. O backlog e a proposta de ADR da primeira entrega não são copiados como autoridades paralelas. Suas versões completas permanecem nas worktrees preservadas e em objetos Git, consultáveis sem checkout, por exemplo:

```powershell
git show d6f31db6bd1bd0e9b9e2f8bbc2ebf1a6e653af5d:docs/engineering/AURORA_CONNECT_BACKLOG.md
git show d6f31db6bd1bd0e9b9e2f8bbc2ebf1a6e653af5d:docs/engineering/ADR/PROPOSED-aurora-connect-transport.md
git show d6f31db6bd1bd0e9b9e2f8bbc2ebf1a6e653af5d:docs/task-aurora-connect-v2-docs.md
git show 5ac2f7e288e77fbb0d5c0d7adbe057f6f76c52a6:docs/engineering/CODEX_PROMPTS/AUR-R4-CONNECT-V2.md
```

O [task documental alpha.14](../task-aur-r4-connect-v2-review.md) permanece como evidência histórica, inclusive seus estados e resultados originais. A restrição daquela tarefa a documentação não restringe a implementação local autorizada nesta solicitação. O prompt consolidado exige continuar os incrementos independentes; pendência de uma segunda rede ou teste humano de voz não interrompe trabalho local seguro.

## Divergências resolvidas antes de atribuir evidências

| Assunto | Pacote alpha.13 | Pacote alpha.14 | Contrato canônico desta execução |
| --- | --- | --- | --- |
| IDs de voz/robustez | `006` reúne SVC/PV; `007` robustez | `006` SVC, `007` PV, `008` robustez | IDs alpha.14; correspondência explícita abaixo e na rastreabilidade |
| Capability requerida | `requiredCapabilities` lista | `requiredCapability` singular | `requiredCapability` singular: cada serviço possui exatamente um protocolo e requer `reliableStream` ou `datagram`; negociação do peer permanece um conjunto |
| Geração da sessão | `sessionId` + `sessionEpoch` | sessão + `revision`/generation | `sessionId` aleatório novo por host/run, não reutilizado; identidade da sessão vinculada à conexão autenticada. Reinício invalida leases/associações anteriores; não inferir epoch de relógio |
| Aprovação do destino | `configurationRevision` privado | `revision` | `serviceRevision` separado da sessão; alteração de destino requer nova aprovação e invalida recursos anteriores. Hash de arquivo, quando houver configuração, é controle privado de concorrência |
| Framing UDP | Forma ainda aberta; reserva de até 32 B | Proposta explícita de 24 B | Um formato de 24 B, descrito na spec e conferido contra codec; peer vem do transporte, nunca de identidade declarada no datagrama |
| Limites | Números candidatos divergentes | Outros números candidatos | Limites de laboratório documentados conforme código/testes, sem tratá-los como SLA ou aprovação de voz |
| Versão Iroh | 1.1.0 consultada em tag | 1.3.0 observada em latest, URL versionada falhou | Pin exato escolhido após inspeção de artefato/API/toolchain; nem antiguidade nem latest são prova de adequação |
| Base recovery | Anterior às entregas seguintes | Preserva CAS/Doctor/cópias | Manter alpha.14 e seus contratos restritos; sem snapshots gerais, GC ou backup de mundos inferidos |

| ID do pacote `docs` (d6f31db) | ID canônico | Correspondência de evidência |
| --- | --- | --- |
| AUR-R4-001 a AUR-R4-005 | Mesmos IDs | Inventário, TCP, UDP, redes e integração respectivamente |
| AUR-R4-006 / EV-R4-006-SVC | AUR-R4-006 | Somente Simple Voice Chat |
| AUR-R4-006 / EV-R4-006-PV | AUR-R4-007 | Somente Plasmo Voice |
| AUR-R4-007 / EV-R4-007 | AUR-R4-008 | Robustez, manifesto e liberação |

Referências antigas a `006` sem sufixo significam o conjunto dos dois mods, não aprovação de ambos. Um `EV-R4-007` só é interpretado junto ao pacote/SHA de origem; não é reatribuído silenciosamente ao Plasmo. As duas entregas eram documentais, portanto seus placeholders EV não são resultados runtime.

## Leitura e limites

Foram lidos integralmente os dois prompts, AGENTS das duas fontes, roadmaps, especificações, dependências, backlogs, propostas de ADR, task files documentais, planos paralelo/integração, ADR-010 e ADR-018; também revisão/rastreabilidade extensas do primeiro pacote e matrizes de compatibilidade. Documentos alpha.14 de recuperação, cópias de diagnóstico, AI Tools e ADR-025/027/028 foram lidos para preservar as fronteiras atuais. Leituras de código, manifests, testes e pesquisa atual da implementação são registradas no task de execução; esta lista não alega auditoria completa do repositório ou de contas externas.

O Doctor continua somente leitura. Recuperação manual utiliza CAS SHA-256/SQLite restrito, prévia/proteção/journal/desfazer; cópias têm identidade independente e aplicação seletiva. Tools não são editor genérico de properties/TOML. Mundos, `saves/**/serverconfig`, credenciais, módulos administrados e executores gerais não são adicionados por Connect. Nenhum resultado histórico é promovido por esta reconciliação.

## Preservação e rollback

R0–R5, prioridades, gates, ownership de monólitos e a ADR-018 permanecem. Não foi necessário copiar qualquer código alpha.13 sobre alpha.14. O harness independente não altera o IPC local nem constitui capability pública do launcher. Relay permitido continua elegível somente após verificação do uso; nenhum custo, conta, infraestrutura ou provedor é habilitado por Markdown.

Rollback documental consiste em reverter somente o diff/commit desta reconciliação em mudança revisada. Conservar alpha.14, documentos históricos, outras worktrees e resultados locais. Não usar reset/clean nem apagar evidências para obter uma árvore limpa.
