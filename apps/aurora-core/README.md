# Aurora Core

Aurora Core é a fundação modular carregada dentro do Minecraft. Ele concentra APIs estáveis, sessão pública, eventos, configurações, menu Aurora e o único canal IPC com o Launcher. Recursos como Skins e Companion são módulos independentes e não fazem parte obrigatória do núcleo.

O estado atual é **IMPLEMENTADO MAS NÃO HOMOLOGADO**: os nove artefatos compilam e passam por verificação de hash/assinatura, mas ainda precisam do roteiro runtime completo em instalações limpas antes de serem anunciados como suporte público.

## Arquitetura

```text
Launcher Rust
  └─ WebSocket 127.0.0.1 + nonce efêmero
       └─ Aurora Core Runtime
            ├─ API de módulos e eventos
            ├─ sessão pública sem tokens
            ├─ configuração versionada em .aurora/config
            ├─ menu Aurora + avatar 3D reutilizável
            ├─ módulo Aurora Companion
            └─ futuros módulos, incluindo Aurora Skins
```

- `api`: contratos Java 8 sem classes de Minecraft ou loader;
- `runtime`: registro de módulos, event bus, sessão, IPC e configuração;
- `minecraft/<versão>`: adaptadores Fabric/Forge e UI nativa;
- `minecraft/legacy/forge-1.12.2`: adaptador legado isolado;
- `compatibility-manifest.json`: matriz central e metadados assinados;
- `releases/core/1.0.0`: artefatos imutáveis embarcados pelo Launcher.

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

Veja também [o protocolo IPC](../../docs/companion-ipc.md), [a matriz de compatibilidade](../../docs/engineering/COMPATIBILITY_MATRIX.md) e [a decisão arquitetural](../../docs/engineering/ADR/ADR-015-aurora-core.md).
