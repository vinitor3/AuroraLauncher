export type CatalogCategoryOption = {
  value: string;
  name: string;
  group: string;
};

const translations: Record<string, string> = {
  adventure: "Aventura",
  atmosphere: "Atmosfera",
  audio: "Áudio",
  blocks: "Blocos",
  bloom: "Brilho",
  cartoon: "Desenho animado",
  challenging: "Desafiador",
  combat: "Combate",
  "colored-lighting": "Iluminação colorida",
  "core-shaders": "Shaders do jogo",
  cursed: "Amaldiçoado",
  decoration: "Decoração",
  economy: "Economia",
  entities: "Entidades",
  environment: "Ambiente",
  equipment: "Equipamentos",
  fantasy: "Fantasia",
  foliage: "Folhagem",
  fonts: "Fontes",
  food: "Comida",
  "game-mechanics": "Mecânicas de jogo",
  gui: "Interface",
  high: "Alto",
  items: "Itens",
  "kitchen-sink": "Pacote completo",
  library: "Bibliotecas",
  lightweight: "Leve",
  locale: "Idiomas",
  low: "Baixo",
  magic: "Magia",
  management: "Gerenciamento",
  medium: "Médio",
  minigame: "Minijogos",
  mobs: "Criaturas",
  modded: "Com mods",
  models: "Modelos",
  multiplayer: "Multijogador",
  optimization: "Otimização",
  "path-tracing": "Path tracing",
  pbr: "PBR",
  potato: "Impacto mínimo",
  quests: "Missões",
  realistic: "Realista",
  reflections: "Reflexos",
  screenshot: "Qualidade para capturas",
  "semi-realistic": "Semirrealista",
  shadows: "Sombras",
  simplistic: "Simplista",
  social: "Social",
  storage: "Armazenamento",
  technology: "Tecnologia",
  themed: "Temático",
  transportation: "Transporte",
  tweaks: "Ajustes",
  utility: "Utilidades",
  "vanilla-like": "Estilo vanilla",
  worldgen: "Geração de mundo",
};

const groupTranslations: Record<string, string> = {
  categories: "Categorias",
  features: "Recursos",
  "performance impact": "Impacto no desempenho",
  resolutions: "Resolução",
};

function normalizedKey(value: string) {
  return value.trim().toLocaleLowerCase("en-US").replace(/\s+/g, "-");
}

export function catalogCategoryLabel(name: string, slug = name) {
  const translated = translations[normalizedKey(slug)] ?? translations[normalizedKey(name)];
  if (translated) return translated;
  return name
    .trim()
    .replace(/[-_]+/g, " ")
    .replace(/\b\p{L}/gu, (letter) => letter.toLocaleUpperCase("pt-BR"));
}

export function catalogCategoryGroupLabel(group: string) {
  const normalized = group.trim().toLocaleLowerCase("en-US");
  return groupTranslations[normalized] ?? catalogCategoryLabel(group || "Categorias");
}

export function groupedCatalogCategories(categories: CatalogCategoryOption[]) {
  const groups = new Map<string, CatalogCategoryOption[]>();
  for (const category of categories) {
    const label = catalogCategoryGroupLabel(category.group || "Categorias");
    const current = groups.get(label) ?? [];
    current.push(category);
    groups.set(label, current);
  }
  const collator = new Intl.Collator("pt-BR", { sensitivity: "base" });
  return [...groups.entries()]
    .map(([group, options]) => ({
      group,
      options: options.sort((left, right) => collator.compare(left.name, right.name)),
    }))
    .sort((left, right) => collator.compare(left.group, right.group));
}
