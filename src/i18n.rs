use std::{collections::HashMap, path::PathBuf, sync::OnceLock};

/// Languages supported by Sleeve.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// Follow the system locale.
    System,
    /// Simplified Chinese.
    ZhCN,
    /// English.
    En,
}

#[allow(dead_code)]
impl Language {
    pub fn locale(self) -> &'static str {
        match self {
            Self::System => "",
            Self::ZhCN => "zh_CN.UTF-8",
            Self::En => "en_US.UTF-8",
        }
    }

    pub fn file_name(self) -> &'static str {
        match self {
            Self::System => detect_file_name(),
            Self::ZhCN => "zh-CN.json",
            Self::En => "en.json",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "zh-CN" => Self::ZhCN,
            "en" => Self::En,
            _ => Self::System,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::ZhCN => "zh-CN",
            Self::En => "en",
        }
    }
}

fn detect_file_name() -> &'static str {
    for variable in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(variable) {
            let value = value.to_lowercase();
            if value.starts_with("zh_cn")
                || value.starts_with("zh-cn")
                || value.starts_with("zh_hans")
                || value.starts_with("zh")
            {
                return "zh-CN.json";
            }
            if value.starts_with("en") {
                return "en.json";
            }
        }
    }
    "en.json"
}

static LANGUAGE: OnceLock<Language> = OnceLock::new();
static TRANSLATIONS: OnceLock<HashMap<String, String>> = OnceLock::new();

/// Initializes translations. Call once before creating the application UI.
pub fn init(language: Language) {
    let _ = LANGUAGE.set(language);
    let path = lang_dir().join(language.file_name());
    let translations = std::fs::read_to_string(&path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default();
    let _ = TRANSLATIONS.set(translations);
}

#[allow(dead_code)]
pub fn current_language() -> Language {
    LANGUAGE.get().copied().unwrap_or(Language::System)
}

#[allow(dead_code)]
pub fn display_language() -> Language {
    match current_language() {
        Language::System if detect_file_name() == "zh-CN.json" => Language::ZhCN,
        Language::System => Language::En,
        language => language,
    }
}

/// Looks up a translation key, falling back to the key when it is missing.
pub fn tr(key: &str) -> String {
    TRANSLATIONS
        .get()
        .and_then(|translations| translations.get(key))
        .cloned()
        .unwrap_or_else(|| key.to_owned())
}

/// Looks up a translation key and substitutes `{name}` placeholders.
pub fn tf(key: &str, arguments: &[(&str, &str)]) -> String {
    arguments.iter().fold(tr(key), |text, (name, value)| {
        text.replace(&format!("{{{name}}}"), value)
    })
}

fn lang_dir() -> PathBuf {
    if let Ok(executable) = std::env::current_exe() {
        if let Some(macos_directory) = executable.parent()
            && let Some(contents_directory) = macos_directory.parent()
        {
            let bundle_languages = contents_directory.join("Resources/lang");
            if bundle_languages.is_dir() {
                return bundle_languages;
            }
        }

        for ancestor in executable.ancestors() {
            let languages = ancestor.join("assets/lang");
            if languages.is_dir() {
                return languages;
            }
            let fhs_languages = ancestor.join("share/sleeve/lang");
            if fhs_languages.is_dir() {
                return fhs_languages;
            }
        }
    }

    let current_directory_languages = PathBuf::from("assets/lang");
    if current_directory_languages.is_dir() {
        return current_directory_languages;
    }

    let fhs_languages = PathBuf::from("share/sleeve/lang");
    if fhs_languages.is_dir() {
        return fhs_languages;
    }

    PathBuf::from("assets/lang")
}

#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $crate::i18n::tr($key)
    };
}

#[macro_export]
macro_rules! tf {
    ($key:expr, $($name:expr => $value:expr),* $(,)?) => {
        $crate::i18n::tf($key, &[$(($name, $value)),*])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_language_preferences() {
        assert_eq!(Language::from_str("zh-CN"), Language::ZhCN);
        assert_eq!(Language::from_str("en"), Language::En);
        assert_eq!(Language::from_str("other"), Language::System);
    }

    #[test]
    fn formats_named_translation_arguments() {
        assert_eq!(tf("missing {name}", &[("name", "value")]), "missing value");
    }
}
