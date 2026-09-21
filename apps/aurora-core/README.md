# Aurora Core

Aurora Core é a fundação modular carregada dentro do Minecraft. Ele concentra APIs estáveis, sessão pública, eventos, configurações, menu Aurora e o único canal IPC com o Launcher. Skins é um módulo independente e não faz parte obrigatória do núcleo. O Assistente permanece no Aurora Launcher; o mod Companion e sua integração in-game foram retirados.

O candidato atual é Core **1.1.5**, que retira somente entradas específicas do Assistente in-game. Resultados do candidato estão no [registro da retirada](../../docs/task-remove-companion.md); não há nova homologação automática. O estado histórico do Core 1.1.4 é **PARCIAL**: nove artefatos compilados e verificados por hash/assinatura. A UI foi testada em quatro combinações no candidato 1.1.3; 1.1.4 corrige os metadados de versão legados e foi revalidado com Skins dentro de um mundo novo na 1.12.2. Ainda falta o roteiro completo de runtime/IPC nas nove combinações. Veja [evidências dos reparos](../../docs/task-skins-runtime-repair.md).

## Arquitetura

```text
Launcher Rust
  └─ WebSocket 127.0.0.1 + nonce efêmero
       └─ Aurora Core Runtime
            ├─ API de módulos e eventos
            ├─ sessão pública sem tokens
            ├─ configuração versionada em .aurora/config
            ├─ menu Aurora + layout/preview 3D reutilizáveis
            ├─ módulo Aurora Skins
            └─ futuros módulos oficiais/terceiros
```

- `api`: contratos Java 8 sem classes de Minecraft ou loader;
- `runtime`: registro de módulos, event bus, sessão, IPC e configuração;
- `minecraft/<versão>`: adaptadores Fabric/Forge e UI nativa;
- `minecraft/legacy/forge-1.12.2`: adaptador legado isolado;
- `compatibility-manifest-1.1.5.json`: matriz central e metadados assinados;
- `releases/core/1.1.5`: candidatos imutáveis referenciados pela fonte do Launcher, após validação de assinatura/contratos.

## Contrato de módulo

Um módulo implementa `AuroraModule`, declara metadados e registra apenas as integrações que usa:

```java
public final class ExampleModule implements AuroraModule {
    public AuroraModuleMetadata metadata() {
        return new AuroraModuleMetadata(
            "example_module", "Example", "1.0.0", "module", "1.0.0");
    }

    public void registerEvents(AuroraEventBus events) {
        events.subscribe(AuroraEvents.Login.class, event -> refresh(event.session()));
    }

    public void registerSettings(AuroraSettingsRegistry settings) {
        settings.register(new AuroraSettingsPage(
            "example_settings", "example_module", "Example", "Configurações do módulo",
            "settings", 100, context -> openScreen(context.nativeParentScreen())));
    }
}
```

Falha de inicialização, página ou listener é isolada ao módulo. O registro parcial é desfeito e o jogo continua com os demais módulos.

## Compatibilidade

| Minecraft | Fabric | Forge | Java em runtime |
| --- | :---: | :---: | ---: |
| 1.12.2 | — | sim | 8 |
| 1.16.5 | sim | sim | 8 |
| 1.19.2 | sim | sim | 17 |
| 1.20.1 | sim | sim | 17 |
| 1.21.1 | sim | sim | 21 |

NeoForge não faz parte do escopo. O manifesto é a fonte de verdade para versão de loader, faixa compatível, JDK de build, tamanho, SHA-256 e assinatura Ed25519.

## Build e testes

Na raiz do repositório:

```powershell
npm run core:build
```

O build compartilhado valida bytecode Java 8 e executa testes de API/runtime. Cada adaptador é então compilado com sua toolchain; o projeto 1.21.1 possui wrapper Gradle 9.7.1 próprio. O comando não altera `releases/core`: releases assinadas são imutáveis e exigem um processo explícito de versionamento e assinatura.

## Segurança e dados

- O IPC escuta somente em `127.0.0.1` e autentica cada execução com nonce aleatório.
- `AuroraSession` nunca contém access token, refresh token ou segredo de serviço.
- Configurações são JSON versionado, salvo por troca atômica; migrações preservam backup.
- O Launcher verifica tamanho, SHA-256 e assinatura Ed25519 usando chave pública fixada no binário antes de instalar.
- Se IPC, avatar ou módulo falhar, o Core registra a falha com prefixo `[Aurora Core]` e mantém recursos locais disponíveis.

Veja também [o protocolo IPC](../../docs/core-ipc.md), [a matriz de compatibilidade](../../docs/engineering/COMPATIBILITY_MATRIX.md) e [a decisão arquitetural](../../docs/engineering/ADR/ADR-015-aurora-core.md).


O wrapper Gradle 8.4 compartilhado fica em `tools/gradle/8.4`; a geração 1.21.1 preserva o wrapper 9.7.1 próprio do Core, usado também pelo Skins. Execute builds sequencialmente para evitar concorrência no cache Loom.
