# Release Strategy

## Estado confirmado

`v0.1.0-alpha` é um prerelease público e a CI do commit tagueado passou. Entretanto, o instalador no Git e o asset GitHub sob essa versão não são o mesmo arquivo. Essa versão fica congelada como artefato histórico; não apagar, substituir ou “corrigir” retroativamente.

## Versionamento

- SemVer: `MAJOR.MINOR.PATCH-prerelease.N`.
- canais: `alpha` (quebra/validação), `beta` (feature complete da meta), `stable` (matriz/SLO/suporte).
- a linha de desenvolvimento atual é `0.1.1-alpha.6`, sincronizada em
  root/package, desktop/package, Cargo e Tauri; os módulos in-game possuem
  versões próprias e uma matriz assinada;
- o preflight local verifica que os manifests do launcher concordam, que o
  checkout está limpo/integrado e que o nome do artefato ainda não existe.

## Pipeline

1. PR com checks obrigatórios e review.
2. Merge limpo na `main` protegida.
3. Build oficial somente por `npm run desktop:build`, em branch de integração
   limpa e depois do commit de produto.
4. Tag anotada criada a partir do commit e preservada sem reescrita.
5. Gerar instalador, JARs, `SHA256SUMS`, manifesto JSON, SBOM CycloneDX/SPDX e proveniência.
6. Assinar updater/artefatos quando houver chave; verificar assinatura antes do upload.
7. Publicar GitHub prerelease imutável e anexar changelog/limitações/matriz.
8. Smoke de download/instalação; falha não substitui asset: cria nova versão.

## Custo zero

GitHub Actions em repositório público usa runners padrão sem cobrança por minuto. Evitar larger runners e limitar retenção de artefatos. Releases GitHub hospedam instaladores; R2 só entra quando houver necessidade e orçamento de quota.

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
