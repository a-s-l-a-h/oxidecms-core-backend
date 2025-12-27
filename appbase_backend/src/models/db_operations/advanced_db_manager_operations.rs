

use redb::{Database, ReadableTable, StorageError, TableDefinition, WriteTransaction};
use rusqlite::{Connection, Error as RusqliteError};
use std::collections::HashMap;
use thiserror::Error;
use uuid::Uuid;

use crate::models::PostMetadata;
use crate::models::advanced_db_manager_models::DependentToDelete;
use super::posts_db_operations as posts_db;

#[derive(Error, Debug)]
pub enum AdvancedDbError {
    #[error("Rusqlite error: {0}")]
    Rusqlite(#[from] RusqliteError),
    #[error("Redb storage error: {0}")]
    RedbStorage(#[from] StorageError),
    #[error("Redb transaction error: {0}")]
    RedbTransaction(#[from] redb::TransactionError),
    #[error("Redb table error: {0}")]
    RedbTable(#[from] redb::TableError),
    #[error("Redb commit error: {0}")]
    RedbCommit(#[from] redb::CommitError),
    #[error("Serde JSON error: {0}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("Invalid UUID: {0}")]
    Uuid(#[from] uuid::Error),
    #[error("Not Found: {0}")]
    NotFound(String),
    #[error("Unsupported Operation: {0}")]
    Unsupported(String),
    #[error("Invalid Input: {0}")]
    InvalidInput(String),
}

type DbResult<T> = Result<T, AdvancedDbError>;

// =================================================================
// ============== GENERIC FETCH & COUNT (DISPATCHERS) ==============
// =================================================================
pub fn get_table_data(
    posts_db: &Database,
    contrib_conn: &Connection,
    is_posts_db: bool,
    table_name: &str,
    page: u32,
    size: u32,
    search_id: Option<&str>,
) -> DbResult<(Vec<HashMap<String, String>>, u32)> {
    let offset = (page.saturating_sub(1)) * size;

    if is_posts_db {
        get_redb_table_data(posts_db, table_name, size, offset, search_id)
    } else {
        get_sqlite_table_data(contrib_conn, table_name, size, offset, search_id)
    }
}

// =================================================================
// ==================== SQLITE (CONTRIBUTOR DB) ====================
// =================================================================
fn get_sqlite_table_data(
    conn: &Connection,
    table_name: &str,
    limit: u32,
    offset: u32,
    search_id: Option<&str>,
) -> DbResult<(Vec<HashMap<String, String>>, u32)> {
    if !table_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(AdvancedDbError::InvalidInput("Invalid table name.".into()));
    }

    let mut base_query = format!("FROM {}", table_name);
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(id) = search_id {
        let pk_col = if table_name == "users" { "id" } else { "post_id" };
        base_query.push_str(&format!(" WHERE {} = ?1", pk_col));
        params.push(Box::new(id.to_string()));
    }

    let count_query = format!("SELECT COUNT(*) {}", base_query);
    let total_rows: u32 = conn.query_row(
        &count_query,
        rusqlite::params_from_iter(params.iter()),
        |row| row.get(0),
    )?;

    let data_query = format!(
        "SELECT * {} ORDER BY rowid DESC LIMIT {} OFFSET {}",
        base_query, limit, offset
    );
    let mut stmt = conn.prepare(&data_query)?;
    let col_names: Vec<String> = stmt.column_names().into_iter().map(String::from).collect();

    let rows_iter = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
        let mut map = HashMap::new();
        for (i, name) in col_names.iter().enumerate() {
            let val: rusqlite::types::Value = row.get(i)?;
            let val_str = match val {
                rusqlite::types::Value::Null => "".to_string(),
                rusqlite::types::Value::Integer(i) => i.to_string(),
                rusqlite::types::Value::Real(f) => f.to_string(),
                rusqlite::types::Value::Text(t) => t,
                rusqlite::types::Value::Blob(_) => "[BLOB]".to_string(),
            };
            map.insert(name.clone(), val_str);
        }
        Ok(map)
    })?;

    let data = rows_iter.collect::<Result<Vec<_>, _>>()?;
    let last_page = (total_rows as f32 / limit as f32).ceil() as u32;

    Ok((data, last_page))
}

pub fn delete_sqlite_rows(conn: &mut Connection, main_table: &str, main_row_id: &str, dependents: &[DependentToDelete]) -> DbResult<()> {
    let tx = conn.transaction()?;

    // Delete main row
    let pk_col_main = if main_table == "users" { "id" } else { "post_id" };
    let query_main = format!("DELETE FROM {} WHERE {} = ?1", main_table, pk_col_main);
    tx.execute(&query_main, [main_row_id])?;

    // Delete dependents
    for dep in dependents {
        let pk_col_dep = if dep.table_name == "users" { "id" } else { "post_id" };
        let query_dep = format!("DELETE FROM {} WHERE {} = ?1", dep.table_name, pk_col_dep);
        tx.execute(&query_dep, [&dep.row_id])?;
    }

    tx.commit()?;
    Ok(())
}


pub fn clean_sqlite_table(conn: &Connection, table_name: &str) -> DbResult<()> {
    if !table_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(AdvancedDbError::InvalidInput("Invalid table name.".into()));
    }
    let query = format!("DELETE FROM {}", table_name);
    conn.execute(&query, [])?;
    if table_name == "users" {
        conn.execute("DELETE FROM sqlite_sequence WHERE name = 'users'", [])?;
    }
    Ok(())
}

pub fn update_sqlite_cell(
    conn: &Connection,
    table_name: &str,
    row_id: &str,
    column_name: &str,
    value: &str,
) -> DbResult<usize> {
    if !table_name.chars().all(|c| c.is_alphanumeric() || c == '_')
        || !column_name.chars().all(|c| c.is_alphanumeric() || c == '_')
    {
        return Err(AdvancedDbError::InvalidInput("Invalid table or column name.".into()));
    }

    let pk_col = if table_name == "users" { "id" } else { "post_id" };
    let query = format!("UPDATE {} SET {} = ?1 WHERE {} = ?2", table_name, column_name, pk_col);
    
    let count = conn.execute(&query, rusqlite::params![value, row_id])?;
    Ok(count)
}

// =================================================================
// ========================= REDB (POSTS DB) =========================
// =================================================================

fn with_redb_table<'a, 'b, F, R>(
    txn: &'a WriteTransaction,
    table_name: &'b str,
    mut f: F,
) -> DbResult<R>
where
    F: FnMut(&mut redb::Table<&[u8; 16], &str>) -> DbResult<R>,
{
    match table_name {
        "posts" => f(&mut txn.open_table(posts_db::POSTS)?),
        "metadata" => f(&mut txn.open_table(posts_db::METADATA)?),
        "pending_posts" => f(&mut txn.open_table(posts_db::PENDING_POSTS)?),
        "pending_metadata" => f(&mut txn.open_table(posts_db::PENDING_METADATA)?),
        _ => Err(AdvancedDbError::NotFound(format!("Redb table '{}' not found.", table_name))),
    }
}

// NEW: Function to delete multiple rows from different tables in one transaction
pub fn delete_redb_rows(db: &Database, main_table: &str, main_row_id: &str, dependents: &[DependentToDelete]) -> DbResult<()> {
    let write_txn = db.begin_write()?;

    // Delete main row
    let main_uuid = Uuid::parse_str(main_row_id)?;
    with_redb_table(&write_txn, main_table, |table| {
        table.remove(&main_uuid.into_bytes())?;
        Ok(())
    })?;

    // Delete selected dependents
    for dep in dependents {
        let dep_uuid = Uuid::parse_str(&dep.row_id)?;
        with_redb_table(&write_txn, &dep.table_name, |table| {
            table.remove(&dep_uuid.into_bytes())?;
            Ok(())
        })?;
    }

    write_txn.commit()?;
    Ok(())
}

fn get_redb_table_data(
    db: &Database,
    table_name: &str,
    limit: u32,
    offset: u32,
    search_id: Option<&str>,
) -> DbResult<(Vec<HashMap<String, String>>, u32)> {
    let read_txn = db.begin_read()?;

    let table_def: TableDefinition<&[u8; 16], &str> = TableDefinition::new(table_name);
    let table = read_txn.open_table(table_def)?;

    let mut data = Vec::new();
    let total_rows = table.len()? as u32;

    if let Some(id_str) = search_id {
        let uuid = Uuid::parse_str(id_str)?;
        if let Some(val_guard) = table.get(&uuid.into_bytes())? {
            let mut map = HashMap::new();
            map.insert("id".to_string(), uuid.to_string());
            map.insert("value".to_string(), val_guard.value().to_string());
            data.push(map);
        }
    } else {
        let iter = table.iter()?.rev();
        for item in iter.skip(offset as usize).take(limit as usize) {
            let (key_guard, val_guard) = item?;
            let uuid = Uuid::from_bytes(*key_guard.value());
            let mut map = HashMap::new();
            map.insert("id".to_string(), uuid.to_string());
            map.insert("value".to_string(), val_guard.value().to_string());
            data.push(map);
        }
    }

    let last_page = (total_rows as f32 / limit as f32).ceil() as u32;
    Ok((data, last_page))
}

pub fn clean_redb_table(db: &Database, table_name: &str) -> DbResult<()> {
    let write_txn = db.begin_write()?;
     with_redb_table(&write_txn, table_name, |table| {
        let keys_to_delete: Vec<_> = table.iter()?
            .map(|res| res.map(|(k, _)| *k.value()))
            .collect::<Result<_,_>>()?;

        for key in keys_to_delete {
            table.remove(&key)?;
        }
        Ok(())
    })?;
    write_txn.commit()?;
    Ok(())
}

pub fn update_redb_cell(
    db: &Database,
    table_name: &str,
    row_id: &str,
    column_name: &str,
    new_value: &str,
) -> DbResult<()> {
    let uuid = Uuid::parse_str(row_id)?;
    let uuid_bytes = uuid.into_bytes();
    let write_txn = db.begin_write()?;

    with_redb_table(&write_txn, table_name, |table| {
        let old_value_str = {
            let old_value_guard = table.get(&uuid_bytes)?
                .ok_or_else(|| AdvancedDbError::NotFound(format!("Row with ID {} not found.", row_id)))?;
            old_value_guard.value().to_string()
        };

        let final_json_str = if table_name.contains("metadata") {
            let mut meta: PostMetadata = serde_json::from_str(&old_value_str)?;
            
            match column_name {
                "title" => meta.title = new_value.to_string(),
                "summary" => meta.summary = new_value.to_string(),
                "tags" => meta.tags = new_value.split(',').map(|s| s.trim().to_string()).collect(),
                "cover_image" => meta.cover_image = Some(new_value.to_string()).filter(|s| !s.is_empty()),
                _ => return Err(AdvancedDbError::Unsupported(format!("Editing column '{}' is not supported.", column_name))),
            }
            serde_json::to_string(&meta)?

        } else if table_name.contains("posts") && column_name == "value" {
            new_value.to_string()
        } else {
            return Err(AdvancedDbError::Unsupported(format!("Editing table '{}' or column '{}' is not supported.", table_name, column_name)));
        };

        table.insert(&uuid_bytes, final_json_str.as_str())?;
        Ok(())
    })?;

    write_txn.commit()?;
    Ok(())
}