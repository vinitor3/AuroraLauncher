# AUR-STARTUP-MOTION — animação perceptível

Data: 2026-09-17. Branch: `codex/aur-startup-motion`, base `7fecc3a`.

## Entrega

- Anéis com partículas orbitando em sentidos opostos, logo flutuante, brilho,
  pontos pulsantes e barra indeterminada. Apenas CSS, sem atrasar autenticação.
- O media query de movimento reduzido antes desligava todos os efeitos e
  preenchia a barra; agora elimina movimento espacial e conserva apenas uma
  pulsação suave de opacidade nos três pontos. A barra fica oculta, sem indicar
  falsamente conclusão. Nenhuma configuração do Windows é modificada.
- Em erro, indicadores de carga são removidos e os ciclos decorativos param;
  retentativa e retorno ao login preservam seus callbacks e acessibilidade.

## Evidências

- Build TypeScript/Vite aprovado (aviso existente de bundle >500 kB).
- 56 testes Node aprovados, incluindo seis novos testes do componente real:
  carregamento, semântica acessível, erro, callbacks e retorno após erro.
- QA independente de componente/CSS sem bloqueadores.
- Fixture local `examples/startup-motion.html` validado em navegador com CSS
  de produção, sem autenticação nem acesso a instâncias reais:
  - sistema informou movimento reduzido; amostras de 359 ms mostraram mudança
    de opacidade nos três pontos, zero transformações e barra oculta;
  - modo normal simulado por CSSMediaRule: 355 ms, cinco elementos com mudança
    de transform, nove ciclos ativos e barra visível;
  - erro: 364 ms, zero ciclos e nenhuma mudança de transform/opacidade;
  - Tentar novamente retomou movimento; Voltar para entrar acionou apenas a
    saída simulada. Modo do fixture restaurado e servidor local encerrado.
- TypeScript focado do fixture aprovado. Nenhuma alteração Rust/Java/Worker,
  portanto esses builds não foram repetidos.

## Estado e limites

**IMPLEMENTADO MAS NÃO HOMOLOGADO** no launcher empacotado. Animação foi
verificada no navegador; novo executável Tauri ainda **PRECISA DE TESTE**.
Nenhum instalador, mudança de versão, push ou deploy nesta tarefa.
A alpha.7 existente permanece imutável e não inclui este reforço visual.
