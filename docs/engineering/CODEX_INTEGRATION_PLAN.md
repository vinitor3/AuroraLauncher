# Plano de integração e QA

## Papel do Integration Agent

O Integration Agent não reimplementa features. Ele cria branch limpa a partir do baseline acordado, aplica commits aprovados na ordem, resolve apenas conflitos mecânicos/glue, executa testes agregados e devolve um commit integrável. Qualquer mudança sem dono volta ao implementador.

## Pré-integração por tarefa

- SHA/branch identificados e diff restrito ao ownership.
- Nenhum secret, binário inesperado ou lockfile sem justificativa.
- Critérios de aceitação e comandos da tarefa registrados.
- Documentação/rollback incluídos.
- `git diff --check` limpo.

## Ordem Wave 0

1. **AUR-R0-001 Security:** aplicar regra e testes; confirmar que o emulador falha sem o patch e passa com ele.
2. **AUR-R0-002 Licensing:** aplicar após D-001; conferir SPDX em Rust/JS/Java e notices.
3. **AUR-R0-003 CI:** integrar checks e pins; ajustar somente paths/scripts necessários.
4. **AUR-R0-004 Release/docs:** aplicar checker/versionamento/docs por último para refletir o estado combinado.

Se AUR-R0-003 e AUR-R0-004 conflitarem no `package.json`, o Integration Agent preserva o script de release e inclui o comando de CI como alias sem mudar comportamento. Conflito sem solução mecânica volta aos dois owners.

## Suíte agregada

Executar em Windows e, quando o workflow existir, confirmar equivalente Ubuntu:

```powershell
Set-Location apps/desktop
npm ci
npm run build
Set-Location src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
Set-Location ../../../edge-proxy
npm ci
npm run check
Set-Location ../firebase/functions
npm ci
npm run lint
Set-Location ../../companion
./gradlew.bat build --no-daemon
Set-Location ../firebase
npx --yes firebase-tools@latest emulators:exec --only firestore --project demo-aurora '<comando da suíte de regras>'
```

Além disso:

- checker de versão/proveniência da tarefa R0-004;
- `git diff --check`;
- busca por tokens/chaves e artefatos grandes novos;
- comparação do diff final contra o ownership aprovado.

## Papel do QA Agent

QA recebe o commit integrado, não branches isoladas. Deve testar comportamento e procurar falhas, sem “arrumar enquanto revisa”. Findings incluem severidade, reprodução, esperado/observado e arquivo/linha.

### Casos adversariais mínimos

Firestore:

- criar próprio perfil como `ADMIN`, com UID divergente, campos extras, stats infladas ou timestamp falso: negar;
- criar perfil inicial válido como `PLAYER`: permitir;
- alterar role/uid/createdAt: negar;
- username com chave/owner divergente, campo extra ou formato inválido: negar;
- admin legítimo existente: operações autorizadas conforme contrato, sem permitir auto-promoção.

Release/licença:

- versões divergentes entre manifests: checker falha;
- tag/asset histórico não é modificado;
- checksum anunciado corresponde byte a byte;
- build sem secret e licença cobre todas as linguagens/ativos declarados.

CI:

- falha de `fmt`, `clippy`, regra, Companion ou versão deve deixar o check vermelho;
- caches não mascaram saída;
- permissões do workflow são mínimas e Actions críticas estão fixadas conforme política.

## Classificação final

- `GO`: todos os critérios do gate e zero finding P0/P1.
- `GO WITH EXCEPTIONS`: somente P2/P3 documentado, sem promessa pública afetada.
- `NO-GO`: P0/P1, teste obrigatório ausente, regra não implantada ou licença indefinida.

## Rollback de integração

Não reescrever nem apagar a release histórica. Se uma tarefa falhar, reverter o commit lógico daquela tarefa na branch de integração e repetir a suíte; não usar reset destrutivo na `main`. Regras Firestore exigem rollback para versão previamente exportada/testada, nunca edição manual improvisada em produção.

## Relatório final do Integration Agent

```text
Baseline / head integrado:
Commits e ordem:
Conflitos e resolução:
Suíte e resultados:
Diferenças de ambiente:
Findings abertos:
Decisão recomendada para QA:
```

