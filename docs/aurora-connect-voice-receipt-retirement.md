# Aposentadoria explícita do recibo de voz — incremento69

Estado: **PROTÓTIPO**, implementado e testado localmente em armazenamento descartável. Depende do [armazenamento67](aurora-connect-durable-voice-receipts.md); a [integração68](aurora-connect-desktop-durable-voice.md) permanece uma entrega distinta. Permanecem normativos [Recovery](instance-recovery.md), [ADR-027](engineering/ADR/ADR-027-manual-instance-recovery.md), [ADR-028](engineering/ADR/ADR-028-doctor-recovery-lab.md), [voz](aurora-connect-voice.md) e [roadmap](engineering/MASTER_ROADMAP.md).

## Bloqueio e ação delimitada

Undo genérico ou edição posterior podem devolver o anúncio original ou alterar seus campos enquanto o recibo seletivo continua `Ready`. Isso deve bloquear restauração conflituosa, mas também impede uma nova aplicação durável do mesmo mod. Não é correto inferir consumo pela aparência da configuração, converter o recibo em `Restored` ou descartar sua autoridade automaticamente.

A ação nova permite **aposentar a restauração seletiva**, após revisão e confirmação distintas. Ela altera somente o registro no índice Recovery: `Ready` geração0 → `Retired` geração1. Não modifica configuração, anúncio, bind, mod, CAS, snapshots, arquivos ou sessões; não cancela nem restaura transporte. `Retired` significa que a autoridade seletiva foi abandonada conscientemente, não que o anúncio original foi devolvido. Histórico e pontos existentes permanecem; somente o pin de recibo ativo deixa de impedir uma futura exclusão explicitamente autorizada pelos mecanismos existentes.

## Contrato nativo

API opt-in sob `connect-voice-lab`, desligada por padrão. Preparo e confirmação são separados; o plano é opaco, exclusivo, não serializável e somente de memória. O preparo seleciona cópia diagnóstica, recibo e geração explicitamente; exige `VoiceMutationGuard`, proveniência, identidade física, índice/record/CAS íntegros, ausência de journal pendente e runtime compatível. Mod e caminho vêm do registro validado; não se exige JAR instalado ou igualdade entre configuração atual e bytes aplicados.

A revisão contém somente identificadores limitados, mod, geração e hashes/tamanhos de observações protegidas, prazo e aviso de perda da restauração seletiva. A ausência real do arquivo pode ser representada; arquivo existente excluído da observação, link, caminho inseguro ou limite excedido não equivale a ausência e recusa. Nenhum byte de configuração, credencial ou segredo passa ao caller. O digest tem domínio próprio e liga UUID, ID/geração/hash exato do registro, fingerprint da instância e observação atual. Não reutiliza aprovação de aplicar, restaurar, fechar ou outra aposentadoria.

Prazo original de300s, civil e monotônico, sem renovação na confirmação. Cancelar ou descartar o plano não escreve. A confirmação revalida guard/processos, identidade, proveniência, runtime, fingerprint e record Ready/geração/hash antes de qualquer publicação. Mudança concorrente exige nova revisão; não se aposentam vários recibos em lote.

## Atomicidade, geração e recuperação

Como nenhuma operação de arquivo é executada, a publicação usa uma única transação SQLite `FULL`: compara a versão esperada e atualiza a linha para `Retired` geração1. Não há novo journal de renames. `Pending` anterior impede preparo/confirmação; jamais apagar journal ou reconstruir autoridade para permitir a ação.

Resultado e metadados de retorno são preparados antes do commit. Falha antes do commit conserva `Ready`; resultado perdido depois do commit é resolvido por listagem read-only, que mostra `Retired`. Reabrir o índice não renova ou reativa o recibo. Outro controlador/duplo clique com geração antiga recusa sem novo efeito. Aposentar uma autoridade não altera recibos de outro mod ou cópia.

A enumeração armazenada recebe estado novo explícito, compreendido também pelo Recovery compilado sem feature de voz. Combinações de estado/geração aceitas: Ready0, Restored1 e Retired1. O estado durável é separado da enumeração de restauração RAM: esta mantém exatamente seus estados anteriores. Não muda schema de convite, envelope UDP, matriz de mods ou perfis de transporte. O caller68 precisa adotar essa capacidade explicitamente em incremento posterior da integração, com rótulo distinto e nenhuma retomada de registros aposentados.

Downgrade: as listas67/68 anteriores não compreendem `Retired` e devem recusar conservadoramente a leitura desse histórico. Isso pode impedir a listagem de outros recibos da cópia nessa versão antiga; não autoriza excluir o índice, reescrever o histórico ou converter a linha para `Ready`/`Restored`. A enumeração e os registros antigos preservam suas representações. Um controlador já carregado mantém apenas a observação anterior: sua próxima operação deve recusar o token/geração obsoleto, sem afirmar que a configuração voltou ao original. A integração futura deve encerrar ou invalidar seu controlador somente após a confirmação da aposentadoria.

## Prova local exigida

- Aplicar duravelmente, executar undo genérico na fixture e comprovar Ready ainda bloqueando nova aplicação; revisar/confirmar aposentadoria e então aplicar novo anúncio com novo recibo.
- Cancelamento, digest errado, plano vencido, alteração de configuração após revisão, outro UUID/mod/geração e journal pendente: nenhum registro/config alterados.
- Duas revisões do mesmo registro: uma confirmação vence; outra recusa por geração/estado.
- Interromper antes/depois do commit, descartar objetos, reabrir e conferir Ready/Retired sem inferir pela configuração.
- Configuração presente com edição divergente ou realmente ausente, limites/link/exclusão: comportamento conforme observação; bytes pessoais não são usados.
- Após aposentadoria, pontos históricos permanecem e listagem não exige imagens removidas legitimamente; recibos Ready independentes continuam verificáveis.
- Default-off e caminhos RAM67/68 preservados; sem Java, rede externa, contas ou configuração pessoal.

## Evidência local — 14/10/2026

O foco combinado69/70 passou8 testes comuns, dos quais6 são do núcleo69, e sete opt-ins de arquivos reais passaram separadamente, dos quais3 são do69. Os3 cobrem os dois mods, undo genérico seguido de nova aplicação durável, cancelamento, digest errado, edição posterior, duas revisões, ausência de arquivo e remoção do JAR. Recovery cobre publicação interrompida antes/depois do commit, reabertura, CAS adulterado, journal pendente, token obsoleto, preservação de tabelas/pontos e histórico após liberação do pin. Relógios privados verificam vencimento civil e monotônico; nenhum relógio do sistema foi alterado.

Regressão padrão, com voz e transporte desligados:278 PASS/5 ignorados, zero falhas. Os testes de falha são injeções determinísticas e reabertura de objetos/índice, não desligamento físico ou morte forçada do processo. Revisão independente e do coordenador sem bloqueador final. Comandos, durações e logs estão no [registro69](task-aur-r4-connect.md#incremento69--aposentadoria-explícita-da-autoridade-seletiva).

## Limites e rollback

A aposentadoria não repara CAS corrompido, resolve journal pendente, restaura bytes nem homologa áudio. Não promete desfazer o abandono seletivo: para alterar configuração novamente é preciso novo plano autorizado. Recuperação genérica e snapshots existentes permanecem disponíveis conforme seus próprios gates. O limite de64 recibos por cópia continua incluindo o histórico; aposentar não é GC e não permite exceder esse limite.

Rollback de código preserva índice/histórico e primeiro resolve qualquer transação pendente por versão compatível. Nunca apagar registros desconhecidos para fazer downgrade. Nenhuma migração retroativa de recibos RAM, GC, limpeza de histórico, push, deploy ou instalador faz parte deste incremento.
