# Interface do launcher, memória e mundos LAN

**IMPLEMENTADO MAS NÃO HOMOLOGADO**

Em 19/11/2026, o checkout principal apontava para `dc2ec5a`. A implementação partiu do Aurora Launcher 0.1.1-alpha.15 (`220cdf6`), que já reunia as telas Java, Social e Connect e o contrato visual de `3f30785`. A matriz usa Core 1.1.5 e Skins 0.3.1 nos nove pares Minecraft/loader.

## Uso

- A navegação, o título e o rodapé cabem na janela de desktop. Listas de modpacks, versões de Java, pedidos e skins aproveitam o espaço restante. Em janelas baixas, clicar em um resultado de **Descobrir** abre a ficha com versão e ações de instalação.
- **Java & engine** define o limite de memória para modpacks atuais que usam o padrão e para novas instâncias. Marcar **Incluir modpacks com memória personalizada** habilita **Aplicar a todos**, mesmo mantendo o valor mostrado. A alteração vale ao iniciar o jogo novamente. **Editar → Memória** permite criar uma exceção ou voltar ao padrão. [Detalhes da memória](java-engine.md).
- **Amigos e comunidade** mostra a quantidade de pedidos recebidos ativos no menu lateral. Clicar nesse botão com pedidos pendentes abre **Recebidos**, inclusive durante a edição de um anúncio. Para compartilhar, abra a partida de um jogador para LAN no Minecraft iniciado pelo Aurora. Em **Compartilhar mundo**, encontre o mundo e escolha amigos ou comunidade. A descoberta usa a versão e a porta verificadas da instância compatível. O anfitrião revisa a conexão. [Fluxos de amizade, publicação e entrada](aurora-connect-social-experience.md).
- No **Guarda-roupa**, use **Explorar**, **Minhas skins**, **Equipada** ou **Importar**. No catálogo, digite o número da página e pressione Enter ou **Ir**. O filtro de Aurora Studio atua no catálogo do desktop. A biblioteca local e a skin equipada conservam os itens já escolhidos pelo jogador. [Fontes de skins](skin-sources.md).

## Verificação

- Layout no Edge em 1240 × 800 e 960 × 620, com o frontend React, uma sessão de teste e respostas simuladas de IPC e catálogo. Nos temas padrão e Lago de vidro, as cinco seções principais couberam na janela com 24 instâncias, seis versões de Java, paginação de modpacks e dois pedidos de amizade. Enter no botão Editar abriu o editor correto. O guarda-roupa também foi exercitado com 50 skins salvas e salto direto da página 1 à 26.
- Regras e SDK do Firestore no emulador com três contas de teste. Dois pedidos, um aceite e um cancelamento produziram a sequência **0 → 1 → 2 → 1 → 0**. Publicação para amigos e para a comunidade foi exercitada separadamente.
- Os builds do desktop padrão e do perfil Connect online passaram, assim como a formatação e o Clippy. Os 33 testes nativos de memória e inicialização passaram, incluindo herança, substituição das personalizações, edição em duas janelas e revisão da memória antes do início da cópia Connect.
- A suíte nativa completa com todas as features somou 947 testes aprovados e 99 ignorados pelo próprio conjunto, com zero falhas. No Windows, usamos a toolchain MSVC da alpha.15 e `cargo test --all-features --locked --offline --jobs 1 -- --test-threads=1` no manifesto desktop. A primeira compilação paralela esgotou a memória desta máquina.
- **PRECISA DE TESTE** para uma partida Minecraft entre dois computadores com este novo fluxo. A descoberta automática cobre mundos LAN iniciados em perfis Fabric e Forge reconhecidos pelo Aurora. A edição da interface usa o serviço e o transporte existentes.
