

use crate::models::{Contributor, PostAction}; // UPDATED
//use rusqlite::{params, Connection, OptionalExtension, Error as RusqliteError};
use bcrypt::{hash, verify, BcryptError};
use chrono::Utc;
use crate::models::EditLogEntry;
//use rusqlite::{Result as RusqliteResult};
use rusqlite::{params, Connection, OptionalExtension, Error as RusqliteError, Result as RusqliteResult};

fn bcrypt_to_rusqlite_error(e: BcryptError) -> RusqliteError {
    RusqliteError::ToSqlConversionFailure(Box::new(e))
}

pub fn create_user(
    conn: &Connection,
    username: &str,
    password: &str,
    role: &str,
) -> Result<(), RusqliteError> {
    let hashed_password = hash(password, bcrypt::DEFAULT_COST).map_err(bcrypt_to_rusqlite_error)?;
    conn.execute(
        "INSERT INTO users (username, password_hash, role) VALUES (?1, ?2, ?3)",
        params![username, hashed_password, role],
    )?;
    Ok(())
}

pub fn read_all_users(conn: &Connection) -> Result<Vec<Contributor>, RusqliteError> {
    let mut stmt = conn.prepare("SELECT id, username, role, is_active, can_edit_and_delete_own_posts, can_edit_any_post, can_delete_any_post, can_approve_posts, last_login_time FROM users ORDER BY id")?;
    let user_iter = stmt.query_map([], |row| {
        Ok(Contributor {
            id: row.get(0)?,
            username: row.get(1)?,
            role: row.get(2)?,
            is_active: row.get(3)?,
            can_edit_and_delete_own_posts: row.get(4)?,
            can_edit_any_post: row.get(5)?,
            can_delete_any_post: row.get(6)?,
            can_approve_posts: row.get(7)?,
            last_login_time: row.get(8)?,
        })
    })?;
    
    let users = user_iter.filter_map(|u| u.ok()).collect();
    Ok(users)
}

pub fn read_user_by_username(conn: &Connection, username: &str) -> Option<Contributor> {
    conn.query_row(
        "SELECT id, username, role, is_active, can_edit_and_delete_own_posts, can_edit_any_post, can_delete_any_post, can_approve_posts, last_login_time FROM users WHERE username = ?1",
        [username],
        |row| {
            Ok(Contributor {
                id: row.get(0)?,
                username: row.get(1)?,
                role: row.get(2)?,
                is_active: row.get(3)?,
                can_edit_and_delete_own_posts: row.get(4)?,
                can_edit_any_post: row.get(5)?,
                can_delete_any_post: row.get(6)?,
                can_approve_posts: row.get(7)?,
                last_login_time: row.get(8)?,
            })
        },
    ).ok()
}

pub fn update_user(
    conn: &Connection,
    user_id: i32,
    username: &str,
    new_password: Option<&str>,
    is_active: bool,
    can_delete_own: bool,
    can_edit_any: bool,
    can_delete_any: bool,
    can_approve_posts: bool,
) -> Result<(), RusqliteError> {
    if let Some(password) = new_password {
        if !password.is_empty() {
            let hashed_password = hash(password, bcrypt::DEFAULT_COST).map_err(bcrypt_to_rusqlite_error)?;
            conn.execute(
                "UPDATE users SET username = ?1, password_hash = ?2, is_active = ?3, can_edit_and_delete_own_posts = ?4, can_edit_any_post = ?5, can_delete_any_post = ?6, can_approve_posts = ?7 WHERE id = ?8",
                params![username, hashed_password, is_active, can_delete_own, can_edit_any, can_delete_any, can_approve_posts, user_id],
            )?;
            return Ok(());
        }
    }

    conn.execute(
        "UPDATE users SET username = ?1, is_active = ?2, can_edit_and_delete_own_posts = ?3, can_edit_any_post = ?4, can_delete_any_post = ?5, can_approve_posts = ?6 WHERE id = ?7",
        params![username, is_active, can_delete_own, can_edit_any, can_delete_any, can_approve_posts, user_id],
    )?;
    Ok(())
}

pub fn delete_user(conn: &Connection, user_id: i32) -> Result<usize, RusqliteError> {
    conn.execute("DELETE FROM users WHERE id = ?1", [user_id])
}

pub fn verify_credentials(
    conn: &Connection,
    username: &str,
    password: &str,
) -> Option<(String, String)> {
    let res: rusqlite::Result<(String, String, bool)> = conn.query_row(
        "SELECT password_hash, role, is_active FROM users WHERE username = ?1",
        [username],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    );

    if let Ok((hash, role, is_active)) = res {
        if is_active && verify(password, &hash).unwrap_or(false) {
            return Some((username.to_string(), role));
        }
    }
    None
}

pub fn update_last_login_time(conn: &Connection, username: &str) -> Result<(), RusqliteError> {
    let now = Utc::now().to_rfc3339();
    conn.execute("UPDATE users SET last_login_time = ?1 WHERE username = ?2", params![now, username])?;
    Ok(())
}


pub fn read_setting(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| row.get(0))
        .optional()
        .unwrap_or(None)
}

pub fn update_setting(conn: &Connection, key: &str, value: &str) -> Result<(), RusqliteError> {
    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        [key, value],
    )?;
    Ok(())
}

pub fn check_permission(conn: &Connection, user: &Contributor, post_id: &str, action: PostAction) -> bool {
    if user.role == "admin" { return true; }

    let post_owner_id: rusqlite::Result<i32> = conn.query_row(
        "SELECT user_id FROM post_ownership WHERE post_id = ?1",
        [post_id],
        |row| row.get(0),
    );

    let is_owner = post_owner_id.map_or(false, |owner_id| owner_id == user.id);

    match action {
        PostAction::Edit => (is_owner && user.can_edit_and_delete_own_posts) || user.can_edit_any_post,
        PostAction::Delete => (is_owner && user.can_edit_and_delete_own_posts) || user.can_delete_any_post,
    }
}

// UPDATED: Refined permission logic
pub fn check_pending_permission(conn: &Connection, user: &Contributor, post_id: &str, action: PostAction) -> bool {
    let post_owner_id: rusqlite::Result<i32> = conn.query_row(
        "SELECT user_id FROM pending_post_ownership WHERE post_id = ?1",
        [post_id],
        |row| row.get(0),
    );

    let is_owner = post_owner_id.map_or(false, |owner_id| owner_id == user.id);

    match action {
        PostAction::Edit => is_owner, // Only the owner can edit their own pending post.
        PostAction::Delete => {
            // Owner, admin, or someone with approval rights can delete.
            is_owner || user.role == "admin" || user.can_approve_posts
        }
    }
}


// --- Functions for Media Attachments ---
pub fn add_media_attachment(
    conn: &Connection,
    id: &str,
    user_id: i32,
    tags: &str,
) -> Result<(), RusqliteError> {
    conn.execute(
        "INSERT INTO media_attachments (id, user_id, tags) VALUES (?1, ?2, ?3)",
        params![id, user_id, tags],
    )?;
    Ok(())
}

pub fn delete_media_attachment(conn: &Connection, id: &str) -> Result<usize, RusqliteError> {
    conn.execute("DELETE FROM media_attachments WHERE id = ?1", [id])
}

pub fn is_media_owner(conn: &Connection, user_id: i32, media_id: &str) -> bool {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM media_attachments WHERE id = ?1 AND user_id = ?2)",
        params![media_id, user_id],
        |row| row.get(0),
    ).unwrap_or(false)
}

pub fn list_media_ids_for_user(conn: &Connection, user_id: i32) -> Result<Vec<String>, RusqliteError> {
    let mut stmt = conn.prepare("SELECT id FROM media_attachments WHERE user_id = ?1")?;
    let rows = stmt.query_map(params![user_id], |row| row.get(0))?;

    let mut ids = Vec::new();
    for id_result in rows {
        ids.push(id_result?);
    }
    Ok(ids)
}

pub fn search_media_by_tag_from_db(
    conn: &Connection,
    tag_query: &str,
    limit: u32,
    offset: u32,
) -> Result<Vec<String>, RusqliteError> {
    let mut stmt = conn.prepare(
        "SELECT id FROM media_attachments WHERE tags LIKE ?1 ORDER BY rowid DESC LIMIT ?2 OFFSET ?3"
    )?;
    let rows = stmt.query_map(params![format!("%{}%", tag_query), limit, offset], |row| row.get(0))?;

    let mut ids = Vec::new();
    for id_result in rows {
        ids.push(id_result?);
    }
    Ok(ids)
}

// --- NEW FUNCTIONS for pending post ownership ---
pub fn add_pending_post_ownership(conn: &Connection, post_id: &str, user_id: i32) -> Result<(), RusqliteError> {
    conn.execute(
        "INSERT INTO pending_post_ownership (post_id, user_id) VALUES (?1, ?2)",
        params![post_id, user_id],
    )?;
    Ok(())
}

pub fn delete_pending_post_ownership(conn: &Connection, post_id: &str) -> Result<usize, RusqliteError> {
    conn.execute("DELETE FROM pending_post_ownership WHERE post_id = ?1", [post_id])
}

// Replace the existing function with this corrected version

pub fn get_pending_post_owner_id(conn: &Connection, post_id: &str) -> Result<i32, RusqliteError> {
    // First, try to find the owner in the 'pending_post_ownership' table.
    // This will succeed for brand new posts.
    let result = conn.query_row(
        "SELECT user_id FROM pending_post_ownership WHERE post_id = ?1",
        [post_id],
        |row| row.get(0),
    );

    match result {
        Ok(user_id) => Ok(user_id), // Found it! Return the user ID.
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            // If it's not in the pending table, it must be an edited post.
            // Now, check the main 'post_ownership' table.
            conn.query_row(
                "SELECT user_id FROM post_ownership WHERE post_id = ?1",
                [post_id],
                |row| row.get(0),
            )
        }
        Err(e) => Err(e), // Another type of database error occurred.
    }
}

pub fn get_username_by_id(conn: &Connection, user_id: i32) -> Result<String, RusqliteError> {
    conn.query_row(
        "SELECT username FROM users WHERE id = ?1",
        [user_id],
        |row| row.get(0),
    )
}

// Replace the existing function
pub fn append_to_edit_log(conn: &Connection, post_id: &str, editor_username: &str) -> RusqliteResult<()> {
    // This query now correctly handles the case where edit_log is NULL by fetching it as an Option<String>.
    let current_log_json: Option<String> = conn.query_row(
        "SELECT edit_log FROM post_ownership WHERE post_id = ?1",
        [post_id],
        |row| row.get(0),
    )?;

    let mut log: Vec<EditLogEntry> = match current_log_json {
        Some(json_str) if !json_str.is_empty() => serde_json::from_str(&json_str).unwrap_or_default(),
        _ => vec![], // This handles both None and Some("")
    };

    let new_entry = EditLogEntry {
        edit_number: (log.len() as u32) + 1,
        editor_username: editor_username.to_string(),
        edited_at: Utc::now(),
    };
    log.push(new_entry);

    let new_log_json = serde_json::to_string(&log).unwrap();
    conn.execute(
        "UPDATE post_ownership SET edit_log = ?1 WHERE post_id = ?2",
        params![new_log_json, post_id],
    )?;
    Ok(())
}