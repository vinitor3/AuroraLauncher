# Prompt AUR-R0-003

Você é o CI Implementer do AuroraLauncher. Execute somente **AUR-R0-003 — CI completa e determinística**, baseline `5aa5fe8`, branch `codex/aur-r0-003-ci-baseline`, worktree dedicado. Leia `AGENTS.md`, todos os workflows atuais, manifests/lockfiles, `docs/engineering/TEST_STRATEGY.md` e a task `docs/engineering/CODEX_TASKS/AUR-R0-003-ci.md`.

Ownership exclusivo: `.github/workflows/**` e scripts novos em `scripts/ci/**`. Não altere código do produto, regras Firebase, licença, versões, binários/JARs ou root `package.json`; se um alias for indispensável, entregue patch separado ao Integration Agent.

Construa jobs legíveis para frontend, Rust, Worker/Functions, Companion e security/governance. A baseline obrigatória inclui: desktop `npm ci && npm run build`; Rust `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`; Worker `npm ci && npm run check`; Functions `npm ci && npm run lint`; Companion `gradlew build --no-daemon`; hook para suíte Firestore AUR-R0-001 e checker AUR-R0-004 quando integrados. Não chame build Companion de runtime 9/9.

Use permissões mínimas, concorrência/cancelamento por PR/ref, caches baseados em lockfiles e versões/toolchains determinísticas. Fixe Actions críticas conforme política de supply chain; não exponha secrets a PRs não confiáveis nem dê write permission sem necessidade. Considere o custo zero de runners padrão em repo público.

Rode localmente tudo que o ambiente permitir, valide YAML/actionlint se disponível e demonstre que cada classe de falha torna o job vermelho sem deixar sabotagem no commit. Registre duração/omissões. Rode `git diff --check` e revise o diff por permissões. Faça um commit `ci: enforce full baseline gates [AUR-R0-003]`. Handoff completo com SHA, jobs, pins, testes, gaps de integração R0-001/R0-004, riscos e rollback. Não faça push/merge.

