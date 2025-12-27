// --- NEW: Add imports and the AppState struct definition here ---
use std::sync::{Arc, RwLock};

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
pub type DbPool = Pool<SqliteConnectionManager>;

pub struct AppState {
    pub contributor_prefix: Arc<RwLock<String>>,
}

// --- Existing module declarations ---
pub mod config;
pub mod helper;
pub mod middleware;
pub mod models;
pub mod routes;
pub mod setup;