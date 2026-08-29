# Contribuindo com o Aurora

Obrigado por ajudar a construir o Aurora Smart Launcher. Enquanto o projeto estiver em alpha privada, combine mudanças maiores com o mantenedor antes de começar.

## Fluxo recomendado

1. Crie uma branch curta a partir de `main`.
2. Faça mudanças pequenas, com responsabilidade bem definida.
3. Não inclua tokens, arquivos `.env`, dados de conta, mundos ou instâncias locais.
4. Execute as verificações relevantes.
5. Abra um pull request explicando comportamento, testes e riscos.

## Verificações locais

```powershell
npm --prefix apps/desktop run build
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
npm --prefix apps/edge-proxy run check
npm --prefix functions run lint
```

Mudanças no Companion devem indicar exatamente quais versões e loaders foram compilados e quais foram testados dentro do Minecraft. Compilação não equivale a homologação em jogo.

## Commits

Prefira mensagens diretas, por exemplo:

- `feat: adiciona filtro de loaders no catálogo`
- `fix: preserva hash ao retomar download`
- `docs: atualiza matriz do companion`

## Pull requests

Inclua objetivo, evidências redigidas, comandos de teste, versões afetadas e pendências conhecidas.

