# AUR-WARDROBE — equipada e recuperação da skin

Data: 2026-09-17. Branch: `codex/aur-wardrobe-recovery`, base `7df9b0e`.

## Diagnóstico e entrega

- A aba Equipada não tinha ação; agora mostra a imagem efetivamente equipada,
  com nome/modelo, diferenciando seleção e aplicação.
- Importações por URL guardavam somente o endereço, e o refresh revogava URLs
  ainda usadas pela prévia. Agora há PNG persistido e descarte após troca da UI.
- O restart privilegiava URL remota, sem ID/modelo da skin local. A recuperação
  usa primeiro a cópia nativa, encontra o PNG exato na biblioteca e mantém uma
  imagem estável da equipada. Também repara a cópia nativa a partir do IndexedDB.
- Atualizações de URL/cache preservam PNG/nome/data e não recriam itens removidos.
  Erro de imagem tem fallback explícito; o render 3D cancela callbacks antigos.
- Minecraft espera a recuperação inicial. O modelo local equipado prevalece
  sobre metadados remotos atrasados. Migração automática de avatar e equipagem
  explícita são ordenadas para não sobrescrever a nova aparência.

## Evidências

- Build TypeScript/Vite aprovado; permanece o aviso já existente de bundle >500 kB.
- Suíte Node desktop: 50 testes aprovados (31 novos). Inclui transação abortada,
  preservação de Blob, recuperação por bytes/ID, callbacks atrasados de imagem,
  troca de conta, navegação Equipada, espera de recuperação e de migração de avatar.
- QA independente encontrou duas corridas (launch antes de recuperar skin e
  refresh de proprietário antigo invalidando o atual); corrigidas e cobertas.
  Revisão final também confirmou a ordenação da migração legada de avatar.
- Smoke no navegador real pelo fixture `examples/wardrobe-recovery.html`:
  PNG sintético 64×64/440 bytes gravado no IndexedDB, recarregado e renderizado
  em miniatura e 3D; imagem inválida mostrou aviso; restaurar mostrou o PNG
  preservado, sem download. Sem conta, upload real ou dados pessoais.
- Sem mudanças Rust, Worker ou Java nesta entrega; esses builds não foram repetidos.

## Limites

**IMPLEMENTADO MAS NÃO HOMOLOGADO** na próxima build desktop. Reiniciar o
executável com conta real, equipar offline e confirmar a skin/modelo dentro do
Minecraft ainda **PRECISA DE TESTE**. Nenhum instalador, deploy ou push realizado.
O instalador alpha.6 existente não contém esta correção.

Se uma skin antiga tinha apenas um link indisponível e nenhuma cópia do PNG,
não é possível reconstruir a imagem; o registro permanece e é preciso reimportar.
Nenhuma skin real foi removida ou alterada durante os testes.
