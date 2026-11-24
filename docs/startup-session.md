# Inicialização, sessão e console Windows

Estado da fonte posterior à alpha.6: **IMPLEMENTADO MAS NÃO HOMOLOGADO**.

## Sessão

A configuração pública local é resolvida antes de observar o Firebase Auth.
Erro ou ausência de resposta do IPC em cinco segundos usa a configuração
embarcada, sem trocar o provedor depois de iniciar a restauração.

Enquanto a autenticação é desconhecida ou o perfil está carregando, somente
a tela animada do Aurora é renderizada. Um callback explícito sem usuário
abre o login. Usuário autenticado e perfil de mesmo UID liberam o launcher;
nenhum perfil guardado em localStorage é usado para presumir autenticação.

O limite de quinze segundos da restauração apresenta erro com ações de tentar
novamente ou sair da sessão, sem spinner infinito e sem logout automático.
Promessas atrasadas são ignoradas após logout, troca de conta, timeout ou
desmontagem. Atualizações de aparência também conferem o UID autenticado.

No cadastro, a observação aguarda a criação do documento do perfil antes de
consultá-lo. Credenciais recusadas mantêm o formulário de login. A animação
não impõe atraso mínimo e respeita `prefers-reduced-motion`.

## Animação de carregamento

Reforço visual na fonte posterior à alpha.7: dois anéis com partículas giram
em sentidos opostos, o logo flutua e pulsa, três pontos alternam a luminosidade
e uma faixa indeterminada percorre a barra. As animações são CSS e não adicionam
espera, timers ou dependências à restauração de sessão.

Antes, `prefers-reduced-motion: reduce` desligava todos os indicadores e deixava
a barra completamente preenchida. Agora esse modo elimina deslocamento, escala
e rotação, oculta a barra e mantém apenas a mudança suave de opacidade dos pontos
(ciclo de 2,4 s). Não modifica configurações do Windows nem força movimento
espacial para quem prefere reduzi-lo. O erro remove os indicadores de carga e
interrompe os ciclos decorativos; retentativa e login continuam disponíveis.

Fixture local: `/examples/startup-motion.html`, usando o componente e CSS de
produção sem autenticação. Permite comparar amostras reais de animação, simular
o media query reduzido e verificar os callbacks de erro. A simulação altera
somente a regra CSS da página de teste, nunca a preferência do sistema.

## Console e diagnóstico

O executável Windows de release usa `windows_subsystem = "windows"`; o console
do ambiente de desenvolvimento continua disponível. Java de detecção, Forge e
jogo são iniciados com `CREATE_NO_WINDOW`, preservando suas janelas gráficas.
Não são escondidas nem encerradas janelas de terminal abertas pelo usuário.

As saídas de verificação/instalação continuam capturadas pelos comandos. A
saída padrão e de erro da JVM do jogo vai para `logs/aurora-java.log` dentro
da instância no Windows. Somente esse log é reiniciado a cada execução; o
launcher não grava argumentos de linha de comando nele. O leitor existente
continua selecionando o log mais recente, inclusive falhas antes de latest.log.
Outros sistemas operacionais mantêm seu comportamento de processo anterior.

Provas e pendências: [tarefa](https://github.com/vinitor3/AuroraLauncher/blob/088bf5b435b1b63e842c11ff1c01648b5e38bced/docs/task-startup-session.md).
