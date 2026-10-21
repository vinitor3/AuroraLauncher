# Observação do caminho no desktop — contrato80

Data21/10/2026. **IMPLEMENTADO MAS NÃO HOMOLOGADO**, provas locais e revisão independente aprovadas. Contrato aprovado antes do código; integra a [observação78](aurora-connect-path-observation.md), validada e registrada em `5c7c51a903c7641a3f86ccec49d08a3fa9116102`, preservando o cleanup79 `a279447a68ec180d267bda45f0799545a4908487`. A proposta anterior numerada79 foi renumerada80 antes de implementar. Provas da biblioteca, da liberação do Guest e da interface são separadas; não atribuir umas às outras.

## Resultado pretendido

Antes80, o painel experimental apresentava `pathState: "notVerified"` e o texto fixo “Não confirmado · relay desligado”. Agora integra uma observação opcional do caminho selecionado pelo transporte autenticado, apoiada nas provas78. O endereço inicial de um convite não determina esse resultado.

Conservar `pathState: "notVerified"` e `relayEnabled: false` para os contratos existentes. A observação acrescenta somente `pathObservation?: {schemaVersion: 1, state: "unknown" | "direct" | "relay" | "closed"}`. Não inferir habilitação de relay pelo resultado: `relayEnabled` continua descrevendo o recurso oferecido pelo caller, enquanto uma observação descreve um caminho do transporte. O desktop atual continua sem um caller para relay público ou fixture relay50.

Não acrescentar capability apenas para apresentar um campo opcional de saída: a observação não concede um serviço, comando, endereço ou permissão. Um backend anterior omite o campo; o frontend novo conserva o texto de ausência de confirmação. Feature `connect-lab` continua desligada por padrão. Caso uma mudança posterior introduza seleção de relay, outro comando ou nova autoridade, deverá ter seu próprio contrato e capability; esta integração não a autoriza.

## Origem e fases

Usar exclusivamente os métodos78 da instância original de `Guest` ou `Host`, sem nova conexão, stream, watcher, probe, tarefa ou cache de resultado. Não expor `Connection`, endereço, família IP, URL, peer, RTT, estatística ou motivo remoto. A posse dos objetos continua no ator e em seus recursos já existentes.

A correção79 muda `GuestSource` para uma referência fraca ao Guest mantido pelo ator. Para observar, fazer upgrade somente durante a consulta síncrona curta e soltá-lo antes do retorno. Não guardar esse Arc no snapshot, manager, fila, worker ou gate. A integração80 deve preservar a prova79 de rebind com manager e trabalhos de abertura ainda vivos; observar caminho não pode reintroduzir retenção do Endpoint.

- Guest individual: a instância original já retida por `ready_game::GuestSource` fornece a observação durante `guestConnected`, com a mesma operação e seu cancelamento original. Esse acesso é independente de autorizar QuickPlay: não chamar `address()` para converter a observação em uma autorização de jogo ou renovar seu prazo. O método78 aplica seus próprios gates temporais e de fechamento. Sem fonte original, omitir o campo.
- Host individual: durante `hostListening`, consultar somente a lease retida em `Attempt.host`, usando `Host::invitation_path_observation`. `None` omite o campo; nunca transformá-lo em uma observação de outro peer. A consulta não consome nem reemite convite.
- Host de grupo: a observação pertence a cada `GroupParticipant`, pela sua própria lease. O snapshot global não recebe uma agregação de caminhos. `direct` para A e `relay` para B, se alguma API futura permitir essa combinação, não produzem uma falsa classificação do grupo inteiro.
- Preparação, revisão, partida anterior à publicação e qualquer fase terminal ou de fechamento: omitir observações novas e remover a projeção opcional anterior. Não preservar uma última amostra direta como se ainda fosse atual. O texto de encerramento pode usar o estado de transporte já existente.

As observações não integram convite, revisão, digest, preparação ou exportação: esses objetos são decisões de autorização, enquanto o caminho pode mudar. `GroupReview` conserva seus participantes sem a extensão opcional. `GroupParticipant` no snapshot pode recebê-la somente depois da publicação. A igualdade de identidade entre revisão e snapshot ignora a observação; não ignora nenhum campo de autoridade existente.

## Consistência e interface

No grupo, manter relógio antes da leitura cumulativa de consumo, e conservar a revogação individual. Uma observação só pode aparecer para participante consumido, não revogado e com sessão válida, no grupo ainda `hostListening`. Estados `direct`, `relay` e `unknown` exigem conexão individual observada ativa; `closed` exige transporte individual inativo. Fazer amostras e revalidar esses gates conservadoramente; se houver desencontro entre duas leituras, omitir a observação naquele snapshot. Nenhuma amostra deve reativar `transportActive` nem substituir o relógio por um booleano da interface.

Em snapshot individual, `direct`, `relay` e `unknown` só são aceitos nas fases vivas acima. `closed` pode aparecer nessas mesmas fases quando o fechamento do transporte foi observado antes que o ator conclua sua transição. A observação não altera fase, transporte, presença Minecraft ou prontidão TCP/UDP. A consulta é uma amostra transitória; fechamento remoto pode ocorrer logo depois dela.

Atualizar a geração apenas quando a projeção efetiva mudar, usando a regra existente do snapshot. Uma mudança de caminho não autoriza repetir preparo, revisão, consentimento, recuperação UDP ou abertura do jogo. Resposta de outra operação/geração não substitui a sessão atual; permanecem as barreiras de conta, stop e IPC perdido. Falta ou invalidade da observação não deve esconder os controles de encerramento.

Aplicar a projeção atual também às respostas de comandos que devolvem `ConnectSnapshot`, não apenas à consulta periódica do manager. Um helper privado comum pode amostrar grupo/individual e clonar a resposta depois dos gates próprios do comando, preservando a idempotência e a validação original de geração da recuperação UDP. A última projeção no DTO serve somente à comparação de mudanças; todo retorno deve revalidá-la, sem usar amostra antiga como autoridade. Não duplicar consultas do grupo no mesmo retorno nem alterar revisões/digests por uma mudança de caminho.

O frontend valida o objeto de observação com exatamente duas chaves, schema1 e os quatro estados conhecidos, rejeitando números, chaves adicionais e estados incoerentes com a fase. Atualizar também as listas fechadas de `group-types.ts` e `host-source-types.ts`; fases pendentes não passam a aceitar o campo por conveniência. Objetos antigos sem o campo continuam válidos. Não aceitar extensão em revisão/exportação, nem aceitar `relayEnabled: true` como consequência deste contrato.

Texto proposto: “Caminho direto observado”, “Relay observado”, “Não confirmado” ou “Conexão encerrada”. Explicar uma vez que o caminho pode mudar e não comprova entrada no mundo. No grupo, mostrar ao lado do participante correspondente. Conservar separadas consulta do servidor, entrada no mundo e estados dos dois mods de voz. Sem diagnóstico de NAT/CGNAT, promessa de ping, homologação externa ou aprovação de operador público.

## Arquivos e validação previstos

Nativo: `connect/mod.rs` para o campo opcional, `live.rs` e `ready_game.rs` para a fonte Guest/Host individual, `group_profile.rs` e `session_group.rs` para a projeção por integrante; helper separado é preferível para manter o código de apresentação concentrado. Feature ausente não deve exigir Tokio ou a dependência opcional do laboratório.

Frontend: `types.ts`, guard próprio da observação, `group-types.ts`, `host-source-types.ts`, `ConnectView.tsx` e `GroupHostPanel.tsx`. Preservar escopo do Ambiente de teste e o fluxo sem configuração pessoal.

Provas a executar somente após78: Guests e Host reais locais com trânsito TCP; amostra individual e duas leases diferentes; revogação A preservando B; ingressos consumidos com sessão ainda válida; término/stop sem amostra ativa residual; ausência do campo nos contratos anteriores; parsing fechado e erro de projeção sem ocultar cleanup; resposta tardia após stop/conta/remount; texto DOM sem URLs/peers novos; build, fmt/clippy e regressões proporcionais de ambas as features. Fixture relay78 não comprova integração relay no desktop, porque esse caller não existe.

Na escrita inicial do contrato, somente auditoria e definição; nenhuma produção80 ou teste foi antecipado como resultado. A implementação e os comandos realmente executados estão registrados abaixo. Não atribuir esses resultados aos incrementos77/78/79.

## Evidências locais80

Frontend implementado com guard fechado próprio, compatibilidade com ausência do campo e amostra individual/por participante. Rodadas do mesmo comando em `apps/desktop`:

```powershell
node --test src/features/connect/path-observation-types.test.mjs src/features/connect/ConnectView.smoke.mjs src/features/connect/fragmented-types.test.mjs src/features/connect-game-launch/ReadyGameLaunch.smoke.mjs src/features/connect/account-transition.smoke.mjs
```

Primeira rodada **253 PASS/0 falhas/0 ignorados**,7506,8202ms; segunda **255 PASS/0 falhas/0 ignorados**,3116,874ms; final **255 PASS/0 falhas/0 ignorados**,2954,5074ms. Logs privados `.aurora/connect-validation/desktop-path-80-ui-{first,second,third}.log`. Não somar rodadas sobrepostas. QA estática após a primeira rodada encontrou que falha/perda da resposta de Stop podia reapresentar a amostra anterior após terminar pending. Corrigido aviso desde o início do Stop e reiterado no catch da operação atual; somente snapshot válido recém-aceito remove o aviso. Dois smokes adicionais cobrem individual/grupo, poll anterior resolvendo durante Stop, erro, refresh pendente/erro e posterior confirmação válida. Nenhuma dessas rodadas apresentou falha de teste; o achado veio da revisão.

`npm run build` em `apps/desktop`: **PASS**,372 módulos,11387ms total/1,47s Vite (`desktop-path-80-ui-build.log`). Aviso preexistente de bundle maior que500kB preservado. DOM e build não são homologação da interface gráfica.

Primeiro foco nativo:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-features --lib desktop_path_ -- --test-threads=1
```

**4 PASS/0 falhas/0 ignorados**,2,45s/compilação57,00s, exit0 (`desktop-path-observation-80-first.log`). Serialização/default-off, Host/Guest reais locais, STATUS, liberação de portas, duas leases originais/revogação independente e resposta da recuperação UDP amostrada depois do gate original de geração, com idempotência preservada. QA posterior pediu precisão de geração: quando cancelamento já está sinalizado e fase ainda não transitou, refresh do grupo não deve acrescentar amostra para removê-la na mesma resposta. Gate e regressão causal serão registrados após execução. Regressão final/fmt/clippy ainda pendentes neste checkpoint.

Precisão aplicada: refresh do grupo não amostra caminhos depois de cancelamento, preservando seus metadados e autoridades. Quinto teste usa runtime current_thread, dois Guests reais previamente observados direct e State mutex. Cancela o token original e chama duas projeções síncronas sem await/yield; o ator não consegue revogar o Host antes das leituras. A primeira remove as amostras e a segunda conserva a mesma geração, ainda em hostListening; depois stop e rebind reais. Isso evita que a prova passe por corrida com o ator. Mesmo comando focal: **5 PASS/0 falhas/0 ignorados**,2,66s/compilação41,23s, exit0 (`desktop-path-observation-80-final-focus.log`). QA independente aprovou gate e causalidade; nenhuma falha de teste foi observada80 até aqui.

Regressão executada: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-features --all-targets -- --test-threads=1` passou **495 PASS/0 falhas/63 ignorados**, oito targets,253,17s somados de testes/compilação1m47s (`desktop-path-observation-80-all-full.log`). Inclui as provas anteriores79 de liberação com manager/worker vivos; a observação não reintroduziu retenção do Endpoint. Ignorados não são promovidos a provas e focais sobrepostos não são somados. Suíte padrão e checagens estáticas continuam em andamento neste checkpoint.

## Limites e recuperação

Suíte padrão executada com o mesmo comando de regressão sem `--all-features`: **290 PASS/0 falhas/5 ignorados**, oito targets,68,58s somados de testes/compilação58,20s (`desktop-path-observation-80-default-full.log`). O novo DTO/serializador funciona sem a dependência opcional de transporte.

`cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all -- --check` PASS. `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-features --all-targets -- -D warnings` PASS18,10s; sem `--all-features` PASS10,32s; substituindo por `--features connect-lab` PASS16,09s. Logs `desktop-path-observation-80-{fmt,clippy-all,clippy-default,clippy-connect}.log`. Todos exit0. Root foi o único executor Cargo, com fontes congeladas; revisão independente da produção, interface e causalidade sem bloqueador. Locks e matriz mantêm os hashes anteriores; nenhuma configuração pessoal foi alterada.

Resultado continua **IMPLEMENTADO MAS NÃO HOMOLOGADO** até provas de interface real e runtime pertinentes. TCP/UDP preservam as evidências anteriores; os testes80 acrescentam STATUS sintético/isolamento/liberação local e apresentação redigida. Direto observado neste PC não é nova topologia externa. Relay público desligado e sem caller desktop; a classificação relay da biblioteca78 não comprova interface de relay. Minecraft sem Java/mundo novo; convites/interface têm DTO/DOM/build, sem nova prova gráfica ou dos convites entre internets. Simple Voice Chat e Plasmo Voice separadamente permanecem sem áudio novo. Nove pares, Assistente, gates/fases e R3 preservados.

Não há nova configuração, arquivo pessoal, processo, serviço externo ou recurso para restaurar. Rollback operacional encerra Connect e aguarda seus workers, preservando jogo registrado e recibos. Reversão local revisada retira os campos opcionais e sua apresentação, preservando transporte, cleanup, gates, recibos de voz e mudanças posteriores; sem reset/clean, push, deploy, release ou instalador. Custos permanecem zero.

## O que você precisa fazer agora

Nenhuma intervenção humana é necessária para implementar e testar essa integração local. Ensaios entre computadores e áudio real permanecem etapas independentes.
