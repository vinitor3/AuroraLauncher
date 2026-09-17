# Um instalador para as entregas do Aurora

Use uma tarefa coordenadora no Codex com este pedido curto:

> Integre entregas prontas de todos os chats Aurora e gere um único instalador.

O coordenador consulta as tarefas do Aurora disponíveis no contexto do aplicativo, identifica as entregas e integra os commits prontos em uma worktree própria. Trabalho ainda em andamento precisa terminar antes de entrar na release. Os scripts abaixo não acordam chats, não enviam mensagens e não fazem merge, reset, push ou deploy automaticamente.

Na raiz da worktree de integração:

```powershell
npm run release:status
npm run release:package
```

`release:status` apenas lê o estado local. Mostra branch, versão, quantidade de mudanças sem commit e se o HEAD de cada worktree já é ancestral do HEAD atual. `dirty` inclui todos os arquivos; `produto` usa os diretórios de produto verificados pelo preflight. “INTEGRADO” descreve o histórico Git e não significa que alterações sem commit estejam incluídas. O relatório não substitui o preflight.

Antes de empacotar, conclua a integração, sincronize a versão em `package.json`, `apps/desktop/package.json`, `apps/desktop/src-tauri/tauri.conf.json` e `apps/desktop/src-tauri/Cargo.toml`, incluindo os lockfiles afetados, e faça commit. Use uma versão ainda não publicada. O preflight existente exige branch própria, checkout limpo e worktrees sem entregas de produto pendentes; nenhuma dessas verificações é desativada pelo wrapper.

`release:package` roda o preflight antes do trabalho caro. Em seguida executa o único build oficial, `npm run desktop:build`, com a validação da matriz dos 27 mods e o empacotamento Tauri. Também verifica o estado Git novamente ao final. O Cargo informa o diretório de saída, respeitando `CARGO_TARGET_DIR` e sua configuração; o wrapper aceita somente um instalador novo com nome, produto e versão esperados.

Instale os JDKs 17 e 21 antes do build. Os scripts selecionam a JVM por etapa: JDK 17 para Gradle 8.4 e toda a matriz Companion; JDK 21 para Core/Skins 1.21.1 com Gradle 9.7.1 e Loom. A versão do compilador declarada nas toolchains permanece independente. A seleção procura `JAVA_HOME`, `JAVA_HOME_17_X64` ou `JAVA_HOME_21_X64`, registros de JDK/Adoptium e instalações Java/Adoptium em Program Files; valida a versão no arquivo `release` e a presença de `java.exe` e `javac.exe`. Cada chamada restaura `JAVA_HOME` ao terminar, inclusive quando falha, sem mudar o `PATH` ou a configuração global. Se faltar algum JDK, o build informa a variável a configurar e não baixa Java automaticamente.

O resultado fica em `releases/`:

- `Aurora Smart Launcher_<versão>_x64-setup.exe`
- O arquivo `.exe.sha256` correspondente.
- O arquivo `.exe.manifest.json` com commit, branch, versão, tamanho, SHA-256 e versões/hashes dos JARs publicados que passaram pela comparação da matriz.

Os arquivos são criados sem sobrescrever releases anteriores. Uma falha durante a cópia final pode deixar uma publicação parcial; preserve esses arquivos e o diretório da execução para diagnóstico antes de qualquer recuperação. O manifesto é copiado por último. Ele não declara homologação em Minecraft nem assinatura digital: o estado de runtime permanece `NÃO VERIFICADO` até testes reais.

Para reduzir tokens, o build guarda sua saída em `.aurora/release-runs/<execução>/build.log`, ignorada pelo Git. A conversa recebe apenas as etapas e o resultado, ou as últimas 45 linhas quando um comando de build falha. Consulte o trecho necessário do log ao investigar uma falha; não cole o arquivo inteiro nem credenciais. A economia vem de coordenar uma única integração e um único build, mantendo as verificações obrigatórias.
