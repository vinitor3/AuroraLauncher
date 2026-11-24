# Doctor e cópias de teste

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. Resultados executados e limitações em [AUR-1167](https://github.com/vinitor3/AuroraLauncher/blob/088bf5b435b1b63e842c11ff1c01648b5e38bced/docs/task-aur-1167-doctor-lab.md). Decisão: [ADR-028](engineering/ADR/ADR-028-doctor-recovery-lab.md).

## Recuperação pelo Doctor

O Doctor exibe uma seção compacta de mudanças recentes e pontos de restauração. Os inventários são observados na preparação e antes do início da JVM, sob o bloqueio comum de mutação. Cada registro associa UUID da tentativa, UUID verificado da recuperação, fingerprint, arquivos e campos permitidos do perfil. Os estados de preparação e lançamento são separados: falhar antes de iniciar não equivale a ter executado a JVM.

As mudanças comparam hashes e o inventário das tentativas registradas. A tela informa quando só existe observação da preparação, quando falta inventário anterior, quando o estado atual mudou ou quando o ponto está indisponível. Os motivos de relevância comparam conteúdo e identidade; nome e data não comprovam que um ponto pertence à tentativa. Uma falha depois de uma atualização não comprova que ela causou a falha.

**Revisar restauração** abre a prévia já existente de Recuperação. Confirmar exige revisar o escopo e reconhecer que mundos não estão incluídos. O backend revalida o ponto e o estado atual e mantém a proteção para desfazer. A observação do Doctor não modifica o escopo dos pontos manuais. Falha ao salvar histórico não autoriza inventar uma associação nem deve impedir uma inicialização saudável.

Históricos anteriores sem identidade verificável não são promovidos a evidência de mudanças. O diagnóstico original continua com seu comportamento compatível para registros legados. Novos registros acompanham a identidade física persistente da pasta em renomeações; exclusão e recriação não herdam tentativas. O inventário detalhado fica em armazenamento local separado do JSON sanitizado compartilhado com Gemini.

## Criar e usar uma cópia

No gerenciamento, abra **Cópias de teste**, ou use o acesso pelo Doctor. Feche o jogo da origem, escolha criar e aguarde as etapas de verificação e cópia. A origem permanece bloqueada contra instalações, updates, ações do Assistente e outra inicialização durante a captura. Processos Minecraft externos também são verificados antes e depois de adquirir o bloqueio.

A cópia recebe identificador reservado `aurora-test-<uuid>`, pasta própria em `instances`, identidade de recuperação nova, registros de tentativas e logs próprios. Sua proveniência registra o UUID da origem, nome na criação e fingerprint usado na captura. Renomear a origem não altera a associação; recriar uma pasta com o mesmo nome não a recupera. O identificador da cópia é conservado, mesmo que o nome de apresentação seja editado.

São copiadas as configurações elegíveis e mods do escopo de recuperação, além dos arquivos locais de runtime e pacotes necessários. Perfis do launcher são reconstruídos somente com campos permitidos. Contas, sessões, histórico, logs, `.aurora` interno da origem e arquivos globais de autenticação não são copiados. Configurações sensíveis seguem a exclusão conservadora da recuperação; módulos administrados são instalados pelo launcher no lançamento, conforme a matriz ativa. Arquivos são cópias físicas verificadas por SHA-256; hardlinks, symlinks e junctions não são meios de compartilhamento.

**Mundos não são copiados por padrão.** É possível criar mundos descartáveis diretamente no jogo da cópia. Para usar um mundo existente, marque a opção explícita e selecione o mundo: seus arquivos são copiados de forma independente com o jogo encerrado. Ele continua fora do escopo de aplicação à origem e dos pontos de recuperação.

Na cópia, selecione mods para ativar/desativar ou ajuste os campos de configuração já suportados. A lista de opções usa a allowlist existente de configurações JSON locais de vídeo: distância de renderização, FPS, distância de entidades, névoa, gráficos detalhados, indicador de FPS e culling. Somente campos existentes com tipo e limites válidos aparecem; não existe editor de comandos ou ferramenta de shell. Ações e sugestões de IA continuam exigindo a autorização específica de seus próprios fluxos.

**Iniciar cópia** usa a engine e a autenticação normais. **Analisar no Doctor** consulta a tentativa da cópia. A seção de resultados registra as tentativas e permite consultar as mudanças efetivamente observadas; edições posteriores não alteram os inventários históricos. Encerramento normal ou ausência do erro anterior é uma observação, não garantia de solução. O recurso é uma cópia para testes, não isolamento de segurança do sistema operacional.

## Aplicar à origem

Selecione as mudanças suportadas e escolha **Revisar alterações para aplicar à instância original**. Apenas alternâncias de mods com os mesmos bytes e campos de configuração permitidos são transferíveis. Outros campos no mesmo JSON não acompanham o campo escolhido. Mundos, logs, caches, instalação de runtime, credenciais e identidade não são transferidos.

A origem é encontrada por UUID verificado, inclusive depois de renomeada. Origem e cópia devem estar encerradas e bloqueadas. O backend compara a origem atual com o fingerprint da criação; uma divergência bloqueia a aplicação e requer novo teste do estado atual. A prévia é persistida e ligada à cópia e aos estados observados. Mudanças relevantes na cópia ou origem após a revisão invalidam a confirmação.

Após confirmação explícita, o mesmo CAS e executor transacional da Recuperação protegem o estado anterior, preparam staging, gravam o journal e aplicam somente o alvo revisado. Sem espaço ou proteção íntegra, a origem permanece intacta. **Desfazer na Recuperação** usa a prévia e confirmação normais sobre a origem verificada. Não há aplicação automática a partir de uma resposta de IA.

## Interrupções, armazenamento e limpeza

Proveniência e fases de publicação ficam em `.aurora/diagnostic-lab` fora das pastas dos jogos. Uma cópia incompleta não é apresentada como pronta e não pode iniciar. Reiniciar o launcher conserva sua identificação para limpeza. A exclusão registra intenção durável, valida propriedade/caminhos e permite retomar uma interrupção; ambientes em execução são bloqueados. A limpeza não percorre a origem nem apaga o CAS compartilhado da recuperação.

O espaço mostrado é tamanho lógico de arquivos, não alocação física em blocos. O limite de captura protegida continua sendo o da recuperação (30 mil arquivos/8 GiB); a cópia de runtime/mundo tem limites próprios. Conteúdo CAS sem referências continua conservado pela política da entrega anterior, portanto excluir uma cópia não implica recuperar todos os bytes usados pelos seus pontos. Interferência externa, corrupção e caminhos inseguros interrompem a operação com mensagem pública fixa.

## Limites da validação

Testes automatizados usam instâncias temporárias, arquivos sintéticos e falhas injetadas. Eles verificam bytes, seleções, identidade, conflitos, corrupção, locks e retomada de operações. Não demonstram partida real de Minecraft, comportamento de mods de terceiros, queda física de energia, disco fisicamente cheio ou autenticação/voz online. Consulte o task file para resultados concretos; esses cenários reais permanecem **PRECISA DE TESTE**.
