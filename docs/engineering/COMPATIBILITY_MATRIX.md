# Compatibilidade da beta

A [matriz ativa](../../scripts/active-mod-matrix.json) define um alvo por combinação de Minecraft e loader. Cada alvo informa o Java do jogo, o JDK de compilação e os módulos Core e Skins selecionados pela release.

| Verificação | O que demonstra |
| --- | --- |
| Build Gradle e contrato de recursos | O JAR correspondente foi compilado para o alvo e contém os adaptadores e recursos esperados. |
| Preflight e manifesto de release | A seleção do instalador corresponde aos JARs publicados com os respectivos hashes. O manifesto do Core também tem assinatura verificada. |
| Execução com o cliente real | Valida somente a combinação e as funções exercitadas, como abertura do cliente, handshake, aplicação de aparência ou encerramento. |

A [página desta beta](../beta.md) registra os resultados atuais. Instalar Fabric ou Forge para uma versão do jogo não implica a disponibilidade dos módulos Aurora nessa combinação. Convites, partida entre computadores e voz precisam de verificações próprias. O [histórico de testes por versão](https://github.com/vinitor3/AuroraLauncher/blob/088bf5b435b1b63e842c11ff1c01648b5e38bced/docs/engineering/COMPATIBILITY_MATRIX.md) conserva as evidências das alphas.
