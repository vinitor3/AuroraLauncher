# Tarefa — sessão sem flash de login e Windows sem CMD

17/09/2026 · branch `codex/aur-startup-session`.
Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**.

## Entrega

- Tela leve com marca Aurora, órbita/brilho e indicador indeterminado enquanto
  a sessão/perfil é verificada; movimento reduzido respeitado.
- Login somente após confirmação de ausência de sessão; launcher somente
  após perfil compatível com o UID autenticado. Falhas têm recuperação.
- Proteções para cadastro, saída, troca de conta, StrictMode e tarefas antigas.
- Release Windows sem console próprio e processos Java sem console auxiliar;
  diagnóstico da JVM preservado em `logs/aurora-java.log`.

## Evidência

- Build React/TypeScript aprovado, com aviso preexistente de tamanho do bundle.
- 19 testes Node aprovados: 12 de sessão e 7 de fila externa existente.
- `cargo fmt --check` e `cargo clippy --all-targets -- -D warnings` aprovados.
- Suíte Rust final: 88 aprovados e 3 testes online ignorados. Os novos testes
  criam subprocessos reais e confirmam ausência de console Windows, captura de
  stdout/stderr e recusa de destino inválido de log.
- A primeira execução teve falha transitória no teste IPC preexistente
  `sends_public_session_without_credentials` (handshake WouldBlock/reset).
  O teste isolado e a repetição da suíte completa passaram sem mudar IPC.
- Prévia local da animação/erro/retentativa inspecionada no navegador. A aplicação
  real no navegador, sem sessão, passou de inicialização para o formulário de
  login. Nenhuma conta criada e nenhuma credencial inserida.
- QA independente: achados de log antigo e atualização de perfil atrasada
  corrigidos; revisão final sem bloqueadores.

## Limites

Reabrir o desktop com sessão Firebase real persistida, iniciar Minecraft e
verificar a ausência do console no executável de release: **PRECISA DE TESTE**
na próxima versão empacotada. A prévia/testes não equivalem a essa homologação.

Nenhum instalador, deploy ou mudança de versão nesta tarefa. O aplicativo já
instalado continua inalterado; a próxima versão deve incluir esta alteração e
a automação de downloads externos anterior.

Detalhes: `startup-session.md`.
