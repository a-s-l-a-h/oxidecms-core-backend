

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct EditLogEntry {
    pub edit_number: u32, // Sequential number for ordering
    pub editor_username: String,
    pub edited_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PostMetadata {
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub last_updated_at: Option<DateTime<Utc>>,
    pub summary: String,
    pub tags: Vec<String>,
    pub cover_image: Option<String>,
    pub has_call_to_action: Option<bool>,
    pub search_keywords: Option<Vec<String>>, 
}

#[derive(Serialize)]
pub struct FullPost {
    pub id: String,
    pub metadata: PostMetadata,
    pub content: String,
}

#[derive(Serialize, Clone)]
pub struct PostSummary {
    pub id: String,
    pub metadata: PostMetadata,
}

// --- NEW STRUCT ---
#[derive(Serialize)]
pub struct PendingPostSummaryWithOwner {
    pub post_summary: PostSummary,
    pub author_name: String,
}
// --- END NEW STRUCT ---


#[derive(Debug, Serialize)]
pub struct Contributor {
    pub id: i32,
    pub username: String,
    pub role: String,
    pub is_active: bool,
    pub can_edit_and_delete_own_posts: bool,
    pub can_edit_any_post: bool,
    pub can_delete_any_post: bool,
    pub can_approve_posts: bool, // <-- NEW FIELD
    pub last_login_time: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Notification {
    pub message: String,
    pub r#type: String, // 'success' or 'error'
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MediaAttachment {
    pub id: String,
    pub file_path: String,
    pub file_format: String,
    pub original_filename: String,
    pub file_size: i64,
    pub summary: String,
    pub tags: String,
    pub uploaded_at: DateTime<Utc>,
}

// NEW: Enum for type-safe permission checking
pub enum PostAction {
    Edit,
    Delete,
}

pub mod db_operations;
pub mod advanced_db_manager_models;