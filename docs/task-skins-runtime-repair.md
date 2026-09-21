# Aurora Skins — reparos de runtime (17–19/09/2026)

> REGISTRO HISTÓRICO: o conteúdo abaixo conserva evidências e decisões da entrega datada; referências a Companion, 27 JARs ou HUD não são instruções atuais. O Assistente permanece no Aurora Launcher; o mod Companion e sua integração in-game foram retirados. Escopo vigente: [retirada do mod](task-remove-companion.md).

Branch: `codex/aur-skins-runtime`, baseada em `07f6edb` (inclui alpha.7 e as entregas anteriores).
Destino: Skins 0.2.4 / Core 1.1.4. Os artefatos anteriores são imutáveis.

## Causas reproduzidas

- Os dois JARs Skins 0.2.3 de 1.19.2 declaravam `aurora_skins.mixins.json`, mas o recurso não existia em `common/src/main/resources` nem nos JARs. O erro informado pelo usuário corresponde exatamente a essa ausência.
- O cliente do catálogo recebia **HTTP 403 com User-Agent padrão Java**. Mesmo endpoint retornou 200 no PowerShell. Com `AuroraSkins/0.2.4`, Java 8u504 e Java 17.0.20.1 buscaram 12 itens, validaram PNG do MCStat e PNG do perfil Notch. Não foi necessário alterar TLS, autenticação ou distribuir secrets.
- Em 1.21.1, `Screen.render` chama `renderBackground` antes dos widgets. A chamada ao final das telas Aurora desfocava o avatar e os textos já desenhados. O fundo agora tem apenas uma passagem, antes do conteúdo; o hook posterior não o redesenha.
- As faixas e painéis roxos eram comandos de desenho explícitos, não parte da imagem da skin. Foram removidos sem alterar a geometria do botão do menu moderno.
- O adaptador 1.12.2 procurava a skin em um campo que não existe nessa versão e usava nomes SRG antigos para braços, pernas e headwear. Agora injeta somente o slot SKIN em `playerTextures/field_187107_a` e desenha as 12 partes corretas. Vanilla continua responsável por capa/elytra.
- A prévia converte explicitamente os eixos de mouse para os eixos do modelo; testes de vetores cobrem olhar horizontal e vertical. Cada renderer moderno possui textura própria; desligamento/resposta assíncrona obsoleta não republica uma textura antiga.
- QA independente reproduziu dois casos adicionais no legado: resize após Core → Skins reinseria o botão do Core, e download vanilla tardio podia restaurar textura e modelo incompatíveis. Foram corrigidos e cobertos por regressões, incluindo ordem dos listeners, falha do opener e saída da tela.

## Referências examinadas, sem copiar implementação

Fontes locais fornecidas: SkinShuffle 2.13.0 e SkinsRestorer 15.12.5.

- [SkinShuffle](https://github.com/IMB11-Mods/SkinShuffle): README, licença, renderização de prévia, hooks de tela e ciclo de vida das texturas. A versão local usa APIs de uma geração diferente de Minecraft e sua licença declara “ALL RIGHTS RESERVED”. A implementação Aurora mantém código próprio: cada prévia tem um identificador de textura exclusivo, liberação própria e restauração do estado gráfico.
- [SkinsRestorer](https://github.com/SkinsRestorer/SkinsRestorer): aplicação de propriedades de perfil e atualização de jogador no servidor. A atualização para os demais jogadores depende de suporte no servidor; não é equivalente a uma substituição visual local. Não foram copiados pacotes, código GPL ou mecanismos de respawn/pacotes de servidor para o cliente Aurora.

As referências orientam a separação entre prévia, textura local ativa e sincronização de servidor. Não se afirma portar todos os recursos dos dois projetos.

## Validação automatizada — CONCLUÍDO

- Skins: 16 testes Java; Core: 9 testes de API/runtime. Sem falhas.
- Duas suítes legadas, compiladas contra fontes e Forge/Core reais com `--release 8`, executadas em Java 8u504: reflexão MCP/SRG, fila da thread cliente, slot de skin/rollback, 12 partes do modelo, restauração GL inclusive em falha, pesquisa/resize/Escape e transição real Core/Skins. O controle negativo com a classe anterior falha no cenário de ownership, como esperado.
- Build Core 9/9, Skins 9/9 e Companion 9/9. Os dois legados foram recompilados após a revisão final; nenhum artefato histórico foi sobrescrito.
- `cargo fmt --all -- --check` e `cargo clippy --all-targets -- -D warnings`: passaram.
- `cargo test` final: **90 passaram, 3 ignorados**, incluindo a regressão de versão do Core legado. Contratos inspecionam o JAR realmente instalado: JSON, mixins declarados e classes referenciadas, serviços/entrypoints, bytecode, dependências e JARs aninhados declarados. Os dois JARs históricos 1.19.2/0.2.3 demonstram a regressão de recurso ausente.
- `verify-mod-release-matrix.ps1 -SkipBuild`: **27/27** artefatos correspondem ao staging compilado atual. Core sem SLF4J/Minecraft indevidamente embutidos; `pack.mcmeta` presente.
- Revisão independente final favorável, sem P1/P2 pendente. Overrides de blur conferidos nos JARs 1.21 Fabric e Forge, não apenas nas fontes.
- Sondagem `SkinCatalogSmoke.java`: Java 8 e 17 passaram em 17/09; repetição em 19/09 confirmou catálogo, PNG MCStat e PNG do perfil Notch em Java 8. A primeira consulta de perfil de 19/09 retornou 404; uma checagem direta da API e a repetição controlada passaram. Intermitência registrada, sem retry infinito nem mudança de Worker/TLS.

Logs locais ignorados pelo Git: `.aurora/core-matrix-build.log`,
`.aurora/skins-final-build.log`, `.aurora/core-legacy-final-build.log`,
`.aurora/skins-legacy-final-build.log`, `.aurora/companion-matrix-build.log`,
`.aurora/skins-runtime-clippy.log`, `.aurora/skins-runtime-rust-tests.log` e
`.aurora/skins-runtime-release-matrix.log`.

Rodada final Core 1.1.4: `.aurora/core-114-matrix-build.log`,
`.aurora/skins-final-api114-serial-build.log`, `.aurora/companion-api114-build.log`,
`.aurora/legacy-final-focused.log`, `.aurora/skins-runtime-final-clippy.log`,
`.aurora/skins-runtime-final-rust-tests.log` e
`.aurora/skins-runtime-final-release-matrix.log` (27/27). Uma tentativa de
build paralelo Skins/Companion colidiu no cache compartilhado Loom 1.19.2;
a repetição serial passou sem remover caches nem modificar releases.
Executar as matrizes sequencialmente, como o preflight oficial já faz.

## Verificação gráfica autorizada — PARCIAL

Executada com a skill computer-use, capturando somente as janelas do jogo.
`scripts/launch-skins-qa.mjs` reutiliza bibliotecas/assets/mods instalados,
mas cria uma pasta nova `.aurora/qa/<rótulo>`, identidade offline fictícia,
cache/biblioteca e configurações independentes. Não lê contas nem copia mundos.
Somente mundos novos das rodadas `forge-1122-c` e `forge-1122-e` foram criados e salvos; nenhum
servidor externo foi aberto. Todas as sessões de QA foram encerradas.

| Rodada de 19/09 | Ambiente | Resultado observado |
| --- | --- | --- |
| `forge-1122-c` | Forge 14.23.5.2860 / Java 8u504 / 22 mods | Modelo completo; botão abre na primeira tentativa; catálogo online; aplicação de Viajante Aurora; textura visível no menu, na mão e em terceira pessoa de frente/costas no mundo novo |
| `forge-1122-d` | Mesmo loader/Java, JARs finais após os dois P2 | Options → Core → Skins; F11 em 1920×1080 mantém 11 botões sem Core sobreposto; Escape retorna ao Core; fontes e avatar legíveis |
| `forge-1122-e` | Forge 14.23.5.2860 / Java 8u504 / Core 1.1.4 + Skins 0.2.4 publicados localmente | Catálogo online e aplicação; skin confirmada na mão e em terceira pessoa frente/costas em outro mundo novo; salvar/sair e retorno ao menu funcionaram |
| `fabric-1165-c` | Fabric 0.18.4 / Java 8u504 / 108 mods | Prévia com textura, catálogo online, aplicação e retorno ao menu; sem skin branca ou fundo roxo; botão sob o avatar |
| `fabric-1192-a` | Fabric 0.14.19 / Java 17 / 146 mods | Inicializa sem falha do mixin; avatar/menu, catálogo online, campo de busca recebe tecla e inicia consulta |
| `fabric-1211-b` | Fabric 0.19.3 / Java 21 / 160 mods | Core 1.1.3 e Skins 0.2.4 exibidos; texto/avatar nítidos, blur somente no fundo; catálogo online e aplicação funcionam |

A rodada legada `c` antecede apenas os reparos de ownership/rollback; não
atribuir seu teste de mundo automaticamente ao último binário. Hashes daquela
rodada: Core `19149d4fbee2ff9c949853cbc7a3334920927fe6a3b57abb77ef0946851ad4b0`,
Skins `c9796aee8a42a0b732d57e430a1b296925340bde0782bad6127ca6b657865100`.

Hashes SHA-256 dos binários das rodadas (Core 1.1.3 salvo indicação):

| Ambiente | Core | Skins |
| --- | --- | --- |
| Forge 1.12.2 (`d`) | `e41ad9d9034b1ac69a9d8550067d80de4640424da49ae22ac7c2370cd7e64e06` | `7eee376e7f525f3cf92e02b8660a837155776d4f279f020efe4ab6fb4b4a3d6a` |
| Forge 1.12.2 (`e`, Core 1.1.4) | `d137fbebd91e803948d15ae4dae7a8b7f96837f724161c6d7a1e49f9dd30bc72` | `7eee376e7f525f3cf92e02b8660a837155776d4f279f020efe4ab6fb4b4a3d6a` |
| Fabric 1.16.5 | `43b865d1e055666f9747753d8fff2c0bd00c516300a68de95dbb95f8e7d451d5` | `c89e95c51e3c590dea72cf926d839ace83b8c0fb2168ec89fc175dc18b2b6bbb` |
| Fabric 1.19.2 | `b4bd10953d5c1aea4acdf31d41f5c9815857391ee1004eea8540f4f5ed7aa49a` | `86733260674084407d1d7fc180359c0ad611ecfa187ef7dd6927869f8f068d0f` |
| Fabric 1.21.1 | `165df8226871924b40c19ff9ebd24cc85bb441a8937ebd5014558bf4e9c20bd8` | `091885bd8038e8913a5378c886757af139897c5b1896f6031d7a30e7875ab76a` |

O launcher/IPC, Companion, capas, servidores e os cinco pares não lançados
continuam **PRECISA DE TESTE** neste conjunto. A sessão offline fictícia não
valida autenticação Microsoft/multiplayer. Mods dos pacotes emitiram seus
próprios avisos; não se afirma corrigir todo o modpack. Digitação via Unicode
do automatizador não entrou no campo 1.16; tecla física na 1.19 e fixtures
legadas confirmaram entrada, sem atribuir a limitação da ferramenta ao mod.

## Artefatos e integração

Novos diretórios selecionados: `releases/core/1.1.4` e `releases/skins/0.2.4`.
Core possui manifesto SHA-256/Ed25519 e chave pública fixada na fonte Rust;
Skins possui `manifest.json` com tamanho/hash dos nove JARs. `core.rs`,
`skins.rs`, metadados dos módulos e scripts de build/preflight apontam para
essas versões. Companion 0.2.3 foi recompilado com a nova API e seu conteúdo
normalizado permaneceu igual ao histórico.

Na auditoria final, `mcmod.info` do Core legado ainda declarava 1.0.0,
embora `@Mod` e manifesto declarassem 1.1.3. Em vez de sobrescrever o
candidato já gerado, foi preservado `releases/core/1.1.3` e criado Core
1.1.4. O recurso agora expande `project.version` durante o build, e um
teste Rust compara sua versão com a do manifesto assinado. As rodadas
gráficas descritas acima continuam identificadas como Core 1.1.3, exceto a
rodada final legada `e`. Não foram reclassificadas como homologação integral
de 1.1.4. A versão mínima compatível do Skins permanece Core 1.1.3.

## Coordenação

A tarefa do instalador alpha.8 é a única coordenadora do empacotamento e mantém o preflight. Esta branch entrega commit e JARs, sem instalador paralelo, push ou deploy do Worker. A integração deve consumir o commit final e os novos hashes, não os JARs históricos. Antes de gerar o instalador, o coordenador deve integrar esta branch e repetir os gates do conjunto; não descartar worktrees pendentes para liberar o build.
