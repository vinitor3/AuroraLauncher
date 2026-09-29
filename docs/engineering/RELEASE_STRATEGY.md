# Release Strategy

## Estado confirmado

Base conferida em 29/09/2026: launcher `0.1.1-alpha.14`, commit `6a107b87adc4882bd464909e22650185f862ea5e`. O [registro da entrega](../task-aur-1167-doctor-lab.md) documenta build oficial, testes automatizados, matriz 18/18 e instalador local com SHA-256/manifesto. Integra recuperação manual, CAS restrito, Doctor e cópias de diagnóstico, preservando Tools, Assistente desktop, sessões, Core 1.1.5 e Skins 0.3.1. Instalação/execução do instalador, homologação completa em Minecraft e assinatura Authenticode permanecem **NÃO VERIFICADO**. Esta revisão não gerou nem publicou artefatos.

`v0.1.0-alpha` é o prerelease público histórico cujo instalador no Git divergia do asset GitHub, conforme a auditoria original. A divergência não descreve automaticamente alpha.14. Todas as versões ficam congeladas como artefatos históricos; não apagar, substituir ou corrigir retroativamente. [Inventário de releases](../../releases/README.md).

## Versionamento

- SemVer: `MAJOR.MINOR.PATCH-prerelease.N`.
- canais: `alpha` (quebra/validação), `beta` (feature complete da meta), `stable` (matriz/SLO/suporte).
- a base atual é `0.1.1-alpha.14`, sincronizada em root/package,
  desktop/package, Cargo, Tauri e lockfiles, conforme a entrega registrada;
  módulos in-game têm versões próprias e nove pares oficiais. A entrega
  verifica assinatura Ed25519 do Core; isso não prova assinatura de todo
  artefato nem assinatura do instalador;
- o preflight local verifica que os manifests do launcher concordam, que o
  checkout está limpo/integrado e que o nome do artefato ainda não existe.

## Pipeline de evolução e liberação

Os passos de proteção de branch, SBOM completo, assinatura e publicação abaixo
continuam gates quando ainda não houver evidência. O empacotamento local de
alpha.14 não os promove automaticamente a concluídos. Licença/proveniência,
Microsoft/Xbox/Minecraft entitlement e os gates R0/R1/R1.5/R2/R2.5/R3/R5 do
roadmap permanecem aplicáveis à distribuição pública.

1. PR com checks obrigatórios e review.
2. Merge limpo na `main` protegida.
3. Coordenador consulta `npm run release:status`, integra apenas entregas prontas,
   preserva mudanças em andamento e faz commit numa branch de integração própria.
   Executa uma vez `npm run release:package`, que invoca o único build oficial
   `npm run desktop:build` com preflights e matriz; [fluxo obrigatório](../release-workflow.md).
4. Tag anotada criada a partir do commit e preservada sem reescrita.
5. Conferir instalador, JARs, hashes e manifesto JSON gerados; completar SBOM CycloneDX/SPDX e proveniência antes da publicação.
6. Assinar updater/artefatos quando houver chave; verificar assinatura antes do upload.
7. Publicar GitHub prerelease imutável e anexar changelog/limitações/matriz.
8. Smoke de download/instalação; falha não substitui asset: cria nova versão.

## Custo zero

Verificar plano, elegibilidade, termos e quotas de CI/armazenamento antes de usar
infraestrutura externa; a existência de um workflow ou de software open source
não comprova gratuidade. Conservar limites de execução/retenção e não ativar
billing, excedente cobrável ou infraestrutura nova por causa desta revisão.

Para Aurora Connect, priorizar direto e admitir relay gratuito somente após
aprovação documentada para o uso pretendido, separando laboratório de distribuição.
Serviço com excedente cobrável permanece desabilitado sem bloqueio efetivo e
comprovado; alertas e contadores locais não bastam. Sem relay permitido, publicar
no máximo o modo direto homologado com limite explícito, nunca fallback pago
silencioso. [Dependências e zero-spend](AURORA_CONNECT_DEPENDENCIES.md).

## Assinaturas

- Tauri updater exige par de assinatura; chave privada somente em secret de release, pública embutida no app.
- Code signing Windows tradicional reduz SmartScreen, mas certificado costuma ter custo. Enquanto não houver opção gratuita confiável, declarar “não assinado”, publicar hash/proveniência e não fingir confiança de publisher.
- Chave de updater e certificado Windows são controles distintos.

## Auto-updater

Adiado até:

- versão/artefato imutáveis;
- endpoint `latest.json` por canal;
- assinatura fail-closed e teste de chave errada/replay/downgrade;
- download para staging, verificação, instalação e rollback/recovery;
- consentimento explícito na alpha.

## Checklist mínimo

- [ ] branch protegida e checks verdes;
- [ ] versão única e tag inédita;
- [ ] changelog e limitações;
- [ ] matriz runtime atualizada;
- [ ] build limpo/reprodutível documentado;
- [ ] SHA-256/SBOM/proveniência;
- [ ] NSIS instalar, iniciar e desinstalar em Windows limpo;
- [ ] nenhum secret/log/conta no artefato;
- [ ] asset publicado corresponde exatamente ao hash anunciado;
- [ ] release anterior permanece intacto.

Para um incremento Connect, acrescentar somente os gates do alcance anunciado:
TCP, UDP, cada mod/versão/loader, dedicado/LAN, modo misto e caminhos de rede são
homologações distintas. O recurso não validado permanece experimental e
desativado ao usuário por padrão. O caminho relay deve ser observável, gratuito
e autorizado para distribuição; uma PoC de laboratório não libera R4 inteira.
Preservar a matriz Core/Skins existente e os recursos alpha.14 durante a integração.
