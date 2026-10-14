# Sessão TCP+UDP no painel — incremento62

Data: 14/10/2026. Estado: **PROTÓTIPO**, feature `connect-lab` desligada por padrão. Integra o [contrato61](aurora-connect-fragmented-session.md) no caller e painel existentes, preservando os perfis curto, sessão TCP56 e UDP curto57.

## Seleção, revisão e duração

Extensão64: a seleção manual abaixo permanece válida. Um comando distinto pode consumir a seleção opaca48 para game/TCP, mantendo UDP manual e monitorando a origem. [Contrato e limites64](aurora-connect-discovered-session-udp.md).

O perfil explícito `sessionFragmentedGameUdp` exige dois destinos manuais loopback, game/TCP e aux/UDP. Os comandos novos são `connect_prepare_session_fragmented_host` com `{mode,bindAddress,gameTarget,udpTarget,admissionSeconds,sessionSeconds}` e `connect_prepare_session_fragmented_guest` com `{mode,expectedHostPublicId}`. A janela principal é o proprietário; o frontend não recebe chave privada nem autoridade de consumo. Não existe descoberta UDP neste perfil.

Ingresso de1–600s e sessão até3600s desde emissão, com sessão pelo menos tão longa quanto ingresso. O desktop aceita somente segundos inteiros exatos, inclusive ao verificar o intervalo assinado. A preparação e a revisão conservam o prazo original de60s e vencem antes se o ingresso vencer. Repetir revisão não renova tempo. Criar ou confirmar exige seleção explícita de perfil: nenhum erro aciona outro parser.

O convite usa schema3 e UDP envelopeV2. Revisão/digest incluem os dois prazos, destinos privados selecionados, serviços, limites e o endereço local do convidado. O host exporta somente durante ingresso ainda não consumido. Após admissão, TCP e associações UDP podem continuar até o prazo original da sessão; sem admissão, o Host encerra ao vencer o ingresso. Cancelamento, revogação e perda do transporte conservam cleanup aguardado.

## Contratos expostos

`capabilities.sessionFragmentedUdpInvitations` é omitido quando a feature está desligada. Quando disponível, contém `profile=sessionFragmentedGameUdp`, `invitationSchemaVersion=3`, `udpEnvelopeVersion=2`, `maxAdmissionSeconds=600`, `maxSessionSeconds=3600`, original4096B/frame1264B, listenerUDP`127.0.0.1:35554` e retorno autorizado16384B/s/20fragmentos/s. O campo anterior `fragmentedUdpInvitations` continua com schema1/prazo120s.

Preparation/Snapshot identificam o perfil desde o preparo. Review/Export/Snapshot revisados carregam `sessionTiming` com perfil/schema3/ambos os vencimentos e `fragmentedUdp` com perfil/schema3/contrato UDP. Só a revisão do anfitrião contém `udpTarget`; exportação não expõe destino privado. Os aliases anteriores mantêm seu significado: `review.lifetimeSeconds` e `export.expiresAtMs` representam ingresso, `snapshot.expiresAtMs` ativo representa fim da sessão.

Os parsers da interface conferem consistência entre perfil, schema, duração, serviços e metadados em cada superfície. Eles não aceitam combinar prazo schema2 com UDP schema1 ou omitir parte do contrato novo. Confirmação continua vinculada à revisão vigente e ao digest; respostas antigas não devem substituir uma operação nova.

## UDP e jogo independentes

O Guest publica TCP assim que pronto e prepara UDP separadamente. `available` exige bind35554 e associação inicial negociada para4096B. Porta ocupada, erro de abertura ou capacidade insuficiente mantém o jogo TCP e apresenta falha apenas de UDP. Não procura porta alternativa, renova autorização ou tenta relay. O Host não ganha uma ponte de convidado nem um anúncio de voz por selecionar o perfil.

Setup e teardown UDP pendentes usam o helper57: observa token, prazo original, verificação civil e conexão; encerra o Guest quando necessário e recolhe a mesma future. Stop/logout/saída aguardam recursos. `Drop` apenas solicita limpeza eventual com runtime vivo. Não há restauração/configuração de voz associada automaticamente ao transporte.

## Validação

Foco nativo: **6 PASS,0 falhas/ignorados**,191filtrados,8,60s e1m07s de compilação. Log privado `desktop-session-udp-62-focused.log`. Dois managers reais e destinos loopback verificam metadados/defaults/DTO, quatro perfis incompatíveis, ausência de I/O antes de autorização, TCP e duas origens UDP após ingresso, expiração absoluta apesar de atividade, revogação, ingresso sem consumo e cleanup/rebind. Porta35554 ocupada falha somente UDP; liberar a porta não provoca tentativa automática e TCP permanece utilizável depois do ingresso.

Interface: **60 PASS,0 falhas/ignorados**,1486,1401ms, incluindo48 regressões e12 casos novos; log `session-udp-62-frontend-first.log`. Parsers recusam perfil/schema/prazos/capacidades/exportações cruzados; DOM verifica seleção, comandos manuais, revisão, dois vencimentos, retorno ao painel, respostas obsoletas, cancelamento e erro UDP separado. QA encontrou e o autor corrigiu descrição combinando perfil encerrado e nova seleção; regressão incluída. Revisão independente final sem bloqueador restante. Fixtures DOM/IPC não substituem janela Tauri.

Build TypeScript/Vite passou, com aviso anterior de chunk>500kB, em `desktop-session-udp-62-build.log`. Não gera instalador. Regressão da interface de anúncio60:33 PASS/0falhas/0ignorados,1213,9532ms (`desktop-session-udp-62-voice-ui.log`).

Regressões Rust completas: **265 PASS/5 ignorados padrão** e **345 PASS/42 ignorados combinado**, zero falhas, nove grupos por execução,45,00s/138,81s somados de testes e42,56s/1m59s de compilação. Logs `desktop-session-udp-62-full-{default,combined}.log`. Incluem recovery/CAS/Doctor; opt-ins históricos de JAR/processo/rede não foram ativados. Contagens sobrepostas/focais não devem ser somadas. Clippy de todos os targets passou em padrão, Connect e combinado (`desktop-session-udp-62-clippy-{default,connect,combined}.log`), assim como fmt e diff finais. Locks, matriz e pins imutáveis53 permanecem iguais.

Comandos realmente executados a partir da worktree:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --features connect-lab --lib engine::connect::live::session_fragmented_tests -- --test-threads=1
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-features -- --test-threads=1
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 -- --test-threads=1
node --test apps/desktop/src/features/connect/ConnectView.smoke.mjs apps/desktop/src/features/connect/fragmented-types.test.mjs
node --test apps/desktop/src/features/voice-announcement/VoiceAnnouncement.smoke.mjs
npm --prefix apps/desktop run build
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all -- --check
cargo fmt --manifest-path apps/connect-lab/Cargo.toml --all -- --check
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-targets -- -D warnings
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-targets --features connect-lab -- -D warnings
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-targets --all-features -- -D warnings
git diff --check
```

O comando Node foi executado pelo responsável da interface; logs e resultado conferidos pelo integrador. Nenhum Java, processo externo do usuário, configuração pessoal ou teste entre redes foi iniciado.

## Limites e rollback

Nenhum Java, mod ou configuração pessoal é alterado neste incremento. Datagrama íntegro e sessão longa possível não comprovam áudio ou estabilidade prolongada. Minecraft, SVC e Plasmo mantêm suas evidências anteriores e validações separadas. Redes externas, contas mistas e os nove pares Core/Skins não ganham homologação por associação. Relay permanece desligado.

Rollback operacional: Encerrar Connect e aguardar `cleanupState=complete`. Código: revert local revisado do incremento62, preservando61 e todos os perfis anteriores. Build sem a feature conserva Connect indisponível. Sem push, deploy, publicação, instalador ou custo.
