use crate::models::db_operations::{posts_db_operations, users_db_operations};
use crate::models::{FullPost, PostSummary};
use crate::DbPool;
use actix_web::web;
use redb::Database;

pub fn verify_contributor_credentials(
    pool: &web::Data<DbPool>,
    username: &str,
    password: &str,
) -> Option<(String, String)> {
    if let Ok(conn) = pool.get() {
        users_db_operations::verify_credentials(&conn, username, password)
    } else {
        None
    }
}

pub fn fetch_post_by_id(id: &str, db: &web::Data<Database>) -> Option<FullPost> {
    posts_db_operations::read_post(db, id)
}

// UPDATED: This function now supports pagination with limit and offset.
pub fn fetch_latest_posts(
    db: &web::Data<Database>,
    limit: u32,
    offset: u32,
) -> Result<Vec<PostSummary>, posts_db_operations::DbError> {
    posts_db_operations::read_latest_post_summaries(db, limit, offset)
}

// UPDATED: This function now supports pagination with limit and offset.
pub fn fetch_posts_by_tag(
    tag: &str,
    db: &web::Data<Database>,
    limit: u32,
    offset: u32,
) -> Result<Vec<PostSummary>, posts_db_operations::DbError> {
    posts_db_operations::read_post_summaries_by_tag(db, &tag.to_lowercase(), limit, offset) // NORMALIZE
}

// NEW FUNCTION: This function handles searching for posts by title with pagination.
pub fn search_posts_by_title(
    title_query: &str,
    db: &web::Data<Database>,
    limit: u32,
    offset: u32,
) -> Result<Vec<PostSummary>, posts_db_operations::DbError> {
    posts_db_operations::read_post_summaries_by_title(db, title_query, limit, offset)
}

pub fn fetch_all_available_tags(db: &web::Data<Database>) -> Result<Vec<String>, posts_db_operations::DbError> {
    posts_db_operations::get_all_available_tags(db)
}

pub fn search_posts_by_keyword(
    keyword_query: &str,
    db: &web::Data<Database>,
    limit: u32,
    offset: u32,
) -> Result<Vec<PostSummary>, posts_db_operations::DbError> {
    posts_db_operations::read_post_summaries_by_keyword(db, keyword_query, limit, offset)
}

// --- NEW HELPER FUNCTION ---
/// Fetches posts that match an intersection of multiple tags, with pagination.
/// This is a simple passthrough to keep the route handler clean.
pub fn fetch_posts_by_tags_intersection(
    db: &web::Data<Database>,
    tags: &[String],
    limit: u32,
    offset: u32,
) -> Result<Vec<PostSummary>, posts_db_operations::DbError> {
    posts_db_operations::read_post_summaries_by_tags_intersection(db, tags, limit, offset)
}