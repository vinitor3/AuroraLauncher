# Releases do Aurora

Os artefatos desta pasta são imutáveis. Um nome de versão já publicado nunca
deve ser sobrescrito nem removido.

## Fluxo obrigatório do instalador desktop

1. Integre e faça commit de todas as mudanças de produto.
2. Garanta que os outros worktrees estejam limpos e que seus branches relevantes
   façam parte do histórico do branch de integração.
3. Incremente a mesma versão em `package.json`, no pacote desktop, em
   `Cargo.toml` e em `tauri.conf.json`.
4. Execute `npm run desktop:build` na raiz. O preflight bloqueia checkout sujo,
   branch principal, versões divergentes, conflitos, worktrees pendentes e
   tentativa de reutilizar um release existente.
5. Copie o bundle NSIS para `releases` sem `-Force`, gere SHA-256 e registre a
   evidência no documento da tarefa.
6. Faça commit do artefato e crie a tag SemVer correspondente.

Não use `npm --prefix apps/desktop run tauri build` para um release oficial,
pois esse comando ignora o preflight da raiz.

## Release integrado atual

- `Aurora Smart Launcher_0.1.1-alpha.3_x64-setup.exe`
- SHA-256: `E535918AD05E26F057BA5415E93BCFB30C778D30B705A6B123F22F739364CEFB`
- Contém o redesign do desktop, Aurora Core 1.0.0 e Companion 0.2.0.
