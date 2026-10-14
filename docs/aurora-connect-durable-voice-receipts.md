# Recibos duráveis de anúncio de voz — incremento67

**PROTÓTIPO**,14/10/2026. APIs novas opt-in sob `connect-voice-lab`, desligada por padrão; comandos e interface pertencem ao incremento68. Os caminhos RAM58/60, seus avisos de encerramento e suas assinaturas permanecem intactos. Provas executadas abaixo e no task; não há homologação de áudio.

Contratos normativos: [Recuperação](instance-recovery.md), [ADR-027](engineering/ADR/ADR-027-manual-instance-recovery.md), [cópias de diagnóstico](diagnostic-lab.md), [ADR-028](engineering/ADR/ADR-028-doctor-recovery-lab.md), [adaptadores de voz](aurora-connect-voice.md), [controlador58](aurora-connect-voice-restoration-controller.md) e [caller60](aurora-connect-desktop-voice.md).

## Problema e fronteira

O anúncio já usa CAS, snapshot anterior e journal de Recovery. O recibo seletivo existe somente em memória. Se o processo terminar depois do commit de Recovery e antes de conservar o controlador, o arquivo está aplicado, mas a autoridade seletiva se perde. Gravar um JSON depois de `apply` deixa essa janela aberta e pode informar falha depois de uma alteração concluída.

O caminho novo publica o recibo na mesma transação SQLite `FULL` que publica undo e encerra o journal. O recibo nunca é importado de um arquivo indicado pelo chamador, descrição de snapshot, nome de pasta, data ou configuração atual. Somente o índice Recovery da identidade verificada participa da retomada. Nenhum byte de configuração, chave privada ou credencial atravessa IPC; não há IPC novo neste incremento.

Continua restrito a cópias diagnósticas verificadas, Fabric1.21.1, SVC2.6.22 e Plasmo2.1.17 fixados. O modo é dedicado somente Aurora. Alterar anúncio não configura o bind, abre ponte ou comprova áudio. Cada participante, inclusive o anfitrião, continua precisando do transporte apropriado. Não amplia matriz, Core/Skins, LAN, modo misto ou produção.

## Armazenamento e compatibilidade

Uma tabela aditiva `voice_receipts` no índice Recovery guarda registros versionados e limitados por instância. O schema global SQLite, as identidades, os manifestos e os journals antigos permanecem na versão1. Journals com transição de recibo usam versão2 e campo próprio; o validador só aceita a combinação exata. O Recovery atual entende esses journals independentemente da feature de voz. Uma versão antiga que não conhece journal2 deve recusá-lo, nunca executá-lo parcialmente.

O registro liga UUID da instância, mod selecionado, identificador do recibo, transação de aplicação, geração, estado, manifestos original/aplicado e hashes/tamanhos das duas imagens. O caminho de configuração vem exclusivamente da enumeração dos dois mods. Bytes ficam nos blobs CAS já verificados, com teto256KiB por imagem. Runtime e identidade física são conferidos na carga. Identidade não é criada ou reparada por listagem/retomada. O reader interno Recovery devolve lista vazia se não há identidade ou tabela de recibos. A API pública de voz exige cópia diagnóstica válida com identidade/índice existentes, recusando sua ausência ou invalidade; tabela de recibos ausente em índice válido resulta em lista vazia. Registro ausente não se transforma em autoridade.

Há no máximo um recibo ativo por instância/mod. Pontos referidos por recibo ativo não podem ser excluídos. Não implementa coleta de blobs nem importação retroativa dos recibos RAM. Dados locais continuam sujeitos às proteções de caminho, link, hardlink, identidade e integridade de Recovery; não se promete resistência a um atacante com controle integral do índice, CAS e proveniência no mesmo usuário do sistema.

## Estados e atomicidade

| Momento | Estado durável e efeito |
| --- | --- |
| Preparar anúncio | Plano apenas RAM; nenhum recibo, snapshot de aplicação ou permissão persistida. |
| Confirmar e validar | Digest, prazo original, guard real, processo Java, cópia, identidade, runtime e fingerprint rechecados. |
| Antes do primeiro rename | Journal2 persistido com a transição de recibo validada contra as operações e manifestos antes/depois. |
| Aplicação pendente | Journal impede mutação/início concorrentes; nenhum recibo Ready disponível para retomar. |
| Commit da aplicação | Undo, recibo Ready e remoção do journal/prévias são uma única transaçãoFULL. |
| Rollback da aplicação | Arquivos anteriores conferidos e repostos; journal removido; nenhum recibo Ready publicado. |
| Preparar restauração | Novo plano RAM com digest/prazo próprios; o recibo Ready não é consumido. |
| Restauração pendente | Journal2 vincula ID, geração e hash do recibo Ready; retomada é recusada enquanto houver journal. |
| Commit da restauração | Recibo passa a Restored com geração incrementada na mesma transação que encerra o journal e publica undo. |
| Rollback da restauração | Bytes anteriores à restauração retornam; recibo permanece Ready, exigindo nova revisão explícita. |

Journal e recibos são dados não confiáveis. Validar toda a associação antes de aplicar, finalizar ou reverter: UUID, schema, transação, mod, caminho único, perfil, hashes, tamanhos, manifestos e geração. Entrada órfã, desconhecida, divergente ou ambígua falha conservadoramente. Edição externa durante recuperação impede sobrescrita e conserva journal/evidências.

Nenhum erro posterior a um commit comprovado deve ser inventado por uma escrita secundária de recibo. Uma falha cujo resultado seja incerto exige inspeção/Recovery; a próxima carga usa o índice transacional, nunca a aparência do arquivo para concluir que a operação ocorreu.

## Retomada e restauração explícitas

`prepare_durable_announcement` retorna o tipo opaco `PreparedDurableAnnouncement`, com revisão/digest próprios antes de sua confirmação. Listagem devolve somente metadados limitados; `resume_restoration` exige instância explícita e `VoiceMutationGuard`. Ela verifica proveniência, associação de identidade existente, ausência de journal, estado/geração, manifestos e blobs. Reconstitui um controlador privado para preparar uma revisão nova. Não retoma uma confirmação, digest ou prazo antigo; não escreve configurações ao carregar ou no Drop.

A restauração estrita exige exatamente os bytes aplicados. A reconciliada continua uma ação distinta, com hash atual informado como pré-condição, revisão e aprovação próprias; nenhum fallback automático é permitido. Valores anteriores/aplicados e propriedade das chaves são derivados dos bytes CAS por parsers SVC/Plasmo, não de strings de apresentação. Alterações alheias são preservadas; conflito em chave própria recusa a operação. A ausência posterior do JAR não impede devolver configuração própria.

Cada confirmação confere novamente estado e geração sob o lock da instância e vincula a transição ao journal. Dois controladores retomados não podem consumir o mesmo recibo duas vezes. `Restored` significa devolução dos campos próprios; a restauração reconciliada não promete o hash integral original. A recuperação de uma transação incompleta é o rollback já existente; não autoriza restaurar automaticamente um anúncio aplicado com sucesso.

## Evidência exigida antes de concluir

Testes devem cobrir ambos os mods e armazenamento temporário próprio: aplicar, descartar objetos/reabrir, listar/retomar, restaurar estritamente e reconciliar mantendo outras edições; retirada do JAR; conflito de chave; digest/prazo incorreto; geração/replay; UUID/cópia divergente; registro/CAS/manifestações alteradas; referência ausente; exclusão de ponto ativo; reader sem inicialização nem reparo.

Interrupções determinísticas antes do journal, antes/depois de cada rename, após todas as trocas/précommit e após commit/préretorno devem produzir somente o estado previsto acima. Repetir durante restauração e rollback, incluindo conflito externo que conserva o bloqueio. Reabrir o armazenamento sem objetos antigos comprova independência da RAM; isso não homologa queda física de energia, disco cheio real ou corrupção arbitrária de hardware. Testes não executam Java nem alteram configuração pessoal. JARs locais, quando necessários ao preparo público, devem respeitar os pins existentes.

## Rollback e integração posterior

Encerrar um controlador não escreve ou elimina o recibo. O caller60 continua usando RAM até integração separada com lista/seleção/retomada explícitas e avisos adequados. Não remover seu aviso de perda de restauração seletiva enquanto ele não usar o caminho durável.

Rollback de código exige primeiro resolver journals2 com uma versão que os compreende; nunca remover entradas desconhecidas para permitir downgrade. Os bytes/CAS e pontos são preservados. Build sem a feature não cria anúncios novos, mas conserva capacidade de recuperar transações interrompidas desse formato. Nenhum push, deploy, instalador, publicação, serviço pago ou configuração global acompanha esta implementação.

## Limites do lifecycle

O preparo durável é uma API distinta e acrescenta domínio, aviso e digest próprios antes da aprovação. Um digest do preparo RAM não confirma o modo durável. Listagem de registros `Restored` verifica a linha e sua identidade/geração, mas não exige imagens históricas já liberadas para exclusão; somente `Ready` pode ser retomado e exige manifestos/CAS íntegros.

Undo/restauração genérica de Recovery não consome o recibo seletivo. Se devolver os campos ao original ou a outros valores, o recibo permanece `Ready`, sua restauração segura pode recusar por conflito e uma nova aplicação durável do mesmo mod fica bloqueada. Não existe aposentadoria manual de recibo neste incremento. Essa ação exigirá contrato e revisão explícitos; jamais será inferida da configuração atual. Limite conhecido, sem promessa de lifecycle completo entre executores.

Arquivos vazios são recusados também pelos renderizadores anteriores: SVC exige `voice_host` existente e Plasmo exige `[host]` explícito. A retomada reconstrói `render(original, porta aplicada)` e exige igualdade exata com a imagem aplicada, inclusive quando `[host.public]` era ausente; não compara a restauração reconciliada com o hash original.

## Evidência local

Fechamento67: foco integrado67/68 `--all-features --lib durable` passou19 testes,9 opt-ins ignorados,6,99s; inclui11 Recovery finais,1 parser67,6 testes68 e1 migração preexistente. O negativo de ID extenso passou. Os quatro opt-ins67 foram executados separadamente conforme abaixo; os cinco68 têm registro próprio. Clippy all-targets/all-features passou12,95s; fmt completo e hashes do source freeze67 conferidos. [Comandos e logs](task-aur-r4-connect.md#incremento67--recibo-durável-e-retomada-de-restauração-de-voz).

O coordenador executou o foco `--all-features --lib durable_` com `--offline --locked --jobs 2 -- --test-threads=1`:12 PASS e4 opt-ins ignorados em5,32s. O filtro inclui uma regressão de migração Companion já existente, não reintroduz o mod; são11 testes novos aprovados (10 Recovery +1 fronteira de parser vazio). A primeira rodada anterior continha7 testes Recovery e passou em9,82s; ela não representa o conjunto final. A regressão padrão anterior à defesa adicional de IDs passou273 testes,5 ignorados,49,10s somados. Não somar rodadas sobrepostas.

A rodada opt-in67 com `AURORA_CONNECT_ARTIFACT_DIR` explícito passou4/4 em92,22s (compilação1,15s), conforme log local `durable-voice-67-pinned.log`. Executou seis ciclos de aplicar/descartar/reabrir/restaurar: dois SVC e quatro Plasmo (tabela pública ausente/presente; vias estrita/reconciliada), além de conflito/identidade/geração/cancelamento/digest/CAS nos testes respectivos. JARs apenas copiados e verificados pelo adaptador; nenhum Java executado. A defesa adicional de ID até37 caracteres e seu negativo passaram no foco final, sem repetir os seis ciclos de artefatos inalterados.
