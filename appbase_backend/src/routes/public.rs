
use crate::helper::public_helpers;
use actix_web::{web, HttpResponse, Responder};
use redb::Database;
use serde::{Deserialize, Deserializer};


fn deserialize_tags<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrVec {
        String(String),
        Vec(Vec<String>),
    }

    match StringOrVec::deserialize(deserializer)? {
        StringOrVec::String(s) => {
            // This logic handles comma-separated tags like "rust,actix"
            Ok(s.split(',')
                .map(|tag| tag.trim().to_string())
                .filter(|tag| !tag.is_empty())
                .collect())
        }
        StringOrVec::Vec(v) => Ok(v),
    }
}

#[derive(Deserialize)]
pub struct ApiQuery {
    limit: Option<u32>,
    offset: Option<u32>,
    q: Option<String>,
}

#[derive(Deserialize)]
pub struct TagFilterQuery {
    #[serde(deserialize_with = "deserialize_tags")]
    tags: Vec<String>,
    // **PAGINATION PARAMETERS ARE CORRECTLY INCLUDED HERE**
    limit: Option<u32>,
    offset: Option<u32>,
}

pub fn config_api(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api")
            .route("/is_server_active", web::get().to(is_server_active))
            .route("/posts/latest", web::get().to(get_latest_posts))
            .route("/posts/search", web::get().to(search_posts_by_keyword))
            .route("/posts/tag/{tag}", web::get().to(get_posts_by_tag))
            .route("/posts/filter", web::get().to(filter_posts_by_tags))
            .route("/posts/{id}", web::get().to(get_post_by_id))
            .route("/tags/available", web::get().to(get_available_tags)),
    );
}

async fn is_server_active() -> impl Responder {
    HttpResponse::Ok().body("active")
}

async fn get_post_by_id(id: web::Path<String>, db: web::Data<Database>) -> impl Responder {
    match public_helpers::fetch_post_by_id(&id, &db) {
        Some(post) => HttpResponse::Ok().json(post),
        None => HttpResponse::NotFound().body("Post not found"),
    }
}

async fn get_latest_posts(db: web::Data<Database>, query: web::Query<ApiQuery>) -> impl Responder {
    let limit = query.limit.unwrap_or(10);
    let offset = query.offset.unwrap_or(0);

    match public_helpers::fetch_latest_posts(&db, limit, offset) {
        Ok(posts) => HttpResponse::Ok().json(posts),
        Err(e) => {
            log::error!("Failed to fetch latest posts: {}", e);
            HttpResponse::InternalServerError().finish()
        }
    }
}

async fn get_posts_by_tag(
    tag: web::Path<String>,
    db: web::Data<Database>,
    query: web::Query<ApiQuery>,
) -> impl Responder {
    let limit = query.limit.unwrap_or(20);
    let offset = query.offset.unwrap_or(0);
    let tag_value = tag.into_inner();

    match public_helpers::fetch_posts_by_tag(&tag_value, &db, limit, offset) {
        Ok(posts) => HttpResponse::Ok().json(posts),
        Err(e) => {
            log::error!("Failed to fetch posts by tag '{}': {}", tag_value, e);
            HttpResponse::InternalServerError().finish()
        }
    }
}

async fn search_posts_by_keyword(
    db: web::Data<Database>,
    query: web::Query<ApiQuery>,
) -> impl Responder {
    let keyword_query = match query.q.as_deref() {
        Some(q) if !q.trim().is_empty() => q.trim(),
        _ => return HttpResponse::BadRequest().json("A non-empty 'q' query parameter is required for search."),
    };

    let limit = query.limit.unwrap_or(10);
    let offset = query.offset.unwrap_or(0);

    match public_helpers::search_posts_by_keyword(keyword_query, &db, limit, offset) {
        Ok(posts) => HttpResponse::Ok().json(posts),
        Err(e) => {
            log::error!("Failed to search posts by keyword '{}': {}", keyword_query, e);
            HttpResponse::InternalServerError().finish()
        }
    }
}

async fn get_available_tags(db: web::Data<Database>) -> impl Responder {
    match public_helpers::fetch_all_available_tags(&db) {
        Ok(mut tags) => {
            tags.sort_unstable();
            HttpResponse::Ok().json(tags)
        },
        Err(e) => {
            log::error!("Failed to fetch available tags: {}", e);
            HttpResponse::InternalServerError().finish()
        }
    }
}

/// Handles requests to the GET /api/posts/filter endpoint.
async fn filter_posts_by_tags(
    db: web::Data<Database>,
    query: web::Query<TagFilterQuery>,
) -> impl Responder {
    if query.tags.is_empty() {
        return HttpResponse::BadRequest()
            .body("Error: At least one 'tag' query parameter must be provided.");
    }

    // --- PAGINATION IS HANDLED HERE ---
    // If 'limit' or 'offset' are not in the URL, use the specified defaults.
    let limit = query.limit.unwrap_or(20);
    let offset = query.offset.unwrap_or(0);

    // Call the helper function with the validated and prepared parameters.
    match public_helpers::fetch_posts_by_tags_intersection(&db, &query.tags, limit, offset) {
        Ok(posts) => HttpResponse::Ok().json(posts),
        Err(e) => {
            log::error!(
                "Failed to fetch posts by tags intersection '{:?}': {}",
                &query.tags,
                e
            );
            HttpResponse::InternalServerError().finish()
        }
    }
}