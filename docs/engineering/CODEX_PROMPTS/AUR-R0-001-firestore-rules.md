# Prompt AUR-R0-001

Você é o Security Implementer do AuroraLauncher. Trabalhe somente na task **AUR-R0-001 — Regras Firestore e auditoria**, a partir do baseline `5aa5fe8`, em branch `codex/aur-r0-001-firestore-rules` e worktree dedicado. Leia `AGENTS.md`, `SECURITY.md`, `docs/engineering/CODEX_TASKS/AUR-R0-001-firestore-rules.md` e a regra atual inteira antes de editar.

Há uma vulnerabilidade P0: a regra histórica permitia ao cliente criar `/users/{uid}` com `role: ADMIN`. Existe um patch de planejamento que força `PLAYER` e allowlist; não confie nele sem reproduzir e testar. Seu resultado deve fechar a escalada sem bloquear o payload inicial legítimo.

Ownership exclusivo: `firebase/firestore.rules`, `firebase/tests/**` e scripts/manifests mínimos dentro de `firebase/`. Não toque frontend, Worker, Functions, código Rust/Java, docs de arquitetura, billing ou dados reais. Não implante em produção; escreva o procedimento separado e declare que produção só fica segura após auditoria e deployment registrados.

Implemente uma suíte determinística no Firestore Emulator, em projeto `demo-aurora`, que prove pelo menos: create próprio como `ADMIN` negado; UID/e-mail/username incoerentes negados; campos extras/stats infladas/timestamp falso negados; perfil PLAYER inicial válido permitido; role/uid/createdAt imutáveis; username com owner/estrutura/formato inválidos negado; operações de admin existente limitadas ao contrato. A suíte deve falhar quando a vulnerabilidade for reintroduzida. Não use credenciais nem PII reais.

Crie também procedimento/script read-only idempotente para auditar perfis privilegiados já existentes, sem imprimir mais PII que o necessário, e documente deploy/rollback. Execute o Emulator e todos os testes. Revise rules diffs para permissões genéricas.

Antes do commit, rode `git diff --check`, verifique que o diff está no ownership e registre comandos/resultados. Faça um único commit `fix(firebase): close profile role escalation [AUR-R0-001]`. No handoff entregue Task ID, branch/SHA, arquivos, resultado, testes, testes omitidos, riscos, auditoria/deployment pendentes e rollback. Não faça push, merge ou deploy sem instrução explícita.

