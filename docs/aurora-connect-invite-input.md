# Entrada de console dos convites — incremento 47c

Data: 09/10/2026. Estado: **PROTÓTIPO**. Correção restrita ao runner TCP `aurora-connect-invite-audit`; o protocolo assinado, a autorização e o transporte permanecem os mesmos dos [ensaios 47/47b](aurora-connect-internet-invitations.md). Não altera o launcher desktop, Minecraft, UDP, mods de voz ou configurações pessoais. Os kits históricos permanecem imutáveis.

## Bloqueio e evidência

Uma tentativa remota relatou `approval_mismatch` depois de receber somente o convite e a orientação de pressionar Enter, antes de o coordenador solicitar `approve`. O anfitrião observou zero participantes admitidos, recusas, destinos TCP e bytes; houve encerramento e rebind. O fluxo exato de entrada do terminal remoto não foi capturado. Enter extra depois de uma colagem que já inclui quebra de linha é uma **HIPÓTESE**, não causa remota comprovada nem diagnóstico de rede.

O coordenador reproduziu localmente um caso concreto com o binário histórico 47b, SHA-256 `7d3403b25ee66e1759adf87fe7d64bec65031d9574cc325b9ed64f09408b3e16`: convite válido seguido de `CRLF CRLF` produz `audit_review` e depois `approval_mismatch`, sem enviar aprovação. O canário UDP próprio recebeu zero pacotes; o filho encerrou com código 1, sem encerramento forçado ou stderr, e o canário fechou. A fixture privada é `.aurora/connect-validation/reproduce-invite-empty-line.mjs`; usa chave/convite efêmeros em RAM e não imprime esses valores. Essa reprodução prova o defeito local específico, sem estabelecer retrospectivamente a causa da tentativa remota.

## Comportamento da correção

`Input::next_line` descarta até **oito linhas vazias por processo**, após o framing já existente de LF/CRLF e eventual BOM. A quota é compartilhada entre identidade/convite, revisão/aprovação, conexão e comandos. A nona linha vazia retorna o código estático `blank_input_limit`; não reinicia o processo nem concede mais tentativas.

O código não usa `trim`: espaços e tabulações continuam sendo dados e falham nas validações correspondentes. Uma linha sem terminador, UTF-8 inválido, NUL ou excesso de tamanho continua recusada. EOF permanece EOF. Há uma única thread de entrada e fila com capacidade 1; a mudança não cria leitores adicionais.

Descartar uma linha vazia nunca gera `approve`, nunca renova a lease, nunca troca a identidade e nunca abre transporte ou destino. Os `Instant` absolutos dos prompts continuam sendo conferidos antes e depois do await. Os demais waits preservam seus timers originais. Comandos antigos mantêm preparação de 60 s, aliases interativos mantêm 600 s, Hello mantém 10 s e a sessão inteira continua limitada ao TTL original máximo de 600 s.

O tratamento também é usado durante a tentativa de conexão. Falha de entrada conserva o código estático, aguarda a tentativa nativa limitada e fecha eventual sucesso tardio antes de retornar. Após conexão, erros continuam passando pelo encerramento aguardado e pelas verificações de liberação existentes. Não há aprovação, fallback de rede, retry ou relay automático.

## Validação local

Arquivos alterados: `apps/connect-lab/src/bin/aurora-connect-invite-audit.rs`, `apps/connect-lab/tests/invite_audit_cli.rs` e este documento. Não há mudança de dependência, lockfile, esquema do convite ou contrato de rede.

Casos adicionados:

- quota global de oito linhas vazias, não normalização de espaço/tab, prazo absoluto e EOF;
- dois processos reais com oito vazias por papel distribuídas entre preparação, aprovação e comandos; TCP de 1 MiB por direção, replay e revogação separados, cleanup/rebind;
- nona vazia após mudar de prompt, espaço em lugar de aprovação, EOF e flood antes do convite;
- o teste existente de digest errado também inclui linhas vazias antes da aprovação inválida.

Comandos executados, a partir da worktree Connect, com target privado para preservar os binários históricos:

```powershell
cargo test --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --target-dir .aurora/connect-validation/relay50-target --bin aurora-connect-invite-audit --test invite_audit_cli -- --test-threads=1 --nocapture
cargo clippy --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --target-dir .aurora/connect-validation/relay50-target --bin aurora-connect-invite-audit --test invite_audit_cli -- -D warnings
cargo fmt --manifest-path apps/connect-lab/Cargo.toml --all -- --check
git diff --check -- apps/connect-lab/src/bin/aurora-connect-invite-audit.rs apps/connect-lab/tests/invite_audit_cli.rs
cargo build --manifest-path apps/connect-lab/Cargo.toml --offline --locked --jobs 2 --target-dir .aurora/connect-validation/relay50-target --bin aurora-connect-invite-audit
```

Resultado: **6 unitários PASS** em 0,03 s; **10 testes de integração PASS** em 13,53 s, com dois opt-ins de IPv6 próprio ignorados. Sem falha nessa execução. Clippy focal com warnings como erros passou em 1,24 s; formatação e diff check passaram. Build privado passou em 0,62 s. Os dois opt-ins existentes de IPv6 próprio não fizeram parte da execução padrão; não contar casos ignorados como aprovação.

Logs privados, sob `.aurora/connect-validation`: `connect-invite47c-focused-1.log`, `connect-invite47c-clippy-1.log` e `connect-invite47c-build-1.log`. O binário novo foi produzido somente em `relay50-target/debug/aurora-connect-invite-audit.exe`, SHA-256 `731f76f126aa7149d8d479b27f804ff85c04d1bdaa931f1507ec595f86191eb6`. A conferência após o build confirmou que o executável histórico em `apps/connect-lab/target/debug` continua com o pin 47b `7d3403b25ee66e1759adf87fe7d64bec65031d9574cc325b9ed64f09408b3e16`.

A composição **anfitrião 47b pinado + convidado 47c pinado** também passou em fixture privada própria, por `node .aurora/connect-validation/mixed-invite47b47c.mjs`: dois processos reais, echo TCP e portas efêmeras em loopback, sem Cargo ou mudança dos executáveis. O convidado recebeu uma linha vazia antes do convite e duas após o convite. Depois de `audit_review`, ainda sem aprovação, permaneceu aberto e sem evento extra; o anfitrião confirmou zero peers, recusas, destinos e bytes. Somente após a aprovação explícita do harness ocorreu admissão.

O mesmo ensaio comparou 1 MiB TCP por direção, desconectou mantendo a identidade, observou replay e depois revogação com `denied` 0 → 1 → 2, um único destino e 2097152 bytes cumulativos. Ambos os processos saíram 0, sem força; host, listener guest e echo passaram rebind. **PASS em 1398 ms**, log `connect-invite47c-mixed-1.log` e resumo redigido em pasta privada nova. Nenhuma linha com peer, convite ou assinatura foi impressa no log; os valores ficaram em RAM durante a troca. Isso demonstra compatibilidade local entre esses dois binários, sem provar rede externa.

Revisão independente do diff e dos logs não encontrou bloqueador. Quota global, ausência de trim, cancelamento do receive e espera/cleanup do dial foram inspecionados. A nona vazia especificamente durante um dial em andamento e Ctrl+C depois de vazias não tiveram fixture runtime dedicada neste incremento; não apresentar essa inspeção como ensaio executado desses dois casos.

## Limites, operação e rollback

O resultado continua sendo um laboratório experimental, sem entrada/expiração/replay/revogação de convites homologados entre duas internets. O transporte TCP/UDP anterior e seus resultados permanecem separados. Nenhuma prova nova de Minecraft, interface, Simple Voice Chat, Plasmo Voice, contas mistas, Core/Skins, NAT/CGNAT ou relay decorre desta correção.

Um eventual kit 47c deve ser uma pasta/ZIP novos, com hash próprio e pin explícito no helper. Não substituir os kits 47/47b nem o executável histórico usado pelas regras do anfitrião. A composição local 47b/47c foi testada separadamente acima e permite preparar o próximo ensaio mantendo o executável do anfitrião. Continua **PRECISA DE TESTE** entre computadores/redes diferentes; não exige alterar o firewall para substituir o binário histórico nesta preparação.

O procedimento humano continua separado: enviar somente o convite, observar `audit_review`, então enviar somente `approve <digest>`. Se o processo terminou e reapareceu `PS ...>`, não colar JSON ou aprovação no shell. A tolerância a vazias não resolve processos já encerrados, convites antigos, identidade trocada ou relógios divergentes.

Encerramento: `stop` no runner próprio; conferir os eventos de cleanup/rebind. EOF/Ctrl+C preservam os caminhos existentes de encerramento. Não há configuração pessoal a restaurar. Rollback de código por commit local revisado, sem reset/clean ou substituição de artefatos históricos; rollback operacional conserva os kits antigos, com sua limitação de entrada documentada.
