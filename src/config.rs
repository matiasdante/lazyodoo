use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::i18n::{Lang, tr, trf};

/// Destino (Odoo) y credenciales. Los secretos no aparecen en `Debug`.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    /// Origen de Odoo (`https://host`), sin ruta.
    pub base_url: String,
    /// Nombre de la base de datos de Odoo.
    pub database: String,
    pub login: String,
    pub secret: String,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("base_url", &self.base_url)
            .field("database", &self.database)
            .field("login", &self.login)
            .finish_non_exhaustive()
    }
}

impl Config {
    /// Normaliza y valida lo ingresado antes de tocar la red.
    pub fn new(base_url: &str, database: &str, login: &str, secret: &str) -> Result<Self, String> {
        let base_url = normalize_url(base_url)?;
        let database = database.trim().to_string();
        if database.is_empty() || database.len() > 63 {
            return Err(tr("Enter the Odoo database (up to 63 characters).").into());
        }
        if !database
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        {
            return Err(tr("The database accepts letters, numbers, _ - and .").into());
        }
        let login = login.trim().to_string();
        if login.is_empty() || secret.is_empty() {
            return Err(tr("Enter the Odoo user and key.").into());
        }
        Ok(Self {
            base_url,
            database,
            login,
            secret: secret.to_string(),
        })
    }
}

fn normalize_url(raw: &str) -> Result<String, String> {
    let raw = raw.trim().trim_end_matches('/');
    let (scheme, rest) = raw
        .split_once("://")
        .ok_or(tr("The URL must start with https://"))?;
    if rest.is_empty() || rest.contains(['@', '?', '#', ' ']) {
        return Err(tr("Invalid URL: no credentials, query or spaces.").into());
    }
    if rest.contains('/') {
        return Err(
            tr("The URL is just the Odoo origin (no path), e.g. https://odoo.example.com").into(),
        );
    }
    let host = rest;
    let host_only = if let Some(v6) = host.strip_prefix('[') {
        v6.split(']').next().unwrap_or("")
    } else {
        host.split(':').next().unwrap_or("")
    };
    if host_only.is_empty() {
        return Err(tr("The URL has no host.").into());
    }
    let local = matches!(host_only, "localhost" | "127.0.0.1" | "::1");
    match scheme {
        "https" => {}
        "http" if local => {}
        _ => return Err(tr("https is required (http only for localhost).").into()),
    }
    Ok(raw.to_string())
}

pub fn default_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("lazyodoo").join("config.toml"))
}

/// `Ok(None)` si todavía no hay config (primer arranque).
pub fn load(path: &Path) -> Result<Option<Config>, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_error("Could not read {path}: {e}", path, &e)),
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path)
            .map_err(|e| e.to_string())?
            .permissions()
            .mode();
        if mode & 0o077 != 0 {
            return Err(trf(
                "{path} has permissions {mode}: other users could read the secrets. Run: chmod 600 {path}",
                &[
                    ("path", &path.display().to_string()),
                    ("mode", &format!("{:o}", mode & 0o777)),
                ],
            ));
        }
    }
    toml::from_str(&text).map(Some).map_err(|e| {
        trf(
            "Invalid config in {path}: {e}. Delete the file and enter the details again.",
            &[("path", &path.display().to_string()), ("e", &e.to_string())],
        )
    })
}

/// Guarda con permisos 0600 (archivo) y 0700 (carpeta), de forma atómica.
pub fn save(path: &Path, config: &Config) -> Result<(), String> {
    let dir = path.parent().ok_or(tr("Invalid config path."))?;
    let text = toml::to_string(config).map_err(|e| e.to_string())?;
    create_private_dir(dir)?;
    let tmp = path.with_extension("toml.tmp");
    write_private(&tmp, text.as_bytes())
        .and_then(|()| fs::rename(&tmp, path))
        .map_err(|e| io_error("Could not save {path}: {e}", path, &e))
}

/// Preferencias que no son secretas (hoy, solo el idioma), junto a la config.
#[derive(Serialize, Deserialize, Default)]
struct Settings {
    lang: Option<Lang>,
}

fn settings_path(config_path: &Path) -> PathBuf {
    config_path.with_file_name("settings.toml")
}

/// El idioma elegido, o `None` si todavía no se eligió (o el archivo no se entiende:
/// en ese caso simplemente se vuelve a preguntar).
pub fn load_lang(config_path: &Path) -> Option<Lang> {
    let text = fs::read_to_string(settings_path(config_path)).ok()?;
    toml::from_str::<Settings>(&text).ok()?.lang
}

pub fn save_lang(config_path: &Path, lang: Lang) -> Result<(), String> {
    let path = settings_path(config_path);
    let dir = path.parent().ok_or(tr("Invalid config path."))?;
    let text = toml::to_string(&Settings { lang: Some(lang) }).map_err(|e| e.to_string())?;
    create_private_dir(dir)?;
    let tmp = path.with_extension("toml.tmp");
    write_private(&tmp, text.as_bytes())
        .and_then(|()| fs::rename(&tmp, &path))
        .map_err(|e| io_error("Could not save {path}: {e}", &path, &e))
}

/// Mensaje traducido con la ruta y el error del sistema.
fn io_error(en: &'static str, path: &Path, e: &std::io::Error) -> String {
    trf(
        en,
        &[("path", &path.display().to_string()), ("e", &e.to_string())],
    )
}

fn create_private_dir(dir: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(|e| io_error("Could not create {path}: {e}", dir, &e))
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(dir).map_err(|e| io_error("Could not create {path}: {e}", dir, &e))
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Config {
        Config::new("https://odoo.example.com/", "odoo", "me@example.com", "key").unwrap()
    }

    #[test]
    fn normalizes_trailing_slash() {
        assert_eq!(cfg().base_url, "https://odoo.example.com");
    }

    #[test]
    fn rejects_bad_urls() {
        for url in [
            "",
            "odoo.example.com",
            "http://odoo.example.com",
            "https://user:pw@odoo.example.com",
            "https://odoo.example.com?x=1",
            "https://",
            "https://odoo.example.com/odoo",
            "ftp://localhost",
        ] {
            assert!(Config::new(url, "odoo", "c", "d").is_err(), "{url}");
        }
    }

    #[test]
    fn allows_http_only_on_localhost() {
        for url in [
            "http://localhost:8000",
            "http://127.0.0.1:8000",
            "http://[::1]:8000",
        ] {
            assert!(Config::new(url, "odoo", "c", "d").is_ok(), "{url}");
        }
    }

    #[test]
    fn rejects_empty_fields() {
        let u = "https://odoo.example.com";
        assert!(Config::new(u, "odoo", "", "d").is_err());
        assert!(Config::new(u, "odoo", "c", "").is_err());
    }

    #[test]
    fn database_is_required_and_restricted() {
        let u = "https://odoo.example.com";
        assert_eq!(
            Config::new(u, " my_db-2 ", "c", "d").unwrap().database,
            "my_db-2"
        );
        for db in [
            "",
            "  ",
            "con espacio",
            "con;punto",
            "db/../x",
            &"x".repeat(64),
        ] {
            assert!(Config::new(u, db, "c", "d").is_err(), "{db:?}");
        }
    }

    #[test]
    fn configs_without_a_database_ask_to_start_over() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        fs::write(
            &path,
            "base_url = \"https://x\"\nlogin = \"c\"\nsecret = \"d\"\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let err = load(&path).unwrap_err();
        assert!(
            err.contains("database") && err.contains("Delete the file"),
            "{err}"
        );
    }

    #[test]
    fn debug_hides_secrets() {
        let text = format!("{:?}", cfg());
        assert!(!text.contains("key"));
    }

    #[cfg(unix)]
    #[test]
    fn save_is_private_and_roundtrips() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("lazyodoo").join("config.toml");
        save(&path, &cfg()).unwrap();
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(path.parent().unwrap()), 0o700);
        assert_eq!(load(&path).unwrap(), Some(cfg()));
    }

    #[cfg(unix)]
    #[test]
    fn load_refuses_open_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        save(&path, &cfg()).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(load(&path).unwrap_err().contains("chmod 600"));
    }

    #[test]
    fn load_missing_is_first_run() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(load(&tmp.path().join("nope.toml")).unwrap(), None);
    }

    #[test]
    fn the_language_is_saved_next_to_the_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lazyodoo").join("config.toml");
        assert_eq!(load_lang(&path), None);
        save_lang(&path, Lang::Zh).unwrap();
        assert_eq!(load_lang(&path), Some(Lang::Zh));
        save_lang(&path, Lang::Pt).unwrap();
        assert_eq!(load_lang(&path), Some(Lang::Pt));
        // no pisa ni lee la config con secretos
        assert_eq!(load(&path), Ok(None));
    }

    #[test]
    fn a_broken_settings_file_just_asks_again() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(dir.path().join("settings.toml"), "lang = \"klingon\"").unwrap();
        assert_eq!(load_lang(&path), None);
    }
}
