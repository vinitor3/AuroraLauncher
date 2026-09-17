# Fase 1.3 — Atualizações de conteúdo

Atualizado em 17 de setembro de 2026.

## Estado

**IMPLEMENTADO MAS NÃO HOMOLOGADO.** O Aurora identifica e troca conteúdo por
um plano compatível e reversível. A validação automatizada cobre resolução,
integridade, falha e rollback; ainda é necessário testar a atualização em
instâncias Minecraft reais antes de classificar o fluxo como homologado.

## Experiência no launcher

A edição de uma instância possui a aba **Atualizações**, separada das listas de
conteúdo instalado e do catálogo. Ela oferece:

- verificação conjunta de mods, shaders e pacotes de recursos;
- estados distintos para atualização disponível, atualizado, não identificado,
  bloqueado e componente administrado pelo Aurora;
- filtros, contadores, seleção individual ou em lote por provedor e imagens já
  conhecidas do catálogo;
- comparação entre versão instalada e versão alvo;
- revisão obrigatória antes de alterar arquivos;
- bloqueio de aplicação e rollback enquanto o Minecraft estiver rodando;
- aviso explícito para dependência ausente ou download oficial obrigatório;
- botão para desfazer a última atualização concluída.

## Resolução e compatibilidade

O desktop calcula o SHA-1 local e usa os endpoints oficiais em lote do
Modrinth para identificar a versão instalada e localizar a release estável mais
recente. O plano exige a mesma versão do Minecraft. Mods exigem o mesmo loader
Forge ou Fabric; resource packs usam o loader `minecraft`; shaders são
comparados somente com os loaders próprios desse tipo de projeto. Uma versão
com data anterior à instalada nunca é tratada como atualização.

Itens CurseForge que possuem `projectId` local são consultados pelo Worker
autenticado. O Worker mantém a chave privada, valida classe, Minecraft,
Forge/Fabric, release, arquivo atual e ordem das versões, e devolve somente um
ticket de download protegido. A consulta é dividida em lotes pequenos para
permanecer dentro do limite de subrequests do Workers Free. Dependências
`required` que não possam ser provadas como instaladas bloqueiam a troca.

Arquivos sem identidade verificável continuam intactos e aparecem como **Não
identificados**. Componentes Aurora Core, Aurora Skins e Aurora Companion não
são atualizados por este fluxo; eles acompanham a versão do launcher.

## Aplicação e rollback

O backend não aceita o plano visual como autoridade. Antes da aplicação ele
confirma novamente a instância, Minecraft, loader, arquivo atual e catálogo.
Downloads usam HTTPS com hosts permitidos, limites de tamanho e SHA-1/SHA-512
quando disponível. Só depois da verificação o arquivo de staging recebe o nome
final.

Cada lote cria journal e backup dentro de
`.aurora/content-updates/<transactionId>`. Colisões de nomes, path traversal,
mudança manual após o scan e atualização concorrente falham sem substituir o
arquivo atual. Se o commit for interrompido, a próxima operação recupera o
estado anterior. O rollback também falha fechado se o usuário modificou um dos
arquivos depois da atualização.

Modpacks Modrinth são reconsultados e atualizados por uma versão exata do mesmo
projeto, Minecraft e loader. O Aurora preserva extras locais e protege os
caminhos mutáveis do pack com snapshot e journal. Modpacks CurseForge são
identificados pelo `fileId`, pois o ZIP de instalação não é guardado; a
detecção está disponível, mas a substituição automática permanece bloqueada
até existir um plano transacional completo para o manifesto CurseForge.

## Limites deliberados

- somente Forge e Fabric; NeoForge continua fora do escopo;
- somente releases estáveis são oferecidas automaticamente;
- arquivo CurseForge sem distribuição direta continua no fluxo oficial e não é
  contornado;
- apenas a última transação concluída fica disponível no botão **Desfazer**;
- o updater do próprio launcher e o CAS local continuam em fases posteriores;
- compilar e testar o código não substitui homologação dentro do Minecraft.

## Verificação desta entrega

Validação local concluída antes da integração:

- `npm run build` no desktop;
- `cargo check` e `cargo clippy --all-targets -- -D warnings`;
- 11 testes do executor transacional e rollback;
- 5 testes do gerenciador de downloads;
- 4 testes locais de modpacks (o teste de catálogo ao vivo permanece ignorado);
- 6 testes dos comandos Tauri.

O instalador, o SHA-256 e a homologação runtime pertencem ao empacotamento
único da release integrada; não são evidência produzida por esta branch.
