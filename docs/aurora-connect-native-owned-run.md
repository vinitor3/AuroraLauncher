# Supervisor nativo de um filho próprio — contrato83

**PROTÓTIPO**,23 focais e537 regressão PASS, checks/QA aprovados; corrida de classificação reproduzida antes da correção.21/10/2026, base `26bed4791a276539de338eb0bb41788ecc26fcf6`, branch `codex/aur-r4-connect`. O usuário autorizou implementar e testar localmente sem os dois computadores. Este recorte não executa Java nem altera arquivos pessoais. Root e QA leram e aprovaram integralmente o contrato SHA256 `3ce0e27fb296ca7051f52eca0d7f3faee968c4c3a7323cf36c4ae511eafa8a0e` antes do código. Checkpoints abaixo conservam seus estados históricos.

## Entrega funcional e limite de autoridade

O bloqueio auditado é a ausência de um owner nativo para uma execução dedicada: RunningInstances acompanha clientes e permite que sobrevivam ao launcher; Node server.mjs possui seu próprio Child, sem transferir esse handle para Rust. A abertura pronta73 é cliente/QuickPlay, não dedicado.81 retira UDP independentemente e82 observa seu binding, mas nenhum deles supervisiona um servidor.

83 entrega a primeira peça executável desse produtor: iniciar e supervisionar exatamente um filho nativo, derivar sua prova original/run e aguardar stop/reap/readers/consultas próprias. O produtor Java, verificação de cópia/proveniência/artefatos, revisão/digest, lock da cópia e EULA são uma entrega posterior, não inferidos deste núcleo. Não chamar83 “servidor integrado” ou “voz vinculada”. Não aceitar JSON/PID/relatório Node como autoridade.

Módulo novo em `engine/connect_owned_run.rs`, interno à biblioteca, somente Windows+connect-lab; registro `pub(crate)` em engine/mod.rs. Pode ter submódulos privados para implementação/testes. Nenhum comando Tauri, serde/DTO, interface, processo externo escolhido pelo usuário, dependência/feature/lock novo ou caller de produção. Anotação estreita documentada de código reservado é permitida; não criar um caller falso para eliminá-la.

## API interna e entradas confiáveis

Uma função síncrona de start recebe `std::process::Command` construído exclusivamente por código Rust confiável, identificador de instância limitado e limites/cancelamento. Esse Command não é plano de execução nem entrada remota;83 não promete validá-lo como Java, arquivo seguro ou cópia. O futuro produtor deve construir argumentos/ambiente fixados depois dos próprios gates, sem shell.83 não expõe nem executa strings de shell. Testes só fornecem o próprio executável de testes.

O start sobrescreve stdio para três pipes e impõe CREATE_NO_WINDOW. Não permite que o caller receba Child, stdin ou streams para lançar trabalho não rastreado. Um run UUID é gerado no nativo para cada start; instância é apenas rótulo confiável limitado, sem reivindicar identidade física/Doctor. Não adicionar adoção por PID, procura de Java, importador de prova, tree kill ou configuração de callbacks de término.

Limites: duração total maior que zero e até3600s; graça de stop maior que zero e até5s, não maior que a duração; saída agregada stdout+stderr maior que zero e até16MiB. Validar antes de spawn; cancelamento já solicitado recusa antes de spawn. Sem alterar relógioWindows. O prazo usa Instant monotônico contado antes do spawn e não renova após stop/consulta. Não prometer que spawn, syscall, reaping ou pipe de descendente terminem dentro de prazo fixo do SO.

Start devolve handle de controle não clonável e observador fraco/completion estreitos, com nomes exatos definidos pelo owner de implementação e revisados antes dos testes. O controle pertence ao futuro produtor; Stop Connect não o chama implicitamente. O start é síncrono: depois de spawn, nenhum ponto de retorno/erro/pânico deixa Child sem owner imediato. Caller futuro não deve executar start/consulta sob mutex do estadoTauri; precisa reservar trabalho antes de deslocá-lo a worker, conforme75–77.83 não integra esse caller.

Após inicialização potencialmente lenta, revalidar cancelamento/deadline/selo sob gate antes de decidir sucesso do start e publicar controle/observer. Se já inválido, conservar owner e concluir cleanup antes de devolver erro; não esperar worker segurando gate. Cancelamento posterior à decisão pode ocorrer imediatamente e será observado pelo mesmo owner, sem reivindicar readiness. Falha ao criar a thread do worker continua coberta pelo owner RAII original.

## Ownership, encerramento e observação

1. Instalar owner RAII imediatamente depois de spawn, antes de extrair pipes, criar prova, leitores ou worker. Falha parcial de inicialização e pânico devem terminar/recolher somente esse filho e juntar os leitores já criados. Não abandonar Child quando thread spawn/proof falhar.
2. Worker próprio retém Child e leitores até reap. Não depender de waiter IPC, runtime Tokio vivo ou callback arbitrário de aplicação para solicitar stop/terminar. Threads de saída drenam bytes em buffers fixos e não retêm, decodificam, exportam ou registram conteúdo. Contagens saturantes e quota agregada; falha de leitura/overflow da quota solicitam encerramento sem parar de drenar prematuramente.
3. Stop, cancelamento, Drop do único handle de controle, prazo, saída precoce e falha selam a entrada de observações. Repetir stop acompanha a mesma conclusão, sem nova execução. Abandonar uma future de stop não cancela o cleanup. Drop somente solicita encerramento; não declara que o cleanup terminou.
4. Enviar uma única linha fixa `stop\n` no pipe próprio, depois fechar stdin. Nenhuma outra escrita de produção nesse pipe. Graça limitada pelo menor entre prazo restante e limite de stop; depois, se ainda vivo, matar apenas pelo Child próprio e aguardar wait. Deadline total pode exigir kill imediato. Saída espontânea, inclusive zero antes de stop solicitado, é falha de execução. Stop normal exige exit0, sem kill.
5. Registrar razão e primeira falha estática sem mascarar erro anterior por stop/exit0. Quota, IO, deadline, saída precoce/nonzero, kill/falha de cleanup e pânico não viram sucesso. Resumo interno fechado contém somente razões/códigos, exit code, forced, contagens, reap e readers concluídos; sem PID, path, args, ambiente, proof, saída bruta, credencial ou dados de conta. Não serializar automaticamente.
6. Derivar `OwnedChildProof` somente do Child original, com instância/run gerados/selecionados no start. Não alargar visibilidade82 ou alterar windows.rs para duplicar/adotar prova. ObservaçãoUDP interna usa require_owned_udp_binding82; o observer não entrega a prova nem um permit reapresentável.
7. Antes da consulta bloquear, registrar um trabalho RAII sob o mesmo selo de stop. No máximo uma consulta em voo por execução; a segunda recebe Busy, sem fila. O trabalho conserva somente a prova original e ownership necessário até terminar; query sempre fora de mutex de lifecycle. Depois da consulta, revalidar selo/prazo/cancelamento e decidir sucesso sob o mesmo gate de registro/stop. Esse é o ponto de linearização: stop ocorrido antes da decisão impede Ok; stop posterior não retira retroativamente um retorno já decidido. Uma resposta favorável anterior ao stop não autoriza retomada.
8. Stop do processo não espera uma consulta Win32 retida para encerrar/recolher o filho. A conclusão global, porém, aguarda a mesma consulta e seus recursos, sem substituir por outra ou descartar o registro. Retirar/drop da prova privada depois que todas as consultas acabarem e somente então publicar resultado final. Uma consulta registrada libera sua prova antes de retirar seu registro. Um observer fraco não prolonga sozinho o processo ou adquire autoridade depois de selado/fechado.
9. Resultado de cleanup só é publicado após reap do Child, join dos leitores e fim das consultas. Completion pode ser conservada/consultada depois de Drop, sem ser owner de processo: carrega somente canal/estado de conclusão e resumo, nunca Child/proof/readers, JoinHandle, closure ou Arc que os mantenha. O owner independente aguarda as caudas. Não afirmar “parou” apenas porque token foi cancelado ou processo saiu se ainda há trabalho próprio. Nenhum bloqueio de State ou wait segurando mutex que o trabalho precisa para terminar.

Implementação pode usar thread nativa/condvar e watch Tokio para notificação de completion, sem incluir features Tokio process/fs novas. Não introduzir supervisor genérico de árvores. Descendentes não são suportados; não procurar/matar processos alheios para forçar EOF. Se um descendente externo conserva pipe, registrar a limitação e manter cleanup pendente em vez de alegar término completo. Processo do launcher inteiro morto pelo SO continua fora da garantia de um owner de thread; futuro produtor não deve prometer Job Object/crash kill inexistente.

## Provas locais obrigatórias

Fixtures são filhos ocultos do executável Rust de teste com marcador e nome exato, protocolo privado fixo/limitado. Nenhum Java, download, áudio, relay, firewall ou instância real. Não selecionar opt-ins históricos por glob. Root é único executor Cargo, sempre fonte congelado, offline/locked/jobs2.

| Caso | Oráculo causal mínimo |
| --- | --- |
| Validação | Limites/instância inválidos e token pré-cancelado recusam antes de spawn; testemunha causal de zero filhos, não apenas resultado Err. |
| Normal | Filho próprio com TCP/UDP literaisV4/V6, transferência real de bytes pela fixture e query82 no mesmo original. Stop uma vez, exit0, readers unidos e portas reabrem. Não inferir STATUS/mod de bytes sintéticos. |
| Identidade | Outro filho permanece vivo e seus endpoints não satisfazem observer; instância/run nativo conferidos por método privado de teste. Substituto no mesmo endpoint não reativa observador/prova antigos. |
| Cancel/Drop | Cancelamento externo e Drop solicitam o mesmo cleanup; completion conservada aguarda reap/readers. Outro filho não é afetado. Stop repetido não envia outra linha. |
| Waiter perdido | Parar em gate privado após pedido, abandonar future e observar pelo mesmo completion que o trabalho continua; não lançar cleanup substituto. |
| Prazo/falha | Filho que ignora stop, saída precoce zero/nonzero, flood stdout e stderr agregados, erro de leitura/pipe via seam privado determinístico, inicialização parcial e pânico. Resultado falha, reap/readers concluídos, sem conteúdo nos resultados. |
| Consulta retida | Gate exclusivamente cfg(test) depois da reserva e antes/fim da consulta real; stop recolhe o filho e permite rebind antes de liberar o gate, mas completion continua pendente. Depois a mesma query recusa e completion conclui. |
| Selo | Query nova depois de stop não executa a consulta; resposta de query positiva retida antes do gate final não é decidida como sucesso se stop já selou. Stop posterior à decisão não recolhe o retorno decidido. Sem renascimento por liberar/reabrir UDP. |

Se um seam injeta falha de leitura, não retirar a necessidade de um reader real independente ser recolhido; não introduzir provider/callback de produção. `cfg(test)` não pode converter saída de processo/JSON em autoridade. Nomes/quantidades exatas dos testes serão registrados após execução; helper ignored não conta como PASS adicional.

## Primeiro checkpoint83 — produção, sem teste de execução ainda

Produção em `engine/connect_owned_run.rs` e `connect_owned_run/worker.rs`, registro interno em engine/mod.rs, seams isolados em connect_owned_run/test_support.rs somente cfg(test). Fonte congelado antes de `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --features connect-lab --lib`: **PASS**,5,06s, exit0/sem avisos; log privado `native-owned-run-83-first-check.log`. Fixture/testes ainda não registrados nessa execução. Isso é apenas compilação, sem reivindicar spawn/reap/query reais83.

API concreta: `start_owned_run(Command, &str, OwnedRunLimits, CancellationToken)` retorna controle único OwnedRun; `observer()` retorna Weak; `request_stop()` sela sincronamente e devolve completion; `completion().wait().await` acompanha o mesmo resultado, sem owner de recursos. `observe_udp(SocketAddr)` síncrono registra uma única query; sem TCP novo ou prova exportada. Resumo fechado conserva primeira razão/falha e contagens/exit/reap/readers. Futuro produtor não pode usar Command de JSON/frontend.

Revisão root durante escrita encontrou falta de rechecagem cancel/deadline na decisão final depois de readers/consultas. Corrigida antes do check: refresh no gate final e seam para prova causal dessa cauda. Owner Worker é instalado imediatamente com Vec::new; falha de thread spawn conserva cleanup RAII e primeira falha. Testes reais e QA final ainda pendentes; não chamar esse checkpoint homologação.

## Primeiro focal83-A — falhas preservadas

`cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --features connect-lab --lib engine::connect_owned_run::tests -- --test-threads=1`: **5 PASS/5 falhas/1 helper ignorado**,1,68s/compilação40,53s, exit101; `native-owned-run-83-focused-first.log`. Quatro avisos intermediários cfg(test), hooks sem casos ainda, não ocultados. As cinco falhas ocorreram em leitura do canal privado da fixture esperando StopObserved; parent aceitou stream de listener não bloqueante sem alterar explicitamente o modo do stream. Root pediu correção só da fixture, sem mudar oráculos de stop/EOF nem atribuir essas falhas à produção. QA identificou ainda EOF no caso ignore-stop como confundidor de forced; correção própria em andamento.

Cinco casos passaram: cancelamento externo, foreign/substituição, deadline, saída precoce e quota agregada. Não são suíte completa nem autorização de homologação. Esse focal já compilou o refino de rechecagem cancel/deadline imediatamente antes de spawn, posterior ao primeiro check.

Revisão encontrou separadamente uma corrida real de classificação: saída já observada antes da linha stop pode concorrer com request_stop antes do registro de razão e parecer normal. Seam privado após poll_exit e antes de classificar, com filho realmente encerrado, permitirá reproduzir RED antes de corrigir produção. Não é causa demonstrada das falhas da fixture. Root coordena Cargo e congela todos os inputs; próximo lote de testes ainda isolado.

## Correção causal83 e loteA aprovado

Fixture corrigida: modo blocking explícito no stream aceito do controle e no streamTCP echo, mantendo timeouts; ignore-stop resiste também a EOF sem inventar evento StopObserved. Nenhum prazo/oráculo de produção ampliado.

Root executou `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --features connect-lab --lib engine::connect_owned_run::tests::exit_reaped_before_requested_stop_is_not_normal_success -- --exact --test-threads=1`: **RED**,0 PASS/1 falha,0,04s/compilação27,70s, exit101 (`native-owned-run-83-exit-race-red.log`). Filho realmente ExitZero/reap, gate privado antes de classificar, request_stop posterior sela Requested; implementação anterior retornou failureNone contra EarlyExit exigido. Porta já livre antes da aprovação de stop, sem mock de saída/Child. ReleaseGate libera o gate no unwind.

Corrigidos dois callsites antes de qualquer escrita de stop: saída observada no drive ou no poll inicial de stop_child registra EarlyExit mesmo com Requested já selado, conservando primeira razão/falha. Poll após envio fixo permanece. Stop escrito/exit0 sem força/falha continua encerramento normal; não se afirma ACK do aplicativo por consequência.

Mesmo comando focal do loteA depois das correções: **11 PASS/0 falhas/1 helper ignorado**,2,06s/compilação14,95s, exit0 (`native-owned-run-83-focused-second.log`). TCP3072B/UDP1264B exatos em loopbackV4/V6, query82 do processo original, release/rebindUDP, foreign/substituto, Stop idempotente/uma linha+EOF, Drop com observer/completion conservados, cancelamento, waiter abandonado com o filho realmente retido, stop ignorado/force, deadline monotônico, EarlyExit0/7, quota stdout/stderr/agregada e worker sem runtimeTokio. Helper é invocado pelos pais, sem contar como PASS extra.

Dois avisos intermediários cfg(test) são helpers de metadados e zero-child ainda reservados ao loteB; não foram suprimidos. O foco não executa Java/Minecraft/voz/Connectentreagentes nem nova topologia. Próximo lote de seams causais e regressões/fmt/clippy/QA final continuam necessários. Não somar rodadas5/11/RED.

## LoteB83 — inicialização, consultas e frescor final

Registrado tests/lifecycle_gates.rs depois do freeze separado. Mesmo comando focal: **23 PASS/0 falhas/1 helper ignorado**,4,39s/compilação27,27s, exit0/sem avisos (`native-owned-run-83-focused-third.log`). Os11 anteriores estão incluídos, não somados. Validação causal com contador de spawn ancorado em positivo real; UUID/rótulos do start sem claim filesystem; quatro fases de inicialização×falha/pânico; falha da criação do worker após filho pronto/bytes; cancel/deadline na inicialização; erro de reader com outro reader real drenando80KiB; falha do pipe de stop e pânico do worker. Queries retidas antes de Win32 e depois de resultado positivo conservam o mesmo job, Busy/Closed não executam nova query, stop recolhe filho/readers e permite rebind antes de liberar o gate, enquanto completion fica pendente. Frescor cancel/deadline antes do report final é testado sem query/observer fazer refresh por outro caminho.

Root e QA independente leram fontes/fixtures/oráculos/logs sem bloqueador nesse checkpoint. Guards de teste liberam gates em unwind; nenhum join bloqueante em Drop de guard, evitando prender worker/Child após falha de asserção. Nenhuma captura de processo pessoal, Java ou voz.

Fmt inicial **PASS**. Primeiro Clippy all-targets/all-features falhou em lint `default_constructed_unit_structs` na construção de Hooks em produção (`native-owned-run-83-clippy-all.log`). Corrigida somente construção por cfg: Arc/default em teste, unidade em produção; sem allow e sem mudança lógica. Na fonte final, `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all -- --check` **PASS** (`native-owned-run-83-fmt-final.log`). `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-features --all-targets -- -D warnings` **PASS**11,99s; mesma chamada sem `--all-features` **PASS**4,75s; substituindo por `--features connect-lab` **PASS**14,61s, exit0/sem avisos (`native-owned-run-83-{clippy-all-final,clippy-default,clippy-connect}.log`). A regressão all-features/all-targets está em execução; não há resultado antecipado.

## Verificação final83 e estado delimitado

Fonte final congelado após corrigir somente a construção cfg de Hooks. Root executou `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-features --all-targets -- --test-threads=1`: **537 PASS/0 falhas/65 ignorados**, oito targets,253,85s somados de testes/compilação1m55s, exit0/sem avisos (`native-owned-run-83-all-full.log`). Inclui os23 focais na fonte final. Ignorados históricos e novo helper não viram PASS adicional. Suíte padrão runtime290 pertence a80 e biblioteca295 a81; não repetidas83. Clippy padrão atual comprova optional-off compilado, sem atribuir runtime novo. Nenhuma alteração frontend; Node/build/Java não reexecutados por83.

Arquivos83: sete Rust `apps/desktop/src-tauri/src/engine/{mod.rs,connect_owned_run.rs,connect_owned_run/worker.rs,connect_owned_run/test_support.rs,connect_owned_run/tests.rs,connect_owned_run/tests/fixture.rs,connect_owned_run/tests/lifecycle_gates.rs}`; domínio83, task, AGENTS, SPEC, ADR, backlog e matriz. Root/QA independente conferiram fontes/oráculos/logs/limites e correção sem bloqueador. Manifests/locks/matriz/versões inalterados, alpha.14/Recovery/CAS/Doctor/Assistente e nove pares preservados; checkout principal não alterado.

| Domínio | Estado após83 |
| --- | --- |
| TCP | Transporte atual preservado; bytes reais da fixture de processo, sem nova API de provaTCP ou dedicado. |
| UDP | Query82 pertence ao processo supervisionado, com Busy/selo e cleanup causal; transporte/retirada81 preservados. |
| Direto | Somente loopbackV4/V6 novo; nenhuma outra topologia/rede homologada. |
| Relay | Público desligado; nenhum uso/conta/custo. |
| Minecraft | Produtor dedicado nativo **NÃO IMPLEMENTADO**; sem Java/login/mundo novos. |
| Convites/interface | Regressões preservadas; sem comando/DTO/UI nova ou homologaçãoGUI. |
| Simple Voice Chat | Vínculo em execução **NÃO IMPLEMENTADO**; áudio **PRECISA DE TESTE**. |
| Plasmo Voice | Vínculo em execução **NÃO IMPLEMENTADO**; áudio **PRECISA DE TESTE**, separado de SVC. |

A garantia de cleanup pressupõe que o processo launcher permaneça vivo: uma futura integração de encerramento normal do app deve aguardar completion antes de terminar main. Drop não fornece essa espera e83 não foi ligado ao app. Nenhum Job Object, controle de árvore, kill por PID/nome ou crash kill é prometido. Descendentes conservando pipes continuam não suportados/cleanup pendente, sem encerrar processos alheios para forçar EOF.

Resultado final **PROTÓTIPO de supervisão nativa de filho próprio**. Rollback de código por revert local revisado apenas83, depois de request_stop e espera da mesma completion das execuções próprias. Sem configuração pessoal, recibo/Recovery novo ou arquivo para restaurar. Nenhum push/deploy/release/instalador ou intervenção humana necessária; produtor de dedicado continua próximo trabalho local autorizado.

## Integrações e estados preservados

Produtor dedicado verdadeiro ainda precisa: selecionar e validar cópia diagnóstica física/proveniência, locks/journals, artefatos Java/Minecraft/Fabric fixados, plano/digest/recheck, ambiente mínimo, papel dedicado, consentimento EULA já recebido e restauração exata após reap. Node não transfere sua autoridade;73 não pode ser servidor disfarçado. Não manter pins que proíbem reescrita dos arquivos mutáveis durante boot; isso terá contrato próprio. UDP mapped observado historicamente no Java continua recusado82, sem normalização silenciosa ou flagJava para obter PASS.

Vínculo de voz vivo, pins/leitura de configuração em execução, mesmo game/TCP e aux/UDP, monitor que retira81, Guest local35554 ocupando uma das oito vagas e integração no caller/UI continuam **NÃO IMPLEMENTADO** nesse ciclo. Áudio SVC e Plasmo **PRECISA DE TESTE**, separadamente. VoiceMutationGuard segue exigindo Java ausente para qualquer mutação; recibos/restauração/aposentadoria e Recovery/CAS/Doctor independentes.

Resultado máximo83: **PROTÓTIPO de supervisão nativa de filho próprio**, não produtor dedicado integrado ou homologação de Minecraft/voz/GUI/redes. TCP/UDP/convites atuais preservados; nenhuma nova topologia, relay público ou matriz. Assistente desktop e nove pares permanecem. Nenhuma intervenção humana necessária neste recorte local.

Rollback somente por revert local revisado do incremento, depois de solicitar stop/aguardar completion das execuções83 próprias. Sem reset/clean, kill por nome/PID, configuração pessoal ou restauração automática de voz. Push/deploy/release/instalador/custo e testes entre computadores continuam fora deste recorte.
