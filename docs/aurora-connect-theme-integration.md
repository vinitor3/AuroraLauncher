# Aurora Connect — ponto de integração com os temas

28/10/2026. Estado: **PARCIAL** — contrato registrado; merge e testes combinados **NÃO VERIFICADOS**. Complementa a [experiência social](aurora-connect-social-experience.md) e a [ADR-033](engineering/ADR/ADR-033-social-desktop-join.md), sem alterar gates funcionais.

## Bases observadas

Connect está no commit `a0d2e24e75ea48a52761f082ccf1678ad914d028`, branch `codex/aur-r4-connect`. A worktree de temas `C:/Users/usuario/.codex/worktrees/launcher-themes/AuroraLauncher`, branch informada `codex/aur-visual-themes`, foi consultada somente para leitura: HEAD `ffc37cc5f44ec74e3afa67cc9a81bf746a552d23`, com mudanças em andamento, incluindo `App.tsx`, contratos nativos/frontend e editor.

Foram lidos `docs/launcher-themes.md` e `docs/engineering/ADR/ADR-027-declarative-launcher-themes.md` daquela worktree e consultados os contratos atuais de variáveis/IDs. Na consulta, `docs/theme-integration.md` ainda não existia e a nova seção de temas de `AGENTS.md` ainda não estava disponível. Não declarar esses dois materiais lidos: lê-los no commit concluído de temas antes da integração. `ThemeSurface` foi anunciado no alinhamento entre trabalhos; sua API não foi encontrada nesta consulta e não é inventada nesta base.

Não copiar arquivos inteiros entre worktrees, nem aplicar o `App.tsx` de uma sobre a outra. Esperar o commit revisado de temas e reconciliar cumulativamente com Connect, barreiras de conta, CAS/Doctor/Recovery, Assistente e nove pares Core/Skins. Alterações não commitadas da outra worktree permanecem sob responsabilidade de seu proprietário.

## Fronteira obrigatória

Temas são documentos locais JSONC fechados de apresentação. Não executam código, não chamam IA e não definem URLs, credenciais, convites, autorização, sessão, transporte ou callbacks. Cores, tipografia, espaçamento, ícones, mídias e cursor usam somente os campos suportados e validados nos dois lados do contrato.

| Ponto existente do Connect | Integração visual futura | Responsabilidade preservada |
| --- | --- | --- |
| `App.tsx`, seções `connect` e `social` | Adotar os IDs estáveis no contrato de navegação de temas | Disponibilidade, seleção e callbacks continuam no App; tema não habilita capability experimental. |
| `SocialView.tsx` e cards | Usar `ThemeSurface` quando sua API estiver integrada e variáveis `--theme-*` com fallback oficial | Busca, amizade, bloqueio, anúncio e Entrar mantêm validação e callbacks confiáveis. |
| `SocialConnectionPanel.tsx` | Superfícies de informação podem receber apresentação suportada | Estado vem do snapshot nativo; autorização, consentimento, erros e controles de segurança não podem ser ocultados, sobrepostos ou substituídos pelo tema. |
| `ConnectView.tsx` | Preservar observação e controles do mesmo manager | Nenhuma confirmação automática ou exportação manual é introduzida em uma tentativa social. |
| `connect-controller.ts` e comandos Tauri | Sem configuração funcional no tema | Mesmo owner/slot/epoch/geração, token em RAM, validação, cancelamento e cleanup. |
| Confirmação/restauração de temas | Manter a camada oficial independente da área tematizada | Recuperação e prazo de confirmação não dependem do tema aplicado. |

Os IDs `connect` e `social` já existem em `LauncherSection` desta base. O contrato de temas observado ainda listava quatro itens (`instances`, `discover`, `appearance`, `java`). A integração deve atualizar em conjunto enumeração/frontend, validação Rust, defaults/migração de documentos antigos, editor, comentários e testes. Não tornar temas antigos inválidos só porque novas seções foram acrescentadas; também não aceitar IDs arbitrários nem permitir que o tema remova ações de segurança. Se uma seção estiver indisponível por gate funcional, o tema não pode torná-la disponível.

## Verificação no merge futuro

- Preservar Entrar público/amigos, consentimento anfitrião, stop/logout e respostas tardias com tema oficial e personalizado; aparência não cria nem recria sessões.
- Conferir migração de temas antigos, fallback, contraste, foco/teclado, cards/lista e mensagem de erro sem perda de informação.
- Confirmar que mídias/cursor/decorações não interceptam cliques em Entrar/Desconectar ou cobrem a revisão e a recuperação.
- Reexecutar testes de navegação/tema e focais sociais; observar o executável real antes de declarar a integração gráfica validada.

Este registro não altera código, dados pessoais, a worktree de temas, recursos Cloudflare ou resultados do Social89. A verificação desta alteração documental é `git diff --check`; não exige rebuild. Não autoriza publicação, deploy, instalador ou mudança dos gates de Minecraft, UDP, voz e redes.
