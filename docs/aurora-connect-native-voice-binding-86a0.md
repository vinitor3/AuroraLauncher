# Connect86A0 — origem nativa e reserva antes da execução

Atualização: 24/10/2026. Estado: **PROTÓTIPO**, privado, Windows, `connect-lab` + `connect-voice-lab` desligadas por padrão. Implementa somente a primeira subdivisão do [contrato86 aprovado](aurora-connect-native-voice-host-binding.md), sobre `208edc9`. Não abre Host, emite convite, publica anúncio, altera configuração ou comprova áudio. O aceite integral86A/B continua pendente.

## Implementação e fronteiras

`DedicatedRun::native_voice_host_binding` só constrói `NativeVoiceHostBinding` a partir do mesmo proprietário Ready com contexto VoiceV2 original. O binding conserva `Weak<Shared>`, nonce, perfil e prazo monotônico originais. Não conserva Child, arquivo, prova, guard, completion ou autoridade de stop. Clonar o binding não mantém a execução viva; uma amostra favorável ou campos serializados não podem construí-lo.

`reserve_tcp` e `reserve_voice` reservam o **mesmo QueryJob84 e slot compartilhado** antes de entregar tokens consumíveis `ReservedTcpQuery`/`ReservedVoiceQuery`. `execute(self)` executa a consulta e mantém a decisão final de lifecycle, inclusive para erro de voz. Descartar um token que não foi despachado libera o job real. As APIs síncronas anteriores usam reserva seguida de execução, sem outro slot, fila, observer ou cache.

O construtor não executa I/O nem aprova a configuração atual: uma alteração de voz ainda pode ser detectada pela consulta posterior, enquanto TCP permanece saudável. Stop/cancelamento/prazo continuam selando a origem; a conclusão e restauração EULA originais aguardam o job real, mesmo quando o filho já foi recolhido. Nenhum dispatcher ou integração Host existe neste incremento: testar Drop do token não equivale a provocar falha de criação de thread no sistema.

Arquivos: `producer.rs`, `producer_voice.rs`, novo `producer_binding.rs`, registro em `tests.rs` e novo `tests/native_voice_binding.rs`, sob `apps/desktop/src-tauri/src/engine/connect_dedicated/`. Recipes, preimagensV1/V2, supervisão83, protocolo, DTOs e interface permanecem sob os contratos anteriores.

## Evidência executada

Todos os comandos abaixo foram executados da raiz desta worktree, offline/locked e Cargo serializado pelo root. As fixtures são processos próprios do executável de teste, com TCP/STATUS e UDP reais; não são novas execuções de Minecraft/Java ou áudio dos mods. Ambos os perfis são exercitados separadamente nos loops dos mesmos pais.

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --release --all-features --lib engine::connect_voice::dedicated::tests::native_voice_binding:: -- --test-threads=1
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --release --all-features --lib engine::connect_voice::dedicated::tests::voice_runtime:: -- --test-threads=1
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --release --all-features --lib engine::connect_voice::dedicated::tests::producer_lifecycle::a_reserved_positive_source_query_blocks_restore_but_not_child_cleanup -- --exact
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --release --all-features --lib engine::connect_voice::dedicated::tests::v1_upgrade_capture::old_persisted_v1_goldens_roundtrip_without_rewriting -- --ignored --exact
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-targets --all-features -- -D warnings
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-targets -- -D warnings
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-targets --features connect-lab -- -D warnings
```

| Prova | Resultado e limite |
| --- | --- |
| Sete pais novos86A0 | **7 PASS / 0 falhas / 0 ignorados**, 175,77s; compile release3m41s. Construção Ready/V1/selada, fontes reais dos dois perfis, slot único/Busy, Drop sem dispatch, gate final, waiter perdido, EULA/completion retidas e binding fraco sem adoção de nova execução. |
| Regressão de seis pais85C1 | Primeira rodada **2 PASS / 4 falhas**, 73,65s. Primeiro erro de rebind43001 `AddrInUse/10048`, seguido de três deadlines Ready. O ensaio gráfico com echo também usava43001; o log não identifica o proprietário no instante da falha. Resultado preservado, sem mudança de código, porta ou timeout. |
| Repetição isolada dos mesmos seis pais | Após encerrar GUI Connect/CLIs/echo e confirmar rebind TCP43001/UDP43002/43003, **6 PASS / 0 falhas / 0 ignorados**, 150,11s. Apoia a hipótese de contenção operacional; não apaga a primeira rodada. Não executar esses testes junto de fixtures GUI/Java que usam43001. |
| Lifecycle TCP anterior | **1 PASS**, 4,91s; consulta positiva retida impede restore/completion, sem impedir recolhimento do filho. |
| GoldensV1 antigos | **1 pai PASS**, 0,07s, conferindo dez goldens persistidos byte-exatos. Nenhuma recaptura ou consumo dos prefixos físicos já usados. |
| fmt / Clippy | **PASS**. Clippy all-features36,35s, default2,53s e connect-only1,63s. |

Logs privados preservados em `.aurora/connect-validation/native-binding-86a0-{focused-first,runtime-regression,runtime-isolated,tcp-lifecycle,v1-goldens,fmt-final,clippy-all-first,clippy-default,clippy-connect}.log`. A primeira conferência fmt foi silenciosa sem arquivo de log; a conferência final executada novamente preservou log vazio com exit0, sem reconstruir uma evidência retroativa. Freeze dos cinco fontes: `native-binding-86a0-source-freeze.json`, SHA256 `1ee40fe1eaf39547f983cf250a0aae6ecffe683e0ed1056f5f9b8e9c304afa46`; contrato aprovado SHA256 `3f5634590413502db006fe87a9812c8decb30681c664bb70d0d557b3bf7c44e4`. Root e QA independente revisaram os mesmos bytes; nenhuma alteração Rust foi feita para repetir a regressão.

## Limites e próximo incremento

TCP/UDP aqui são consultas da origem própria e testes locais; as pontes existentes conservam suas evidências separadas. Direto externo, relay, convites/interface, login/mundo Minecraft, Simple Voice Chat e Plasmo Voice não recebem novo PASS por86A0. Os ensaios Java85D/E anteriores e os ensaios GUI TCP24/10 pertencem a registros próprios. Não houve dependência/lockfile/matriz, configuração pessoal, faturamento, firewall, push, deploy, release ou instalador.

Faltam revisão/confirmação86, integração Host/Guests, monitor e retirada terminalUDP por lease, com todas as corridas/owners previstos no contrato. Um recorte utilizável não pode continuar encaminhando voz depois de invalidar sua origem. Não expor esse binding na UI nem promover uma observação positiva isolada a autorização de serviço.

Rollback de código reverte este incremento local revisado. Operacionalmente, descartar tokens não despachados ou aguardar os executados conserva o proprietário original responsável pelo stop, recolhimento e restauração. Não recuperar a cópia histórica NeedsRecovery por consequência nem reutilizar uma execução encerrada.
