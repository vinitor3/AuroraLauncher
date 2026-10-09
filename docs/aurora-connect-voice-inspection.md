# Aurora Connect — inspeção nativa de anúncio de voz

## Estado e alcance — incremento 59

**PROTÓTIPO**, base `2e190bf`; testes locais concluídos abaixo. Esta revisão implementa uma leitura Rust limitada para um futuro caller de configuração no desktop. Não acrescenta comando Tauri, interface, aplicação, autorização, servidor, Java, instalação de mod, conexão ou voz disponível. A feature `connect-voice-lab` continua desligada por padrão. Nenhuma alteração de matriz Core/Skins, Assistente, transporte, convite ou prazo de sessão.

Contratos antecedentes: [voz](aurora-connect-voice.md), [controller de restauração 58](aurora-connect-voice-restoration-controller.md), [cópias de diagnóstico](diagnostic-lab.md), [recovery](instance-recovery.md), [especificação Connect](engineering/AURORA_CONNECT_SPEC.md). A inspeção é somente para preparar **aplicação** posterior; não deve ser gate para restauração. O recibo/controller 58 continua permitindo restaurar a configuração quando o JAR foi removido ou desabilitado, segundo os gates próprios dessa operação.

## API e limites

`inspect_announcement(&Instance, &VoiceMutationGuard, VoiceMod)` recebe uma instância e um mod selecionados explicitamente. O gate puro `is_lab_candidate` recusa uma origem comum antes de consultar processos, DB ou inventário; a comprovação integral da cópia continua dentro do reader estrito. O guard é a autoridade opaca já existente: lock real por instância e observação Windows da ausência de `java.exe`/`javaw.exe`, sem encerrar processos. A inspeção reconsulta processos antes dos arquivos e depois do último inventário. O guard não fica retido no resultado. JVM renomeada/embutida e início externo posterior continuam limitações conhecidas.

O futuro caller deverá resolver `InstanceId` por `InstanceLayout` do diretório de dados conhecido do aplicativo. Não receberá caminho arbitrário da interface. Ter porta UDP 35554 ou destino selecionado no perfil 57 não identifica um mod, uma cópia de diagnóstico ou um arquivo de configuração.

Exige cópia de diagnóstico pronta e de identidade física verificada, runtime que declare Minecraft 1.21.1/Fabric de forma consistente e exatamente um JAR habilitado com SHA-256 e tamanho fixados. SVC 2.6.22 e Plasmo 2.1.17 mantêm os pins do adaptador nativo. Presença desses bytes não demonstra carregamento pelo jogo. A inspeção não baixa arquivos nem lê a configuração da origem para usá-la como substituta.

Lê somente o caminho de configuração fixo do mod selecionado, limitado a 256 KiB, após o inventário seguro existente. O inventário continua sujeito a 30 mil arquivos/8 GiB; duas passagens podem ser demoradas. Rejeita links/reparse points e hardlinks, configuração ausente, ambígua, inválida ou conteúdo que mudou entre observações. Não cria configuração ausente. Reutiliza as gramáticas reais dos adaptadores, sem executar o renderer: um anúncio já aplicado pode ser observado mesmo quando uma nova aplicação seria um no-op recusado.

O resultado serializável contém apenas:

- mod, versão/SHA-256/tamanho esperados do JAR;
- caminho **relativo fixo**, SHA-256 e tamanho da configuração observada;
- campo configurado `voice_host` do SVC (máximo de 1024 bytes, sem truncamento), ou presença/ausência de `host.public` e seus `ip`/`port` no Plasmo;
- limitações estáticas que distinguem observação, anúncio configurado, mod carregado, transporte e áudio.

`AnnouncementInspection` e `ObservedAnnouncement` não implementam `Deserialize` ou `Debug`. São apresentação local e podem incluir um endereço que o próprio usuário configurou; não entram em diagnóstico compartilhado, evento remoto ou log. Não retornam arquivo completo, comentários, runtime arbitrário, inventário, UUID, caminho absoluto, plano, digest de aprovação, recibo ou segredo. Porta zero no Plasmo, string vazia no SVC ou ausência de `host.public` permanecem valores **não resolvidos**, sem inferência sobre bind efetivo. IP Plasmo vazio continua recusado pela gramática existente. Não há medição de MTU nem prova de capacidade de voz.

O resultado não é autoridade para alteração. `prepare_announcement` continua exigindo pedido e hash revisados, todos os seus próprios gates, revisão/digest novos, snapshot e confirmação independente. Nenhum prazo ou objeto de autorização é criado pela inspeção.

## Leitura existente de recovery e SQLite

O novo helper não chama `Store::open`, `capture_observation`, `has_pending` ou recovery automático: esses caminhos existentes podem inicializar identidade, SQLite ou CAS. Reutiliza a validação de identidade física/marker/DB da consulta `identity_lookup`, preservando a assinatura e comportamento legado quando essa consulta é chamada fora do inspector.

No caminho estrito, mantém um handle do banco que nega escrita no Windows, fixa os diretórios e confere o cabeçalho SQLite antes de abrir a conexão. Aceita somente leitura/escrita de página no modo rollback (`1/1`) e recusa qualquer sidecar `-journal`, `-wal` ou `-shm`, inclusive vazio. Banco ausente, identidade ausente, schema desconhecido, ownership divergente ou journal lógico pendente são recusados sem inicializar ou reparar nada.

`SQLITE_OPEN_READ_ONLY` sozinho não é a garantia: WAL/SHM podem exigir inicialização. Não se usa `immutable=1`, checkpoint, PRAGMA persistente, remoção de sidecar ou descarte de transações. `PRAGMA user_version` é apenas consulta. A recusa conservadora de um estado WAL exige tratamento separado de recovery; a inspeção não sugere que o usuário apague arquivos.

O handle estrito permanece vivo durante o callback que verifica a proveniência da cópia. Isso importa porque `require_test_copy → ensure_launchable → record → identity_lookup` faz uma leitura SQLite legada transitiva. Essa cadeia só começa **depois** do gate de header/sidecars e enquanto o bloqueio de escrita permanece retido. No fim, repete inventário, journal lógico e identidade antes de liberar o resultado.

A afirmação de somente leitura se refere à inspeção **com guard já adquirido**. A aquisição prévia do guard pode criar o arquivo de lock existente do launcher. Não se promete ausência de metadados de acesso do sistema operacional, nem atomicidade contra software externo capaz de ignorar os locks da plataforma.

## Arquivos e integração

Arquivos novos:

- `apps/desktop/src-tauri/src/engine/recovery_readonly.rs`;
- `apps/desktop/src-tauri/src/engine/connect_voice_inspection.rs`;
- `apps/desktop/src-tauri/src/engine/connect_voice_plasmo_inspection.rs`;
- `apps/desktop/src-tauri/src/engine/connect_voice_inspection_tests.rs`.

Integração mínima efetuada depois do commit 58:

1. `readonly` registrado em `recovery.rs`; corpo de `identity_lookup` delegado ao mesmo reader em modo legado e reexportação interna de `with_existing_observation` somente sob `connect-voice-lab`.
2. `inspection` registrado em `connect_voice.rs`, com exportação da função/DTOs locais.
3. Helper de parsing registrado como filho privado de `connect_voice_plasmo.rs`; visibilidade limitada a `connect_voice`. Nenhum campo privado de autoridade passa a ser público.

Não requer dependência, lockfile, feature adicional ou mudança de chamada existente de preparação/restauração. A migração da consulta de identidade recebeu revisão independente por diff; as regressões pertinentes estão registradas abaixo.

## Verificação

Há sete testes regulares com fixtures próprias e seis opt-ins que exigem os JARs locais exatos em `AURORA_CONNECT_ARTIFACT_DIR`, sem download ou Java. Os opt-ins são selecionados pelo módulo, sem `include-ignored` geral.

Os testes fotografam a árvore inteira da fixture, tipos/caminhos/tamanhos/SHA-256 e os bytes integrais do DB depois de adquirir o guard e antes da inspeção; comparam novamente após sucesso ou recusa. Incluem origem, cópia, CAS, banco, sidecars, markers e locks. Para arquivos de tamanho observado zero, inclusive o lock Windows, representam conteúdo vazio sem tentar ler a região bloqueada; o tamanho e a presença continuam no oráculo, sem alegar leitura dos bytes do lock. Os demais arquivos são efetivamente lidos. A limpeza aceita somente diretório temporário canônico, filho direto de temp, com nome/UUID gerado pela própria fixture.

Oráculos: identidade/DB ausentes sem criação; cada sidecar isolado; modo WAL real sem sidecars; schema/journal/ownership; guard alheio; probe de processo; pin incorreto; JAR duplicado; runtime incompatível; hardlink de configuração; origem e cópia não pronta; anúncio original/ausente/vazio/zero/já aplicado; limites de campo/arquivo; schema de saída fechado; processo reaparecendo depois do inventário; preparação posterior recusando hash antigo. O teste de mudança entre inventários introduz uma edição deliberada no callback do reader e exige `Changed`, conservando exatamente essa única diferença — não é ensaio de corrida do sistema operacional.

Falhas preparatórias preservadas nos logs: primeira listagem recusou a posição indevida do submódulo Plasmo dentro do módulo de testes; segunda recusou visibilidade insuficiente do helper para reexportação. Corrigidos somente registro/visibilidade restrita; terceira listagem compilou os doze nomes. A primeira execução falhou 12/12 antes da inspeção porque o snapshot tentava ler o arquivo de lock vazio já bloqueado pelo Windows. A correção descrita acima mantém esse arquivo no oráculo. A segunda execução passou 11/12 em126,78s; a única falha estava **depois** da inspeção, no teste que também chama `prepare_announcement` legado. Esse caminho chama `Store::open` e pode mudar bookkeeping SQLite mesmo ao recusar hash antigo. Seu oráculo passou a exigir `StalePreview`, zero pontos/previews/journals novos e todos os demais arquivos intactos. A comparação integral do DB permanece obrigatória para toda chamada ao inspector. Nenhuma dessas correções alterou o reader de produção.

O foco completo subsequente passou12/12. Depois dessa rodada, a revisão acrescentou somente o gate antecipado puro de candidato e seu teste causal. Esse teste usa origem com DB removido e probe marcado para falhar se consultado; exige `UnsafePath`, zero consultas ao probe e árvore intacta. Os sete regulares passaram no fonte posterior. Os seis opt-ins JAR positivos permanecem evidência **anterior** ao gate aditivo; não foram repetidos nem apresentados como uma rodada13/13. A passagem de candidato válido até os gates de arquivo continua coberta pelos regulares; não mudou parsing, pin, SQLite ou construção do DTO.

Comandos executados na raiz da worktree com o target desktop padrão. O comando focal com `--include-ignored` foi executado antes do gate antecipado; as suites gerais nunca incluem opt-ins:

```powershell
$env:AURORA_CONNECT_ARTIFACT_DIR = (Resolve-Path .aurora/connect-voice-artifacts).Path
cargo test --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --features connect-voice-lab --lib engine::connect_voice::inspection::tests -- --list
cargo test --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --features connect-voice-lab --lib engine::connect_voice::inspection::tests -- --include-ignored --test-threads=1
cargo test --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --features connect-voice-lab --lib engine::connect_voice::inspection::tests -- --test-threads=1
cargo check --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets
cargo check --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --features connect-voice-lab
cargo check --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --all-features
cargo clippy --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
cargo clippy --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --features connect-voice-lab -- -D warnings
cargo clippy --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all -- --check
cargo test --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml -- --test-threads=1
cargo test --offline --locked --jobs 2 --manifest-path apps/desktop/src-tauri/Cargo.toml --all-features -- --test-threads=1
```

| Verificação | Resultado | Log privado em `.aurora/connect-validation/` |
| --- | --- | --- |
| Terceira listagem focal | PASS,12 nomes; compilação13,41s | `voice59-focused-list-3.log` |
| Foco completo anterior ao gate puro | **12 PASS,0FAIL,0ignored**,128,44s; compilação9,79s | `voice59-focused-final.log` |
| Regulares no fonte com gate antecipado | **7 PASS,0FAIL,6ignored**,4,29s; compilação8,19s | `voice59-early-gate-focused.log` |
| Check todos os targets padrão / voz / combinado, antes do gate | PASS,4,17s /6,49s /9,04s | `voice59-check-{default,voice,combined}.log` |
| Clippy todos os targets padrão / voz / combinado, antes do gate | PASS,4,66s /7,34s /8,62s | `voice59-clippy-{default,voice,combined}.log` |
| Suíte padrão completa | **261 PASS,0FAIL,5ignored**,44,97s somados; compilação55,73s | `voice59-full-default.log` |
| Suíte combinada anterior ao gate puro | **331 PASS,0FAIL,34ignored**,122,04s somados; compilação1m42s | `voice59-full-combined.log` |
| Check voz / combinado no fonte final | PASS,3,63s /4,32s | `voice59-check-{voice,combined}-final.log` |
| Clippy voz / combinado no fonte final | PASS,4,48s /5,20s | `voice59-clippy-{voice,combined}-final.log` |
| Formatação no fonte final | PASS | `voice59-fmt-final.log` |
| Suíte combinada no fonte final | **332 PASS,0FAIL,34ignored**,122,84s somados; compilação28,99s | `voice59-full-combined-final.log` |

A suíte padrão não depende do inspector sob feature e não foi repetida pelo gate puro. A suíte combinada final passou224 testes de biblioteca,42 do binário e66 integrações. As suites completas exercitam recovery e diagnostic lab existentes: identidade física, diretório renomeado/recriado, corrupção, journals, rollback e isolamento. Tempos somados dos testes não são duração de parede. Dos34ignored combinados,6 pertencem aos opt-ins59 executados no foco anterior,7 aos opt-ins58 já verificados naquele incremento e21 aos opt-ins históricos restantes. Java/writer, serviços externos e instância pessoal não foram ativados por esta rodada geral. Os cinco ignorados no padrão são opt-ins históricos de rede/serviço/instância pessoal.

Não houve Java, rede externa, configuração pessoal ou download. A suíte combinada reutiliza testes locais de transporte em loopback, sem ampliar evidência de voz. Os JARs locais foram reconferidos: SVC5369475B/SHA256 `972a01592f670aa57f2dc2e3587a656b798106037468c61e4bfb49eba7d6ae36`; Plasmo12365966B/SHA256 `46e1a4ae51786457712991ba05bf573e2d11331aeadb63b7d108839def022a2b`. A cópia imutável `native53-reviewed/aurora_launcher_core-977e96f36303eeb7.exe` permaneceu31179264B/SHA256 `1cf119159312e99f47e06a9db01341bbb9b98074102b7a270257b57b3d41fb7c`; compilar no target padrão não a repina. Após o foco e novamente após a suíte final, a consulta encontrou zero diretórios com o prefixo exclusivo das fixtures59.

Revisão independente do reader, migração legada, DTO/parsing, gates, fixtures e foco não identificou bloqueadores. `git diff --check` passou. O snapshot privado `voice59-source-freeze.json` registra base e SHA-256 dos sete fontes Rust e deste domínio, sem publicar binário. Os três fontes existentes alterados são somente `recovery.rs`, `connect_voice.rs` e `connect_voice_plasmo.rs`; os outros quatro são os novos arquivos enumerados acima. Os demais documentos gerais do incremento são mantidos na revisão canônica pelo coordenador.

## Rollback e próximo caller

O rollback de código é a reversão do commit próprio, preservando 58 e os adaptadores existentes; não exige recuperação de configuração porque o inspector não escreve.

Um caller posterior ainda precisará vincular seleção/cópia/janela/sessão, aguardar trabalho de arquivos em curso ao cancelar/sair, conservar o controller 58 em RAM após aplicação e exigir revisão separada de restauração estrita ou reconciliada. Não haverá restore automático no fechamento UDP, logout ou Drop. A interface e a disponibilidade de áudio permanecem **NÃO IMPLEMENTADO/PRECISA DE TESTE** neste incremento.
