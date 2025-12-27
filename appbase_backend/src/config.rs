use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::env;
use config; // Explicitly import the config crate

#[derive(Debug, Deserialize, Clone)]
pub struct WebConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub web: WebConfig,
    // These fields will be populated from the .env file
    pub database_path: String,
    pub media_path: String,
    pub allowed_origins: String,
    pub log_level: String,
    pub session_secret_key: String,
    pub admin_url_prefix: String,
    pub use_secure_cookies: bool, // <-- ADD THIS LINE
}

impl Config {
    pub fn from_env(env_path: &Path) -> Result<Self, config::ConfigError> {
        // Load the specified .env file. Propagate an error if it fails.
        dotenvy::from_path(env_path)
            .map_err(|e| config::ConfigError::Message(format!(
                "FATAL: Failed to load .env file from '{}'. Error: {}", env_path.display(), e
            )))?;

        // --- VALIDATION & EXTRACTION LOGIC ---
        // Explicitly read DATABASE_PATH and MEDIA_PATH from the environment.
        let database_path = env::var("DATABASE_PATH")
            .map_err(|_| config::ConfigError::Message(
                "FATAL: Environment variable 'DATABASE_PATH' is not set in your .env file.".to_string()
            ))?;

        let media_path = env::var("MEDIA_PATH")
            .map_err(|_| config::ConfigError::Message(
                "FATAL: Environment variable 'MEDIA_PATH' is not set in your .env file.".to_string()
            ))?;
            
        // NEW: Extract SESSION_SECRET_KEY
        let session_secret_key = env::var("SESSION_SECRET_KEY")
            .map_err(|_| config::ConfigError::Message(
                "FATAL: Environment variable 'SESSION_SECRET_KEY' is not set in your .env file.".to_string()
            ))?;
        
        // NEW: Validate the secret key length. It must be 128 hex characters (64 bytes).
        if session_secret_key.len() != 128 || !session_secret_key.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(config::ConfigError::Message(
                "FATAL: 'SESSION_SECRET_KEY' must be 128 hexadecimal characters long (64 bytes).".to_string()
            ));
        }

        // NEW: Extract ADMIN_URL_PREFIX
        let admin_url_prefix = env::var("ADMIN_URL_PREFIX")
            .map_err(|_| config::ConfigError::Message(
                "FATAL: Environment variable 'ADMIN_URL_PREFIX' is not set in your .env file.".to_string()
            ))?;

        // Validate that the prefix is not empty and contains valid characters.
        if admin_url_prefix.is_empty() || !admin_url_prefix.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            return Err(config::ConfigError::Message(
                "FATAL: 'ADMIN_URL_PREFIX' must not be empty and can only contain letters, numbers, underscores, and hyphens.".to_string()
            ));
        }

        // NEW: Extract ALLOWED_ORIGINS, defaulting to an empty string if not set.
        let allowed_origins = env::var("ALLOWED_ORIGINS").unwrap_or_else(|_| "".to_string());
        
        // NEW: Extract LOG_LEVEL, defaulting to "info" if not set.
        let log_level = env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string());

        // NEW: Extract USE_SECURE_COOKIES, defaulting to false if not set or invalid.
        let use_secure_cookies = env::var("USE_SECURE_COOKIES")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);


        // Check that the paths are absolute.
        if Path::new(&database_path).is_relative() {
            return Err(config::ConfigError::Message(format!(
                "FATAL: The 'DATABASE_PATH' in your .env file is a relative path ('{}'). It MUST be an absolute path.",
                database_path
            )));
        }

        if Path::new(&media_path).is_relative() {
            return Err(config::ConfigError::Message(format!(
                "FATAL: The 'MEDIA_PATH' in your .env file is a relative path ('{}'). It MUST be an absolute path.",
                media_path
            )));
        }
        // --- END VALIDATION & EXTRACTION ---

        let builder = config::Config::builder()
            // Load base settings from the TOML file (e.g., for web host/port).
            .add_source(config::File::new("config/default.toml", config::FileFormat::Toml))
            
            // Manually set the paths from the environment variables we just read and validated.
            .set_override("database_path", database_path)?
            .set_override("media_path", media_path)?

            // Manually set the session key from the environment variable.
            .set_override("session_secret_key", session_secret_key)?
            
            // Manually set the allowed origins from the environment variable.
            .set_override("allowed_origins", allowed_origins)?
            
            // Manually set the log level from the environment variable.
            .set_override("log_level", log_level)?

            // Manually set the secure cookie setting.
            .set_override("use_secure_cookies", use_secure_cookies)?
            
            // Manually set the admin prefix from the environment variable.
            .set_override("admin_url_prefix", admin_url_prefix)?
            
            .build()?;

        builder.try_deserialize()
    }
    
    // ... (keep the rest of the impl block: users_db_path and posts_db_path) ...
    /// Returns the full path to the contributors database file inside its own folder.
    pub fn users_db_path(&self) -> PathBuf {
        PathBuf::from(&self.database_path)
            .join("contributors")
            .join("contributors.db")
    }

    /// Returns the full path to the posts database file inside its own folder.
    pub fn posts_db_path(&self) -> PathBuf {
        PathBuf::from(&self.database_path)
            .join("posts")
            .join("posts.db")
    }
}