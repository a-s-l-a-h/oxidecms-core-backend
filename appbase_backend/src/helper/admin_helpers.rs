use crate::models::db_operations::{posts_db_operations, users_db_operations};
use crate::models::Contributor;
use crate::DbPool;
use actix_web::web;
use redb::Database;
use rusqlite::Connection;
use serde::Serialize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AdminHelperError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Redb Database error: {0}")]
    RedbDatabase(#[from] posts_db_operations::DbError),
    #[error("R2D2 Pool error: {0}")] // NEW
    Pool(#[from] r2d2::Error),
    #[error("User not found")]
    NotFound,
    #[error("An unexpected error occurred")]
    Other,
}

#[derive(Serialize)]
pub struct Settings {
    pub contributor_path_prefix: String,
    pub max_file_upload_size_mb: String,
    pub allowed_mime_types: String,
}

// Helper to get a connection from the pool
fn get_conn(pool: &web::Data<DbPool>) -> Result<r2d2::PooledConnection<r2d2_sqlite::SqliteConnectionManager>, AdminHelperError> {
    pool.get().map_err(AdminHelperError::Pool)
}

pub fn create_new_contributor(
    pool: &web::Data<DbPool>, // UPDATED
    username: &str,
    password: &str,
    role: &str,
) -> Result<(), AdminHelperError> {
    let conn = get_conn(pool)?;
    users_db_operations::create_user(&conn, username, password, role)?;
    Ok(())
}

pub fn fetch_all_contributors(pool: &web::Data<DbPool>) -> Result<Vec<Contributor>, AdminHelperError> { // UPDATED
    let conn = get_conn(pool)?;
    Ok(users_db_operations::read_all_users(&conn)?)
}

pub fn update_contributor(
    pool: &web::Data<DbPool>, // UPDATED
    user_id: i32,
    username: &str,
    new_password: Option<&str>,
    is_active: bool,
    can_delete_own: bool,
    can_edit_any: bool,
    can_delete_any: bool,
    can_approve_posts: bool,
) -> Result<(), AdminHelperError> {
    let conn = get_conn(pool)?;
    users_db_operations::update_user(&conn, user_id, username, new_password, is_active, can_delete_own, can_edit_any, can_delete_any, can_approve_posts)?;
    Ok(())
}

pub fn delete_contributor(pool: &web::Data<DbPool>, user_id: i32) -> Result<usize, AdminHelperError> { // UPDATED
    let conn = get_conn(pool)?;
    Ok(users_db_operations::delete_user(&conn, user_id)?)
}

// This function is an exception and takes a direct connection because it's used
// during the initial server startup before the pool is fully integrated into Actix's app_data.
pub fn get_settings(conn: &Connection) -> Settings {
    let prefix = users_db_operations::read_setting(conn, "contributor_path_prefix")
        .unwrap_or_else(|| "default-path-not-found".to_string());
    
    let max_size = users_db_operations::read_setting(conn, "max_file_upload_size_mb")
        .unwrap_or_else(|| "10".to_string());
        
    let mime_types = users_db_operations::read_setting(conn, "allowed_mime_types")
        .unwrap_or_else(|| "".to_string()); // Secure default

    Settings {
        contributor_path_prefix: prefix,
        max_file_upload_size_mb: max_size,
        allowed_mime_types: mime_types,
    }
}

pub fn update_setting(pool: &web::Data<DbPool>, key: &str, value: &str) -> Result<(), AdminHelperError> { // UPDATED
    let conn = get_conn(pool)?;
    users_db_operations::update_setting(&conn, key, value)?;
    Ok(())
}

// --- NEW TAG MANAGEMENT HELPERS ---

pub fn add_tag(db: &web::Data<Database>, tag: &str) -> Result<(), AdminHelperError> {
    Ok(posts_db_operations::add_available_tag(db, tag)?)
}

pub fn delete_tag(db: &web::Data<Database>, tag: &str) -> Result<(), AdminHelperError> {
    Ok(posts_db_operations::delete_available_tag(db, tag)?)
}

pub fn get_all_tags(db: &web::Data<Database>) -> Result<Vec<String>, AdminHelperError> {
    Ok(posts_db_operations::get_all_available_tags(db)?)
}