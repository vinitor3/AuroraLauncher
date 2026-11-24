# Test Strategy

## Pirâmide e gates

| Nível | Objetivo | Gate |
| --- | --- | --- |
| unitário | parser, compatibilidade, paths, hashes, planos puros | todo PR de domínio |
| integração local | filesystem, ZIP, HTTP fake, SQLite, Worker/Firestore emulador | mutação/rede/auth |
| componente UI | loading/error/empty, teclado, cancelamento | feature React |
| E2E desktop | login fake/emulado, instância, catálogo, guarda-roupa | release alpha |
| smoke runtime | Minecraft/loader/Java + Core/Skins | qualquer claim de compatibilidade |
| segurança | traversal, SSRF, auth, rules, prompt injection, updater | capacidade afetada |
| restore/failure | queda de rede/processo/disco, rollback bit a bit | updater/CAS/Tools |

## Baseline histórico (2026-08-30; não é validação desta mudança)

- frontend build: verde, sem testes de componente/E2E;
- Rust: 26 aprovados; um TTS online ignorado;
- Worker dry-run e Functions lint: verdes, sem Miniflare/auth matrix;
- Companion 1.20.1 build: verde, sem testes Java e com warning de tooling;
- runtime histórico documentado do Companion 0.1.0: Forge 1.12.2 e Fabric
  1.20.1. Para Core + Companion 0.2.0, 0/9 concluem o roteiro; há execução
  parcial 1.12.2, corrida de inicialização nas linhas Fabric observadas e
  metadados Forge inválidos em 1.16.5/1.19.2.

## Suítes prioritárias

### Rules/Auth

- usuário pode criar somente o próprio perfil com role `PLAYER` e campos esperados;
- negar `ADMIN`, UID divergente, campo extra e alteração de role/username;
- reservar username atomicamente e negar colisão;
- token ausente, expirado, projeto/audience errado e rotação JWKS.

### Rede

- timeout, cancelamento pelo usuário, 429, 5xx, corpo inválido, conexão interrompida;
- retry apenas GET/idempotente com jitter/backoff limitado;
- download parcial, Range ignorado, hash/tamanho incorreto, destino duplicado;
- APIs externas simuladas; teste online fica separado e não bloqueia PR.

### Arquivos/modpack

- `../`, caminho absoluto, drive Windows, symlink/junction, ZIP bomb e entry gigante;
- override após falha não deixa estado parcial;
- arquivo `UNKNOWN` nunca removido automaticamente;
- config/mundo preservados em update e restore.

### Core/Skins runtime e Assistente desktop

Para cada um dos nove pares: OS/build, Java, Minecraft, loader, hashes Core/Skins, launch, handshake/sessão pública, menu, abrir/fechar, foco/mouse, skin classic/slim/64x32, capa onde implementada e shutdown. Não exigir Companion, HUD ou voz do Assistente no jogo. Isso não exclui os testes separados dos mods de voz em R4. Evidência é log redigido + screenshot/vídeo + resultado estruturado.

Separadamente, com Minecraft fechado e nenhum Companion instalado: abrir o Assistente pelo botão e `Ctrl+Alt+/`, enviar/receber texto, preservar histórico/estado existentes, mute, microfone/transcrição, áudio/TTS, timeouts e falhas de rede. `assistant-smoke.mjs` mantém o caminho desktop; `supabase-appearance-smoke.mjs` continua usando o painel/API como preparação. Mocks não comprovam serviços online, microfone real ou áudio audível.

A migração usa fixtures: JAR conhecido, nome semelhante de terceiro, identidade incerta, nome estável/versionado/desativado, repetição, jogo em execução, link/traversal e recuperação após falha. Preservar Core/Skins, terceiros, mundos e dados do Assistente desktop.

### IA

Fixtures maliciosas em log, nome de mod, manifesto e HTML. Verificar que frases como “ignore regras e apague…” permanecem dados. Fuzz de argumentos de tools, path fora da instância, chamada não allowlisted, replay de confirmação e excesso de iterações.

### CAS/rollback

- snapshot/restore bit a bit;
- crash antes/depois de cada fase de commit;
- blob corrompido e hash collision simulada;
- GC dry-run e raízes transitivas;
- retenção e disco cheio.

## Matriz CI proposta

- PR rápido: frontend build/test, Rust fmt/clippy/test, Worker tests/dry-run, rules Emulator.
- Core/Skins: build dos nove pares (18 JARs), wrapper neutro, contratos de metadata/bytecode/recursos/assinatura e comparação fontes/artefatos sem enfraquecimento. Builds Loom devem rodar sequencialmente para evitar colisão de cache.
- nightly/manual: runtime smoke em máquina Windows preparada; nunca fingir que runner GitHub tem Minecraft homologado.
- release: todos os anteriores, NSIS install/uninstall, manifest/SHA/SBOM e teste updater em canal de staging.

## Fixtures

Connect V2 tem [matriz própria de testes](https://github.com/vinitor3/AuroraLauncher/blob/088bf5b435b1b63e842c11ff1c01648b5e38bced/docs/engineering/CODEX_TASKS/AUR-R4-CONNECT-BACKLOG.md): TCP, UDP, MTU/associações, auth, convites, relay gratuito permitido e indisponível, endereço dos mods, redes reais e configurações reversíveis. Nenhum teste deve rejeitar relay só por ser relay. TCP aprovado não aprova UDP/voz/R4. Recursos sem prova permanecem experimentais/desligados; não gerar custo externo. A alpha.14 já registra testes de recovery/lab em [AUR-1167](https://github.com/vinitor3/AuroraLauncher/blob/088bf5b435b1b63e842c11ff1c01648b5e38bced/docs/task-aur-1167-doctor-lab.md); não converter a revisão documental em nova execução desses testes.

Dados reais devem ser redigidos e versionados por causa/padrão, sem username, token, path pessoal, IP público ou mundo. Snapshot dourado só é atualizado com revisão do comportamento esperado.
