# Gerar o instalador no Windows

A raiz do projeto pode ficar em qualquer unidade. Depois de mover uma cópia principal que possui worktrees, execute `git worktree repair` nessa raiz. Confira o resultado com `npm run release:status`.

1. Instale Node.js 24, Rust estável, ferramentas C++ MSVC, Windows SDK, WebView2 e JDKs 17, 21 e 25. Configure o ambiente de compilação x64. O Gradle busca os JDKs no Windows ou em `JAVA_HOME_17_X64`, `JAVA_HOME_21_X64` e `JAVA_HOME_25_X64`.
2. Rode `npm ci --prefix apps/desktop` e `cargo fetch --locked --manifest-path apps/desktop/src-tauri/Cargo.toml`. As compilações Gradle também precisam de seus artefatos em cache para as etapas offline.
3. Integre os commits das worktrees, sincronize a nova versão no projeto, desktop, Cargo, Tauri e lockfiles. Faça commit em uma branch de integração limpa.
4. Na raiz, execute `npm run release:package`. O comando confere pendências, recompila e compara os módulos, valida as assinaturas do Core e executa o build Tauri/NSIS. O log fica em `.aurora/release-runs`.
5. Confira o SHA-256, o manifesto e o instalador em `releases/`. Crie uma tag para a fonte que gerou o executável e anexe os três arquivos à nova release no GitHub. Use **pre-release** para a beta.

Cada versão tem seu próprio nome. O hash identifica o arquivo publicado. A assinatura Authenticode e a homologação dentro do jogo são verificações separadas.
