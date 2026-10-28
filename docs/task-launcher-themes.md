# Temas declarativos do launcher

Base: `dc2ec5ac36a88f54b1130bac4789f8ecd56641d2`, launcher `0.1.1-alpha.13`, Core `1.1.5`, Skins `0.3.1`. Branch `codex/aur-visual-themes`; worktree exclusiva `launcher-themes`. Nenhuma mudança nos módulos Java ou release.

Status: IMPLEMENTADO MAS NÃO HOMOLOGADO.

## Escopo e contrato da wave

O Aurora apenas interpreta dados locais. Nenhuma geração por IA, HTML, CSS, JavaScript, URLs remotas, comandos ou plugins são aceitos no tema. Um campo JSON inclui apresentação e recursos embutidos; anexos locais são convertidos nesse formato. Temas salvos, exportação, associação opcional à seleção de uma instância/modpack, vídeo silencioso e efeitos internos suaves (aurora, água e cerejeira). Prévia de 10 segundos sem confirmação restaura o padrão. Falha/crash deixa marcador e a próxima inicialização usa o padrão, preservando a biblioteca. Encerramento normal preserva o tema confirmado.

Documento v1 (todos os campos obrigatórios, objetos fechados):

```ts
type NavId = "instances" | "discover" | "appearance" | "java";
type ThemeAsset = { mime: "image/png" | "video/mp4" | "video/webm"; data: string };
type AuroraTheme = {
  schemaVersion: 1; id: string; name: string;
  colors: { background: string; panel: string; text: string; muted: string; accent: string; accentText: string; border: string };
  typography: { font: "system" | "rounded" | "serif" | "mono"; scale: number };
  layout: { sidebar: "left" | "right"; density: "comfortable" | "compact"; navOrder: NavId[]; radius: number };
  effects: { background: "none" | "aurora" | "water" | "sakura"; intensity: number; speed: number; glass: number };
  assets: Record<string, ThemeAsset>;
  media: { background: string | null; logo: string | null };
  icons: Record<NavId, string>;
  decorations: Array<{ type: "text" | "image"; text: string; asset: string | null; align: "left" | "center" | "right" }>;
};
type ThemeLibrary = { themes: AuroraTheme[]; selectedThemeId: string | null; bindings: Record<string,string>; automatic: boolean; recovered: boolean; persistent: boolean };
type ThemePreview = { token: string; deadlineMs: number };
```

IDs `[a-z0-9][a-z0-9-]{0,47}`; id `aurora-default` reservado. Cores somente `#RRGGBB`. Escala 0.9–1.15; raio 0–24; intensidade 0–1; velocidade 0.25–2; vidro 0–16. Nav order contém os quatro IDs exatamente uma vez. Ícones aceitam `default`, `star`, `leaf`, `moon`, `heart`, `gamepad`, `sparkles`, ou `asset:<id>` de PNG. Máximo 6 decorações; textos de 160 caracteres, nome de 60. Máximo 4 assets; PNG 2 MiB, dimensão máxima 4096 e 8 milhões de pixels; vídeo 20 MiB; assets somados 24 MiB; JSON 34 MiB. Assinatura de mídia deve corresponder ao MIME. Sem campos adicionais em qualquer objeto, IDs especiais/prototype e referências inexistentes. Fontes internas, sem URLs.

Comandos: `themes_load`, `themes_save({json})`, `themes_delete({id})`, `themes_preview({id, instanceId:null|string}) -> ThemePreview`, `themes_confirm({token})`, `themes_reset`, `themes_heartbeat({customized:boolean})`, `themes_set_automatic({enabled:boolean})`, `themes_unbind({instanceId})`. Todos exceto preview e heartbeat retornam ThemeLibrary. Preview começa depois do preload; o nativo arbitra prazo/token e grava pendência ANTES da prévia. Confirmar global define selectedThemeId; confirmar instância define binding e automatic=true. Reset cancela prévia, selectedThemeId=null e automatic=false, mantendo temas/bindings. Editar tema salvo invalida seleção/bindings desse id. Evento `aurora-theme-reset` sem assets; frontend recarrega biblioteca. Heartbeat 1s; ausência por ~5s com tema ativo em janela visível força padrão e recarrega webview. Encerramento normal limpa marcador de execução somente após persistência segura; fechamento inesperado com customização recupera padrão. Concorrência entre processos usa lock de proprietário e sessão secundária em memória (persistent=false), iniciando no padrão.

Exports frontend: `AuroraTheme`, `ThemeLibrary`, `ThemePreview`, `NavId`, `parseTheme(text)`, `DEFAULT_THEME`, `THEME_EXAMPLES`, `buildThemeAiPrompt(theme)`, `themeVariables(theme)`, `ThemeScene({theme})`. Variáveis CSS com prefixo `--theme-`.

ThemeManager recebe `{library, instances:Array<{id:string;name:string}>, selectedInstanceId:string, busy:boolean, error:string, onSave(json):Promise<void>, onPreview(id,instanceId:string|null):Promise<void>, onDelete(id):Promise<void>, onReset():Promise<void>, onAutomatic(enabled):Promise<void>}`. Chamadas com erro rejeitam e erro é apresentado pelo provider. Confirmação/contagem ficam fora da árvore tematizada.

## Evidências e pendências

Implementação: biblioteca/editor e anexos, parser TypeScript e schema Rust fechados, renderer SVG/CSS interno, integração à seleção de instância e barra de navegação, camada de confirmação externa ao tema, persistência, watchdog e recuperação via bandeja. Documentação: [uso](launcher-themes.md) e [ADR-027](engineering/ADR/ADR-027-declarative-launcher-themes.md).

Validação executada em 28/10/2026:

- `npm --prefix apps/desktop run build`: CONCLUÍDO; TypeScript e build Vite passaram. Vite avisou sobre bundle acima de 500 KiB; não bloqueou a compilação.
- `node --experimental-strip-types --test apps/desktop/src/features/themes/model.test.mjs`: CONCLUÍDO; 11 testes incluindo mídia, APNG, orçamento, esquema fechado e limite de estrutura antes do parser.
- `node apps/desktop/src/features/themes/ThemeProvider.smoke.mjs`: CONCLUÍDO; provider real sob React/linkedom testou confirmação, contexto A/B, vínculo, automático desligado, falha do bridge/reset e prazo real de dez segundos preservando biblioteca.
- `node --experimental-strip-types --test apps/desktop/src/lib/assistantPreservation.test.mjs`: CONCLUÍDO; dez testes de regressão do Assistente passaram.
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`: CONCLUÍDO; suíte completa passou (dois testes opt-in ignorados). Após os reforços finais somente de temas, os 20 testes nativos focados foram repetidos e passaram.
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --bin aurora-launcher-core themes::tests`: CONCLUÍDO; 20 testes. Inclui relógio monotônico, token/replay, timeout, crash, sessões paralelas, arquivo bloqueado no Windows, backup, symlink, falha na persistência segura e APNG.
- `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all -- --check` e `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings`: CONCLUÍDO. Um aviso de clippy foi corrigido e a execução final passou.
- Revisão independente: corrigidos contexto da instância alterado pela prévia, reaplicação após falha de reset, decode repetido de mídia, perda de marcador após erro de disco e animação APNG fora das regras de movimento.
- QA visual no navegador local: biblioteca, tema de cerejeiras, confirmação protegida e associação com modpack de exemplo verificados. Somente fixtures, sem credenciais.
- Launcher nativo de desenvolvimento aberto no Windows e tela Personalização observada, com sessão existente carregada. A janela é intitulada **Aurora — Teste de personalização**. O launcher instalado e os JARs não foram substituídos.

Homologação de mídias reais/codec, desempenho de GPU fraca, pausa efetiva ao minimizar/bandeja e avaliação visual do usuário: PRECISA DE TESTE. Nenhum instalador/release foi gerado. O servidor Vite e `tauri dev --no-watch` permanecem em execução para o teste solicitado; a configuração de título/beforeDevCommand está somente em `.aurora/theme-dev.json`, ignorada pelo Git.
