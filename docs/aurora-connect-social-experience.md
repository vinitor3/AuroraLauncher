# Aurora Connect — amigos, sessões públicas e entrada simples

Requisito explícito confirmado e detalhado pelo usuário em 24/10/2026. O [incremento87](aurora-connect-social-87.md) implementa **PROTÓTIPO** local de amigos, busca de nick Aurora, presença opcional e anúncios públicos/para amigos, com SDK/regras em emuladores e interface. Essas capacidades continuam **IMPLEMENTADO MAS NÃO HOMOLOGADO** para publicação e desligadas no build normal. **Entrar sem copiar convite**, backend de autorização social do transporte e seleção automática de caminho continuam **NÃO IMPLEMENTADO / PRECISA DE TESTE**. Este documento define a experiência final de R4; não promove o laboratório à liberação pública.

O registro87 separa a GUI original, a subrodada de capa Modrinth no executável e a correção posterior do hot reload do preview. Os seis testes causais dessa correção não reatribuem a GUI anterior ao preview novo; testes locais de dados sociais também não significam isolamento de todas as consultas nativas de rede do launcher.

Autoridade: [especificação Connect](engineering/AURORA_CONNECT_SPEC.md), [roadmap](engineering/MASTER_ROADMAP.md), tarefas existentes [AUR-R4-004/005/008](engineering/CODEX_TASKS/AUR-R4-CONNECT-BACKLOG.md), [política de dependências/custos](engineering/AURORA_CONNECT_DEPENDENCIES.md) e [prompt consolidado](engineering/CODEX_PROMPTS/AUR-R4-CONNECT-V2.md). Permanecem todas as fases, prioridades, gates e os nove pares Core/Skins; não há Companion/HUD ou ampliação da matriz.

## Experiência exigida

O fluxo final deve se parecer com uma lista de amigos e sessões de jogo: ver a disponibilidade autorizada, escolher **Entrar** ou **Pedir para entrar**, conectar e receber um resultado claro. O usuário não precisa copiar chaves, endereço, JSON ou aprovação do laboratório no fluxo normal suportado. A troca manual atual é ferramenta de desenvolvimento, não o desenho final da experiência social.

Detalhamento visual e funcional solicitado pelo usuário, a partir das referências anexadas:

- A pessoa pesquisada precisa ter conta no **Aurora Launcher**. Buscar pelo nick do Aurora, sem procurar/adicionar jogadores Minecraft/Mojang só porque usam o mesmo nome. A primeira implementação faz busca exata autenticada pelo nick completo, sem enumeração global; ampliação futura da busca exige preservar privacidade e limites.
- A lista de amigos precisa apresentar pedidos recebidos/enviados, aceitação, cancelamento/recusa, remoção e bloqueio. No lugar de uma indicação genérica de “Disponível”, a presença autorizada pode mostrar o nome do modpack em execução no Aurora, com expiração e estado desconhecido/oculto quando não houver evidência atual. Presença não prova jogador dentro do mundo.
- As sessões compartilhadas com a comunidade devem aparecer em **lista ou grade de cartões médios**, com foto/capa permitida do modpack/servidor, título, nome do modpack, versão do Minecraft, quantidade de participantes e somente informações úteis adicionais, como loader e política de ingresso. Não acrescentar contadores de votos, publicidade, IP para copiar ou estatísticas sem fonte apenas por existirem na referência visual.
- O anfitrião escolhe no Aurora se o mundo/sessão fica privado, para amigos ou público. O anúncio só é exposto conforme essa escolha; abrir descoberta não altera automaticamente o servidor do jogo. No produto final, **Entrar** obtém a autorização interna e conecta sem exigir troca manual de convite; **Pedir para entrar** é reservado à política que exige aprovação.
- Imagem, título, nick e metadados são conteúdo não confiável. O catálogo não concede carregamento de URLs arbitrárias, execução de HTML, acesso a perfis privados ou autoridade sobre o transporte. Chat e importação de contatos não estão implícitos na comparação com a Steam.

O anfitrião escolhe explicitamente a visibilidade e a política de ingresso da sessão. Criar uma sessão começa privado; torná-la disponível a amigos ou ao público exige ação clara do responsável. Abrir uma sessão não altera sozinho o servidor Minecraft, whitelist, autenticação, modpack ou configurações pessoais.

| Visibilidade | Quem pode descobrir | Entrada esperada |
| --- | --- | --- |
| **Privada** | Somente destinatários autorizados; não aparece no catálogo público ou para todos os amigos. | Convite direcionado; útil também para jogar com alguém que ainda não é amigo. |
| **Amigos** | Amigos aceitos do anfitrião, conforme sua preferência de presença. | **Entrar** sem convite manual quando a sessão permite ingresso; **Pedir para entrar** se o anfitrião escolheu aprovação. |
| **Pública** | Usuários autorizados a consultar o catálogo limitado de sessões que seus anfitriões publicaram voluntariamente. | **Entrar** sem convite manual, ou **Pedir para entrar** quando a política anunciada exige aprovação. |

As visibilidades definem descoberta; a política de ingresso define se a ação do convidado basta ou se precisa de aprovação do anfitrião. São decisões distintas. Uma sessão pública não exige ser amigo; uma sessão para amigos não exige convite individual nem nova confirmação do anfitrião a cada ingresso quando ele já escolheu permitir entradas de amigos. O convite continua disponível como recurso adicional, não como obrigação universal.

Amizade exige aceitação das partes. Bloqueio prevalece sobre amizade, convite e publicidade. Recusar ou cancelar um pedido não publica a sessão, não concede amizade e não autoriza uma conexão. Pedidos têm prazo, limite de pendências e frequência, cancelamento, recusa e proteção contra repetição; os valores concretos precisam do contrato e dos testes do backend antes da implementação remota. Não importar contatos nem enviar solicitações automaticamente.

## Autorização permanece obrigatória

**Entrar sem convite manual** significa que o aplicativo obtém e apresenta internamente a autorização individual necessária. Não significa uma ponte aberta nem ingresso anônimo ao transporte. A escolha explícita de visibilidade/política pelo anfitrião pode autorizar a emissão automática para participantes elegíveis; isso deve ser um contrato novo e testado, sem reutilizar ou relaxar a aprovação manual do laboratório.

No ingresso, o backend confiável deve verificar identidade social, relação de amizade quando exigida, bloqueios, política e revisão atuais, disponibilidade/capacidade, destinatário, chave do transporte, sessão, serviços e prazos. A autorização emitida é vinculada ao participante e protegida contra reutilização. O cliente ou um anúncio público não pode declarar sozinho que alguém é amigo ou elegível. O vínculo entre conta e chave requer prova real; um nick informado não a substitui.

Mudança de política/visibilidade deve invalidar anúncios e pedidos incompatíveis e impedir novas admissões indevidas. O responsável consegue remover um participante e encerrar a sessão; isso revoga suas permissões e aguarda a limpeza correspondente. O contrato de implementação deve especificar, antes de habilitar a função, o efeito de desfazer amizade/bloquear ou restringir uma sessão sobre ingressos já autorizados e conexões ativas; o padrão conservador para bloqueio é revogar o participante afetado. Nenhuma dessas ações deve reviver convite consumido, renovar o prazo de uma conexão ou encerrar o jogo de terceiros por inferência.

Os convites assinados, consumo único, revogação, isolamento e encerramento já exercitados são fundamentos aproveitáveis para esses modos. A emissão social, reentrada, alteração dinâmica de participantes e descoberta ainda precisam de seus próprios contratos e testes. Os grupos estáticos e as chaves temporárias atuais não comprovam um serviço de amigos ou de sessões públicas.

## Catálogo público e presença

**Lista global** significa catálogo de sessões explicitamente públicas, não listagem global de contas, amigos, presença privada ou documentos internos. A regra anterior de negar enumeração global de usuários permanece. Consultas públicas precisam de projeção mínima própria, paginação limitada, frequência limitada, expiração lógica e retenção comprovadas. Não tornar as sessões privadas legíveis para construir essa lista.

Antes de entrar, expor somente os campos aprovados para descoberta, como identificador público opaco da sessão, título limitado, versão/compatibilidade e disponibilidade declarada. Conteúdo livre é não confiável e deve ser limitado e exibido como texto. Não publicar tokens, convites de ingresso, chaves privadas, ofertas de negociação, destinos locais ou grafo de amizades. Endereços necessários à conexão são entregues somente no fluxo autorizado; uma conexão direta pode revelar IP ao participante e não oferece anonimato.

Anúncio/presença não prova servidor acessível ou jogador dentro do mundo. Mostrar separadamente disponibilidade anunciada, transporte autenticado, servidor respondendo, jogador no mundo e voz de cada mod. Encerrar o host ou perder sua presença remove ou torna indisponível o anúncio conforme o prazo contratado, sem prometer hospedagem 24 horas.

Firebase/Worker existentes não concedem automaticamente backend, quotas ou regras para essas funções. Preparar contratos, regras positivas/negativas e testes locais é autorizado; publicar regras/infraestrutura ou usar serviço externo novo depende das permissões e da verificação de custo previstas no prompt. Firestore não transporta jogo, áudio ou arquivos. Sem backend elegível, o recurso social permanece desligado e a limitação é explícita.

## Escolha automática da conexão

Após verificar a elegibilidade e obter a autorização de ingresso, o objetivo de produto é selecionar automaticamente o melhor caminho **observado e permitido** para aquele par, sem exigir que o usuário escolha IPv4, IPv6 ou copie endereços. Conectividade, autorização, capacidades TCP/UDP necessárias, segurança e custo são requisitos prévios; um candidato rápido que não os atende é inelegível.

A política mantém prioridade de conexão direta e compara os candidatos elegíveis com medidas do transporte real: tempo de conexão, RTT, variação, perda/disponibilidade e estabilidade, com amostra suficiente e histerese para evitar trocas contínuas. Não usar um ping ICMP isolado nem um endereço candidato como prova do caminho de jogo. Os pesos, limites de medição e condições de troca serão fixados e testados no incremento de seleção, sem inventar números de desempenho neste requisito.

Relay entra apenas quando gratuito e autorizado para o uso pretendido, nos termos e quotas verificados; não há relay público de produção aprovado por este documento. Sua disponibilidade não obriga a escolhê-lo nem permite mudança paga silenciosa. Se a política priorizar direto diante de um relay com RTT menor, a escolha deve ser explicável e não anunciada como menor ping absoluto. O compromisso é a melhor conexão elegível segundo a política, não garantir o menor ms possível em toda internet.

A interface mostra o caminho efetivamente observado e dados recentes, com estado desconhecido quando faltam. Uma migração só pode preservar sessão/serviços se a biblioteca e testes demonstrarem continuidade e mudança de capacidade UDP; caso contrário, apresentar reconexão ou falha clara. Não trocar de autoridade, renovar permissões ou reabrir serviço revogado para melhorar uma métrica. RTT da rede não é atraso total de voz nem desempenho do servidor Minecraft.

## Incrementos e aceite

Este refinamento pertence aos IDs existentes: **004** seleção/conectividade, **005** amigos/presença/descoberta/ingresso/interface e **008** robustez/privacidade/liberação. Não cria outra ordem de roadmap nem torna homologação de voz pré-requisito para implementar o fluxo TCP básico já autorizado.

| Caso obrigatório | Resultado esperado antes de anunciar a capacidade |
| --- | --- |
| Privada e não amigo convidado | Só o destinatário autorizado descobre/entra; terceiros não enumeram sessão nem reutilizam o convite. |
| Amigos com ingresso aberto | Amigo aceito vê **Entrar** e conecta sem copiar convite ou obter aprovação individual adicional; não amigo é recusado. |
| Aprovação de ingresso | **Pedir para entrar**, aceitar/recusar/cancelar/expirar funcionam; nenhuma autorização nasce antes da decisão exigida. |
| Sessão pública | Aparece apenas após publicação voluntária, permite o ingresso segundo a política e sai da lista após fechamento/expiração. |
| Bloqueio e alteração de política | Corridas de emissão/admissão, pedido pendente e participante ativo respeitam a política e a revogação especificadas; não vazam anúncios privados. |
| Regras e limites | Testes positivos/negativos do backend impedem enumeração de contas, escrita como outro host, falsificação de amizade, dados excessivos, replay e leitura após revogação; paginação/quotas/retenção são verificadas. |
| Seleção automática | Candidatos com latência variável, indisponíveis ou sem capability são filtrados/selecionados conforme política; UI reflete o caminho real e não oscila por uma única amostra. |
| Falha de direto/relay | Usa somente alternativa permitida comprovada ou informa falta de caminho; indisponibilidade/quota não ativa cobrança nem muda permissões. |
| Fluxo completo | Interface real entre dois computadores/redes distintas, dois jogadores, encerramento/queda e recursos liberados; fixtures locais continuam evidência separada. |

Esses são critérios de liberação **PRECISA DE TESTE**. O [domínio87](aurora-connect-social-87.md) registra os resultados locais já executados de relações, regras, anúncios e interface, e suas limitações; a autorização/entrada social automática e a liberação remota não estão demonstradas por eles. TCP, UDP, Minecraft, Simple Voice Chat, Plasmo Voice, sincronização por manifesto e homologação dos nove pares continuam avaliados separadamente.

Rollback desta atualização remove somente o refinamento documental revisado; não modifica regras remotas, contas, configuração pessoal, executável ou transporte. A implementação futura deve oferecer desligamento independente das entradas sociais e revogação/cleanup, preservando os contratos de segurança e os caminhos já comprovados.
