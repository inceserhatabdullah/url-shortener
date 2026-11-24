use std::{
    fmt::Display,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    str::FromStr,
};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageKind {
    Memory,
    JsonFile,
}

impl FromStr for StorageKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "memory" => Ok(Self::Memory),
            "json" => Ok(Self::JsonFile),
            other => Err(format!("expected 'memory' or 'json', got '{other}'")),
        }
    }
}

// konfigurasyon okunurken oluşabilecek hatalar
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("invalid value for {key}: {reason}")]
    Invalid { key: &'static str, reason: String },
}

// uygulamanın ayarları
#[derive(Debug, Clone)]
pub struct Config {
    pub host: IpAddr,
    pub port: u16,
    pub base_url: String,
    pub short_code_length: usize,
    pub storage: StorageKind,
    pub data_file: PathBuf,
    pub request_timeout_secs: u64,
}

impl Config {
    // .env okur (önce main'de yüklenir)
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    // değerleri verilen fonksiyondan okur, testlerde kaynak verebilmek için ayrı tutuldu; global ortama dokunmak gerekmez.
    fn from_lookup<F>(lookup: F) -> Result<Self, ConfigError>
    where
        F: Fn(&str) -> Option<String>,
    {
        let host = parse_or(&lookup, "HOST", IpAddr::from([127, 0, 0, 1]))?;
        let port = parse_or(&lookup, "PORT", 3000u16)?;

        // base url verilmediyse host/port makul bir varsayılan üret
        let raw_base_url =
            get(&lookup, "BASE_URL").unwrap_or_else(|| format!("http://localhost:{port}"));
        let base_url = validate_base_url(&raw_base_url)?;

        let short_code_length = parse_or(&lookup, "SHORT_CODE_LENGTH", 7usize)?;
        if !(4..=16).contains(&short_code_length) {
            return Err(ConfigError::Invalid {
                key: "SHORT_CODE_LENGTH",
                reason: format!("must be between 4 and 16, got {short_code_length}"),
            });
        }

        let storage = parse_or(&lookup, "STORAGE", StorageKind::Memory)?;
        let data_file = parse_or(&lookup, "DATA_FILE", PathBuf::from("data/links.json"))?;
        let request_timeout_secs = parse_or(&lookup, "REQUEST_TIMEOUT_SECS", 10u64)?;

        Ok(Self {
            host,
            port,
            base_url,
            short_code_length,
            storage,
            data_file,
            request_timeout_secs,
        })
    }

    pub fn socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

fn get<F>(lookup: &F, key: &str) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    lookup(key).filter(|v| !v.trim().is_empty())
}

pub fn parse_or<T, F>(lookup: &F, key: &'static str, default: T) -> Result<T, ConfigError>
where
    T: FromStr,
    T::Err: Display,
    F: Fn(&str) -> Option<String>,
{
    match get(lookup, key) {
        None => Ok(default),
        Some(raw) => raw.trim().parse::<T>().map_err(|e| ConfigError::Invalid {
            key,
            reason: e.to_string(),
        }),
    }
}

fn validate_base_url(raw: &str) -> Result<String, ConfigError> {
    let parsed = url::Url::parse(raw.trim()).map_err(|e| ConfigError::Invalid {
        key: "BASE_URL",
        reason: e.to_string(),
    })?;

    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(ConfigError::Invalid {
            key: "BASE_URL",
            reason: format!("scheme must be http or https, got {}", parsed.scheme()),
        });
    }

    Ok(raw.trim().trim_end_matches("/").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // verilen çiftlerden mock data üretir
    fn lookup_from(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key| map.get(key).cloned()
    }

    #[test]
    fn uses_defaults_when_nothing_is_set() {
        let config = Config::from_lookup(lookup_from(&[])).unwrap();

        assert_eq!(config.port, 3000);
        assert_eq!(config.base_url, "http://localhost:3000");
        assert_eq!(config.short_code_length, 7);
        assert_eq!(config.storage, StorageKind::Memory);
    }

    #[test]
    fn reads_overrides() {
        let config = Config::from_lookup(lookup_from(&[
            ("PORT", "8080"),
            ("STORAGE", "JSON"),
            ("SHORT_CODE_LENGTH", "10"),
        ]))
        .unwrap();

        assert_eq!(config.port, 8080);
        assert_eq!(config.storage, StorageKind::JsonFile);
        assert_eq!(config.short_code_length, 10);
        // base ulr verilmedi, port'a göre türetilmeli
        assert_eq!(config.base_url, "http://localhost:8080");
    }

    #[test]
    fn strips_trailing_slash_from_base_url() {
        let config = Config::from_lookup(lookup_from(&[("BASE_URL", "https://sho.rt/")])).unwrap();
        assert_eq!(config.base_url, "https://sho.rt");
    }

    #[test]
    fn treats_empty_values_as_unset() {
        let config = Config::from_lookup(lookup_from(&[("PORT", " ")])).unwrap();
        assert_eq!(config.port, 3000);
    }

    #[test]
    fn reject_invalid_port() {
        let config_error = Config::from_lookup(lookup_from(&[("PORT", "abc")])).unwrap_err();
        assert!(config_error.to_string().contains("PORT"));
    }

    #[test]
    fn reject_non_http_base_url() {
        let config_error =
            Config::from_lookup(lookup_from(&[("BASE_URL", "ftp://sho.rt")])).unwrap_err();
        assert!(config_error.to_string().contains("BASE_URL"));
    }
}
