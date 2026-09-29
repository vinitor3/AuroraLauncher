# ADR-027 — Recuperação manual de instâncias

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. Data: 29/09/2026. Adoção limitada da [ADR-006](ADR-006-local-cas.md); preserva os contratos de atualização, Companion, Doctor e ações confirmadas das ADRs 007, 018, 021, 024 e 025.

## Contexto

A restauração das ações do Assistente cobre somente os alvos daquela ação. O jogador precisa registrar um estado explícito de mods e configurações e recuperá-lo depois de alterações independentes. O nome da pasta não é identidade suficiente, uma cópia não pode herdar autorização de restauração e um manifesto local é dado não confiável.

## Decisão

Acrescentar uma área manual Recuperação e um serviço nativo específico. Pontos publicados contêm um manifesto versionado com inventário, digests SHA-256, tamanhos, escopo e perfil permitido. Blobs são cópias independentes endereçadas pelo conteúdo; nunca hardlinks da instância. O índice transacional só torna o ponto disponível depois da escrita e verificação dos dados. A exclusão retira a referência do índice e conserva conteúdo sem referências nesta etapa, evitando uma coleta insegura.

Usar a mesma exclusão mútua por instância do launcher. Captura, aplicação e recuperação exigem disponibilidade da instância; processos externos devem ser considerados. Uma captura revalida o inventário e os bytes antes de publicar. A restauração exige prévia com adições, substituições, remoções e mudanças do perfil, confirmação explícita e nova comparação do estado observado. Nenhuma proposta de IA recebe autoridade sobre este serviço.

Proteger o estado anterior antes de modificar a instância. Preparar conteúdo em staging e persistir a intenção antes de cada troca. Uma operação interrompida conserva journal e originais e deve ser recuperada sob o bloqueio antes de permitir outra mutação ou inicialização. Integridade inválida bloqueia a operação, preservando a evidência local para recuperação.

Restaurar somente o escopo declarado, preservando identidade e vínculos internos. Mundos, contas, credenciais, sessões, caminhos externos e instalação compartilhada ficam fora do contrato. Core, Skins e Companion têm tratamento explícito: pontos não reinstalam Companion nem substituem módulos administrados por artefatos antigos. O perfil não autoriza trocar para uma combinação de runtime incompatível com a base instalada.

## Consequências

Esta fase adota CAS somente para Recuperação. Não migra backups e journals de updates/Assistente, não unifica executores existentes e não altera a matriz Minecraft/loaders/Java. O tamanho lógico de um ponto pode ser maior que o espaço físico novo; a interface deve distinguir essas medidas. Exclusões precisam aparecer antes da confirmação: o ponto não é um backup integral da instância e não inclui mundos.

Arquivos e manifestos locais são validados, incluindo caminhos, links, identidade e limites. Falhas devem produzir mensagens públicas sem stack traces, caminhos externos ou credenciais. Operações longas ficam fora da thread da interface; cancelamento só é permitido enquanto não houver aplicação destrutiva.

O incremento seguinte são pontos automáticos antes de atualizações, sujeito a política de retenção, espaço e confirmação apropriadas. Não está implementado por esta decisão. Restauração por IA, diagnóstico em cópia temporária, mundos e nuvem também permanecem fora do escopo.

Contrato concreto, formato e limitações: [Recuperação de instâncias](../../instance-recovery.md). Evidências executadas e pendências: [AUR-1166](../../task-aur-1166-instance-recovery.md).
