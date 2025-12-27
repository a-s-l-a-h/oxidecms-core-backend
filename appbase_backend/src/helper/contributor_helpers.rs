use crate::models::db_operations::{posts_db_operations, users_db_operations};
use crate::models::{Contributor, PostSummary, MediaAttachment, FullPost, PendingPostSummaryWithOwner, PostAction};
use crate::config::Config;
use crate::DbPool;
use actix_web::{web, web::BytesMut};
use actix_multipart::Multipart;
use futures_util::StreamExt;
use redb::Database;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use chrono::Utc;
use std::collections::{HashSet, BTreeMap};
use crate::helper::sanitization_helpers;

// --- NEW: Secure MIME type to extension mapping ---
/// Securely maps a validated MIME type to a safe file extension.
/// This is intentionally not configurable to prevent insecure mappings.
fn mime_to_safe_extension(mime_type: &str) -> Option<&'static str> {
    // Using a BTreeMap for efficient, ordered lookups.
    let map: BTreeMap<&str, &str> = [
        ("application/pdf", "pdf"),
        ("application/zip", "zip"),
        ("audio/mpeg", "mp3"),
        ("audio/wav", "wav"),
        ("image/gif", "gif"),
        ("image/jpeg", "jpg"),
        ("image/png", "png"),
        ("image/webp", "webp"),
        ("model/gltf-binary", "glb"),
        ("model/obj", "obj"),
        ("video/mp4", "mp4"),
        ("video/webm", "webm"),
    ].iter().cloned().collect();

    map.get(mime_type).cloned()
}


// --- Existing Helper Functions (Updated for DbPool) ---
pub fn get_contributor_details(pool: &web::Data<DbPool>, username: &str) -> Option<Contributor> {
    let conn = pool.get().ok()?;
    users_db_operations::read_user_by_username(&conn, username)
}

pub fn can_contributor_perform_action(
    pool: &web::Data<DbPool>,
    contributor: &Contributor,
    post_id: &str,
    action: PostAction, // UPDATED
) -> bool {
    if let Ok(conn) = pool.get() {
        users_db_operations::check_permission(&conn, contributor, post_id, action)
    } else {
        false
    }
}

// Check permission for pending posts
pub fn can_contributor_perform_pending_action(
    pool: &web::Data<DbPool>,
    contributor: &Contributor,
    post_id: &str,
    action: PostAction, // UPDATED
) -> bool {
    if let Ok(conn) = pool.get() {
        users_db_operations::check_pending_permission(&conn, contributor, post_id, action)
    } else {
        false
    }
}

pub fn get_all_available_tags(db: &web::Data<Database>) -> Result<Vec<String>, posts_db_operations::DbError> {
    posts_db_operations::get_all_available_tags(db)
}

// --- NEW/MODIFIED Helper Functions ---

pub fn submit_post_for_approval(
    db: &web::Data<Database>, pool: &web::Data<DbPool>, contributor: &Contributor,
    title: &str, summary: &str, content: &str, tags_str: &str,
    search_keywords_str: &str, cover_image: Option<&str>, has_call_to_action: Option<bool>,
) -> Result<String, Box<dyn std::error::Error>> {
    // Sanitize all inputs before saving to the database
    let clean_content = sanitization_helpers::sanitize_markdown_content(content);
    let clean_title = sanitization_helpers::strip_all_html(title);
    let clean_summary = sanitization_helpers::strip_all_html(summary);
    let clean_tags = sanitization_helpers::strip_all_html(tags_str);
    let clean_keywords = sanitization_helpers::strip_all_html(search_keywords_str);
    let clean_cover_image = cover_image.map(|url| sanitization_helpers::strip_all_html(url));

    let conn = pool.get()?;
    let new_post_id = posts_db_operations::create_pending_post(
        db, &clean_title, &clean_summary, &clean_content, &clean_tags,
        &clean_keywords, clean_cover_image.as_deref(), has_call_to_action
    )?;
    users_db_operations::add_pending_post_ownership(&conn, &new_post_id, contributor.id)?;
    Ok(new_post_id)
}

// Replace the existing function
pub fn update_pending_post(
    db: &web::Data<Database>, post_id: &str, title: &str, summary: &str, content: &str,
    tags_str: &str, search_keywords_str: &str, cover_image: Option<&str>,
    has_call_to_action: Option<bool>,
) -> Result<(), Box<dyn std::error::Error>> {
    let clean_content = sanitization_helpers::sanitize_markdown_content(content);
    let clean_title = sanitization_helpers::strip_all_html(title);
    let clean_summary = sanitization_helpers::strip_all_html(summary);
    let clean_tags = sanitization_helpers::strip_all_html(tags_str);
    let clean_keywords = sanitization_helpers::strip_all_html(search_keywords_str);
    let clean_cover_image = cover_image.map(|url| sanitization_helpers::strip_all_html(url));

    posts_db_operations::update_pending_post(
        db, post_id, &clean_title, &clean_summary, &clean_content, &clean_tags,
        &clean_keywords, clean_cover_image.as_deref(), has_call_to_action
    ).map_err(|e| e.into())
}

// // MODIFIED: This function now updates a published post.
// pub fn update_post(
//     db: &web::Data<Database>,
//     post_id: &str,
//     title: &str,
//     summary: &str,
//     content: &str,
//     tags_str: &str,
//     search_keywords_str: &str,
//     cover_image: Option<&str>,
//     has_call_to_action: Option<bool>,
// ) -> Result<(), Box<dyn std::error::Error>> {
//     posts_db_operations::update_post(db, post_id, title, summary, content, tags_str, search_keywords_str, cover_image, has_call_to_action)
//         .map_err(|e| e.into())
// }

// Replace the old update_post function with this one.
/// Handles edits to a PUBLISHED post by logging the change and moving the post
/// to the pending queue for re-approval. It does NOT update the live post directly.
pub fn re_submit_for_approval(
    db: &web::Data<Database>, pool: &web::Data<DbPool>, editor: &Contributor,
    post_id: &str, title: &str, summary: &str, content: &str, tags_str: &str,
    search_keywords_str: &str, cover_image: Option<&str>, has_call_to_action: Option<bool>,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = pool.get()?;

    // 1. Log the edit action first.
    users_db_operations::append_to_edit_log(&conn, post_id, &editor.username)?;

    // 2. Atomically move the post from published to pending state.
    posts_db_operations::move_published_to_pending(db, post_id)?;

    // 3. Update the content of the (now pending) post with the new sanitized data.
    update_pending_post(
        db, post_id, title, summary, content, tags_str,
        search_keywords_str, cover_image, has_call_to_action
    )?;

    Ok(())
}

// MODIFIED: This function now deletes a published post.
pub fn delete_post(
    db: &web::Data<Database>,
    pool: &web::Data<DbPool>,
    post_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = pool.get()?;
    posts_db_operations::delete_post(db, &conn, post_id)
        .map_err(|e| e.into())
}

// // NEW: Fetches pending posts for the approval queue.
// pub async fn fetch_pending_posts_with_owners(
//     db: &web::Data<Database>,
//     pool: &web::Data<DbPool>,
//     limit: u32,
//     offset: u32,
// ) -> Result<Vec<PendingPostSummaryWithOwner>, Box<dyn std::error::Error>> {
//     let summaries = posts_db_operations::read_all_pending_post_summaries_paginated(db, limit, offset)?;
//     let mut results = Vec::new();
//     let conn = pool.get()?; // Get one connection for all lookups
//     for summary in summaries {
//         let user_id = users_db_operations::get_pending_post_owner_id(&conn, &summary.id)?;
//         let author_name = users_db_operations::get_username_by_id(&conn, user_id).unwrap_or_else(|_| "Unknown".to_string());
//         results.push(PendingPostSummaryWithOwner {
//             post_summary: summary,
//             author_name,
//         });
//     }
//     Ok(results)
// }
// Replace the existing function with this one

pub async fn fetch_pending_posts_with_owners(
    db: &web::Data<Database>,
    pool: &web::Data<DbPool>,
    limit: u32,
    offset: u32,
) -> Result<Vec<PendingPostSummaryWithOwner>, Box<dyn std::error::Error>> {
    let summaries = posts_db_operations::read_all_pending_post_summaries_paginated(db, limit, offset)?;
    let mut results = Vec::new();
    let conn = pool.get()?; // Get one connection for all lookups

    for summary in summaries {
        // Use `if let` to safely handle cases where an owner might not be found.
        // This prevents the application from crashing if a post has no owner record.
        if let Ok(user_id) = users_db_operations::get_pending_post_owner_id(&conn, &summary.id) {
            let author_name = users_db_operations::get_username_by_id(&conn, user_id)
                .unwrap_or_else(|_| "Unknown".to_string());
            
            results.push(PendingPostSummaryWithOwner {
                post_summary: summary,
                author_name,
            });
        } else {
            // If an owner is not found for a pending post, log it as a warning but don't crash.
            // This orphan post will simply not be shown in the approval list.
            log::warn!(
                "Orphan pending post found with ID: {}. It has no owner and will be skipped.",
                &summary.id
            );
        }
    }
    Ok(results)
}

// NEW: Gets full details of a single pending post for review.
pub fn get_pending_post_details(db: &web::Data<Database>, id: &str) -> Option<FullPost> {
    posts_db_operations::read_pending_post(db, id)
}

// NEW: Gets full details of a single PENDING post for its OWNER.
pub fn get_own_pending_post_details(db: &web::Data<Database>, pool: &web::Data<DbPool>, user: &Contributor, post_id: &str) -> Option<FullPost> {
    if !can_contributor_perform_pending_action(pool, user, post_id, PostAction::Edit) {
        return None;
    }
    posts_db_operations::read_pending_post(db, post_id)
}

// NEW: Gets full details of a single PUBLISHED post for its OWNER or an ADMIN.
pub fn get_own_post_details(db: &web::Data<Database>, pool: &web::Data<DbPool>, user: &Contributor, post_id: &str) -> Option<FullPost> {
    if !can_contributor_perform_action(pool, user, post_id, PostAction::Edit) {
        return None;
    }
    posts_db_operations::read_post(db, post_id)
}


// NEW: Approves a pending post.
pub fn approve_post(
    db: &web::Data<Database>,
    pool: &web::Data<DbPool>,
    post_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = pool.get()?;
    posts_db_operations::approve_post(db, &conn, post_id).map_err(|e| e.into())
}

// NEW: Deletes a post from the pending queue.
pub fn delete_pending_post(
    db: &web::Data<Database>,
    pool: &web::Data<DbPool>,
    post_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = pool.get()?;
    // Transaction-like behavior: DB op first. If it fails, we don't touch ownership.
    posts_db_operations::delete_pending_post(db, post_id)?;
    users_db_operations::delete_pending_post_ownership(&conn, post_id)?;
    Ok(())
}

// NEW: Fetches a contributor's own pending posts.
pub fn fetch_own_pending_posts(
    db: &web::Data<Database>,
    pool: &web::Data<DbPool>,
    user_id: i32,
    limit: u32,
    offset: u32,
) -> Result<Vec<PostSummary>, posts_db_operations::DbError> {
    let conn = pool.get().map_err(|_| posts_db_operations::DbError::NotFound("DB connection failed".to_string()))?;
    posts_db_operations::read_pending_post_summaries_by_user(db, &conn, user_id, limit, offset)
}


// --- Functions below are heavily modified ---

pub async fn save_media_attachment(
    config: web::Data<Config>,
    pool: web::Data<DbPool>,
    user_id: i32,
    mut payload: Multipart,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    let conn = pool.get()?;
    
    // --- FETCH DYNAMIC SETTINGS FIRST ---
    let max_file_size_mb_str = users_db_operations::read_setting(&conn, "max_file_upload_size_mb")
        .unwrap_or_else(|| "10".to_string());
    let max_file_size_mb = max_file_size_mb_str.parse::<u64>().unwrap_or(10);
    let max_file_size_bytes = max_file_size_mb * 1024 * 1024;

    // Fetch the allowed MIME types string from the database.
    let allowed_mime_types_str = users_db_operations::read_setting(&conn, "allowed_mime_types")
        .unwrap_or_else(|| "".to_string()); // Default to empty string

    // If the setting is empty, no file types are allowed. This is a secure default.
    if allowed_mime_types_str.is_empty() {
        return Err("File uploads are currently disabled. No MIME types are configured.".into());
    }

    // Parse the database string into a collection for easy lookup.
    let allowed_mime_types: HashSet<String> = allowed_mime_types_str
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();
    
    let mut file_path = PathBuf::new();
    let mut file_size: u64 = 0;
    let mut tags = String::new();
    let mut summary = String::new();
    let mut original_filename = String::new();
    let mut file_ext_str = String::new();
    let file_id = Uuid::new_v4();
    let file_id_str = file_id.to_string();

    while let Some(item) = payload.next().await {
        let mut field = item?;
        let field_name = field.content_disposition().get_name().unwrap_or_default().to_string();

        match field_name.as_str() {
            "file" => {
                let content_type = field.content_type().ok_or("Content-Type not available.")?;
                let content_type_str = content_type.to_string();

                // --- 1. VALIDATE against the database setting ---
                if !allowed_mime_types.contains(&content_type_str) {
                    return Err(format!("Unsupported file type: '{}'. Please upload one of the allowed types.", content_type_str).into());
                }

                // --- 2. SECURELY MAP the validated MIME to an extension ---
                file_ext_str = match mime_to_safe_extension(&content_type_str) {
                    Some(ext) => ext.to_string(),
                    None => {
                        log::error!("Admin configured allowed MIME type '{}' which has no safe extension mapping.", content_type_str);
                        return Err("An internal server configuration error occurred. Please contact an administrator.".into());
                    }
                };

                let filename = field.content_disposition().get_filename().unwrap_or("upload.tmp");
                original_filename = filename.to_string();

                // --- 3. CONSTRUCT filename with the safe extension ---
                let dir1 = &file_id_str[0..2];
                let dir2 = &file_id_str[2..4];
                let new_filename = format!("{}.{}", &file_id_str, &file_ext_str);
                let base_media_path = PathBuf::from(&config.media_path);
                let path = base_media_path.join("attachments").join(dir1).join(dir2);

                // Use web::block for ALL blocking file system operations
                web::block({
                    let path_clone = path.clone();
                    move || fs::create_dir_all(&path_clone)
                }).await??;

                let final_path = path.join(new_filename);
                file_path = final_path.clone();

                let mut f = web::block({
                    let final_path_clone = final_path.clone();
                    move || fs::File::create(final_path_clone)
                }).await??;
                
                while let Some(chunk) = field.next().await {
                    let data = chunk?;
                    file_size += data.len() as u64;
                    if file_size > max_file_size_bytes {
                        drop(f); 
                        let _ = fs::remove_file(&file_path);
                        return Err(format!("File is too large. Maximum size is {}MB.", max_file_size_mb).into());
                    }
                    f = web::block(move || f.write_all(&data).map(|_| f)).await??;
                }
            }
            "tags" | "summary" => {
                let mut data = BytesMut::new();
                while let Some(chunk) = field.next().await {
                    data.extend_from_slice(&chunk?);
                }
                // Handle UTF-8 error without panicking
                let value = String::from_utf8(data.to_vec())
                    .map_err(|_| "Invalid UTF-8 in form field.")?;

                if value.trim().is_empty() {
                    return Err(format!("{} is mandatory and cannot be empty.", field_name).into());
                }

                // Enforce length limits
                if field_name == "summary" && value.len() > 500 {
                     return Err("Summary cannot exceed 500 characters.".into());
                }
                 if field_name == "tags" && value.len() > 200 {
                     return Err("Tags cannot exceed 200 characters in total.".into());
                }

                if field_name == "tags" {
                    tags = value;
                } else {
                    summary = value;
                }
            }
            _ => (),
        }
    }
    
    if file_path.as_os_str().is_empty() { return Err("No file was uploaded.".into()); }
    
    let display_path = format!("/media/attachments/{}/{}/{}.{}", &file_id_str[0..2], &file_id_str[2..4], file_id_str, file_ext_str);
    
    let sidecar_data = MediaAttachment {
        id: file_id_str.clone(),
        file_path: display_path.clone(),
        file_format: file_ext_str,
        original_filename,
        file_size: file_size as i64,
        summary,
        tags: tags.clone(),
        uploaded_at: Utc::now(),
    };

    let sidecar_json = serde_json::to_string_pretty(&sidecar_data)?;
    let sidecar_path = file_path.with_extension("json");
    fs::write(sidecar_path, sidecar_json)?;
    
    users_db_operations::add_media_attachment(&conn, &file_id_str, user_id, &tags)?;

    Ok((display_path.replace('\\', "/"), file_id_str))
}


fn read_sidecar(path: &Path) -> Result<MediaAttachment, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let metadata: MediaAttachment = serde_json::from_str(&content)?;
    Ok(metadata)
}


pub fn get_user_media(config: &web::Data<Config>, pool: &web::Data<DbPool>, user_id: i32) -> Result<Vec<MediaAttachment>, rusqlite::Error> {
    let conn = pool.get().map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into()))?;
    let media_ids = users_db_operations::list_media_ids_for_user(&conn, user_id)?;
    let mut attachments = Vec::new();
    
    let base_path = PathBuf::from(&config.media_path).join("attachments");

    for id in media_ids {
        let dir1 = &id[0..2];
        let dir2 = &id[2..4];
        let sidecar_path = base_path.join(dir1).join(dir2).join(format!("{}.json", id));

        if sidecar_path.exists() {
            if let Ok(data) = read_sidecar(&sidecar_path) {
                attachments.push(data);
            }
        }
    }
    attachments.sort_by(|a, b| b.uploaded_at.cmp(&a.uploaded_at));
    Ok(attachments)
}


pub async fn delete_media(
    config: &web::Data<Config>,
    pool: &web::Data<DbPool>,
    user: &Contributor,
    media_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = pool.get()?;
    let is_owner = users_db_operations::is_media_owner(&conn, user.id, media_id);
    if user.role != "admin" && !is_owner {
        return Err("Permission denied. You are not the owner of this media.".into());
    }

    // UPDATED: Prioritize database consistency
    // 1. Delete the database record first.
    users_db_operations::delete_media_attachment(&conn, media_id)?;
    
    // 2. Attempt to delete files, but only log errors, don't fail the whole operation.
    let base_path = PathBuf::from(&config.media_path).join("attachments");
    let dir1 = &media_id[0..2];
    let dir2 = &media_id[2..4];
    let sidecar_path = base_path.join(dir1).join(dir2).join(format!("{}.json", media_id));

    if sidecar_path.exists() {
        if let Ok(sidecar_data) = read_sidecar(&sidecar_path) {
            let file_to_delete_path = base_path.join(dir1).join(dir2).join(format!("{}.{}", media_id, sidecar_data.file_format));
            
            // Use web::block for blocking file operations
            web::block(move || fs::remove_file(&file_to_delete_path))
                .await
                .map_err(|e| format!("Blocking error on file delete: {}", e))?
                .unwrap_or_else(|e| log::error!("Failed to delete media file for {}: {}", media_id, e));
        }

        web::block(move || fs::remove_file(&sidecar_path))
            .await
            .map_err(|e| format!("Blocking error on sidecar delete: {}", e))?
            .unwrap_or_else(|e| log::error!("Failed to delete sidecar file for {}: {}", media_id, e));
    } else {
        log::warn!("Sidecar file for media_id {} was already missing during deletion.", media_id);
    }
    
    Ok(())
}

pub fn fetch_posts_for_user(
    db: &web::Data<Database>,
    pool: &web::Data<DbPool>,
    user_id: i32,
    limit: u32,
    offset: u32,
) -> Result<Vec<PostSummary>, posts_db_operations::DbError> {
    let conn = pool.get().map_err(|_| posts_db_operations::DbError::NotFound("DB connection failed".to_string()))?;
    posts_db_operations::read_post_summaries_by_user(db, &conn, user_id, limit, offset)
}


pub fn search_all_media_by_tag(
    config: &web::Data<Config>,
    pool: &web::Data<DbPool>,
    tag_query: &str,
    limit: u32,
    offset: u32,
) -> Vec<MediaAttachment> {
    let conn = match pool.get() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    
    let media_ids = match users_db_operations::search_media_by_tag_from_db(&conn, tag_query, limit, offset) {
        Ok(ids) => ids,
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();
    let attachments_dir = PathBuf::from(&config.media_path).join("attachments");

    if !attachments_dir.exists() { return results; }

    for media_id in media_ids {
        let dir1 = &media_id[0..2];
        let dir2 = &media_id[2..4];
        let sidecar_path = attachments_dir.join(dir1).join(dir2).join(format!("{}.json", media_id));

        if sidecar_path.exists() {
            if let Ok(sidecar) = read_sidecar(&sidecar_path) {
                results.push(sidecar);
            }
        }
    }

    results.sort_by(|a, b| b.uploaded_at.cmp(&a.uploaded_at));
    results
}

pub fn check_similar_posts(
    db: &web::Data<Database>,
    title: &str,
    tags_str: &str,
    check_type: &str,
    exclude_id: Option<&str>,
) -> Result<Vec<PostSummary>, posts_db_operations::DbError> {
    let tags_to_check: HashSet<String> = tags_str
        .split(',')
        .map(|s| s.trim().to_lowercase()) // NORMALIZE
        .filter(|s| !s.is_empty())
        .collect();

    let (check_by_title, check_by_tags) = match check_type {
        "title" => (true, false),
        "tags" => (false, true),
        "both" => (true, true),
        _ => (false, false),
    };

    if !check_by_title && !check_by_tags {
        return Ok(Vec::new());
    }

    posts_db_operations::find_similar_posts(db, title, &tags_to_check, check_by_title, check_by_tags, exclude_id)
}

pub fn search_posts(
    db: &web::Data<Database>,
    search_type: &str,
    query: &str,
    limit: u32,
    offset: u32,
) -> Result<Vec<PostSummary>, posts_db_operations::DbError> {
    match search_type {
        "post_id" => {
            posts_db_operations::read_post_summary_by_id(db, query)
                .map(|opt| opt.into_iter().collect())
        }
        "tag" => {
            posts_db_operations::read_post_summaries_by_tag(db, &query.to_lowercase(), limit, offset) // NORMALIZE
        }
        "title" => {
            posts_db_operations::read_post_summaries_by_title(db, query, limit, offset)
        }
        "keyword" => { 
            posts_db_operations::read_post_summaries_by_keyword(db, query, limit, offset)
        }
        _ => {
            Ok(Vec::new())
        }
    }
}