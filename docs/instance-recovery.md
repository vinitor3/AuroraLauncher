# Recuperação de instâncias

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. Pontos manuais registram o estado protegido de uma instância. A restauração tem prévia, confirmação e proteção do estado anterior para desfazer. Não demonstra que um mod ou configuração causou uma falha e não promete corrigir um diagnóstico do Doctor. Evidências: [AUR-1166](task-aur-1166-instance-recovery.md).

## Uso pelo jogador

No gerenciamento da instância, abra **Recuperação**. Com o jogo encerrado, escolha **Criar ponto de restauração**, acrescente uma descrição opcional e aguarde a cópia e a verificação. Um ponto só aparece na lista após publicação completa. A lista mostra data, descrição, escopo e tamanho lógico; **Detalhes** mostra inventário, hashes, exclusões e campos do perfil. Pontos com manifesto ausente ou dados corrompidos aparecem como indisponíveis, sem tamanho inventado, e podem ser excluídos; não ocultam pontos íntegros.

**Restaurar** compara o ponto com o estado atual. A prévia lista arquivos adicionados, substituídos e removidos, além dos campos do perfil que mudarão. Arquivos criados depois do ponto dentro do escopo aparecem como remoções. O jogador confirma que revisou as mudanças e que entende a exclusão dos mundos antes de confirmar a restauração. **Cancelar** abandona a revisão sem alterar arquivos.

**Desfazer restauração** prepara uma nova prévia do estado protegido anterior à última restauração. Também exige confirmação e considera alterações realizadas depois da operação anterior. **Excluir** retira um ponto da lista após confirmação, sem apagar arquivos da instância ou conteúdo compartilhado de outros pontos.

Operações de disco ocorrem fora da thread da interface. A interface mostra atividade e mantém o estado das operações; não oferece interrupção enquanto a captura, verificação ou aplicação estiver em andamento. Fechar uma revisão antes de confirmar é seguro. Depois de uma interrupção do processo, reabra Recuperação para verificar o journal e recuperar o estado seguro antes de jogar.

## Escopo protegido

| Caminho ou dado | Contrato |
| --- | --- |
| `mods/**/*.jar`, `mods/**/*.jar.disabled`, `mods/**/*.jar.off` | JARs locais, incluindo o sufixo que conserva o estado ativado/desativado. Outros arquivos dentro de `mods/` ficam excluídos. Módulos administrados têm exclusão própria. |
| `config/**` | Configurações locais elegíveis dos mods. |
| `defaultconfigs/**` | Configurações padrão locais elegíveis; não inclui a cópia de configuração dentro de mundos. |
| `options.txt` | Opções locais do jogo. |
| `optionsof.txt` | Opções locais do OptiFine, quando presentes. |
| `optionsshaders.txt` | Opções locais de shaders, quando presentes; os pacotes de shaders ficam excluídos. |
| `aurora-modpack.json` → `name` | Nome de apresentação do perfil, quando presente. A aplicação mescla somente esse campo ao documento atual. |

A allowlist de perfil desta versão contém exatamente `aurora-modpack.json.name`. `source`, `projectId`, `versionId`, `minecraft`, `loader`, IDs de arquivos/versões do provedor, URLs, ícone, identidade, Java, parâmetros de execução e campos desconhecidos não são restaurados. `aurora-instance.json` participa da conferência de ambiente, mas seus campos não são repostos pelo ponto. Mudança de versão/loader entre captura e restauração bloqueia o uso do ponto para outro ambiente.

As configurações elegíveis são texto UTF-8 de até 4 MiB por arquivo, sem bytes nulos nem indícios de credencial, sessão, chave ou caminho externo. O filtro conservador examina nome/caminho e conteúdo; arquivos excluídos são listados individualmente no ponto e na prévia. Isso pode excluir uma configuração legítima que mencione um termo sensível. Não há interpretação semântica universal de formatos de terceiros: mantenha credenciais fora de configurações de mods; o filtro não constitui classificação infalível de segredos arbitrários.

Não inclui `saves/**`, configurações `serverconfig` dentro de mundos, `screenshots/**`, `logs/**`, caches, `.aurora` interno da instância, Java, `versions/**`, `libraries/**`, `assets/**`, `natives/**`, `resourcepacks/**` ou `shaderpacks/**`. Também não inclui contas globais, login, credenciais, sessões, tokens, caminhos externos ou instalação compartilhada. Caminhos fora da lista permitida ficam intactos. Diretórios vazios não constituem arquivos de backup.

**Mundos não são salvos nem restaurados.** Um ponto local não substitui um backup separado dos mundos. A restauração de configurações padrão não repõe configurações já incorporadas a um mundo.

Core, Skins e o Companion retirado não são artefatos restauráveis por esta área. A identificação considera nomes, metadados dos JARs e namespaces Aurora, incluindo artefatos renomeados e desativados. Um ponto antigo não reinstala Companion nem substitui Core/Skins por versões antigas ou incompatíveis. A seleção de módulos continua sob os mecanismos atuais do launcher e a matriz ativa. Essa limitação aparece na área e na confirmação. Se um caminho que o ponto restauraria agora contiver um módulo administrado, a prévia é recusada. Exclusões registradas no momento da captura continuam excluídas mesmo se o conteúdo posterior parecer elegível.

## Consistência e identidade

As operações exigem o bloqueio exclusivo de mutação já usado por jogo, instalações, updates e ações confirmadas do Assistente. A camada Tauri verifica processos rastreados e processos Java externos antes e depois de adquirir o bloqueio. Minecraft reconhecido sem um `--gameDir` localizável também bloqueia a operação, porque não é possível confirmar qual instância está em uso. O fluxo de Recuperação falha de forma conservadora quando não consegue comprovar que a instância está livre; a verificação de processos da entrega desktop é Windows.

A captura verifica arquivos e inventário antes de publicar. Alteração detectada durante a leitura impede a publicação do ponto. O jogador deve manter editores, sincronizadores e outros programas sem editar a instância durante esse período. A confirmação revalida manifesto, blobs e o estado que originou a prévia; mudança relevante exige nova revisão.

A associação persistente combina UUID do marcador local, identidade física do diretório e registro correspondente na tabela `instances` fora da instância. A identidade física reutiliza a verificação do Assistente: identificador do arquivo/diretório no volume e criação, não somente nome ou caminho. Renomear a instância conserva a associação. Excluir e recriar uma pasta com o mesmo nome não autoriza usar os pontos da anterior; copiar ou editar o marcador para apontar para outro UUID também não concede essa associação.

O primeiro marcador de identidade é escrito e sincronizado em arquivo temporário e só então publicado por renomeação atômica. Uma falha antes dessa publicação não deixa identidade parcial nem impede iniciar uma instância cujos arquivos protegidos não foram alterados.

Manifestos são dados não confiáveis. A aplicação valida schema, identidade, limites, hashes e caminhos permitidos; rejeita caminhos absolutos, traversal, ADS, nomes reservados do Windows, colisões de caminho e escapes por links simbólicos ou junctions. Handles dos diretórios impedem sua troca durante a operação no Windows, e leituras protegidas não seguem links nem aceitam arquivos com múltiplos hardlinks. Os blobs são cópias independentes e não usam hardlinks com os arquivos ativos. Conflitos em que um caminho mudou de arquivo para diretório ou vice-versa, ou foi renomeado somente por maiúsculas/minúsculas, são recusados na prévia: o journal desta versão opera arquivos e não converte a estrutura de diretórios. Resolva essa mudança estrutural manualmente antes de tentar novamente.

## Armazenamento e falhas

O armazenamento é local, por conteúdo SHA-256, com manifestos versionados e índice transacional SQLite. Conteúdo idêntico é compartilhado entre pontos. Tamanho lógico conta os arquivos de cada ponto integralmente; conteúdo físico compartilhado soma os bytes dos arquivos na área de blobs, incluindo eventuais resíduos de escrita interrompida. Essa medida não representa alocação por blocos do filesystem nem o custo total de banco, manifestos, staging e metadados. A captura limita o inventário protegido a 30 mil arquivos e 8 GiB; exceder um limite aborta a operação em vez de publicar um ponto parcial.

Layout abaixo da pasta de dados do launcher:

```text
.aurora/recovery/
  index.sqlite3
  blobs/sha256/<primeiros-dois-hex>/<sha256>
  manifests/<point-uuid>.json
  publish.lock
instances/<instância>/.aurora/recovery-identity.json
instances/<instância>/.aurora/recovery-stage/<transaction-uuid>/new/<índice>
instances/<instância>/.aurora/recovery-stage/<transaction-uuid>/old/<índice>
```

O SQLite usa `synchronous=FULL`, journal `DELETE`, chaves estrangeiras e schema versão 1. Versões de schema desconhecidas são recusadas. As tabelas `instances`, `points`, `refs`, `previews`, `journals` e `undo` registram associação, pontos, referências a blobs, prévias, journal da transação e o último ponto para desfazer. O manifesto versão 1 associa o ponto à identidade, registra o ambiente de runtime e inclui a lista `path`/`size`/`sha256`, perfil permitido, data, descrição, exclusões, escopo, contagem e tamanho lógico. Também guarda digests dos perfis completos para detectar mudanças, sem copiar seus campos não permitidos para o ponto. O índice guarda o hash do manifesto. Prévias persistidas incluem a impressão do estado atual e o hash do manifesto selecionado; alterar qualquer um exige nova revisão. A confirmação identifica a prévia gerada pelo serviço e não aceita um inventário de alterações arbitrário do frontend.

A exclusão não realiza coleta de blobs nesta entrega. Dados sem referências permanecem no disco, inclusive dados de capturas interrompidas que nunca viraram pontos visíveis. Essa escolha conserva backups vivos até haver uma coleta segura e auditável. Não exclua manualmente arquivos do armazenamento: a presença de um nome no índice não substitui a verificação de integridade do conteúdo.

Antes de restaurar, o serviço prepara o estado anterior e o conteúdo de destino. A reserva de espaço é conservadora e pode exigir espaço mesmo quando parte dos bytes já está compartilhada no CAS. Falta de espaço ou falha de preparação bloqueia a alteração da instância. O novo ponto do tipo `undo` e o staging são preparados antes de registrar a intenção transacional; o journal liga cada operação aos manifestos verificados de origem e destino. O perfil é mesclado com o documento atual e seus bytes anteriores ficam no staging somente para reverter a operação em andamento.

Cada substituição move o arquivo anterior para `old/<índice>` e publica o preparado de `new/<índice>`. No Windows, as trocas usam `MoveFileExW` com `WRITE_THROUGH` e sem substituir destinos inesperados; gravações sincronizam os dados antes de publicação. O journal só é encerrado na mesma transação SQLite que disponibiliza o novo desfazer. Uma interrupção anterior a esse commit reverte as trocas pelos originais, inclusive quando todas as trocas já ocorreram; não depende da memória do processo. A recuperação verifica o conjunto inteiro de alterações antes de começar a revertê-las e não sobrescreve uma edição externa inesperada. Arquivos temporários conhecidos e verificados são removidos após conclusão; diretórios vazios e staging interrompido antes do journal podem permanecer.

Enquanto um journal estiver pendente, o bloqueio comum impede iniciar jogo ou outra mutação. Abrir ou atualizar a área Recuperação tenta recuperar a operação sob o bloqueio e depois executa a consulta solicitada. Corrupção que impeça comprovar uma recuperação segura conserva o bloqueio e os dados. Testes exercitam interrupções determinísticas, disco indisponível/falha de staging e recuperação real no filesystem temporário; queda física de energia e saturação real do disco não foram homologadas.

Mensagens públicas distinguem espaço insuficiente, dados ausentes/corrompidos, prévia desatualizada, instância ocupada, caminhos/identidade inválidos e falha de escrita. A interface usa mensagens fixas em português, sem exibir stack trace ou erros de filesystem brutos.

## Compatibilidade e próximos passos

Doctor, Gemini, histórico, ações confirmadas e seu próprio desfazer continuam funcionando com os contratos existentes. O Doctor oferece acesso à área Recuperação, sem afirmar que um ponto resolverá uma falha. Não há migração ampla dos journals/backups anteriores para CAS. A adoção limitada e suas consequências estão na [ADR-027](engineering/ADR/ADR-027-manual-instance-recovery.md).

Na alpha.14, [Doctor e cópias de teste](diagnostic-lab.md) reutilizam esta identidade e este executor: pontos relevantes podem ser revisados no Doctor, e mudanças selecionadas da cópia podem ser aplicadas à origem com proteção e desfazer. Essas integrações não ampliam o escopo dos pontos. Não há upload, sincronização, restauração acionada por IA ou backup/restauração de mundos. Pontos automáticos antes de atualizações continuam **NÃO IMPLEMENTADO**; sua política de retenção e espaço permanece uma etapa própria.
