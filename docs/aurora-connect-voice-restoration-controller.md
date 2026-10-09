# Aurora Connect — controlador de restauração de anúncio

Incremento58, base033dbea. Estado: **PROTÓTIPO**. Esta fundação Rust conserva a autorização de restauração entre chamadas; não acrescenta comandos Tauri, interface, mutação de configuração pessoal, transporte ou voz habilitada. Contratos preservados: [adaptadores de voz](aurora-connect-voice.md), [Recuperação](instance-recovery.md), [cópias de diagnóstico](diagnostic-lab.md), [ADR-027](engineering/ADR/ADR-027-manual-instance-recovery.md) e [ADR-028](engineering/ADR/ADR-028-doctor-recovery-lab.md).

## Bloqueio e contrato

`PreparedRestoration<'a>` mantém um empréstimo mutável de `RestorationReceipt`. Esse contrato funciona no supervisor síncrono existente, mas não permite armazenar diretamente o plano e o recibo separados entre chamadas desktop. Recriar o plano na confirmação alteraria identidade da revisão, fingerprint, bytes e prazo; serializar o recibo entregaria autoridade indevida ao frontend.

`RestorationReceipt::into_controller(self)` transfere o recibo para `RestorationController`. A posse permanece exclusiva e em memória: não há `Clone`, `Debug`, serde, construtor público alternativo ou exportação de bytes. As APIs emprestadas anteriores continuam disponíveis e inalteradas.

- `prepare_strict(instance, guard)` chama a preparação estrita existente e conserva o mesmo `PreparedAnnouncement` produzido. Devolve somente uma cópia da revisão para apresentação.
- `prepare_reconciled(instance, guard, expected_current_sha256)` é uma ação explícita distinta. O hash atual é pré-condição; não autoriza restaurar nem seleciona esse modo após falha estrita.
- Ambas recusam uma revisão já pendente antes de consultar processos ou arquivos. Recibos `NeedsRecovery` e `Restored` também não preparam outra revisão.
- `confirm(instance, guard, approved_digest)` retira a única revisão pendente e delega à confirmação emprestada existente, sem gerar plano, fingerprint, bytes ou prazo novos. Cada tentativa de confirmação consome essa revisão, inclusive digest incorreto, expiração ou recusa de contexto.
- `cancel_review()` apenas descarta a revisão pendente. `state()` e `has_pending_review()` expõem estado limitado; não fazem I/O, não validam disponibilidade runtime e não renovam prazo. Uma revisão expirada permanece pendente até cancelamento ou confirmação recusada.

Digest incorreto, prazo expirado e falhas anteriores ao executor conservam o recibo `Ready`, permitindo nova preparação e aprovação explícitas. O início do executor passa a `NeedsRecovery`; falha subsequente exige a Recuperação existente. Sucesso passa a `Restored`. Cancelar não retira `NeedsRecovery` nem reativa recibo consumido. O clone de revisão não é usado como fonte de bytes ou autorização editável.

O controlador não mantém `VoiceMutationGuard`. O chamador pode liberar o guard após preparação/aplicação e adquirir outro antes da confirmação/restauração. A API exige o guard opaco real e mantém todas as rechecagens de processo, lock, identidade, runtime e inventário. A confirmação continua síncrona: cancelamento antes de executar não interrompe uma transação iniciada. Nenhum construtor ou probe de teste torna-se público.

## Escopo e recuperação

Conservam-se os adaptadores SVC2.6.22 e Plasmo2.1.17, artefatos fixados, Fabric1.21.1, cópias de diagnóstico verificadas e modo dedicado somente Aurora. Anúncio, porta/bind efetivo, ponte de transporte e áudio são evidências distintas. O controlador não seleciona mod, destino, porta, transporte, perfil de convite, orçamento UDP ou duração da sessão.

Drop/cancelamento não escrevem arquivos, não restauram automaticamente, não fecham TCP e não encerram Java. Falha UDP não concede aprovação para mutação ou restauração. O recibo e a revisão desaparecem ao encerrar/crashar o processo; o journal existente cobre transações interrompidas, mas não fornece restauração seletiva automática depois do reinício. Essa limitação precisa ser exposta por qualquer caller desktop futuro.

## Verificação executada

Testes novos em `engine/connect_voice_restore_controller_tests.rs` usam duas cópias temporárias independentes por cenário quando necessário, JARs locais fixados e probes privados controlados. Não baixam nem executam Java. Testes de filesystem ficam opt-in e exigem `AURORA_CONNECT_ARTIFACT_DIR` explícito; não usar `--include-ignored` na suíte geral.

Cobertura executada: escrita/restauração exatas, recibo movível sem empréstimo de guard, liberação/retomada do lock, revisão concorrente recusada antes de I/O, clone de apresentação sem autoridade, cancelamento sem novo ponto, digest antigo/incorreto, expiração monotônica sem alterar relógio do Windows, edição alheia antes/depois da revisão, hash atual incorreto, conflito de chave, cópia/runtime divergentes, reconciliação explícita com bloco Plasmo existente/ausente, falhas de processo antes/depois do executor, falha própria no staging, replay, origem intacta e Drop sem restauração. Cleanup canonicaliza o diretório temporário e sua raiz; exige filho direto com nome UUID exato antes da exclusão. Consulta posterior encontrou zero diretórios remanescentes com o prefixo exclusivo dessas fixtures.

Comandos efetivamente executados na raiz da worktree, usando o target desktop padrão. O filtro foi listado antes da execução; nenhum helper interativo/Java entrou na seleção:

```powershell
$env:AURORA_CONNECT_ARTIFACT_DIR = (Resolve-Path .aurora/connect-voice-artifacts).Path
cargo test --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --features connect-voice-lab --lib engine::connect_voice::restoration::controller::tests -- --list
cargo test --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --features connect-voice-lab --lib engine::connect_voice::restoration::controller::tests -- --include-ignored --test-threads=1
cargo check --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets
cargo check --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --features connect-voice-lab
cargo check --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --all-features
cargo clippy --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
cargo clippy --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --features connect-voice-lab -- -D warnings
cargo clippy --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all -- --check
cargo test --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --all-features -- --test-threads=1
```

| Verificação | Resultado | Log privado em `.aurora/connect-validation/` |
| --- | --- | --- |
| Compilação e listagem focal | PASS,20,77s; somente oito nomes novos | `voice58-focused-list.log` |
| Foco completo do controlador | **8 PASS,0FAIL,0ignored**,221,19s; sete testes de filesystem com ambos os mods, um contrato de posse `Send+'static` | `voice58-focused-1.log` |
| Check todos os targets: padrão / voz / combinado | PASS,3,55s /12,07s /4,75s | `voice58-check-{default,voice,combined}.log` |
| Clippy todos os targets: padrão / voz / combinado | PASS,1,00s /10,92s /5,63s | `voice58-clippy-{default,voice,combined}.log` |
| Formatação | PASS | `voice58-fmt.log` |
| Suíte combinada sem opt-ins históricos | **325 PASS,0FAIL,28ignored**,118,76s somados nos testes; compilação1m31s | `voice58-combined-full.log` |

A suíte combinada teve217PASS/26ignored na biblioteca,42PASS/1ignored no binário e66PASS/1ignored nas integrações. Sete dos28ignored são os novos casos de filesystem, já executados com sucesso no foco8/8. Os outros21 opt-ins históricos permanecem fora da rodada: artefatos/probes/writer/Java preparados separadamente, serviços externos, instância pessoal explícita e helper interno de descoberta chamado somente pela fixture dona. Não há uma nova homologação desses opt-ins. A soma118,76s é a soma dos tempos de testes reportados, não duração total de parede. Não houve falha de compilação/teste58 nem correção após a execução focal.

Os JARs locais foram reconferidos: SVC5369475B/SHA256 `972a01592f670aa57f2dc2e3587a656b798106037468c61e4bfb49eba7d6ae36`; Plasmo12365966B/SHA256 `46e1a4ae51786457712991ba05bf573e2d11331aeadb63b7d108839def022a2b`. Nenhum bytecode foi executado. A cópia imutável do helper53 em `native53-reviewed/aurora_launcher_core-977e96f36303eeb7.exe` permaneceu SHA256 `1cf119159312e99f47e06a9db01341bbb9b98074102b7a270257b57b3d41fb7c`; compilar testes no target padrão não repina esse artefato. Não houve mudança de dependência/lockfile, download, configuração pessoal ou relógio do Windows.

Revisão independente do código e do foco8/8 não identificou bloqueadores. `git diff --check` dos cinco arquivos do incremento passou. Snapshot privado `voice58-source-freeze.json` registra base e SHA256 dos quatro fontes Rust e deste domínio; não publica binário nem altera pins de distribuição. Os quatro fontes são `connect_voice.rs` (reexport), `connect_voice_restore.rs` (submódulo privado e transferência consumível), `connect_voice_restore_controller.rs` e `connect_voice_restore_controller_tests.rs`.

## Rollback e próximos gates

Antes da confirmação, cancelar a revisão ou descartar o controlador não altera disco. Depois de uma aplicação/restauração, usar nova revisão apropriada ou a Recuperação existente se o recibo estiver em `NeedsRecovery`; não remover CAS/journals para tentar novamente. Rollback de código é um revert local revisado somente deste incremento, preservando APIs emprestadas e demais funcionalidades.

Caller Tauri/UI, associação segura à cópia/geração/janela, inspeção limitada de configuração e estados de recuperação continuam pendentes. TCP, UDP, direto, relay, Minecraft, convites/interface, Simple Voice Chat e Plasmo Voice conservam suas evidências anteriores; áudio de cada mod permanece **PRECISA DE TESTE**. Não há alteração da matriz, Core/Skins, Assistente, política de custo ou autorização de release/instalador.
