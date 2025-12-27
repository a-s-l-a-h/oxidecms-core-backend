

use crate::models::db_operations::{advanced_db_manager_operations as advanced_db_ops, users_db_operations};
use crate::models::advanced_db_manager_models::{
    DbSelection, PaginatedResponse, DbStructureResponse, DbInfo, TableInfo, DependentToDelete, FoundDependency
};
use crate::DbPool;
use actix_web::web;
use redb::{Database, ReadableTable, TableDefinition};
use std::collections::HashMap;
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum HelperError {
    #[error("Database Operation Failed: {0}")]
    DbError(#[from] advanced_db_ops::AdvancedDbError),
    #[error("User Database Error: {0}")]
    UserDbError(#[from] rusqlite::Error),
    #[error("Pool Error: {0}")]
    PoolError(#[from] r2d2::Error),
    #[error("Forbidden: {0}")]
    Forbidden(String),
    #[error("Invalid Credentials")]
    InvalidCredentials,
    #[error("Not Found")]
    NotFound,
}

type HelperResult<T> = Result<T, HelperError>;

// --- Centralized Database Structure Definition ---

fn get_db_structure_definition() -> DbStructureResponse {
    DbStructureResponse {
        databases: vec![
            DbInfo {
                id: "postsdb".to_string(),
                name: "Posts DB (posts.db)".to_string(),
                tables: vec![
                    TableInfo { name: "posts".to_string(), cleanable: false, dependencies: vec!["metadata".to_string()] },
                    TableInfo { name: "metadata".to_string(), cleanable: false, dependencies: vec!["posts".to_string()] },
                    TableInfo { name: "pending_posts".to_string(), cleanable: true, dependencies: vec!["pending_metadata".to_string()] },
                    TableInfo { name: "pending_metadata".to_string(), cleanable: true, dependencies: vec!["pending_posts".to_string()] },
                ],
            },
            DbInfo {
                id: "contributordb".to_string(),
                name: "Contributors DB (contributors.db)".to_string(),
                tables: vec![
                    TableInfo { name: "users".to_string(), cleanable: false, dependencies: vec![] },
                    TableInfo { name: "settings".to_string(), cleanable: false, dependencies: vec![] },
                    TableInfo { name: "post_ownership".to_string(), cleanable: true, dependencies: vec![] },
                    TableInfo { name: "pending_post_ownership".to_string(), cleanable: true, dependencies: vec![] },
                    TableInfo { name: "media_attachments".to_string(), cleanable: true, dependencies: vec![] },
                ],
            },
        ],
        editable_cells: [
            ("settings".to_string(), vec!["value".to_string()]),
            ("users".to_string(), vec!["username".to_string()]),
            ("posts".to_string(), vec!["value".to_string()]),
            ("pending_posts".to_string(), vec!["value".to_string()]),
            ("metadata".to_string(), vec!["title".to_string(), "summary".to_string(), "tags".to_string(), "cover_image".to_string()]),
            ("pending_metadata".to_string(), vec!["title".to_string(), "summary".to_string(), "tags".to_string(), "cover_image".to_string()]),
        ].iter().cloned().collect(),
    }
}

pub fn get_db_structure() -> DbStructureResponse {
    get_db_structure_definition()
}

pub async fn get_row_dependencies(
    posts_db: web::Data<Database>,
    _pool: web::Data<DbPool>,
    db_selection: DbSelection,
    table_name: String,
    row_id: String,
) -> HelperResult<Vec<FoundDependency>> {
    let structure = get_db_structure_definition();
    let db_id = match db_selection {
        DbSelection::PostsDb => "postsdb",
        DbSelection::ContributorDb => "contributordb",
    };

    let table_info = structure.databases.iter()
        .find(|db| db.id == db_id)
        .and_then(|db| db.tables.iter().find(|t| t.name == table_name))
        .ok_or(HelperError::NotFound)?;

    if table_info.dependencies.is_empty() {
        return Ok(vec![]);
    }
    
    let dependencies_to_check = table_info.dependencies.clone();

    let dependencies = web::block(move || -> HelperResult<Vec<FoundDependency>> {
        let mut found = Vec::new();
        match db_selection {
            DbSelection::PostsDb => {
                let uuid = Uuid::parse_str(&row_id).map_err(advanced_db_ops::AdvancedDbError::Uuid)?;
                let read_txn = posts_db.begin_read().map_err(advanced_db_ops::AdvancedDbError::from)?;

                for dep_table_name in &dependencies_to_check {
                    let table_def: TableDefinition<&[u8; 16], &str> = TableDefinition::new(dep_table_name);
                    let table = read_txn.open_table(table_def).map_err(advanced_db_ops::AdvancedDbError::from)?;
                    
                    // START FIX: Break the operation into two steps to clarify lifetimes
                    let get_result = table.get(&uuid.into_bytes()).map_err(advanced_db_ops::AdvancedDbError::from)?;
                    
                    if let Some(val_guard) = get_result {
                    // END FIX
                        let value = val_guard.value();
                        let preview = if dep_table_name.contains("metadata") {
                            serde_json::from_str::<crate::models::PostMetadata>(value)
                                .map(|m| format!("Title: {}", m.title))
                                .unwrap_or_else(|_| "Invalid Metadata JSON".to_string())
                        } else {
                            format!("{:.100}...", value)
                        };

                        found.push(FoundDependency {
                            table_name: dep_table_name.clone(),
                            row_id: row_id.clone(),
                            preview,
                        });
                    }
                }
            }
            DbSelection::ContributorDb => {
            }
        }
        Ok(found)
    }).await.unwrap()?;

    Ok(dependencies)
}


pub async fn get_paginated_table_data(
    posts_db: web::Data<Database>,
    pool: web::Data<DbPool>,
    db_selection: DbSelection,
    table_name: String,
    page: u32,
    size: u32,
    search_id: Option<String>,
) -> HelperResult<PaginatedResponse> {
    let is_posts_db = matches!(db_selection, DbSelection::PostsDb);
    let table_name_for_block = table_name.clone();

    let (data, last_page) = web::block(move || -> HelperResult<(Vec<HashMap<String, String>>, u32)> {
        let contrib_conn = pool.get()?;
        let (data, last_page) = advanced_db_ops::get_table_data(
            &posts_db,
            &contrib_conn,
            is_posts_db,
            &table_name_for_block,
            page,
            size,
            search_id.as_deref(),
        )?;
        Ok((data, last_page))
    }).await.unwrap()?;

    let transformed_data = if is_posts_db && (table_name.contains("metadata")) {
        data.into_iter().map(|mut row| {
            if let Some(val_str) = row.get("value") {
                if let Ok(meta) = serde_json::from_str::<crate::models::PostMetadata>(val_str) {
                    row.insert("title".to_string(), meta.title);
                    row.insert("summary".to_string(), meta.summary);
                    row.insert("tags".to_string(), meta.tags.join(", "));
                    row.insert("cover_image".to_string(), meta.cover_image.unwrap_or_default());
                    row.insert("created_at".to_string(), meta.created_at.to_string());
                }
            }
            row.remove("value");
            row
        }).collect()
    } else {
        data
    };

    Ok(PaginatedResponse { data: transformed_data, last_page })
}

pub async fn delete_table_rows(
    posts_db: web::Data<Database>,
    pool: web::Data<DbPool>,
    db_selection: DbSelection,
    table_name: String,
    row_id: String,
    dependents: Vec<DependentToDelete>,
) -> HelperResult<()> {
    web::block(move || {
        match db_selection {
            DbSelection::PostsDb => {
                advanced_db_ops::delete_redb_rows(&posts_db, &table_name, &row_id, &dependents)?;
            }
            DbSelection::ContributorDb => {
                let mut conn = pool.get()?;
                advanced_db_ops::delete_sqlite_rows(&mut conn, &table_name, &row_id, &dependents)?;
            }
        }
        Ok::<(), HelperError>(())
    }).await.unwrap()
}

pub async fn clean_table_with_auth(
    posts_db: web::Data<Database>,
    pool: web::Data<DbPool>,
    current_admin_user: String,
    admin_password_attempt: String,
    db_selection: DbSelection,
    table_name: String,
    clean_dependents: bool,
) -> HelperResult<()> {
    let pool_clone = pool.clone();
    let is_valid_password = web::block(move || -> Result<bool, HelperError> {
        let conn = pool_clone.get()?;
        let user_details = users_db_operations::read_user_by_username(&conn, &current_admin_user)
            .ok_or(HelperError::InvalidCredentials)?;

        let hash: String = conn.query_row(
            "SELECT password_hash FROM users WHERE id = ?1",
            [user_details.id],
            |row| row.get(0),
        )?;
        Ok(bcrypt::verify(&admin_password_attempt, &hash).unwrap_or(false))
    }).await.unwrap()?;

    if !is_valid_password {
        return Err(HelperError::InvalidCredentials);
    }
    
    let table_info_owned = get_db_structure_definition().databases.into_iter()
        .flat_map(|db| db.tables)
        .find(|t| t.name == table_name);

    if !table_info_owned.as_ref().map_or(false, |t| t.cleanable) {
        return Err(HelperError::Forbidden("This table cannot be cleaned.".to_string()));
    }

    web::block(move || {
        match db_selection {
            DbSelection::PostsDb => {
                advanced_db_ops::clean_redb_table(&posts_db, &table_name)?;
                if clean_dependents {
                    if let Some(info) = &table_info_owned {
                        for dep_name in &info.dependencies {
                           advanced_db_ops::clean_redb_table(&posts_db, dep_name)?;
                        }
                    }
                }
            }
            DbSelection::ContributorDb => {
                let conn = pool.get()?;
                advanced_db_ops::clean_sqlite_table(&conn, &table_name)?;
                 if clean_dependents {
                     if let Some(info) = &table_info_owned {
                        for dep_name in &info.dependencies {
                            advanced_db_ops::clean_sqlite_table(&conn, dep_name)?;
                        }
                    }
                }
            }
        }
        Ok::<(), HelperError>(())
    }).await.unwrap()
}

pub async fn update_table_cell(
    posts_db: web::Data<Database>,
    pool: web::Data<DbPool>,
    db_selection: DbSelection,
    table_name: String,
    row_id: String,
    column_name: String,
    value: String,
) -> HelperResult<()> {
    let editable_map = get_db_structure_definition().editable_cells;
    let is_editable = editable_map.get(&table_name).map_or(false, |cols| cols.contains(&column_name));

    if !is_editable {
        return Err(HelperError::Forbidden(format!(
            "Editing column '{}' in table '{}' is not allowed.",
            column_name, table_name
        )));
    }

    web::block(move || {
        match db_selection {
            DbSelection::PostsDb => {
                advanced_db_ops::update_redb_cell(&posts_db, &table_name, &row_id, &column_name, &value)?;
            }
            DbSelection::ContributorDb => {
                let conn = pool.get()?;
                advanced_db_ops::update_sqlite_cell(&conn, &table_name, &row_id, &column_name, &value)?;
            }
        }
        Ok::<(), HelperError>(())
    }).await.unwrap()
}