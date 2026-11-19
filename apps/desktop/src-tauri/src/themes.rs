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
const MAX_GIF_BYTES: usize = 24 * 1024 * 1024;
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
#[serde(try_from = "ThemeDocument", rename_all = "camelCase")]
pub struct AuroraTheme {
    schema_version: u32,
    id: String,
    name: String,
    colors: ThemeColors,
    #[serde(default)]
    window: ThemeWindow,
    #[serde(default)]
    cursor: ThemeCursor,
    typography: ThemeTypography,
    layout: ThemeLayout,
    effects: ThemeEffects,
    assets: BTreeMap<String, ThemeAsset>,
    media: ThemeMedia,
    icons: ThemeIcons,
    decorations: Vec<ThemeDecoration>,
}

// Keep migration in deserialization so both imports and persisted libraries
// receive the same closed, deterministic navigation upgrade.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ThemeDocument {
    schema_version: u32,
    id: String,
    name: String,
    colors: ThemeColors,
    #[serde(default)]
    window: ThemeWindow,
    #[serde(default)]
    cursor: ThemeCursor,
    typography: ThemeTypography,
    layout: ThemeLayout,
    effects: ThemeEffects,
    assets: BTreeMap<String, ThemeAsset>,
    media: ThemeMedia,
    icons: ThemeDocumentIcons,
    decorations: Vec<ThemeDecoration>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeDocumentIcons {
    instances: String,
    discover: String,
    appearance: String,
    java: String,
    #[serde(default, deserialize_with = "present_icon")]
    connect: Option<String>,
    #[serde(default, deserialize_with = "present_icon")]
    social: Option<String>,
}

fn present_icon<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}

impl TryFrom<ThemeDocument> for AuroraTheme {
    type Error = &'static str;

    fn try_from(mut document: ThemeDocument) -> Result<Self, Self::Error> {
        let order = &mut document.layout.nav_order;
        let legacy = order.len() == 4
            && order.iter().collect::<BTreeSet<_>>().len() == 4
            && order.iter().all(|id| {
                matches!(
                    id,
                    NavId::Instances | NavId::Discover | NavId::Appearance | NavId::Java
                )
            });
        let (connect, social) = match (document.icons.connect, document.icons.social) {
            (None, None) if legacy => {
                order.extend([NavId::Connect, NavId::Social]);
                ("default".to_owned(), "default".to_owned())
            }
            (Some(connect), Some(social)) if !legacy && order.len() == 6 => (connect, social),
            _ => return Err(THEME_ERROR),
        };
        Ok(Self {
            schema_version: document.schema_version,
            id: document.id,
            name: document.name,
            colors: document.colors,
            window: document.window,
            cursor: document.cursor,
            typography: document.typography,
            layout: document.layout,
            effects: document.effects,
            assets: document.assets,
            media: document.media,
            icons: ThemeIcons {
                instances: document.icons.instances,
                discover: document.icons.discover,
                appearance: document.icons.appearance,
                java: document.icons.java,
                connect,
                social,
            },
            decorations: document.decorations,
        })
    }
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
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
struct ThemeWindow {
    titlebar_color: String,
    titlebar_text: String,
    frame_color: String,
    titlebar_height: f64,
    border_width: f64,
    outer_radius: f64,
    content_inset: f64,
}

impl Default for ThemeWindow {
    fn default() -> Self {
        Self {
            titlebar_color: "#171224".into(),
            titlebar_text: "#f8f6ff".into(),
            frame_color: "#0b0812".into(),
            titlebar_height: 44.0,
            border_width: 1.0,
            outer_radius: 20.0,
            content_inset: 16.0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
struct ThemeCursor {
    style: CursorStyle,
    color: String,
    size: f64,
    trail: CursorTrail,
}

impl Default for ThemeCursor {
    fn default() -> Self {
        Self {
            style: CursorStyle::System,
            color: "#f2efff".into(),
            size: 24.0,
            trail: CursorTrail::None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum CursorStyle {
    System,
    Pixel,
    Ring,
    Crosshair,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum CursorTrail {
    None,
    Sparkle,
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
    #[serde(default = "default_sidebar_width")]
    sidebar_width: f64,
    #[serde(default = "default_status_side")]
    status_side: Sidebar,
    #[serde(default = "default_section_order")]
    section_order: Vec<SectionId>,
    #[serde(default = "default_decoration_height")]
    decoration_height: f64,
}

fn default_sidebar_width() -> f64 {
    240.0
}
fn default_status_side() -> Sidebar {
    Sidebar::Right
}
fn default_section_order() -> Vec<SectionId> {
    vec![
        SectionId::Header,
        SectionId::Decorations,
        SectionId::Content,
    ]
}
fn default_decoration_height() -> f64 {
    160.0
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
    Connect,
    Social,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
enum SectionId {
    Header,
    Decorations,
    Content,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ThemeEffects {
    background: BackgroundEffect,
    intensity: f64,
    speed: f64,
    glass: f64,
    #[serde(default)]
    transition: ThemeTransition,
    #[serde(default = "default_transition_ms")]
    transition_ms: f64,
}

fn default_transition_ms() -> f64 {
    300.0
}

#[derive(Clone, Default, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum ThemeTransition {
    #[default]
    Fade,
    Slide,
    Zoom,
    None,
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
    #[serde(rename = "image/gif")]
    Gif,
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
    connect: String,
    social: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ThemeDecoration {
    #[serde(rename = "type")]
    kind: DecorationKind,
    text: String,
    #[serde(deserialize_with = "nullable_string")]
    asset: NullableString,
    align: DecorationAlign,
    #[serde(default = "default_decoration_position")]
    x: f64,
    #[serde(default = "default_decoration_position")]
    y: f64,
    #[serde(default = "default_decoration_width")]
    width: f64,
    #[serde(default = "default_element_height")]
    height: f64,
    #[serde(default = "default_decoration_font_size")]
    font_size: f64,
}

fn default_decoration_position() -> f64 {
    50.0
}
fn default_decoration_width() -> f64 {
    30.0
}
fn default_element_height() -> f64 {
    64.0
}
fn default_decoration_font_size() -> f64 {
    20.0
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

#[derive(Debug, PartialEq)]
struct GifMetadata {
    width: u16,
    height: u16,
    frames: u32,
    duration_ms: u32,
}

struct GifReader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> GifReader<'a> {
    fn take(&mut self, length: usize) -> Option<&'a [u8]> {
        let end = self.cursor.checked_add(length)?;
        let result = self.bytes.get(self.cursor..end)?;
        self.cursor = end;
        Some(result)
    }

    fn byte(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    fn sub_blocks(&mut self) -> Option<usize> {
        let mut total = 0usize;
        loop {
            let length = usize::from(self.byte()?);
            if length == 0 {
                return Some(total);
            }
            self.take(length)?;
            total += length;
        }
    }
}

fn gif_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

// Validate the finite GIF container and its resource budget without decompressing
// LZW, allocating frame canvases, or interpreting extensions as application code.
// Actual pixel decoding is performed by the renderer before its preview starts.
fn inspect_gif(bytes: &[u8]) -> Option<GifMetadata> {
    if bytes.len() > MAX_GIF_BYTES {
        return None;
    }
    let mut reader = GifReader { bytes, cursor: 0 };
    let header = reader.take(13)?;
    if !matches!(&header[..6], b"GIF87a" | b"GIF89a") {
        return None;
    }
    let width = gif_u16(header, 6);
    let height = gif_u16(header, 8);
    let pixels = u64::from(width) * u64::from(height);
    if width == 0 || height == 0 || width > 1920 || height > 1080 || pixels > 2_073_600 {
        return None;
    }
    let global_colors = if header[10] & 0x80 != 0 {
        1usize << ((header[10] & 7) + 1)
    } else {
        0
    };
    if global_colors != 0 {
        if usize::from(header[11]) >= global_colors {
            return None;
        }
        reader.take(3 * global_colors)?;
    }
    let mut frames = 0u32;
    let mut duration_ms = 0u32;
    let mut graphic_control: Option<(u16, Option<u8>)> = None;
    loop {
        match reader.byte()? {
            0x3b => {
                return if frames > 0 && graphic_control.is_none() && reader.cursor == bytes.len() {
                    Some(GifMetadata {
                        width,
                        height,
                        frames,
                        duration_ms,
                    })
                } else {
                    None
                };
            }
            0x2c => {
                let image = reader.take(9)?;
                let left = u32::from(gif_u16(image, 0));
                let top = u32::from(gif_u16(image, 2));
                let image_width = u32::from(gif_u16(image, 4));
                let image_height = u32::from(gif_u16(image, 6));
                if image_width == 0
                    || image_height == 0
                    || left + image_width > u32::from(width)
                    || top + image_height > u32::from(height)
                    || image[8] & 0x18 != 0
                {
                    return None;
                }
                let colors = if image[8] & 0x80 != 0 {
                    let count = 1usize << ((image[8] & 7) + 1);
                    reader.take(3 * count)?;
                    count
                } else {
                    global_colors
                };
                let (delay_cs, transparent) = graphic_control.take().unwrap_or((0, None));
                if colors == 0 || transparent.is_some_and(|index| usize::from(index) >= colors) {
                    return None;
                }
                if !(2..=8).contains(&reader.byte()?) || reader.sub_blocks()? == 0 {
                    return None;
                }
                frames += 1;
                duration_ms += (u32::from(delay_cs) * 10).max(40);
                if frames > 240 || pixels * u64::from(frames) > 160_000_000 || duration_ms > 60_000
                {
                    return None;
                }
            }
            0x21 => match reader.byte()? {
                0xf9 => {
                    if graphic_control.is_some() {
                        return None;
                    }
                    let control = reader.take(6)?;
                    if control[0] != 4
                        || control[5] != 0
                        || control[1] & 0xe2 != 0
                        || (control[1] >> 2) & 7 > 3
                    {
                        return None;
                    }
                    graphic_control = Some((
                        gif_u16(control, 2),
                        (control[1] & 1 != 0).then_some(control[4]),
                    ));
                }
                0xfe => {
                    reader.sub_blocks()?;
                }
                0xff => {
                    if reader.byte()? != 11 {
                        return None;
                    }
                    if !matches!(reader.take(11)?, b"NETSCAPE2.0" | b"ANIMEXTS1.0") {
                        return None;
                    }
                    let looping = reader.take(5)?;
                    if looping[0] != 3 || looping[1] != 1 || looping[4] != 0 {
                        return None;
                    }
                }
                // Plain text can render outside the bounded image descriptors;
                // unknown extensions have no place in this appearance format.
                _ => return None,
            },
            _ => return None,
        }
    }
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

fn strip_jsonc(json: &str) -> Result<Vec<u8>, String> {
    let mut output = json.as_bytes().to_vec();
    let mut cursor = 0;
    let mut in_string = false;
    let mut escaped = false;
    while cursor < output.len() {
        let byte = output[cursor];
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            cursor += 1;
            continue;
        }
        if byte == b'"' {
            in_string = true;
            cursor += 1;
            continue;
        }
        if byte == b'/' && output.get(cursor + 1) == Some(&b'/') {
            while cursor < output.len() && !matches!(output[cursor], b'\r' | b'\n') {
                output[cursor] = b' ';
                cursor += 1;
            }
            continue;
        }
        if byte == b'/' && output.get(cursor + 1) == Some(&b'*') {
            output[cursor] = b' ';
            output[cursor + 1] = b' ';
            cursor += 2;
            let mut closed = false;
            while cursor < output.len() {
                if output[cursor] == b'*' && output.get(cursor + 1) == Some(&b'/') {
                    output[cursor] = b' ';
                    output[cursor + 1] = b' ';
                    cursor += 2;
                    closed = true;
                    break;
                }
                if !matches!(output[cursor], b'\r' | b'\n') {
                    output[cursor] = b' ';
                }
                cursor += 1;
            }
            if !closed {
                return Err("O comentário /* */ do tema não foi fechado.".into());
            }
            continue;
        }
        cursor += 1;
    }
    if in_string {
        return Err(THEME_ERROR.into());
    }
    Ok(output)
}

fn validate_json_budget(bytes: &[u8]) -> Result<(), String> {
    let mut stack = Vec::with_capacity(8);
    let mut containers = 0usize;
    let mut separators = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for byte in bytes {
        if in_string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match *byte {
            b'"' => in_string = true,
            b'{' | b'[' => {
                containers += 1;
                stack.push(*byte);
                if stack.len() > 8 || containers > 128 {
                    return Err("O tema excede o limite permitido de complexidade do JSON.".into());
                }
            }
            b'}' | b']' => {
                let expected = if *byte == b'}' { b'{' } else { b'[' };
                if stack.pop() != Some(expected) {
                    return Err(THEME_ERROR.into());
                }
            }
            b',' | b':' => {
                separators += 1;
                if separators > 1024 {
                    return Err("O tema excede o limite permitido de complexidade do JSON.".into());
                }
            }
            _ => {}
        }
    }
    if in_string || !stack.is_empty() {
        return Err(THEME_ERROR.into());
    }
    Ok(())
}

impl AuroraTheme {
    fn parse(json: &str) -> Result<Self, String> {
        if json.len() > MAX_JSON_BYTES {
            return Err("O JSON do tema excede o limite de 34 MiB.".into());
        }
        // Input is always data: strip only comments, with whitespace preserving
        // token boundaries. Bound its structure before any JSON allocation.
        let json = strip_jsonc(json)?;
        validate_json_budget(&json)?;
        let theme: Self = serde_json::from_slice(&json).map_err(|_| THEME_ERROR.to_owned())?;
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
                &self.window.titlebar_color,
                &self.window.titlebar_text,
                &self.window.frame_color,
                &self.cursor.color,
            ]
            .iter()
            .any(|color| luminance(color).is_none())
            || !bounded(self.typography.scale, 0.9, 1.15)
            || !bounded(self.layout.radius, 0.0, 24.0)
            || !bounded(self.window.titlebar_height, 36.0, 64.0)
            || !bounded(self.window.border_width, 0.0, 3.0)
            || !bounded(self.window.outer_radius, 0.0, 24.0)
            || !bounded(self.window.content_inset, 0.0, 24.0)
            || !bounded(self.cursor.size, 16.0, 40.0)
            || !bounded(self.layout.sidebar_width, 180.0, 300.0)
            || !bounded(self.layout.decoration_height, 80.0, 400.0)
            || self.layout.section_order.len() != 3
            || self
                .layout
                .section_order
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != 3
            || self.layout.nav_order.len() != 6
            || self.layout.nav_order.iter().collect::<BTreeSet<_>>().len() != 6
            || !bounded(self.effects.intensity, 0.0, 1.0)
            || !bounded(self.effects.speed, 0.25, 2.0)
            || !bounded(self.effects.glass, 0.0, 16.0)
            || !bounded(self.effects.transition_ms, 100.0, 700.0)
            || self.assets.len() > 4
            || self.decorations.len() > 6
        {
            return Err(THEME_ERROR.into());
        }
        if contrast(&colors.text, &colors.panel).unwrap_or_default() < 4.5
            || contrast(&colors.accent_text, &colors.accent).unwrap_or_default() < 4.5
            || contrast(&self.window.titlebar_text, &self.window.titlebar_color).unwrap_or_default()
                < 4.5
        {
            return Err("O contraste entre texto/painel, texto de destaque/cor de destaque e texto/barra de título deve ser de pelo menos 4,5:1.".into());
        }
        let mut total_bytes = 0;
        for (id, asset) in &self.assets {
            if !checked_id(id) {
                return Err(THEME_ERROR.into());
            }
            let maximum = match asset.mime {
                AssetMime::Png => MAX_IMAGE_BYTES,
                AssetMime::Gif => MAX_GIF_BYTES,
                AssetMime::Mp4 | AssetMime::Webm => MAX_VIDEO_BYTES,
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
                    AssetMime::Gif => inspect_gif(&bytes).is_some(),
                    AssetMime::Mp4 => validate_mp4(&bytes),
                    AssetMime::Webm => validate_webm(&bytes),
                }
            {
                return Err("Recurso visual inválido: use PNG até 2 MiB, GIF de fundo até 24 MiB/1920×1080/240 quadros/60 s, MP4 ou WebM até 20 MiB e no máximo 24 MiB de recursos por tema.".into());
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
            &self.icons.connect,
            &self.icons.social,
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
                || !bounded(decoration.x, 0.0, 100.0)
                || !bounded(decoration.y, 0.0, 100.0)
                || !bounded(decoration.width, 8.0, 100.0)
                || !bounded(decoration.height, 24.0, 160.0)
                || !bounded(decoration.font_size, 12.0, 56.0)
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

    #[test]
    fn navigation_migrates_only_complete_legacy_documents_and_preserves_order() {
        let mut legacy = document();
        legacy["layout"]["navOrder"] = json!(["java", "appearance", "instances", "discover"]);
        let migrated = AuroraTheme::parse(&legacy.to_string()).unwrap();
        let normalized = serde_json::to_value(&migrated).unwrap();
        assert_eq!(
            normalized["layout"]["navOrder"],
            json!([
                "java",
                "appearance",
                "instances",
                "discover",
                "connect",
                "social"
            ])
        );
        assert_eq!(migrated.icons.connect, "default");
        assert_eq!(migrated.icons.social, "default");
        assert_eq!(migrated.icons.java, "default");
        assert_eq!(
            AuroraTheme::parse(&normalized.to_string()).unwrap(),
            migrated
        );

        let mut current = normalized.clone();
        current["layout"]["navOrder"] = json!([
            "social",
            "java",
            "connect",
            "appearance",
            "instances",
            "discover"
        ]);
        current["icons"]["social"] = json!("heart");
        current["icons"]["connect"] = json!("gamepad");
        assert_eq!(
            serde_json::to_value(AuroraTheme::parse(&current.to_string()).unwrap()).unwrap(),
            current
        );

        for (path, invalid) in [
            (
                "/layout/navOrder",
                json!(["instances", "discover", "appearance", "java"]),
            ),
            (
                "/layout/navOrder",
                json!(["instances", "discover", "appearance", "java", "connect"]),
            ),
            (
                "/layout/navOrder",
                json!([
                    "instances",
                    "discover",
                    "appearance",
                    "java",
                    "connect",
                    "connect"
                ]),
            ),
            (
                "/layout/navOrder",
                json!([
                    "instances",
                    "discover",
                    "appearance",
                    "java",
                    "connect",
                    "shell"
                ]),
            ),
            ("/icons/social", Value::Null),
            ("/icons/social", json!("asset:missing")),
        ] {
            let mut value = current.clone();
            *value.pointer_mut(path).unwrap() = invalid;
            assert!(
                AuroraTheme::parse(&value.to_string()).is_err(),
                "accepted {path}"
            );
        }
        for keys in [vec!["connect"], vec!["connect", "social"]] {
            let mut value = current.clone();
            for key in keys {
                value["icons"].as_object_mut().unwrap().remove(key);
            }
            assert!(AuroraTheme::parse(&value.to_string()).is_err());
        }
        legacy["icons"]["connect"] = json!("default");
        assert!(AuroraTheme::parse(&legacy.to_string()).is_err());
    }

    #[test]
    fn navigation_migration_loads_saved_library_without_losing_selection_or_bindings() {
        let fixture = Fixture::new();
        fs::create_dir_all(&fixture.0).unwrap();
        let old = document();
        let id = old["id"].as_str().unwrap();
        let stored = json!({"schemaVersion": 1, "themes": [old.clone()], "selectedThemeId": id, "bindings": {"example-instance": id}, "automatic": true});
        fs::write(
            fixture.0.join("library.json"),
            serde_json::to_vec(&stored).unwrap(),
        )
        .unwrap();
        let (library, recovered) = load_library(&fixture.0).unwrap();
        assert!(!recovered);
        assert_eq!(library.selected_theme_id.as_deref(), Some(id));
        assert_eq!(
            library.bindings.get("example-instance").map(String::as_str),
            Some(id)
        );
        assert!(library.automatic);
        assert_eq!(library.themes[0].layout.nav_order.len(), 6);
        assert_eq!(library.themes[0].icons.social, "default");
        // Reading a legacy library does not overwrite its source or a backup.
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(fixture.0.join("library.json")).unwrap())
                .unwrap(),
            stored
        );
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

    fn gif(width: u16, height: u16, delays: &[u16]) -> Vec<u8> {
        let mut bytes = b"GIF89a".to_vec();
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&height.to_le_bytes());
        bytes.extend_from_slice(&[0x80, 0, 0, 0, 0, 0, 255, 255, 255]);
        for delay in delays {
            bytes.extend_from_slice(&[0x21, 0xf9, 4, 0]);
            bytes.extend_from_slice(&delay.to_le_bytes());
            bytes.extend_from_slice(&[0, 0]);
            // A real 1×1 frame, using the global two-color palette and LZW.
            bytes.extend_from_slice(&[0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 0x44, 0x01, 0]);
        }
        bytes.push(0x3b);
        bytes
    }

    #[test]
    fn schema_is_closed_at_every_object_and_null_fields_are_required() {
        AuroraTheme::parse(&document().to_string()).unwrap();
        for path in [
            "",
            "/colors",
            "/window",
            "/cursor",
            "/typography",
            "/layout",
            "/effects",
            "/media",
            "/icons",
            "/decorations/0",
        ] {
            let mut value = serde_json::to_value(theme()).unwrap();
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
    fn legacy_v1_themes_and_libraries_receive_all_new_defaults() {
        let legacy = theme();
        assert_eq!(legacy.schema_version, 1);
        assert_eq!(legacy.window, ThemeWindow::default());
        assert_eq!(legacy.cursor, ThemeCursor::default());
        assert_eq!(legacy.layout.sidebar_width, 240.0);
        assert_eq!(legacy.layout.status_side, Sidebar::Right);
        assert_eq!(legacy.layout.section_order, default_section_order());
        assert_eq!(legacy.layout.decoration_height, 160.0);
        assert_eq!(legacy.effects.transition, ThemeTransition::Fade);
        assert_eq!(legacy.effects.transition_ms, 300.0);
        let decoration = &legacy.decorations[0];
        assert_eq!(
            (
                decoration.x,
                decoration.y,
                decoration.width,
                decoration.height,
                decoration.font_size
            ),
            (50.0, 50.0, 30.0, 64.0, 20.0)
        );
        let canonical = serde_json::to_value(&legacy).unwrap();
        assert_eq!(canonical["window"]["titlebarHeight"], 44.0);
        assert_eq!(canonical["cursor"]["style"], "system");
        assert_eq!(canonical["cursor"]["color"], "#f2efff");
        assert_eq!(canonical["cursor"]["size"], 24.0);
        assert_eq!(canonical["cursor"]["trail"], "none");
        assert_eq!(canonical["decorations"][0]["fontSize"], 20.0);
        assert_eq!(AuroraTheme::parse(&canonical.to_string()).unwrap(), legacy);

        let fixture = Fixture::new();
        fs::create_dir_all(&fixture.0).unwrap();
        let old_library = json!({"schemaVersion":1,"themes":[document()],"selectedThemeId":"test-theme","bindings":{},"automatic":false});
        fs::write(fixture.0.join("library.json"), old_library.to_string()).unwrap();
        let mut runtime = fixture.open(Instant::now());
        assert_eq!(runtime.library.themes[0].window, ThemeWindow::default());
        assert_eq!(
            runtime.library.selected_theme_id.as_deref(),
            Some("test-theme")
        );
        runtime.shutdown().unwrap();
    }

    #[test]
    fn partial_window_fields_default_without_allowing_unknown_keys_or_nulls() {
        let mut value = document();
        value["window"] = json!({"borderWidth": 3});
        let parsed = AuroraTheme::parse(&value.to_string()).unwrap();
        assert_eq!(parsed.window.border_width, 3.0);
        assert_eq!(parsed.window.titlebar_color, "#171224");
        assert_eq!(parsed.window.titlebar_text, "#f8f6ff");
        assert_eq!(parsed.window.frame_color, "#0b0812");
        assert_eq!(parsed.window.content_inset, 16.0);
        for bad in [
            Value::Null,
            json!({"titlebarHeight":null}),
            json!({"script":"alert(1)"}),
            json!({"__proto__":{}}),
        ] {
            value["window"] = bad;
            assert!(AuroraTheme::parse(&value.to_string()).is_err());
        }
    }

    #[test]
    fn extended_geometry_effects_and_titlebar_contrast_are_bounded() {
        let canonical = serde_json::to_value(theme()).unwrap();
        for (path, minimum, maximum) in [
            ("/window/titlebarHeight", 36.0, 64.0),
            ("/window/borderWidth", 0.0, 3.0),
            ("/window/outerRadius", 0.0, 24.0),
            ("/window/contentInset", 0.0, 24.0),
            ("/layout/sidebarWidth", 180.0, 300.0),
            ("/layout/decorationHeight", 80.0, 400.0),
            ("/effects/transitionMs", 100.0, 700.0),
            ("/decorations/0/x", 0.0, 100.0),
            ("/decorations/0/y", 0.0, 100.0),
            ("/decorations/0/width", 8.0, 100.0),
            ("/decorations/0/height", 24.0, 160.0),
            ("/decorations/0/fontSize", 12.0, 56.0),
        ] {
            for boundary in [minimum, maximum] {
                let mut value = canonical.clone();
                *value.pointer_mut(path).unwrap() = json!(boundary);
                AuroraTheme::parse(&value.to_string()).unwrap();
            }
            for bad in [
                json!(minimum - 0.01),
                json!(maximum + 0.01),
                Value::Null,
                json!("url(https://example.invalid)"),
            ] {
                let mut value = canonical.clone();
                *value.pointer_mut(path).unwrap() = bad;
                assert!(
                    AuroraTheme::parse(&value.to_string()).is_err(),
                    "accepted invalid {path}"
                );
            }
        }
        for (path, bad) in [
            (
                "/window/titlebarColor",
                json!("url(https://example.invalid)"),
            ),
            ("/window/frameColor", json!("transparent")),
            ("/window/titlebarText", json!("#171224")),
            ("/layout/statusSide", json!("top")),
            (
                "/layout/sectionOrder",
                json!(["header", "content", "content"]),
            ),
            ("/layout/sectionOrder", json!(["content", "header"])),
            ("/effects/transition", json!("custom-script")),
        ] {
            let mut value = canonical.clone();
            *value.pointer_mut(path).unwrap() = bad;
            assert!(
                AuroraTheme::parse(&value.to_string()).is_err(),
                "accepted invalid {path}"
            );
        }
        for transition in ["fade", "slide", "zoom", "none"] {
            let mut value = canonical.clone();
            value["effects"]["transition"] = json!(transition);
            value["layout"]["sectionOrder"] = json!(["content", "decorations", "header"]);
            value["layout"]["statusSide"] = json!("left");
            AuroraTheme::parse(&value.to_string()).unwrap();
        }
    }

    #[test]
    fn jsonc_comments_preserve_string_escapes_unicode_and_token_boundaries() {
        let mut value = with_asset("image/png", &png());
        value["name"] = json!("Canal // https://x /*texto*/");
        value["decorations"][0]["text"] = json!("Aspas \" // intactas /* e \\ unicode 🌸");
        let plain = value.to_string();
        let commented = format!(
            "// Tema Aurora 🌸\r\n/* {} */\n{} // fim",
            "[ { : , ".repeat(2000),
            plain.replace("\"schemaVersion\":1", "\"schemaVersion\":/* versão */1")
        );
        assert_eq!(
            AuroraTheme::parse(&commented).unwrap(),
            AuroraTheme::parse(&plain).unwrap()
        );
        let cleaned = strip_jsonc("1/*não concatenar*/2\r\n//linha\r\n").unwrap();
        let cleaned = String::from_utf8(cleaned).unwrap();
        assert!(cleaned.starts_with("1 "));
        assert!(cleaned.ends_with("\r\n       \r\n"));
        for bad in [
            plain.replace("\"schemaVersion\":1", "\"schemaVersion\":1/*x*/0"),
            plain.replace("null", "nu/*x*/ll"),
            plain.replace("\"schemaVersion\"", "\"schema\"/*x*/\"Version\""),
            format!("{plain} /* sem fim"),
            format!("{plain} /*/"),
            format!("{plain};alert(1)"),
        ] {
            assert!(AuroraTheme::parse(&bad).is_err());
        }
    }

    #[test]
    fn jsonc_budget_is_checked_before_parse_and_ignores_only_string_content() {
        let deep = format!("{}0{}", "[".repeat(9), "]".repeat(9));
        assert!(validate_json_budget(deep.as_bytes()).is_err());
        let containers = format!("[{}]", vec!["{}"; 128].join(","));
        assert!(validate_json_budget(containers.as_bytes()).is_err());
        let separators = format!("[{}]", vec!["0"; 1026].join(","));
        assert!(validate_json_budget(separators.as_bytes()).is_err());
        let strings = format!(
            "{{\"text\":{}}}",
            serde_json::to_string(&"[{,:]}\\\"".repeat(5000)).unwrap()
        );
        validate_json_budget(strings.as_bytes()).unwrap();
        let too_big_comment = format!("/*{}*/{}", " ".repeat(MAX_JSON_BYTES), document());
        assert!(AuroraTheme::parse(&too_big_comment)
            .unwrap_err()
            .contains("34 MiB"));
        for invalid in [
            b"{]".as_slice(),
            b"[}".as_slice(),
            b"{\"x\":\"unterminated".as_slice(),
        ] {
            assert!(validate_json_budget(invalid).is_err());
        }
    }

    #[test]
    fn cursor_defaults_partial_fields_and_all_trusted_styles_are_bounded() {
        let mut value = document();
        value["cursor"] = json!({"style":"ring"});
        let parsed = AuroraTheme::parse(&value.to_string()).unwrap();
        assert_eq!(parsed.cursor.style, CursorStyle::Ring);
        assert_eq!(parsed.cursor.color, "#f2efff");
        assert_eq!(parsed.cursor.size, 24.0);
        assert_eq!(parsed.cursor.trail, CursorTrail::None);
        for style in ["system", "pixel", "ring", "crosshair"] {
            for size in [16, 40] {
                value["cursor"] =
                    json!({"style":style,"color":"#abcdef","size":size,"trail":"sparkle"});
                AuroraTheme::parse(&value.to_string()).unwrap();
            }
        }
        for invalid in [
            Value::Null,
            json!({"style":"url(https://example.invalid/cursor.cur)"}),
            json!({"style":null}),
            json!({"color":"url(x)"}),
            json!({"size":15.9}),
            json!({"size":40.1}),
            json!({"size":null}),
            json!({"trail":"javascript"}),
            json!({"onClick":"launch"}),
            json!({"__proto__":{}}),
        ] {
            value["cursor"] = invalid;
            assert!(AuroraTheme::parse(&value.to_string()).is_err());
        }
    }

    #[test]
    fn gif_accepts_bounded_backgrounds_but_never_icons_logos_or_decorations() {
        let bytes = gif(1920, 1080, &[10, 20]);
        assert_eq!(
            inspect_gif(&bytes),
            Some(GifMetadata {
                width: 1920,
                height: 1080,
                frames: 2,
                duration_ms: 300
            })
        );
        let mut value = with_asset("image/gif", &bytes);
        AuroraTheme::parse(&value.to_string()).unwrap();
        value["media"]["logo"] = json!("sample");
        assert!(AuroraTheme::parse(&value.to_string()).is_err());
        value["media"]["logo"] = Value::Null;
        value["icons"]["java"] = json!("asset:sample");
        assert!(AuroraTheme::parse(&value.to_string()).is_err());
        value["icons"]["java"] = json!("default");
        value["decorations"][0]["type"] = json!("image");
        value["decorations"][0]["asset"] = json!("sample");
        assert!(AuroraTheme::parse(&value.to_string()).is_err());
        assert!(AuroraTheme::parse(&with_asset("image/png", &bytes).to_string()).is_err());
        assert!(AuroraTheme::parse(&with_asset("image/gif", &png()).to_string()).is_err());
        let mut version87 = gif(1, 1, &[0]);
        version87[..6].copy_from_slice(b"GIF87a");
        assert_eq!(inspect_gif(&version87).unwrap().duration_ms, 40);
    }

    #[test]
    fn gif_limits_canvas_frames_aggregate_pixels_duration_and_empty_streams() {
        for (width, height, delays) in [
            (0, 1, vec![0]),
            (1, 0, vec![0]),
            (1921, 1080, vec![0]),
            (1920, 1081, vec![0]),
            (1, 1, vec![0; 241]),
            (1440, 810, vec![0; 138]),
            (1, 1, vec![6001]),
            (1, 1, vec![]),
        ] {
            assert!(inspect_gif(&gif(width, height, &delays)).is_none());
        }
        assert!(inspect_gif(&gif(1440, 810, &[0; 137])).is_some());
        assert_eq!(
            inspect_gif(&gif(1, 1, &[1; 240])).unwrap().duration_ms,
            9600
        );
        assert_eq!(inspect_gif(&gif(1, 1, &[6000])).unwrap().duration_ms, 60000);
        let mut oversized = gif(1, 1, &[0]);
        oversized.resize(MAX_GIF_BYTES + 1, 0);
        assert!(inspect_gif(&oversized).is_none());
    }

    #[test]
    fn gif_rejects_truncation_outside_images_palettes_lzw_and_reserved_controls() {
        let valid = gif(1, 1, &[0]);
        for end in 0..valid.len() {
            assert!(
                inspect_gif(&valid[..end]).is_none(),
                "accepted truncation at {end}"
            );
        }
        for (offset, bad) in [
            (0, b'X'),
            (11, 2),
            (19, 0x22),
            (21, 3),
            (22, 2),
            (22, 0xe0),
            (22, 0x10),
            (26, 1),
            (28, 1),
            (32, 0),
            (36, 0x08),
            (36, 0x10),
            (37, 1),
            (37, 9),
            (38, 255),
        ] {
            let mut bytes = valid.clone();
            bytes[offset] = bad;
            assert!(
                inspect_gif(&bytes).is_none(),
                "accepted invalid byte {offset}"
            );
        }
        let mut transparent = valid.clone();
        transparent[22] = 1;
        transparent[25] = 2;
        assert!(inspect_gif(&transparent).is_none());
        let mut no_palette = valid.clone();
        no_palette[10] = 0;
        no_palette.drain(13..19);
        assert!(inspect_gif(&no_palette).is_none());
        // A local palette can replace the missing global table.
        no_palette[30] = 0x80;
        no_palette.splice(31..31, [0, 0, 0, 255, 255, 255]);
        assert!(inspect_gif(&no_palette).is_some());
        let mut empty_data = valid.clone();
        empty_data[38] = 0;
        empty_data.drain(39..42);
        assert!(inspect_gif(&empty_data).is_none());
        let mut trailing = valid.clone();
        trailing.push(0);
        assert!(inspect_gif(&trailing).is_none());
        let mut duplicate_control = valid.clone();
        duplicate_control.splice(27..27, valid[19..27].iter().copied());
        assert!(inspect_gif(&duplicate_control).is_none());
        let mut pending_control = valid.clone();
        pending_control.splice(
            valid.len() - 1..valid.len() - 1,
            valid[19..27].iter().copied(),
        );
        assert!(inspect_gif(&pending_control).is_none());
    }

    #[test]
    fn gif_extensions_are_finite_noninteractive_and_allow_only_known_looping_records() {
        let original = gif(1, 1, &[0]);
        let extension = |payload: &[u8]| {
            let mut bytes = original.clone();
            bytes.splice(19..19, payload.iter().copied());
            bytes
        };
        for application in [b"NETSCAPE2.0", b"ANIMEXTS1.0"] {
            let mut payload = vec![0x21, 0xff, 11];
            payload.extend_from_slice(application);
            payload.extend_from_slice(&[3, 1, 0, 0, 0]);
            assert!(inspect_gif(&extension(&payload)).is_some());
            payload[2] = 10;
            assert!(inspect_gif(&extension(&payload)).is_none());
        }
        assert!(inspect_gif(&extension(b"\x21\xfe\x08alert(1)\0")).is_some());
        for payload in [
            b"\x21\x01\0".as_slice(),
            b"\x21\xff\x0bJAVASCRIPT!\x03\x01\0\0\0".as_slice(),
            b"\x21\xff\x0bNETSCAPE2.0\x03\x02\0\0\0".as_slice(),
            b"\x21\xee\0".as_slice(),
        ] {
            assert!(inspect_gif(&extension(payload)).is_none());
        }
        let mut no_delay = original.clone();
        no_delay.drain(19..27);
        assert_eq!(inspect_gif(&no_delay).unwrap().duration_ms, 40);
    }

    #[test]
    fn user_provided_gifs_match_native_metadata_and_theme_budgets_when_available() {
        let Some(directory) = std::env::var_os("AURORA_TEST_THEME_GIFS_DIR") else {
            return;
        };
        let directory = PathBuf::from(directory);
        let cases = [
            ("e5a17eb79c5472b5fea8ab36282f3696.gif", 500, 387, 19, 760),
            ("cherry-blossom-minecraft.gif", 498, 281, 44, 2200),
            ("4e9851eedd0e229bf4a9fb4c85207b93.gif", 540, 304, 75, 4990),
            ("6222e36c080b8ea33d9319b788af0f7c.gif", 1440, 810, 131, 6040),
        ];
        for (index, (name, width, height, frames, duration_ms)) in cases.into_iter().enumerate() {
            let bytes = fs::read(directory.join(name)).unwrap();
            let metadata = inspect_gif(&bytes).expect("provided GIF rejected");
            assert_eq!(
                metadata,
                GifMetadata {
                    width,
                    height,
                    frames,
                    duration_ms
                }
            );
            AuroraTheme::parse(&with_asset("image/gif", &bytes).to_string()).unwrap();
            println!(
                "GIF {}: {}×{}, {} frames, {} ms, {} bytes: accepted",
                index + 1,
                width,
                height,
                frames,
                duration_ms,
                bytes.len()
            );
        }
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
