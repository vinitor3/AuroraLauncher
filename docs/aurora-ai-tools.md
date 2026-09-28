# Aurora AI Tools

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. O Assistente e o Doctor podem consultar Gemini e apresentar sugestões de ações locais. A IA propõe; o jogador revisa cada alteração e confirma separadamente. [Evidência da tarefa](task-ai-tools.md).

## Fluxo do jogador

Uma consulta explícita envia a pergunta e o contexto reduzido da instância. Receber a resposta não altera arquivos. “Revisar sugestão” pede ao broker Rust uma prévia com alvo, estado anterior, estado proposto e avisos. A confirmação usa o plano preparado; cancelamento ou expiração o invalida. Instância alterada, nova execução ou troca de contexto exige nova revisão.

O histórico local mostra diagnósticos, orientações e alterações. “Revisar desfazer” produz outra prévia e exige nova confirmação. Uma operação pendente é apresentada como pendente, com possibilidade de recuperação quando o backup validado está disponível. A interface não presume que uma sugestão, confirmação ou saída normal do processo tenha corrigido o problema original.

## Capacidades atuais

| Capacidade | Escopo autorizado |
| --- | --- |
| `remove_mod` | Um JAR ou JAR desabilitado presente no inventário nativo, com identidade reconhecida e sem proteção de módulo gerenciado; preserva o original em armazenamento local da ação. |
| `set_setting` | Um valor booleano ou numérico exposto de um JSON diretamente em `config/`, dentro de tipo e limites conhecidos. O broker altera somente a opção aprovada e serializa o JSON. |
| Restauração | Reposição dos bytes originais da ação identificada por journal autenticado, desde que o estado atual corresponda à alteração registrada ou a um estado interrompido validado. |

Opções expostas incluem distância de renderização, limite de FPS, distância de entidades, névoa, gráficos detalhados, exibição de FPS e ocultação de entidades/blocos fora de visão. Chaves e ancestrais associados a autenticação, conta, segurança, rede, permissão e Aurora são excluídos. Arquivos TOML/CFG, configurações arbitrárias, parâmetros de Java/RAM, shell, download, instalação e execução de código não fazem parte do contrato.

O inventário examina até 256 entradas por diretório, 128 mods, 16 JSONs de configuração e 64 opções. Cada JAR tem limite de 64 MiB, cada JSON 256 KiB e o orçamento total de leitura é 256 MiB. Metadados de mod são lidos com limite próprio de 64 KiB; ausência ou formato não reconhecido resulta em identificação genérica protegida. Um inventário limitado é marcado como tal e não demonstra completude.

## Fronteira do modelo

O Worker exige sessão Firebase Aurora. A chave Gemini permanece no Worker, enviada ao provedor pelo cabeçalho apropriado. `/v1/assistant`, `/v1/doctor/advice` e `/v1/ai/tools` usam o mesmo transporte com cancelamento, limites de leitura e mensagens públicas estáveis. Doctor e Tools compartilham seis consultas por minuto por conta na memória do Worker; esse limite não é uma quota global persistente.

Tools envia a pergunta, nomes/versões de metadados sanitizados, opções permitidas e seus valores, IDs temporários de alvos, categorias do Doctor e as 12 entradas mais recentes do histórico canônico. Nomes, pergunta e histórico são dados não confiáveis. Não são enviados arquivos, JARs, logs, caminhos, nome da instância, identificador de execução, ID da conta ou relatório integral. O token de sessão autentica o Worker e não integra o conteúdo Gemini.

O histórico possui duas saídas distintas: `read_history` entrega a orientação aceita para exibição local; `safe_for_model` reconstrói títulos e resumos fixos usando categorias, tipo da ação e rótulos permitidos. A prosa de uma resposta anterior nunca é reutilizada nessa projeção. O modelo recebe o que foi proposto/tentado e o resultado observado, sem um histórico de instruções livres da IA.

O alias `gemini-flash-latest` é tentado primeiro. Somente ausência do alias ou 503 permite uma tentativa adicional de `gemini-3.8-flash` no mesmo prazo total; quota ou autorização recusada encerram a chamada. O modelo efetivamente retornado é validado e pode ser exibido pela interface. O servidor tem 35 segundos para autenticação, entrada, provedor e saída; o cliente tem 40 segundos incluindo obtenção de token. A resposta estruturada é validada no Worker e novamente no desktop, independentemente do schema enviado ao Gemini.

Erros distinguem sessão, função ainda não publicada, chave não configurada/recusada, quota, modelo indisponível, indisponibilidade do serviço, prazo, entrada obsoleta e resposta inválida. Corpos de erro do provedor, chaves, exceções e stacks não são repassados ao jogador.

## Autorização e recuperação locais

O contexto nativo vale por dez minutos e pertence à janela e à instância. O plano vale por cinco minutos, é de uso único e contém digest da prévia. O broker não aceita um caminho vindo do modelo: associa o ID temporário a um alvo do próprio inventário e reconstrói o conteúdo proposto. Antes de aplicar, confirma hashes atuais, jogo encerrado e bloqueio exclusivo de mutação da instância. Core, Skins e conteúdo protegido não podem ser removidos por esse fluxo.

Leituras e alterações verificam confinamento, ancestry sem symlink/junction e links físicos. No Windows, handles dos diretórios protegem as operações contra troca de diretório durante a mutação. Os originais ficam em `.aurora/ai-tools/<ação>/`; plano, hashes e identidade da instância são autenticados por HMAC com chave local do launcher fora da instância. A chave nunca entra no modelo ou frontend. Alterações posteriores impedem restauração automática; conteúdo de recuperação permanece preservado.

O histórico resumido fica em `.aurora/ai-tools-history`, com até 64 registros/128 KiB, gravação sob lock próprio e snapshot anterior recuperável. A identidade do diretório permite preservar registros ao renomear e iniciar histórico vazio numa cópia da instância. Registros antigos saem do resumo quando atingem o limite. Isso não apaga os backups das ações: não há limpeza automática desses arquivos nesta implementação. Recibos autenticados das ações permitem consultar recuperação independentemente de uma falha no resumo genérico; um journal inválido é ignorado na listagem e nunca autoriza restauração.

Uma ação só é comparada com outra execução que tenha começado depois dela. Mesma execução ou análise atrasada não produz conclusão nova. `unresolved` requer falha posterior e repetição de uma categoria conhecida; `unknown_cause` sozinho não basta. `normal_exit` informa apenas encerramento normal. Cancelamento, resultado desconhecido ou evidência diferente/insuficiente permanecem `inconclusive`. Categorias repetidas não identificam automaticamente um mod culpado.

## Limites da evidência

O diagnóstico local é determinístico; a orientação Gemini pode ser incorreta. Preview, validação de schema e confirmação reduzem o alcance da ação, mas não comprovam que a sugestão seja apropriada para um modpack. A recuperação protege os arquivos modificados por estas duas capacidades, não é um backup integral de mundo ou instância. Dados sensíveis presentes em perguntas escritas pelo jogador não são automaticamente anonimizados.

Conforme [task-ai-tools.md](task-ai-tools.md), Assistente, Doctor e Tools responderam por Gemini real em preview remoto com entradas sintéticas; houve também indisponibilidade intermitente do provedor. O Worker foi atualizado em produção e o instalador alpha.13 foi gerado pelo fluxo oficial. Essas provas não homologam Minecraft/loader/Java, TTS/microfone reais, qualidade de uma proposta específica ou uma ação completa na UI nativa do jogador.
