# Contribuindo

Crie uma branch a partir da versão mais recente. Descreva o comportamento alterado, a forma de reproduzir e o que foi testado. Ao mudar um mod, informe as versões de Minecraft, loader e Java e se houve teste dentro do jogo.

Na raiz, execute as verificações dos componentes afetados:

```powershell
npm --prefix apps/desktop run build
node --test "apps/desktop/tests/*.test.mjs"
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
npm --prefix apps/edge-proxy run check
npm --prefix apps/edge-proxy test
npm --prefix functions run lint
```

Use dados de teste. Trate tokens, senhas, chaves de assinatura, mundos e logs pessoais como privados. [Fluxo de release](docs/release-workflow.md) · [Segurança](SECURITY.md)
