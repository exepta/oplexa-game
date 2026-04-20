use fluent_bundle::{FluentResource, concurrent::FluentBundle};
use unic_langid::LanguageIdentifier;
use oplexa_shared::utils::key_utils::last_was_separator;

const LANG_DEFAULTS_PATH: &str = "assets/lang/defaults.toml";
const LANG_ROOT: &str = "assets/lang";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LanguageFamily {
    German,
    English,
}

#[derive(Deserialize, Clone, Debug)]
struct LanguageDefaults {
    #[serde(default = "default_german_locale")]
    german_default: String,
    #[serde(default = "default_english_locale")]
    english_default: String,
}

impl Default for LanguageDefaults {
    fn default() -> Self {
        Self {
            german_default: default_german_locale(),
            english_default: default_english_locale(),
        }
    }
}

#[derive(Resource)]
struct ClientLanguageState {
    active_locale: String,
    bundle: FluentBundle<FluentResource>,
}

impl FromWorld for ClientLanguageState {
    fn from_world(world: &mut World) -> Self {
        let defaults = load_language_defaults();
        let requested = world
            .get_resource::<GlobalConfig>()
            .map(|config| config.interface.language.clone())
            .unwrap_or_else(default_english_locale);
        let active_locale = resolve_active_locale(requested.as_str(), &defaults);

        let bundle = load_bundle_for_locale(active_locale.as_str())
            .or_else(|| load_bundle_for_locale(defaults.english_default.as_str()))
            .unwrap_or_else(|| build_bundle(defaults.english_default.as_str()));

        info!(
            "Loaded client language '{}'(requested='{}')",
            active_locale, requested
        );

        Self {
            active_locale,
            bundle,
        }
    }
}

impl ClientLanguageState {
    fn localize_name_key(&self, key: &str) -> String {
        let Some(message) = self.bundle.get_message(key) else {
            return humanize_name_key(key);
        };
        let Some(pattern) = message.value() else {
            return humanize_name_key(key);
        };

        let mut errors = Vec::new();
        let text = self.bundle.format_pattern(pattern, None, &mut errors);
        if !errors.is_empty() {
            warn!(
                "Fluent formatting returned {} error(s) for key '{}' in locale '{}'",
                errors.len(),
                key,
                self.active_locale
            );
        }
        let rendered = text.trim();
        if rendered.is_empty() {
            humanize_name_key(key)
        } else {
            rendered.to_string()
        }
    }
}

fn localize_item_name(
    language: &ClientLanguageState,
    item: &crate::core::inventory::items::ItemDef,
) -> String {
    let key = normalize_lookup_key(item.name.as_str(), item.localized_name.as_str());
    language.localize_name_key(key.as_str())
}

fn localize_block_name(
    language: &ClientLanguageState,
    block: &crate::core::world::block::BlockDef,
) -> String {
    let key = normalize_lookup_key(block.name.as_str(), block.localized_name.as_str());
    language.localize_name_key(key.as_str())
}

fn localize_block_name_for_id(
    language: &ClientLanguageState,
    block_registry: &BlockRegistry,
    block_id: crate::core::world::block::BlockId,
) -> String {
    let block = block_registry.def(block_id);
    localize_block_name(language, block)
}

fn normalize_lookup_key(name_key: &str, fallback_identity: &str) -> String {
    let trimmed = name_key.trim();
    if is_name_key(trimmed) {
        return trimmed.to_string();
    }
    name_key_from_identity(fallback_identity)
}

fn is_name_key(value: &str) -> bool {
    value.starts_with("KEY_")
        && value
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
}

fn name_key_from_identity(value: &str) -> String {
    let base = value
        .rsplit(':')
        .next()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or(value);
    to_name_key(base)
}

fn to_name_key(value: &str) -> String {
    let mut key = String::with_capacity(value.len() + 4);
    key.push_str("KEY_");
    last_was_separator(value, &mut key);
    while key.ends_with('_') {
        key.pop();
    }
    key
}

fn humanize_name_key(key: &str) -> String {
    let raw = key.trim().strip_prefix("KEY_").unwrap_or(key);
    raw.split('_')
        .filter(|part| !part.is_empty())
        .map(capitalize_word)
        .collect::<Vec<_>>()
        .join(" ")
}

fn capitalize_word(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => {
            let mut value = String::new();
            value.extend(first.to_uppercase());
            value.push_str(&chars.as_str().to_ascii_lowercase());
            value
        }
        None => String::new(),
    }
}

fn load_language_defaults() -> LanguageDefaults {
    let Ok(raw) = fs::read_to_string(LANG_DEFAULTS_PATH) else {
        warn!(
            "Language defaults '{}' missing. Falling back to built-in defaults.",
            LANG_DEFAULTS_PATH
        );
        return LanguageDefaults::default();
    };
    toml::from_str::<LanguageDefaults>(raw.as_str()).unwrap_or_else(|error| {
        warn!(
            "Invalid language defaults at '{}': {}. Falling back to built-in defaults.",
            LANG_DEFAULTS_PATH, error
        );
        LanguageDefaults::default()
    })
}

fn resolve_active_locale(raw_requested: &str, defaults: &LanguageDefaults) -> String {
    let requested = normalize_locale_token(raw_requested);
    if requested.is_empty() {
        return normalize_locale_token(defaults.english_default.as_str());
    }

    if let Some(family) = language_family_alias(requested.as_str()) {
        return match family {
            LanguageFamily::German => normalize_locale_token(defaults.german_default.as_str()),
            LanguageFamily::English => normalize_locale_token(defaults.english_default.as_str()),
        };
    }

    if locale_dir_for(requested.as_str()).is_some() {
        requested
    } else {
        normalize_locale_token(defaults.english_default.as_str())
    }
}

fn language_family_alias(value: &str) -> Option<LanguageFamily> {
    match value.to_ascii_lowercase().as_str() {
        "german" | "germany" | "de" | "de_de" | "deutsch" => Some(LanguageFamily::German),
        "english" | "en" | "en_us" | "us" | "usa" | "america" => Some(LanguageFamily::English),
        _ => None,
    }
}

fn normalize_locale_token(value: &str) -> String {
    value.trim().replace('-', "_")
}

fn locale_dir_for(locale: &str) -> Option<PathBuf> {
    locale_dir_candidates(locale)
        .into_iter()
        .find(|path| path.exists() && path.is_dir())
}

fn locale_dir_candidates(locale: &str) -> Vec<PathBuf> {
    vec![
        Path::new(LANG_ROOT).join("german").join(locale),
        Path::new(LANG_ROOT).join("english").join(locale),
        Path::new(LANG_ROOT).join(locale),
    ]
}

fn load_bundle_for_locale(locale: &str) -> Option<FluentBundle<FluentResource>> {
    let locale_dir = locale_dir_for(locale)?;
    let mut bundle = build_bundle(locale);
    let mut resource_count = 0usize;

    for path in fluent_file_paths(locale_dir.as_path()) {
        let Ok(raw) = fs::read_to_string(path.as_path()) else {
            warn!("Unable to read fluent file '{}'", path.display());
            continue;
        };
        let Ok(resource) = FluentResource::try_new(raw) else {
            warn!("Unable to parse fluent file '{}'", path.display());
            continue;
        };
        if let Err(errors) = bundle.add_resource(resource) {
            warn!(
                "Fluent file '{}' contains {} conflicting message(s)",
                path.display(),
                errors.len()
            );
        } else {
            resource_count += 1;
        }
    }

    if resource_count == 0 {
        None
    } else {
        Some(bundle)
    }
}

fn build_bundle(locale: &str) -> FluentBundle<FluentResource> {
    let language_id = locale
        .replace('_', "-")
        .parse::<LanguageIdentifier>()
        .unwrap_or_else(|_| {
            "en-US"
                .parse::<LanguageIdentifier>()
                .expect("hardcoded locale must parse")
        });
    let mut bundle = FluentBundle::new_concurrent(vec![language_id]);
    bundle.set_use_isolating(false);
    bundle
}

fn fluent_file_paths(locale_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(locale_dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("ftl") {
            files.push(path);
        }
    }
    files.sort_unstable();
    files
}

fn default_german_locale() -> String {
    "de_DE".to_string()
}

fn default_english_locale() -> String {
    "en_US".to_string()
}
