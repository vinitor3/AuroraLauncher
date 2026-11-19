# Integração de novas telas com temas

Este contrato permite que novas telas adotem a personalização existente sem criar outro motor de temas. Os exemplos de Java e Aurora Connect abaixo são pontos de integração para as respectivas features; não declaram que essas funcionalidades estejam implementadas.

Leia também [AGENTS.md](../AGENTS.md), [o formato e o uso dos temas](launcher-themes.md) e a [decisão de arquitetura](engineering/ADR/ADR-035-declarative-launcher-themes.md). Antes de iniciar trabalho de Aurora, confira o HEAD e a matriz ativa no repositório atual, seguindo o fluxo de worktree do AGENTS.

## Uma superfície visual confiável

Use `ThemeSurface` para um painel que deve herdar cores, fonte, espaçamento e cantos. O componente recebe somente `surfaceId`, `as`, `labelledBy`, `className` e `children`. Ele não recebe ações, permissões, caminhos, configurações de Java ou conteúdo do tema como propriedades executáveis.

```tsx
import { useId } from "react";
import { ThemeSurface } from "../themes/ThemeSurface";

type JavaMemorySettingsProps = {
  ramMiB: number;
  maxRamMiB: number;
  busy: boolean;
  onRamChange: (ramMiB: number) => void;
  onSave: () => void;
};

export function JavaMemorySettings({
  ramMiB, maxRamMiB, busy, onRamChange, onSave,
}: JavaMemorySettingsProps) {
  const titleId = useId();
  const memoryId = useId();
  return <ThemeSurface surfaceId="java-memory-settings" labelledBy={titleId}>
    <h2 id={titleId} className="theme-surface-heading">Memória do jogo</h2>
    <p className="theme-surface-description">Escolha a memória desta instância.</p>
    <label htmlFor={memoryId}>Memória: {ramMiB} MiB</label>
    <input id={memoryId} type="range" min={512} max={maxRamMiB} step={128}
      value={ramMiB} disabled={busy}
      onChange={(event) => onRamChange(Number(event.currentTarget.value))} />
    <button type="button" className="theme-surface-action" disabled={busy}
      onClick={onSave}>Salvar configuração Java</button>
  </ThemeSurface>;
}
```

`surfaceId="java-memory-settings"` é uma constante da feature. É exposta apenas como `data-theme-surface`; não cria uma seção no menu nem um vínculo no JSON. Use nomes estáveis, por exemplo `java-memory-settings` e `connect-session-panel`. O ID de acessibilidade de um título pode ser gerado por `useId`, separadamente desse identificador estável. O wrapper permite `section`, `article` ou `div`; o próprio painel fornece seus títulos e estrutura semântica.

O estado de RAM, os limites por máquina/instância, argumentos JVM, escolha de executável e os comandos de persistência/launch continuam na feature e no backend. O slider do exemplo não substitui a validação real desses valores no fluxo de Java.

Uma tela de Connect segue o mesmo padrão:

```tsx
export function ConnectPanel({ status, busy, onInvite }: {
  status: string;
  busy: boolean;
  onInvite: () => void;
}) {
  const titleId = useId();
  return <ThemeSurface as="article" surfaceId="connect-session-panel"
    labelledBy={titleId}>
    <h2 id={titleId} className="theme-surface-heading">Aurora Connect</h2>
    <p role="status">{status}</p>
    <button type="button" className="theme-surface-action" disabled={busy}
      onClick={onInvite}>Convidar para a sessão</button>
  </ThemeSurface>;
}
```

Autenticação, convites, autorização de participantes, endereços, códigos de sessão, rede e sua confirmação permanecem na implementação de Connect. Nunca acrescente esses valores ao documento de aparência ou às instruções copiadas para uma IA externa. O tema não muda o destino de um botão nem concede uma operação nova.

## Tokens e funcionamento antes do provider

`ThemeSurface` não cria um `ThemeProvider`. Dentro do launcher, ele herda os tokens produzidos por `themeVariables`. Fora dessa árvore, usa os padrões oficiais abaixo e continua legível. Isso permite adotar o wrapper durante o desenvolvimento de uma feature antes de integrar a tela ao launcher.

| Token | Uso | Padrão oficial |
| --- | --- | --- |
| `--theme-panel` | Fundo opaco da superfície e dos campos | `#110d20` |
| `--theme-text` | Texto principal | `#f2efff` |
| `--theme-muted` | Descrições secundárias | `#aea7bd` |
| `--theme-accent` | Botão principal | `#c9a7ff` |
| `--theme-accent-text` | Texto do botão principal | `#180b25` |
| `--theme-border` | Borda da superfície e dos campos | `#443557` |
| `--theme-radius` | Cantos da superfície | `12px` |
| `--theme-font` | Família local de fontes | `Inter, ui-sans-serif, system-ui, sans-serif` |
| `--theme-scale` | Escala do texto da superfície | `1` |
| `--theme-spacing` | Escala do preenchimento | `1` |

Classes auxiliares opcionais: `theme-surface-heading`, `theme-surface-description` e `theme-surface-action`. Para um botão secundário, use `className="secondary theme-surface-action"`. Mensagens essenciais, limites de RAM, erros e dados que orientam uma ação usam a cor de texto principal; descrições secundárias não devem ser a única forma de comunicar essas informações.

A superfície tem fundo opaco para preservar a relação de contraste validada pelo tema. CSS específico da feature pode consumir esses tokens com os mesmos fallbacks. Evite cores fixas em painéis comuns; mantenha cores oficiais em avisos de segurança e ações destrutivas. O wrapper não decide grid, altura, foco, estados de carregamento ou comportamento de formulários.

## Registrar uma seção ou uma nova opção visual

Adotar cores e tipografia em um painel existente normalmente exige apenas o wrapper ou os tokens. Uma nova seção de navegação, uma nova região reordenável ou uma propriedade visual por feature exige integração explícita; não aparece automaticamente porque existe um `surfaceId`.

Quando ampliar o contrato:

1. Defina IDs estáveis no código, com ações e componentes conhecidos. Ajuste a navegação e as regiões reais no `App.tsx` ou no componente de domínio. Preserve ordem de leitura e teclado ao reordenar regiões.
2. Atualize os tipos, IDs permitidos, validação e padrões em `features/themes/model.ts`, e a validação independente em `src-tauri/src/themes.rs`. Não aceite IDs arbitrários ou propriedades desconhecidas como solução temporária.
3. Planeje a compatibilidade antes de acrescentar um item obrigatório. Por exemplo, uma navegação v1 com os quatro IDs anteriores deve ganhar o novo ID por uma migração determinística que preserve a ordem existente; simplesmente exigir uma lista maior quebraria os temas salvos. A validação após a migração continua fechada.
4. Atualize os comentários de `formatThemeCode`, as instruções de `buildThemeAiPrompt`, o editor visual, exemplos, renderização e testes na mesma entrega. Defina valores padrão para opções novas quando isso preservar o significado de documentos v1 anteriores; use uma versão nova quando a semântica exigir.
5. Valide os dados antes de enviá-los ao renderer e antes de persistir no lado nativo. Use a biblioteca, a prévia e a confirmação existentes. Não monte outro importador ou outro cronômetro de aplicação dentro da feature.

Os elementos do tema permanecem dados locais: cores, fontes permitidas, opções visuais tipadas e recursos locais verificados. Não acrescente HTML/CSS/JavaScript livre, seletores fornecidos pelo usuário, handlers, `invoke`, shell, URLs, caminhos, shaders ou permissões ao documento. Servidores, instâncias, packs e sessões Connect não podem importar ou ativar automaticamente um tema recebido pela rede.

## Remoção e migração

A biblioteca atual associa temas a instâncias/modpacks; `surfaceId` não adiciona uma associação por painel. Se uma extensão futura introduzir vínculos de superfícies ou se uma seção registrada for removida, a migração deve preservar a biblioteca e desativar vínculos com superfícies que já não existam. Não apague um tema inteiro porque uma tela foi retirada, e não redirecione um ID removido para outra ação.

Trate somente versões antigas conhecidas na migração, usando os mecanismos de armazenamento protegido e publicação atômica da biblioteca. Ajuste referências/ordens reconhecidas de forma determinística e mantenha a recuperação para o padrão quando o documento não puder ser utilizado. Essa compatibilidade não autoriza aceitar IDs desconhecidos em uma importação nova. Teste um tema v1 anterior, um tema salvo com a associação removida e um documento atual após reiniciar o launcher.

## Áreas protegidas e validação da entrega

Autenticação, contas, credenciais, autorização, confirmações de operações sensíveis, alertas de segurança, contagem de dez segundos e recuperação ficam sob controle oficial do Aurora. As ações de minimizar, maximizar e fechar continuam fixas; os campos visuais permitidos da barra de título não alteram seus handlers. A recuperação por interface, atalho, bandeja, watchdog nativo e marcador de encerramento inesperado deve continuar acessível.

Para uma integração de superfície, execute o build TypeScript/frontend e verifique a tela com os fallbacks oficiais, um tema v1 anterior e um tema atual com cores e fonte diferentes. Confirme que os callbacks, estados `disabled`, limites e confirmações da feature mantêm seu comportamento. Não use o tema como fonte de dados de Java ou Connect.

Para alterações no contrato, execute também os testes de `model.test.mjs` e os smokes relevantes de provider, editor, workspace e mídia/cursor. Se houver mudança em Rust, complete `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test` para o manifesto desktop. Mudanças na recuperação exigem teste de expiração, ausência de confirmação, travamento e reabertura. Compilar não substitui homologação no desktop; registre o que foi executado e o que continua **PRECISA DE TESTE**.

Coordene a integração com os outros chats: um responsável por `App.tsx`, `commands.rs` e demais monolíticos em cada wave, sem sobrescrever trabalho em andamento. Registre os IDs e opções adicionados, defaults e migrações, arquivos alterados, verificações e pendências. A feature deve integrar o contrato comum, preservando as funcionalidades posteriores já presentes na base ativa.
