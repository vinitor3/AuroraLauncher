# AUR-R0-001 — Regras Firestore e auditoria

- **Fase/prioridade:** R0 / P0.
- **Objetivo:** eliminar autoatribuição de `ADMIN` e transformar autorização Firestore em contrato testado.
- **Contexto/problema:** a regra original aceitava o `role` enviado no create de `/users/{uid}`. Existe patch local que força `PLAYER` e allowlist, mas só teve sintaxe validada.
- **Resultado esperado:** regras fechadas por padrão; suíte Emulator positiva/negativa; procedimento idempotente para listar perfis com role privilegiada; deployment permanece etapa explícita.
- **Dependências:** Firebase Emulator; credenciais não são necessárias para testes `demo-*`. Auditoria/implantação real exige autorização/credencial do projeto.

## Escopo e ownership

- **Pode/deve tocar:** `firebase/firestore.rules`, `firebase/tests/**`; manifesto/script mínimo dentro de `firebase/` se necessário.
- **Não tocar:** frontend, Worker, Functions, `SECURITY.md`, dados reais ou configuração de billing.
- **Interfaces:** coleções `users`, `usernames` e controles admin existentes; preservar clientes válidos atuais.

## Passos

1. Reproduzir no Emulator o create malicioso com `role: ADMIN` contra a regra anterior ou fixture equivalente.
2. Implementar/validar allowlist exata, UID/e-mail/username coerentes, `role: PLAYER`, `createdAt == request.time`, skin/defaults/stats iniciais.
3. Cobrir create/update/delete/read de usuário comum, username e admin legítimo; testar campos extras, tipos/tamanhos e imutabilidade.
4. Criar auditoria read-only que liste IDs/roles anormais sem imprimir tokens/PII além do necessário.
5. Documentar deployment e rollback; não implantar automaticamente neste PR.

## Testes/comandos

```powershell
Set-Location firebase
npx --yes firebase-tools@latest emulators:exec --only firestore --project demo-aurora '<comando determinístico da suíte>'
```

Testes obrigatórios: `ADMIN` próprio negado; UID divergente negado; campo extra/stats infladas/timestamp falso negados; perfil PLAYER inicial válido permitido; role/uid/createdAt imutáveis; username inválido negado; admin existente somente nas ações documentadas.

## Aceitação e Definition of Done

- Exploit reproduzido e convertido em regression test.
- Suíte roda sem rede/conta real depois de dependências instaladas e falha quando a vulnerabilidade é reintroduzida.
- Nenhuma regra permissiva genérica e nenhum dado/secret real no fixture.
- Auditoria e deployment/rollback documentados; produção não é declarada segura sem evidência de implantação.

## Riscos e rollback

Regra estreita pode bloquear criação legítima se o payload real divergir. Capturar contrato do cliente e testar exatamente. Rollback usa a regra previamente exportada e testada; nunca `allow read, write: if true`.

## Git/PR

- Branch/worktree: `codex/aur-r0-001-firestore-rules` / `AuroraLauncher-wt-r0-001`.
- Commit: `fix(firebase): close profile role escalation [AUR-R0-001]`.
- PR depende de: nenhuma; integração primeiro.
- Execução paralela: **sim**, ownership `firebase/**` exclusivo.

