# Personalização do Aurora

O botão **Personalização**, no cabeçalho do launcher, abre a biblioteca e o campo único de código JSON. O Aurora somente lê esse formato; nenhuma IA é usada pelo recurso.

1. Abra **Jardim de cerejeiras**, **Lago de vidro**, **Aurora boreal**, um tema salvo ou um novo tema.
2. Edite o JSON ou use **Copiar instruções para IA** em um serviço de sua escolha. O texto copiado contém somente regras e o tema do editor; não inclui conta, credenciais ou caminhos do computador. Recursos de imagem/vídeo anexados fazem parte do tema copiado/exportado.
3. **Adicionar fundo ou vídeo** e **Adicionar logotipo** colocam os arquivos locais dentro do mesmo JSON. Não cole links da internet. Imagens PNG/JPEG/WebP são normalizadas para PNG; vídeo MP4/WebM deve ter até 20 MiB, Full HD e três minutos. A reprodução depende dos codecs do WebView. Vídeos são sempre silenciosos.
4. Use **Verificar código** e **Salvar na biblioteca**. Salvar o documento ainda não aplica a aparência. Alterar um tema existente cancela suas seleções e associações anteriores, exigindo nova prévia.
5. Escolha o escopo **Launcher inteiro** ou uma instância/modpack e clique em **Prévia de 10 s** no cartão salvo. Confirme **Manter tema** dentro do prazo. Sem confirmação, o padrão oficial é restaurado.

Associar um tema ao modpack torna a troca automática ao selecionar aquela instância. O tema associado tem prioridade sobre o geral enquanto a opção **Trocar tema ao selecionar um modpack** estiver ativa. É possível remover vínculos individualmente ou desativar a troca preservando as associações. Temas remotos de modpacks não são importados automaticamente.

**Restaurar padrão**, o botão fixo **Restaurar aparência**, **Ctrl+Shift+F12** e **Restaurar aparência padrão** no menu da bandeja recuperam o visual oficial. A biblioteca e os vínculos são preservados; a troca automática fica desligada até nova ação explícita. Depois de travamento ou fechamento inesperado, a próxima abertura usa o padrão. Fechar normalmente pelo menu da bandeja preserva um tema confirmado; fechar uma prévia sem confirmar restaura o padrão.

Importar/exportar permite compartilhar documentos completos. Limites: até 32 temas e 96 MiB na biblioteca; quatro recursos por documento, até 24 MiB somados; PNG de até 2 MiB e 8 milhões de pixels; documento até 34 MiB. PNG animado, SVG livre, URLs, caminhos, HTML, CSS e JavaScript não são aceitos. Cores e contraste precisam ser válidos. Fontes vêm do sistema. A confirmação e os controles de recuperação usam a aparência oficial.

Água, aurora e cerejeiras usam efeitos internos com intensidade/velocidade configuráveis. As pétalas têm quantidade fixa limitada; efeitos respeitam movimento reduzido do sistema e pausam com o documento oculto. A troca entre temas usa uma transição curta. Desempenho em cada GPU e comportamento de pausa do WebView ao minimizar ainda precisam de teste na máquina de destino.

Em uma sessão secundária, a biblioteca de temas pode ficar temporária para preservar a sessão principal. O aviso aparece no editor. Exporte o tema para guardá-lo nessa situação. A prévia em navegador de desenvolvimento também usa armazenamento temporário.
