//! Declarative, local-only appearance data. No theme value is executable code,
//! a filesystem path, a network address, or an invocation of another command.
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

const MAX_JSON_BYTES: usize = 34 * 1024 * 1024;
const MAX_LIBRARY_BYTES: u64 = 96 * 1024 * 1024;
const MAX_THEMES: usize = 32;
const MAX_IMAGE_BYTES: usize = 2 * 1024 * 1024;
const MAX_VIDEO_BYTES: usize = 20 * 1024 * 1024;
const MAX_ASSET_BYTES: usize = 24 * 1024 * 1024;
const PREVIEW_DURATION: Duration = Duration::from_secs(10);
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(5);
const STORAGE_ERROR: &str = "Não foi possível acessar a biblioteca local de temas com segurança.";
const THEME_ERROR: &str =
    "Tema inválido: use somente o formato Aurora v1 e seus limites permitidos.";
const PREVIEW_ERROR: &str = "A prévia expirou ou foi cancelada. A aparência padrão foi restaurada.";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(transparent)]
struct NullableString(Option<String>);

fn nullable_string<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<NullableString, D::Error> {
    Option::<String>::deserialize(deserializer).map(NullableString)
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuroraTheme {
    schema_version: u32,
    id: String,
    name: String,
    colors: ThemeColors,
    typography: ThemeTypography,
    layout: ThemeLayout,
    effects: ThemeEffects,
    assets: BTreeMap<String, ThemeAsset>,
    media: ThemeMedia,
    icons: ThemeIcons,
    decorations: Vec<ThemeDecoration>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ThemeColors {
    background: String,
    panel: String,
    text: String,
    muted: String,
    accent: String,
    accent_text: String,
    border: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct ThemeTypography {
    font: ThemeFont,
    scale: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum ThemeFont {
    System,
    Rounded,
    Serif,
    Mono,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ThemeLayout {
    sidebar: Sidebar,
    density: Density,
    nav_order: Vec<NavId>,
    radius: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Sidebar {
    Left,
    Right,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Density {
    Comfortable,
    Compact,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
enum NavId {
    Instances,
    Discover,
    Appearance,
    Java,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct ThemeEffects {
    background: BackgroundEffect,
    intensity: f64,
    speed: f64,
    glass: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum BackgroundEffect {
    None,
    Aurora,
    Water,
    Sakura,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct ThemeAsset {
    mime: AssetMime,
    data: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
enum AssetMime {
    #[serde(rename = "image/png")]
    Png,
    #[serde(rename = "video/mp4")]
    Mp4,
    #[serde(rename = "video/webm")]
    Webm,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct ThemeMedia {
    #[serde(deserialize_with = "nullable_string")]
    background: NullableString,
    #[serde(deserialize_with = "nullable_string")]
    logo: NullableString,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct ThemeIcons {
    instances: String,
    discover: String,
    appearance: String,
    java: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct ThemeDecoration {
    #[serde(rename = "type")]
    kind: DecorationKind,
    text: String,
    #[serde(deserialize_with = "nullable_string")]
    asset: NullableString,
    align: DecorationAlign,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum DecorationKind {
    Text,
    Image,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum DecorationAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredLibrary {
    schema_version: u32,
    themes: Vec<AuroraTheme>,
    selected_theme_id: Option<String>,
    bindings: BTreeMap<String, String>,
    automatic: bool,
}

impl StoredLibrary {
    fn empty() -> Self {
        Self {
            schema_version: 1,
            ..Self::default()
        }
    }

    fn configured(&self) -> bool {
        self.selected_theme_id.is_some() || (self.automatic && !self.bindings.is_empty())
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeLibrary {
    themes: Vec<AuroraTheme>,
    selected_theme_id: Option<String>,
    bindings: BTreeMap<String, String>,
    automatic: bool,
    recovered: bool,
    persistent: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemePreview {
    token: String,
    deadline_ms: u64,
}

struct PendingPreview {
    token: String,
    theme_id: String,
    instance_id: Option<String>,
    deadline: Instant,
}

struct ThemeRuntime {
    library: StoredLibrary,
    directory: PathBuf,
    // An OS lock prevents another launcher from treating this live process as
    // a crash, or from overwriting its library. Additional sessions use memory.
    _owner: Option<File>,
    persistent: bool,
    recovered: bool,
    pending: Option<PendingPreview>,
    last_heartbeat: Instant,
    customized_active: bool,
    // Reset takes effect in memory even if its safe snapshot cannot be written.
    // Never remove the crash marker while disk may still select an old theme.
    needs_safe_persist: bool,
    stopped: bool,
}

#[derive(Default)]
pub struct ThemeState(Mutex<Option<ThemeRuntime>>);

fn checked_id(value: &str) -> bool {
    !matches!(value, "constructor" | "prototype" | "__proto__")
        && !value.is_empty()
        && value.len() <= 48
        && (value.as_bytes()[0].is_ascii_lowercase() || value.as_bytes()[0].is_ascii_digit())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_instance_id(value: &str) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= 160
        && !value.chars().any(char::is_control)
        && !matches!(
            value,
            "constructor" | "prototype" | "__proto__" | "." | ".."
        )
        && !value.contains(['/', '\\'])
}

fn text_in_bounds(value: &str, max: usize, nonempty: bool) -> bool {
    (!nonempty || !value.trim().is_empty())
        && value.chars().count() <= max
        && !value.chars().any(char::is_control)
}

fn bounded(value: f64, min: f64, max: f64) -> bool {
    value.is_finite() && (min..=max).contains(&value)
}

fn luminance(color: &str) -> Option<f64> {
    if color.len() != 7
        || !color.starts_with('#')
        || !color[1..].bytes().all(|b| b.is_ascii_hexdigit())
    {
        return None;
    }
    let component = |offset| {
        let value = u8::from_str_radix(&color[offset..offset + 2], 16).ok()? as f64 / 255.0;
        Some(if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        })
    };
    Some(0.2126 * component(1)? + 0.7152 * component(3)? + 0.0722 * component(5)?)
}

fn contrast(first: &str, second: &str) -> Option<f64> {
    let first = luminance(first)?;
    let second = luminance(second)?;
    Some((first.max(second) + 0.05) / (first.min(second) + 0.05))
}

fn validate_png(bytes: &[u8]) -> bool {
    if bytes.len() < 33 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" || &bytes[8..16] != b"\0\0\0\rIHDR" {
        return false;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    if width == 0
        || height == 0
        || width > 4096
        || height > 4096
        || u64::from(width) * u64::from(height) > 8_000_000
    {
        return false;
    }
    // Static PNG only. APNG would animate inside an <img>, outside the
    // launcher's reduced-motion and hidden-window media controls. Inspect
    // actual chunks, never matching arbitrary compressed pixel bytes.
    let mut cursor = 8;
    let mut saw_image_data = false;
    while bytes.len().saturating_sub(cursor) >= 12 {
        let length = u32::from_be_bytes(bytes[cursor..cursor + 4].try_into().unwrap()) as usize;
        if length > bytes.len() - cursor - 12 {
            return false;
        }
        let kind = &bytes[cursor + 4..cursor + 8];
        let end = cursor + 12 + length;
        if matches!(kind, b"acTL" | b"fcTL" | b"fdAT") || (kind == b"IHDR" && cursor != 8) {
            return false;
        }
        if kind == b"IDAT" {
            saw_image_data = true;
        }
        if kind == b"IEND" {
            return length == 0 && end == bytes.len() && saw_image_data;
        }
        cursor = end;
    }
    false
}

fn validate_mp4(bytes: &[u8]) -> bool {
    if bytes.len() < 16 || &bytes[4..8] != b"ftyp" {
        return false;
    }
    let size = u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize;
    const BRANDS: [&[u8; 4]; 10] = [
        b"isom", b"iso2", b"iso3", b"iso4", b"iso5", b"iso6", b"mp41", b"mp42", b"avc1", b"M4V ",
    ];
    size >= 16
        && size <= bytes.len()
        && size.is_multiple_of(4)
        && BRANDS.iter().any(|brand| &bytes[8..12] == *brand)
}

fn validate_webm(bytes: &[u8]) -> bool {
    if bytes.len() < 12 || &bytes[..4] != b"\x1a\x45\xdf\xa3" {
        return false;
    }
    let Some((length, prefix)) = ebml_number(&bytes[4..], false) else {
        return false;
    };
    if length > 4096 {
        return false;
    }
    let start = 4 + prefix;
    let Some(header) = bytes.get(start..start + length as usize) else {
        return false;
    };
    let mut cursor = 0;
    let mut found = false;
    while cursor < header.len() {
        let Some((id, id_bytes)) = ebml_number(&header[cursor..], true) else {
            return false;
        };
        cursor += id_bytes;
        let Some((length, size_bytes)) = ebml_number(&header[cursor..], false) else {
            return false;
        };
        cursor += size_bytes;
        let Some(end) = cursor.checked_add(length as usize) else {
            return false;
        };
        let Some(value) = header.get(cursor..end) else {
            return false;
        };
        if id == 0x4282 {
            if found || value != b"webm" {
                return false;
            }
            found = true;
        }
        cursor = end;
    }
    found
}

fn ebml_number(bytes: &[u8], identifier: bool) -> Option<(u64, usize)> {
    let first = *bytes.first()?;
    if first == 0 {
        return None;
    }
    let length = first.leading_zeros() as usize + 1;
    if length > if identifier { 4 } else { 8 } {
        return None;
    }
    let input = bytes.get(..length)?;
    let mut value = if identifier {
        u64::from(first)
    } else {
        u64::from(first & ((0xffu16 >> length) as u8))
    };
    for byte in &input[1..] {
        value = (value << 8) | u64::from(*byte);
    }
    // Unknown-length EBML is forbidden for the finite, bounded file header.
    if !identifier && value == (1u64 << (7 * length)) - 1 {
        return None;
    }
    Some((value, length))
}

impl AuroraTheme {
    fn parse(json: &str) -> Result<Self, String> {
        if json.len() > MAX_JSON_BYTES {
            return Err("O JSON do tema excede o limite de 34 MiB.".into());
        }
        let theme: Self = serde_json::from_str(json).map_err(|_| THEME_ERROR.to_owned())?;
        theme.validate()?;
        Ok(theme)
    }

    fn validate(&self) -> Result<(), String> {
        let colors = &self.colors;
        if self.schema_version != 1
            || !checked_id(&self.id)
            || self.id == "aurora-default"
            || !text_in_bounds(&self.name, 60, true)
            || [
                &colors.background,
                &colors.panel,
                &colors.text,
                &colors.muted,
                &colors.accent,
                &colors.accent_text,
                &colors.border,
            ]
            .iter()
            .any(|color| luminance(color).is_none())
            || !bounded(self.typography.scale, 0.9, 1.15)
            || !bounded(self.layout.radius, 0.0, 24.0)
            || self.layout.nav_order.len() != 4
            || self.layout.nav_order.iter().collect::<BTreeSet<_>>().len() != 4
            || !bounded(self.effects.intensity, 0.0, 1.0)
            || !bounded(self.effects.speed, 0.25, 2.0)
            || !bounded(self.effects.glass, 0.0, 16.0)
            || self.assets.len() > 4
            || self.decorations.len() > 6
        {
            return Err(THEME_ERROR.into());
        }
        if contrast(&colors.text, &colors.panel).unwrap_or_default() < 4.5
            || contrast(&colors.accent_text, &colors.accent).unwrap_or_default() < 4.5
        {
            return Err("O contraste entre texto/painel e texto de destaque/cor de destaque deve ser de pelo menos 4,5:1.".into());
        }
        let mut total_bytes = 0;
        for (id, asset) in &self.assets {
            if !checked_id(id) {
                return Err(THEME_ERROR.into());
            }
            let maximum = if asset.mime == AssetMime::Png {
                MAX_IMAGE_BYTES
            } else {
                MAX_VIDEO_BYTES
            };
            if asset.data.len() > maximum.div_ceil(3) * 4 {
                return Err("Um recurso visual excede seu limite de tamanho.".into());
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&asset.data)
                .map_err(|_| THEME_ERROR.to_owned())?;
            total_bytes += bytes.len();
            if bytes.len() > maximum
                || total_bytes > MAX_ASSET_BYTES
                || !match asset.mime {
                    AssetMime::Png => validate_png(&bytes),
                    AssetMime::Mp4 => validate_mp4(&bytes),
                    AssetMime::Webm => validate_webm(&bytes),
                }
            {
                return Err("Recurso visual inválido: use PNG até 2 MiB/4096 px/8 milhões de pixels, MP4 ou WebM até 20 MiB e no máximo 24 MiB de recursos por tema.".into());
            }
        }
        let image_exists = |id: &str| {
            self.assets
                .get(id)
                .is_some_and(|asset| asset.mime == AssetMime::Png)
        };
        if self
            .media
            .background
            .0
            .as_ref()
            .is_some_and(|id| !self.assets.contains_key(id))
            || self
                .media
                .logo
                .0
                .as_ref()
                .is_some_and(|id| !image_exists(id))
        {
            return Err(
                "O tema referencia uma imagem ou um vídeo que não está incluído nele.".into(),
            );
        }
        for icon in [
            &self.icons.instances,
            &self.icons.discover,
            &self.icons.appearance,
            &self.icons.java,
        ] {
            if !matches!(
                icon.as_str(),
                "default" | "star" | "leaf" | "moon" | "heart" | "gamepad" | "sparkles"
            ) && !icon.strip_prefix("asset:").is_some_and(image_exists)
            {
                return Err(
                    "Ícone inválido: escolha um ícone interno ou um recurso PNG do próprio tema."
                        .into(),
                );
            }
        }
        for decoration in &self.decorations {
            if !text_in_bounds(&decoration.text, 160, false)
                || match decoration.kind {
                    DecorationKind::Text => decoration.asset.0.is_some(),
                    DecorationKind::Image => {
                        !decoration.asset.0.as_deref().is_some_and(image_exists)
                    }
                }
            {
                return Err(THEME_ERROR.into());
            }
        }
        Ok(())
    }
}

fn validate_library(library: &StoredLibrary) -> Result<(), String> {
    if library.schema_version != 1
        || library.themes.len() > MAX_THEMES
        || library.bindings.len() > 1024
    {
        return Err(STORAGE_ERROR.into());
    }
    let mut ids = BTreeSet::new();
    for theme in &library.themes {
        theme.validate()?;
        if !ids.insert(theme.id.as_str()) {
            return Err(STORAGE_ERROR.into());
        }
    }
    if library
        .selected_theme_id
        .as_ref()
        .is_some_and(|id| !ids.contains(id.as_str()))
        || library
            .bindings
            .iter()
            .any(|(instance, id)| !valid_instance_id(instance) || !ids.contains(id.as_str()))
    {
        return Err(STORAGE_ERROR.into());
    }
    Ok(())
}

fn check_path(path: &Path) -> Result<(), String> {
    crate::doctor_runs::check_path(path).map_err(|_| STORAGE_ERROR.to_owned())
}

fn linked(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn checked_file(path: &Path, maximum: u64) -> Result<Option<File>, String> {
    check_path(path)?;
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(STORAGE_ERROR.into()),
        Ok(metadata) if !metadata.is_file() || linked(&metadata) || metadata.len() > maximum => {
            return Err(STORAGE_ERROR.into())
        }
        Ok(_) => {}
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000);
    }
    let file = options.open(path).map_err(|_| STORAGE_ERROR.to_owned())?;
    let metadata = file.metadata().map_err(|_| STORAGE_ERROR.to_owned())?;
    if !metadata.is_file() || linked(&metadata) || metadata.len() > maximum {
        return Err(STORAGE_ERROR.into());
    }
    Ok(Some(file))
}

fn read_checked(path: &Path, maximum: u64) -> Result<Option<Vec<u8>>, String> {
    let Some(file) = checked_file(path, maximum)? else {
        return Ok(None);
    };
    let mut bytes = Vec::new();
    file.take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| STORAGE_ERROR.to_owned())?;
    if bytes.len() as u64 > maximum {
        return Err(STORAGE_ERROR.into());
    }
    Ok(Some(bytes))
}

fn remove_checked(path: &Path) -> Result<(), String> {
    if checked_file(path, MAX_LIBRARY_BYTES)?.is_some() {
        fs::remove_file(path).map_err(|_| STORAGE_ERROR.to_owned())?;
    }
    Ok(())
}

fn atomic_write(directory: &Path, filename: &str, bytes: &[u8]) -> Result<(), String> {
    check_path(directory)?;
    let path = directory.join(filename);
    let backup = directory.join(format!("{filename}.backup"));
    let temporary = directory.join(format!("{}.pending", uuid::Uuid::new_v4()));
    check_path(&temporary)?;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| STORAGE_ERROR.to_owned())?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| STORAGE_ERROR.to_owned())?;
        drop(file);
        let existed = checked_file(&path, MAX_LIBRARY_BYTES)?.is_some();
        if existed {
            remove_checked(&backup)?;
            check_path(directory)?;
            fs::rename(&path, &backup).map_err(|_| STORAGE_ERROR.to_owned())?;
        }
        check_path(directory)?;
        if fs::rename(&temporary, &path).is_err() {
            if existed {
                let _ = fs::rename(&backup, &path);
            }
            return Err(STORAGE_ERROR.to_owned());
        }
        // The committed primary remains valid even if old backup removal fails.
        let _ = remove_checked(&backup);
        Ok(())
    })();
    if result.is_err() {
        let _ = remove_checked(&temporary);
    }
    result
}

fn acquire_owner(directory: &Path) -> Result<Option<File>, String> {
    check_path(directory)?;
    fs::create_dir_all(directory).map_err(|_| STORAGE_ERROR.to_owned())?;
    let path = directory.join("owner.lock");
    check_path(&path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000);
    }
    let file = options.open(&path).map_err(|_| STORAGE_ERROR.to_owned())?;
    let metadata = file.metadata().map_err(|_| STORAGE_ERROR.to_owned())?;
    if !metadata.is_file() || linked(&metadata) {
        return Err(STORAGE_ERROR.into());
    }
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(fs::TryLockError::WouldBlock) => Ok(None),
        Err(fs::TryLockError::Error(_)) => Err(STORAGE_ERROR.into()),
    }
}

fn load_library(directory: &Path) -> Result<(StoredLibrary, bool), String> {
    let parse = |bytes: &[u8]| -> Result<StoredLibrary, String> {
        let library = serde_json::from_slice(bytes).map_err(|_| STORAGE_ERROR.to_owned())?;
        validate_library(&library)?;
        Ok(library)
    };
    let primary = read_checked(&directory.join("library.json"), MAX_LIBRARY_BYTES)?;
    if let Some(bytes) = primary.as_ref() {
        if let Ok(library) = parse(bytes) {
            return Ok((library, false));
        }
    }
    if let Some(bytes) = read_checked(&directory.join("library.json.backup"), MAX_LIBRARY_BYTES)? {
        return parse(&bytes).map(|library| (library, true));
    }
    if primary.is_some() {
        return Err(STORAGE_ERROR.into());
    }
    Ok((StoredLibrary::empty(), false))
}

impl ThemeRuntime {
    fn open(directory: PathBuf, now: Instant) -> Result<Self, String> {
        let owner = acquire_owner(&directory)?;
        let persistent = owner.is_some();
        let (mut library, mut recovered) = load_library(&directory)?;
        if persistent {
            let marker = match read_checked(&directory.join("running"), 32)? {
                Some(marker) => Some(marker),
                None => read_checked(&directory.join("running.backup"), 32)?,
            };
            if marker.as_deref().is_some_and(|bytes| bytes != b"default") {
                recovered = true;
            }
        }
        if recovered || !persistent {
            // A safe-mode latch keeps initial instance selection from
            // automatically restoring a theme after a crash. Secondary
            // launchers start independently with the default appearance;
            // only their local library and available bindings are copied.
            library.selected_theme_id = None;
            library.automatic = false;
        }
        let customized_active = library.configured();
        let runtime = Self {
            library,
            directory,
            _owner: owner,
            persistent,
            recovered,
            pending: None,
            last_heartbeat: now,
            customized_active,
            needs_safe_persist: false,
            stopped: false,
        };
        if runtime.recovered && runtime.persistent {
            runtime.persist()?;
        }
        runtime.mark_running()?;
        Ok(runtime)
    }

    fn snapshot(&self) -> ThemeLibrary {
        ThemeLibrary {
            themes: self.library.themes.clone(),
            selected_theme_id: self.library.selected_theme_id.clone(),
            bindings: self.library.bindings.clone(),
            automatic: self.library.automatic,
            recovered: self.recovered,
            persistent: self.persistent,
        }
    }

    fn persist(&self) -> Result<(), String> {
        validate_library(&self.library)?;
        let bytes = serde_json::to_vec(&self.library).map_err(|_| STORAGE_ERROR.to_owned())?;
        if bytes.len() as u64 > MAX_LIBRARY_BYTES {
            return Err("A biblioteca de temas excede o limite de 96 MiB. Exclua um tema antes de salvar outro.".into());
        }
        if self.persistent {
            atomic_write(&self.directory, "library.json", &bytes)
        } else {
            Ok(())
        }
    }

    fn mark_running(&self) -> Result<(), String> {
        if !self.persistent {
            return Ok(());
        }
        let active = self.customized_active || self.pending.is_some() || self.library.configured();
        atomic_write(
            &self.directory,
            "running",
            if active { b"customized" } else { b"default" },
        )
    }

    fn commit_library(&mut self, next: StoredLibrary) -> Result<(), String> {
        // Never publish an active configuration while the durable execution
        // marker still says "default". A failed transaction may safely leave
        // a conservative dirty marker, but cannot lose crash recovery.
        if next.configured() && self.persistent {
            atomic_write(&self.directory, "running", b"customized")?;
        }
        let previous = std::mem::replace(&mut self.library, next);
        if let Err(error) = self.persist() {
            self.library = previous;
            return Err(error);
        }
        self.needs_safe_persist = false;
        self.mark_running()
    }

    fn save(&mut self, theme: AuroraTheme) -> Result<(), String> {
        let mut next = self.library.clone();
        let id = theme.id.clone();
        if let Some(index) = next.themes.iter().position(|item| item.id == id) {
            next.themes[index] = theme;
            if next.selected_theme_id.as_ref() == Some(&id) {
                next.selected_theme_id = None;
            }
            next.bindings.retain(|_, theme_id| theme_id != &id);
        } else {
            if next.themes.len() >= MAX_THEMES {
                return Err("A biblioteca permite até 32 temas salvos.".into());
            }
            next.themes.push(theme);
        }
        self.commit_library(next)?;
        self.pending = None;
        self.customized_active = self.library.configured();
        self.mark_running()
    }

    fn delete(&mut self, id: &str) -> Result<(), String> {
        if !self.library.themes.iter().any(|theme| theme.id == id) {
            return Err("Tema não encontrado.".into());
        }
        let mut next = self.library.clone();
        next.themes.retain(|theme| theme.id != id);
        next.bindings.retain(|_, theme_id| theme_id != id);
        if next.selected_theme_id.as_deref() == Some(id) {
            next.selected_theme_id = None;
        }
        self.commit_library(next)?;
        self.pending = None;
        self.customized_active = self.library.configured();
        self.mark_running()
    }

    fn preview(
        &mut self,
        id: String,
        instance_id: Option<String>,
        now: Instant,
        now_ms: u64,
    ) -> Result<ThemePreview, String> {
        if !self.library.themes.iter().any(|theme| theme.id == id) {
            return Err("Tema não encontrado.".into());
        }
        if instance_id
            .as_deref()
            .is_some_and(|id| !valid_instance_id(id))
        {
            return Err("Instância inválida para associar ao tema.".into());
        }
        let token = uuid::Uuid::new_v4().to_string();
        self.pending = Some(PendingPreview {
            token: token.clone(),
            theme_id: id,
            instance_id,
            deadline: now + PREVIEW_DURATION,
        });
        self.last_heartbeat = now;
        self.customized_active = true;
        // This durable marker precedes returning a token to the renderer.
        if let Err(error) = self.mark_running() {
            self.pending = None;
            return Err(error);
        }
        Ok(ThemePreview {
            token,
            deadline_ms: now_ms.saturating_add(PREVIEW_DURATION.as_millis() as u64),
        })
    }

    fn confirm(&mut self, token: &str, now: Instant) -> Result<(), String> {
        let Some(pending) = self.pending.as_ref() else {
            return Err(PREVIEW_ERROR.into());
        };
        if pending.token != token || now >= pending.deadline {
            self.reset(true)?;
            return Err(PREVIEW_ERROR.into());
        }
        let mut next = self.library.clone();
        if let Some(instance) = &pending.instance_id {
            next.bindings
                .insert(instance.clone(), pending.theme_id.clone());
            next.automatic = true;
        } else {
            next.selected_theme_id = Some(pending.theme_id.clone());
        }
        self.commit_library(next)?;
        self.pending = None;
        self.recovered = false;
        self.customized_active = true;
        self.mark_running()
    }

    fn reset(&mut self, recovered: bool) -> Result<(), String> {
        self.pending = None;
        self.customized_active = false;
        self.recovered = recovered;
        let mut next = self.library.clone();
        next.selected_theme_id = None;
        next.automatic = false;
        // Even on a disk error, the in-memory renderer must receive default.
        self.library = next;
        self.needs_safe_persist = true;
        self.persist()?;
        self.needs_safe_persist = false;
        self.mark_running()
    }

    fn set_automatic(&mut self, enabled: bool) -> Result<(), String> {
        let mut next = self.library.clone();
        next.automatic = enabled;
        self.commit_library(next)?;
        self.recovered = false;
        Ok(())
    }

    fn unbind(&mut self, instance: &str) -> Result<(), String> {
        if !valid_instance_id(instance) {
            return Err("Instância inválida para remover a associação.".into());
        }
        let mut next = self.library.clone();
        next.bindings.remove(instance);
        self.commit_library(next)
    }

    fn tick(&mut self, now: Instant, visible: bool) -> (bool, bool) {
        let expired = self
            .pending
            .as_ref()
            .is_some_and(|preview| now >= preview.deadline);
        let hung = visible
            && self.customized_active
            && now.saturating_duration_since(self.last_heartbeat) >= HEARTBEAT_TIMEOUT;
        if expired || hung {
            let _ = self.reset(true);
            return (true, hung);
        }
        (false, false)
    }

    fn shutdown(&mut self) -> Result<(), String> {
        self.stopped = true;
        if self.pending.is_some() {
            self.reset(true)?;
        }
        if self.needs_safe_persist {
            self.persist()?;
            self.needs_safe_persist = false;
        }
        if self.persistent {
            remove_checked(&self.directory.join("running"))?;
            remove_checked(&self.directory.join("running.backup"))?;
        }
        Ok(())
    }
}

fn with_runtime<T>(
    window: &tauri::WebviewWindow,
    operation: impl FnOnce(&mut ThemeRuntime) -> Result<T, String>,
) -> Result<T, String> {
    if window.label() != "main" {
        return Err("A personalização pertence à janela principal do launcher.".into());
    }
    let state = window.state::<ThemeState>();
    let mut slot = state.0.lock().map_err(|_| STORAGE_ERROR.to_owned())?;
    if slot.is_none() {
        let directory = window
            .app_handle()
            .path()
            .app_data_dir()
            .map_err(|_| STORAGE_ERROR.to_owned())?
            .join("visual-themes");
        *slot = Some(ThemeRuntime::open(directory, Instant::now())?);
    }
    operation(slot.as_mut().ok_or(STORAGE_ERROR)?)
}

#[tauri::command]
pub fn themes_load(window: tauri::WebviewWindow) -> Result<ThemeLibrary, String> {
    with_runtime(&window, |runtime| Ok(runtime.snapshot()))
}

#[tauri::command]
pub fn themes_save(window: tauri::WebviewWindow, json: String) -> Result<ThemeLibrary, String> {
    let theme = AuroraTheme::parse(&json)?;
    with_runtime(&window, |runtime| {
        runtime.save(theme)?;
        Ok(runtime.snapshot())
    })
}

#[tauri::command]
pub fn themes_delete(window: tauri::WebviewWindow, id: String) -> Result<ThemeLibrary, String> {
    with_runtime(&window, |runtime| {
        runtime.delete(&id)?;
        Ok(runtime.snapshot())
    })
}

#[tauri::command]
pub fn themes_preview(
    window: tauri::WebviewWindow,
    id: String,
    instance_id: Option<String>,
) -> Result<ThemePreview, String> {
    with_runtime(&window, |runtime| {
        runtime.preview(
            id,
            instance_id,
            Instant::now(),
            crate::doctor_runs::now_ms(),
        )
    })
}

#[tauri::command]
pub fn themes_confirm(window: tauri::WebviewWindow, token: String) -> Result<ThemeLibrary, String> {
    let result = with_runtime(&window, |runtime| {
        runtime.confirm(&token, Instant::now())?;
        Ok(runtime.snapshot())
    });
    if result.is_err() {
        let _ = window.app_handle().emit("aurora-theme-reset", ());
    }
    result
}

#[tauri::command]
pub fn themes_reset(window: tauri::WebviewWindow) -> Result<ThemeLibrary, String> {
    let result = with_runtime(&window, |runtime| {
        runtime.reset(false)?;
        Ok(runtime.snapshot())
    });
    let _ = window.app_handle().emit("aurora-theme-reset", ());
    result
}

#[tauri::command]
pub fn themes_set_automatic(
    window: tauri::WebviewWindow,
    enabled: bool,
) -> Result<ThemeLibrary, String> {
    with_runtime(&window, |runtime| {
        runtime.set_automatic(enabled)?;
        Ok(runtime.snapshot())
    })
}

#[tauri::command]
pub fn themes_unbind(
    window: tauri::WebviewWindow,
    instance_id: String,
) -> Result<ThemeLibrary, String> {
    with_runtime(&window, |runtime| {
        runtime.unbind(&instance_id)?;
        Ok(runtime.snapshot())
    })
}

#[tauri::command]
pub fn themes_heartbeat(window: tauri::WebviewWindow, customized: bool) -> Result<(), String> {
    with_runtime(&window, |runtime| {
        let now = Instant::now();
        // The native deadline wins even if the renderer resumes after a stall.
        if runtime
            .pending
            .as_ref()
            .is_some_and(|preview| now >= preview.deadline)
        {
            let reset_result = runtime.reset(true);
            let _ = window.app_handle().emit("aurora-theme-reset", ());
            reset_result?;
        }
        runtime.last_heartbeat = now;
        runtime.customized_active = customized || runtime.pending.is_some();
        Ok(())
    })
}

pub fn start_watchdog(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(250));
        let window = app.get_webview_window("main");
        let visible = window.as_ref().is_some_and(|window| {
            window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(false)
        });
        let state = app.state::<ThemeState>();
        let Ok(mut slot) = state.0.lock() else {
            break;
        };
        let Some(runtime) = slot.as_mut() else {
            continue;
        };
        if runtime.stopped {
            break;
        }
        let (reset, reload) = runtime.tick(Instant::now(), visible);
        drop(slot);
        if reset {
            let _ = app.emit("aurora-theme-reset", ());
        }
        if reload {
            if let Some(window) = window {
                let _ = window.reload();
            }
        }
    });
}

pub fn restore_from_tray(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = with_runtime(&window, |runtime| runtime.reset(false));
        let _ = app.emit("aurora-theme-reset", ());
        let _ = window.reload();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub fn shutdown(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<ThemeState>() {
        if let Ok(mut slot) = state.0.lock() {
            if let Some(runtime) = slot.as_mut() {
                let _ = runtime.shutdown();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!("aurora-theme-test-{}", uuid::Uuid::new_v4())))
        }

        fn open(&self, now: Instant) -> ThemeRuntime {
            ThemeRuntime::open(self.0.clone(), now).unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            assert!(self.0.starts_with(std::env::temp_dir()));
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn document() -> Value {
        json!({
            "schemaVersion": 1, "id": "test-theme", "name": "Teste",
            "colors": { "background": "#111111", "panel": "#222222", "text": "#ffffff", "muted": "#bbbbbb", "accent": "#88ffaa", "accentText": "#111111", "border": "#444444" },
            "typography": { "font": "system", "scale": 1 },
            "layout": { "sidebar": "left", "density": "comfortable", "navOrder": ["instances", "discover", "appearance", "java"], "radius": 12 },
            "effects": { "background": "water", "intensity": 0.5, "speed": 1, "glass": 6 },
            "assets": {}, "media": { "background": null, "logo": null },
            "icons": { "instances": "gamepad", "discover": "star", "appearance": "leaf", "java": "default" },
            "decorations": [{ "type": "text", "text": "Meu launcher", "asset": null, "align": "center" }]
        })
    }

    fn theme() -> AuroraTheme {
        AuroraTheme::parse(&document().to_string()).unwrap()
    }

    fn with_asset(mime: &str, bytes: &[u8]) -> Value {
        let mut value = document();
        value["assets"]["sample"] = json!({ "mime": mime, "data": base64::engine::general_purpose::STANDARD.encode(bytes) });
        value["media"]["background"] = json!("sample");
        value
    }

    fn png() -> Vec<u8> {
        include_bytes!("../icons/32x32.png").to_vec()
    }

    #[test]
    fn schema_is_closed_at_every_object_and_null_fields_are_required() {
        AuroraTheme::parse(&document().to_string()).unwrap();
        for path in [
            "",
            "/colors",
            "/typography",
            "/layout",
            "/effects",
            "/media",
            "/icons",
            "/decorations/0",
        ] {
            let mut value = document();
            value
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("javascript".into(), json!("alert(1)"));
            assert!(
                AuroraTheme::parse(&value.to_string()).is_err(),
                "accepted unknown property at {path}"
            );
        }
        for path in ["/media", "/decorations/0"] {
            let mut value = document();
            let field = if path == "/media" {
                "background"
            } else {
                "asset"
            };
            value
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(
                AuroraTheme::parse(&value.to_string()).is_err(),
                "accepted missing nullable field"
            );
        }
        let mut value = with_asset("image/png", &png());
        value["assets"]["sample"]["url"] = json!("https://example.invalid/image.png");
        assert!(AuroraTheme::parse(&value.to_string()).is_err());
    }

    #[test]
    fn executable_values_unknown_enums_remote_references_and_prototype_keys_are_rejected() {
        for (path, bad) in [
            ("/id", json!("../outside")),
            ("/id", json!("aurora-default")),
            ("/id", json!("constructor")),
            ("/id", json!("prototype")),
            ("/colors/accent", json!("url(https://example.invalid)")),
            (
                "/typography/font",
                json!("https://example.invalid/font.woff"),
            ),
            ("/effects/background", json!("eval")),
            ("/icons/java", json!("<svg onload='alert(1)'>")),
            ("/media/background", json!("file:///C:/secret")),
            (
                "/layout/navOrder",
                json!(["instances", "discover", "java", "java"]),
            ),
            ("/decorations/0/asset", json!("missing")),
        ] {
            let mut value = document();
            *value.pointer_mut(path).unwrap() = bad;
            assert!(
                AuroraTheme::parse(&value.to_string()).is_err(),
                "accepted {path}"
            );
        }
        for key in ["__proto__", "constructor", "prototype", "file:///outside"] {
            let mut value = with_asset("image/png", &png());
            let asset = value["assets"]["sample"].clone();
            value["assets"][key] = asset;
            assert!(AuroraTheme::parse(&value.to_string()).is_err());
        }
        // Text is data, not markup; rendering uses text nodes in the frontend.
        let mut value = document();
        value["decorations"][0]["text"] = json!("<img src=x onerror=alert(1)>");
        AuroraTheme::parse(&value.to_string()).unwrap();
    }

    #[test]
    fn contrast_dimensions_resource_limits_and_numeric_limits_are_enforced() {
        for (path, bad) in [
            ("/typography/scale", json!(1.151)),
            ("/layout/radius", json!(25)),
            ("/effects/intensity", json!(-0.01)),
            ("/effects/speed", json!(0.24)),
            ("/effects/glass", json!(17)),
            ("/name", json!("a".repeat(61))),
            ("/colors/text", json!("#222222")),
            ("/colors/accentText", json!("#88ffaa")),
        ] {
            let mut value = document();
            *value.pointer_mut(path).unwrap() = bad;
            assert!(
                AuroraTheme::parse(&value.to_string()).is_err(),
                "accepted {path}"
            );
        }
        for (width, height) in [(0u32, 1u32), (4097, 1), (4096, 4096)] {
            let mut image = png();
            image[16..20].copy_from_slice(&width.to_be_bytes());
            image[20..24].copy_from_slice(&height.to_be_bytes());
            assert!(AuroraTheme::parse(&with_asset("image/png", &image).to_string()).is_err());
        }
        let mut value = document();
        value["decorations"] = json!((0..7)
            .map(|_| json!({"type":"text","text":"x","asset":null,"align":"left"}))
            .collect::<Vec<_>>());
        assert!(AuroraTheme::parse(&value.to_string()).is_err());
        let mut value = document();
        for index in 0..5 {
            value["assets"][format!("asset-{index}")] = json!({"mime":"image/png","data":base64::engine::general_purpose::STANDARD.encode(png())});
        }
        assert!(AuroraTheme::parse(&value.to_string()).is_err());
        let mut oversized = png();
        oversized.resize(MAX_IMAGE_BYTES + 1, 0);
        assert!(AuroraTheme::parse(&with_asset("image/png", &oversized).to_string()).is_err());
    }

    #[test]
    fn media_signatures_base64_and_image_only_roles_are_checked() {
        AuroraTheme::parse(&with_asset("image/png", &png()).to_string()).unwrap();
        let mp4 = b"\0\0\0\x18ftypisom\0\0\0\0isommp42";
        AuroraTheme::parse(&with_asset("video/mp4", mp4).to_string()).unwrap();
        let webm = b"\x1a\x45\xdf\xa3\x87\x42\x82\x84webm";
        AuroraTheme::parse(&with_asset("video/webm", webm).to_string()).unwrap();
        for (mime, bytes) in [
            ("image/png", mp4.as_slice()),
            ("video/mp4", png().as_slice()),
            ("video/webm", b"\x1a\x45\xdf\xa3\x80\x42\x82\x84webm"),
        ] {
            assert!(AuroraTheme::parse(&with_asset(mime, bytes).to_string()).is_err());
        }
        let mut value = with_asset("video/mp4", mp4);
        value["media"]["logo"] = json!("sample");
        assert!(AuroraTheme::parse(&value.to_string()).is_err());
        value["media"]["logo"] = Value::Null;
        value["icons"]["java"] = json!("asset:sample");
        assert!(AuroraTheme::parse(&value.to_string()).is_err());
        let mut value = with_asset("image/png", &png());
        value["assets"]["sample"]["data"] = json!("data:image/png;base64,AAAA");
        assert!(AuroraTheme::parse(&value.to_string()).is_err());
        assert!(!validate_webm(b"\x1a\x45\xdf\xa3\xff\x42\x82\x84webm"));
        assert!(!validate_mp4(b"\0\0\0\x18ftypqt  \0\0\0\0isommp42"));
    }

    #[test]
    fn png_envelope_rejects_apng_truncation_and_chunks_after_iend() {
        fn insert_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
            let mut image = png();
            let mut chunk = Vec::new();
            chunk.extend_from_slice(&(data.len() as u32).to_be_bytes());
            chunk.extend_from_slice(kind);
            chunk.extend_from_slice(data);
            chunk.extend_from_slice(&[0; 4]); // CRC is checked by the WebView decoder.
            image.splice(33..33, chunk);
            image
        }
        for kind in [b"acTL", b"fcTL", b"fdAT"] {
            let image = insert_chunk(kind, &[0; 8]);
            assert!(AuroraTheme::parse(&with_asset("image/png", &image).to_string()).is_err());
        }
        // Names occurring in a different chunk's data are not animation chunks.
        assert!(validate_png(&insert_chunk(b"tEXt", b"note\0acTL")));
        let image = png();
        assert!(!validate_png(&image[..33]));
        assert!(!validate_png(&image[..image.len() - 1]));
        let mut trailing = image.clone();
        trailing.extend_from_slice(b"extra");
        assert!(!validate_png(&trailing));
        let mut overflow = image;
        overflow[33..37].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(!validate_png(&overflow));
    }

    #[test]
    fn preview_is_native_ten_seconds_single_use_and_timeout_restores_default() {
        let fixture = Fixture::new();
        let now = Instant::now();
        let mut runtime = fixture.open(now);
        runtime.save(theme()).unwrap();
        let first = runtime
            .preview("test-theme".into(), None, now, 1000)
            .unwrap();
        assert_eq!(first.deadline_ms, 11_000);
        runtime
            .confirm(&first.token, now + Duration::from_secs(9))
            .unwrap();
        assert_eq!(
            runtime.library.selected_theme_id.as_deref(),
            Some("test-theme")
        );
        assert!(runtime
            .confirm(&first.token, now + Duration::from_secs(9))
            .is_err());
        let second = runtime
            .preview("test-theme".into(), None, now, 1000)
            .unwrap();
        assert!(runtime
            .confirm(&second.token, now + PREVIEW_DURATION)
            .is_err());
        assert_eq!(runtime.library.selected_theme_id, None);
        assert!(!runtime.library.automatic);
        assert_eq!(runtime.library.themes.len(), 1);
        let third = runtime
            .preview("test-theme".into(), None, now, 1000)
            .unwrap();
        let fourth = runtime
            .preview("test-theme".into(), None, now, 1000)
            .unwrap();
        assert_ne!(third.token, fourth.token);
        assert!(runtime.confirm(&third.token, now).is_err());
        assert!(runtime.pending.is_none());
    }

    #[test]
    fn watchdog_uses_monotonic_time_and_recovers_hung_customization_without_renderer() {
        let fixture = Fixture::new();
        let now = Instant::now();
        let mut runtime = fixture.open(now);
        runtime.save(theme()).unwrap();
        runtime.preview("test-theme".into(), None, now, 0).unwrap();
        assert_eq!(
            runtime.tick(now + Duration::from_secs(4), true),
            (false, false)
        );
        assert_eq!(
            runtime.tick(now + Duration::from_secs(5), true),
            (true, true)
        );
        assert!(runtime.pending.is_none());
        assert_eq!(runtime.library.selected_theme_id, None);
        runtime.preview("test-theme".into(), None, now, 0).unwrap();
        assert_eq!(
            runtime.tick(now + Duration::from_secs(6), false),
            (false, false)
        );
        // Hidden/minimized previews still expire, but do not count as hung.
        assert_eq!(
            runtime.tick(now + Duration::from_secs(10), false),
            (true, false)
        );
        runtime.preview("test-theme".into(), None, now, 0).unwrap();
        runtime.last_heartbeat = now + Duration::from_secs(9);
        assert_eq!(
            runtime.tick(now + Duration::from_secs(10), true),
            (true, false)
        );
    }

    #[test]
    fn crash_returns_default_preserves_library_and_disables_automatic_reapplication() {
        let fixture = Fixture::new();
        let now = Instant::now();
        let mut runtime = fixture.open(now);
        runtime.save(theme()).unwrap();
        let preview = runtime
            .preview("test-theme".into(), Some("Meu modpack".into()), now, 0)
            .unwrap();
        assert_eq!(fs::read(fixture.0.join("running")).unwrap(), b"customized");
        runtime.confirm(&preview.token, now).unwrap();
        assert!(runtime.library.automatic);
        drop(runtime); // Simulate a crash: no shutdown callback.
        let mut recovered = fixture.open(now);
        assert!(recovered.recovered);
        assert_eq!(recovered.library.selected_theme_id, None);
        assert!(!recovered.library.automatic);
        assert_eq!(
            recovered
                .library
                .bindings
                .get("Meu modpack")
                .map(String::as_str),
            Some("test-theme")
        );
        assert_eq!(recovered.library.themes.len(), 1);
        recovered.shutdown().unwrap();
    }

    #[test]
    fn clean_exit_preserves_confirmed_theme_but_never_preserves_unconfirmed_preview() {
        let fixture = Fixture::new();
        let now = Instant::now();
        let mut runtime = fixture.open(now);
        runtime.save(theme()).unwrap();
        let preview = runtime.preview("test-theme".into(), None, now, 0).unwrap();
        runtime.confirm(&preview.token, now).unwrap();
        runtime.shutdown().unwrap();
        drop(runtime);
        let mut reopened = fixture.open(now);
        assert!(!reopened.recovered);
        assert_eq!(
            reopened.library.selected_theme_id.as_deref(),
            Some("test-theme")
        );
        reopened.preview("test-theme".into(), None, now, 0).unwrap();
        reopened.shutdown().unwrap();
        drop(reopened);
        let mut final_open = fixture.open(now);
        assert_eq!(final_open.library.selected_theme_id, None);
        assert!(!final_open.library.automatic);
        final_open.shutdown().unwrap();
    }

    #[test]
    fn simultaneous_launchers_are_isolated_and_secondary_cannot_overwrite_primary() {
        let fixture = Fixture::new();
        let now = Instant::now();
        let mut primary = fixture.open(now);
        primary.save(theme()).unwrap();
        let preview = primary.preview("test-theme".into(), None, now, 0).unwrap();
        primary.confirm(&preview.token, now).unwrap();
        let before = fs::read(fixture.0.join("library.json")).unwrap();
        let mut secondary = fixture.open(now);
        assert!(!secondary.persistent);
        assert!(!secondary.recovered);
        assert_eq!(secondary.library.selected_theme_id, None);
        assert!(!secondary.library.automatic);
        assert_eq!(secondary.library.themes.len(), 1);
        secondary.delete("test-theme").unwrap();
        secondary.shutdown().unwrap();
        assert_eq!(fs::read(fixture.0.join("library.json")).unwrap(), before);
        assert_eq!(fs::read(fixture.0.join("running")).unwrap(), b"customized");
        assert_eq!(primary.library.themes.len(), 1);
        primary.shutdown().unwrap();
    }

    #[test]
    fn editing_deleting_and_unbinding_clear_stale_selections_and_bindings() {
        let fixture = Fixture::new();
        let now = Instant::now();
        let mut runtime = fixture.open(now);
        runtime.save(theme()).unwrap();
        let preview = runtime
            .preview("test-theme".into(), Some("Pack".into()), now, 0)
            .unwrap();
        runtime.confirm(&preview.token, now).unwrap();
        runtime.unbind("Pack").unwrap();
        assert!(runtime.library.bindings.is_empty());
        for bad in ["../outside", "__proto__", "a/b", "", "a\n"] {
            assert!(runtime
                .preview("test-theme".into(), Some(bad.into()), now, 0)
                .is_err());
            assert!(runtime.unbind(bad).is_err());
        }
        let preview = runtime
            .preview("test-theme".into(), Some("Pack".into()), now, 0)
            .unwrap();
        runtime.confirm(&preview.token, now).unwrap();
        let preview = runtime.preview("test-theme".into(), None, now, 0).unwrap();
        runtime.confirm(&preview.token, now).unwrap();
        runtime.save(theme()).unwrap();
        assert_eq!(runtime.library.selected_theme_id, None);
        assert!(runtime.library.bindings.is_empty());
        runtime.delete("test-theme").unwrap();
        assert!(runtime.library.themes.is_empty());
        assert!(runtime.preview("test-theme".into(), None, now, 0).is_err());
        runtime.shutdown().unwrap();
    }

    #[test]
    fn storage_failure_does_not_publish_a_preview_or_overwrite_library() {
        let fixture = Fixture::new();
        let now = Instant::now();
        let mut runtime = fixture.open(now);
        runtime.save(theme()).unwrap();
        let before = fs::read(fixture.0.join("library.json")).unwrap();
        // A directory in place of the journal is unsafe, never a writable file.
        fs::remove_file(fixture.0.join("running")).unwrap();
        fs::create_dir(fixture.0.join("running")).unwrap();
        assert!(runtime.preview("test-theme".into(), None, now, 0).is_err());
        assert!(runtime.pending.is_none());
        assert_eq!(fs::read(fixture.0.join("library.json")).unwrap(), before);
    }

    #[test]
    fn interrupted_atomic_write_recovers_backup_and_never_auto_applies_it() {
        let fixture = Fixture::new();
        let now = Instant::now();
        let mut runtime = fixture.open(now);
        runtime.save(theme()).unwrap();
        let preview = runtime.preview("test-theme".into(), None, now, 0).unwrap();
        runtime.confirm(&preview.token, now).unwrap();
        runtime.shutdown().unwrap();
        drop(runtime);
        fs::rename(
            fixture.0.join("library.json"),
            fixture.0.join("library.json.backup"),
        )
        .unwrap();
        let mut recovered = fixture.open(now);
        assert!(recovered.recovered);
        assert_eq!(recovered.library.themes.len(), 1);
        assert_eq!(recovered.library.selected_theme_id, None);
        recovered.shutdown().unwrap();
    }

    #[test]
    fn interrupted_execution_marker_write_still_recovers_customized_session() {
        let fixture = Fixture::new();
        let now = Instant::now();
        let mut runtime = fixture.open(now);
        runtime.save(theme()).unwrap();
        let preview = runtime.preview("test-theme".into(), None, now, 0).unwrap();
        runtime.confirm(&preview.token, now).unwrap();
        drop(runtime);
        fs::rename(fixture.0.join("running"), fixture.0.join("running.backup")).unwrap();
        let mut recovered = fixture.open(now);
        assert!(recovered.recovered);
        assert_eq!(recovered.library.selected_theme_id, None);
        assert_eq!(recovered.library.themes.len(), 1);
        recovered.shutdown().unwrap();
    }

    #[test]
    fn failed_library_replace_rolls_back_memory_and_keeps_previous_snapshot() {
        let fixture = Fixture::new();
        let now = Instant::now();
        let mut runtime = fixture.open(now);
        runtime.save(theme()).unwrap();
        let before = fs::read(fixture.0.join("library.json")).unwrap();
        fs::create_dir(fixture.0.join("library.json.backup")).unwrap();
        let mut added = theme();
        added.id = "another-theme".into();
        assert!(runtime.save(added).is_err());
        assert_eq!(runtime.library.themes.len(), 1);
        assert_eq!(fs::read(fixture.0.join("library.json")).unwrap(), before);
        assert!(!fs::read_dir(&fixture.0)
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| entry.path().extension().is_some_and(|ext| ext == "pending")));
    }

    #[test]
    fn failed_reset_and_failed_shutdown_keep_crash_marker_until_disk_is_safe() {
        let fixture = Fixture::new();
        let now = Instant::now();
        let mut runtime = fixture.open(now);
        runtime.save(theme()).unwrap();
        let preview = runtime.preview("test-theme".into(), None, now, 0).unwrap();
        runtime.confirm(&preview.token, now).unwrap();
        let before = fs::read(fixture.0.join("library.json")).unwrap();
        // Block only snapshot replacement; the execution marker stays writable.
        fs::create_dir(fixture.0.join("library.json.backup")).unwrap();
        assert!(runtime.reset(true).is_err());
        assert!(runtime.needs_safe_persist);
        assert_eq!(runtime.library.selected_theme_id, None);
        assert_eq!(fs::read(fixture.0.join("library.json")).unwrap(), before);
        assert!(runtime.shutdown().is_err());
        assert_eq!(fs::read(fixture.0.join("running")).unwrap(), b"customized");
        fs::remove_dir(fixture.0.join("library.json.backup")).unwrap();
        drop(runtime);
        let mut recovered = fixture.open(now);
        assert!(recovered.recovered);
        assert_eq!(recovered.library.selected_theme_id, None);
        assert!(!recovered.library.automatic);
        assert_eq!(recovered.library.themes.len(), 1);
        recovered.shutdown().unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn windows_locked_snapshot_reset_is_retried_before_clean_shutdown_marker_removal() {
        use std::os::windows::fs::OpenOptionsExt;
        let fixture = Fixture::new();
        let now = Instant::now();
        let mut runtime = fixture.open(now);
        runtime.save(theme()).unwrap();
        let preview = runtime.preview("test-theme".into(), None, now, 0).unwrap();
        runtime.confirm(&preview.token, now).unwrap();
        // Read access is allowed, but Windows must refuse rename/delete while
        // this unrelated handle holds FILE_SHARE_READ without FILE_SHARE_DELETE.
        let blocker = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(fixture.0.join("library.json"))
            .unwrap();
        assert!(runtime.reset(true).is_err());
        assert!(runtime.needs_safe_persist);
        assert_eq!(fs::read(fixture.0.join("running")).unwrap(), b"customized");
        drop(blocker);
        // A normal exit retries the pending safe snapshot, then may clear the
        // marker; the following boot must still start with the default theme.
        runtime.shutdown().unwrap();
        assert!(!fixture.0.join("running").exists());
        drop(runtime);
        let mut reopened = fixture.open(now);
        assert!(!reopened.recovered);
        assert_eq!(reopened.library.selected_theme_id, None);
        assert_eq!(reopened.library.themes.len(), 1);
        reopened.shutdown().unwrap();
    }

    #[test]
    fn saved_theme_limit_is_enforced_even_in_memory_only_sessions() {
        let fixture = Fixture::new();
        let now = Instant::now();
        let _primary = fixture.open(now);
        let mut secondary = fixture.open(now);
        assert!(!secondary.persistent);
        for index in 0..MAX_THEMES {
            let mut added = theme();
            added.id = format!("theme-{index}");
            secondary.save(added).unwrap();
        }
        let mut added = theme();
        added.id = "one-too-many".into();
        assert!(secondary.save(added).is_err());
        assert_eq!(secondary.library.themes.len(), MAX_THEMES);
    }

    #[test]
    fn corrupt_storage_is_rejected_without_replacing_it() {
        let fixture = Fixture::new();
        fs::create_dir_all(&fixture.0).unwrap();
        let bad = br#"{"schemaVersion":1,"themes":[],"selectedThemeId":"missing","bindings":{},"automatic":true}"#;
        fs::write(fixture.0.join("library.json"), bad).unwrap();
        assert!(ThemeRuntime::open(fixture.0.clone(), Instant::now()).is_err());
        assert_eq!(fs::read(fixture.0.join("library.json")).unwrap(), bad);
        assert!(!fixture.0.join("running").exists());
    }

    #[test]
    fn linked_library_is_rejected_and_external_target_untouched() {
        let fixture = Fixture::new();
        let external = Fixture::new();
        fs::create_dir_all(&fixture.0).unwrap();
        fs::create_dir_all(&external.0).unwrap();
        let target = external.0.join("protected.json");
        fs::write(&target, b"protected").unwrap();
        #[cfg(windows)]
        let result = std::os::windows::fs::symlink_file(&target, fixture.0.join("library.json"));
        #[cfg(not(windows))]
        let result = std::os::unix::fs::symlink(&target, fixture.0.join("library.json"));
        if result.is_err() {
            return;
        } // Windows without symlink privilege.
        assert!(ThemeRuntime::open(fixture.0.clone(), Instant::now()).is_err());
        assert_eq!(fs::read(target).unwrap(), b"protected");
    }
}
