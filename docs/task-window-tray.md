# AUR-WINDOW-TRAY — janela e bandeja do launcher

Data: 2026-09-22. Branch de integração: `codex/aur-release-alpha9`.

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO** no executável instalado.

## Comportamento

- O X e o fechamento nativo (inclusive Alt+F4) ocultam somente a janela
  principal. O processo do launcher, seus downloads e o jogo continuam com
  seu ciclo de vida atual. Janelas de download externo mantêm o fechamento
  próprio e não são interceptadas por essa regra.
- O ícone Aurora da bandeja abre/restaura/foca a janela com clique esquerdo.
  O menu oferece **Abrir Aurora** e **Fechar o launcher**. Esta última ação
  encerra explicitamente o aplicativo sem executar `kill` no Minecraft.
- Se o sistema não disponibilizar a bandeja, ou ocultar a janela falhar,
  o fechamento normal é preservado para não deixar um aplicativo inacessível.
- Barra própria com botões de minimizar, maximizar/restaurar e fechar,
  rótulos acessíveis, foco de teclado e dica sobre a bandeja. A região do
  título permite arrastar; Tauri trata o duplo clique nativo de maximização.
- A barra aparece também no carregamento e login. O espaço de 44 px é
  descontado das telas, painéis e modais; controles permanecem alcançáveis.
- Maximização é lida do estado real da janela, inclusive após redimensionar.
  Erros nas ações recebem mensagem curta; listeners são removidos ao desmontar.
- Apenas as permissões necessárias de janela são expostas ao frontend;
  criação da bandeja e encerramento do aplicativo ficam no Rust.

## Validação executada

- `npm --prefix apps/desktop run build`: aprovado; aviso existente de chunk >500 kB.
- `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml`: aprovado.
- `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all -- --check`: aprovado.
- `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings`: aprovado.
- Nenhuma mudança em Core, Skins ou protocolo IPC nesta entrega.

## Homologação ainda necessária

**PRECISA DE TESTE** no Windows com o executável desta versão: arrastar,
duplo clique, minimizar, maximizar/restaurar, X/Alt+F4, reabrir pelo ícone e
pelo menu, encerramento explícito, ausência de rolagem excedente nas telas e
modais. Repetir ocultar/reabrir com download e Minecraft em execução para
confirmar continuidade. Compilação e análise estática não comprovam estes
fluxos visuais e nativos.

Referências oficiais consultadas: [System Tray](https://v2.tauri.app/learn/system-tray/)
e [Window Customization](https://v2.tauri.app/learn/window-customization/).
