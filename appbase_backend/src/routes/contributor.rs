use crate::helper::{contributor_helpers, public_helpers};
use crate::middleware::AuthenticatedContributor;
use crate::models::db_operations::users_db_operations;
use crate::models::{MediaAttachment, PostSummary, Contributor, PostAction};
use crate::config::Config;
use crate::AppState;
use actix_session::Session;
use actix_web::{web, HttpResponse, Responder, Error};
use actix_multipart::Multipart;
use redb::Database;
//use rusqlite::Connection;
use tera::{Context, Tera};
//use url::form_urlencoded;
use serde::Serialize;
use serde_json::json;
use actix_csrf::extractor::{Csrf, CsrfGuarded, CsrfToken};
use serde::Deserialize;


// --- Structs for forms and query params ---
#[derive(Deserialize)]
struct LoginForm {
    csrf_token: CsrfToken,
    username: String,
    password: String,
}

impl CsrfGuarded for LoginForm {
    fn csrf_token(&self) -> &CsrfToken { &self.csrf_token }
}

#[derive(Deserialize)]
struct SimilarCheckPayload {
    title: String,
    tags: String,
    check_type: String,
}

#[derive(Deserialize)]
struct PaginationQuery {
    page: Option<u32>,
    limit: Option<u32>,
}

#[derive(Deserialize)]
struct FullPostUpdateRequest {
    title: String,
    summary: String,
    content: String,
    tags: String,
    search_keywords: String,
    cover_image: Option<String>,
    has_call_to_action: Option<bool>,
}

#[derive(Deserialize)]
struct PostSearchQuery {
    search_type: String,
    q: String,
    page: Option<u32>,
    limit: Option<u32>,
}

#[derive(Deserialize)]
pub struct SearchQuery {
    q: String,
    page: Option<u32>,
    limit: Option<u32>,
}

#[derive(Serialize)]
struct ApiResponse<T: Serialize> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct ApproveRequest {
    confirmation: String,
}


// --- Route Configuration ---
pub fn config_login(cfg: &mut web::ServiceConfig) {
    cfg.route("/login", web::get().to(show_contributor_login_form))
        .route("/login", web::post().to(handle_contributor_login))
        .route("/logout", web::post().to(handle_contributor_logout));
}

pub fn config_dashboard(cfg: &mut web::ServiceConfig) {
    cfg.route("/dashboard", web::get().to(show_dashboard))
        // NEW: Route to the approval page
        .route("/approve", web::get().to(show_approve_page))
        .route("/submit_post", web::post().to(submit_post_action)) // Renamed from create_post
        .route("/delete_post", web::post().to(delete_post_action))
        .route("/upload_media", web::post().to(upload_media_action))
        .route("/delete_media", web::post().to(delete_media_action))
        .service(
            web::scope("/api")
                .route("/mymedia", web::get().to(get_my_media_action))
                .route("/myposts", web::get().to(get_my_posts_action))
                .route("/media/search", web::get().to(search_media_action))
                .route("/tags", web::get().to(get_available_tags_action))
                .route("/posts/check_similar", web::post().to(check_similar_posts_action))
                .route("/posts/search", web::get().to(search_posts_action))
                .route("/posts/{post_id}", web::get().to(get_post_details_api)) // NEW: Get published post details
                .route("/posts/{post_id}/update", web::post().to(update_full_post_action))
                // --- NEW API Endpoints ---
                .route("/pending", web::get().to(get_pending_posts_api))
                .route("/pending/{post_id}", web::get().to(get_pending_post_details_api))
                .route("/pending/{post_id}/approve", web::post().to(approve_post_api))
                .route("/pending/{post_id}/delete", web::post().to(delete_pending_post_api))
                .route("/mypending", web::get().to(get_my_pending_posts_api))
                .route("/mypending/{post_id}", web::get().to(get_my_pending_post_details_api)) // NEW: Get own pending post details
                .route("/mypending/{post_id}/update", web::post().to(update_my_pending_post_api)) // NEW: Update own pending post
                .route("/mypending/{post_id}/delete", web::post().to(delete_my_pending_post_api))
        );
}


// --- Utility to get current user details ---
fn get_current_user(auth_user: &AuthenticatedContributor, pool: &web::Data<crate::DbPool>) -> Result<Contributor, HttpResponse> {
    contributor_helpers::get_contributor_details(pool, &auth_user.username)
        .ok_or_else(|| HttpResponse::InternalServerError().json(json!({"success": false, "error": "Authenticated user not found."})))
}


// --- Login/Logout Handlers (Unchanged) ---
async fn show_contributor_login_form( session: Session, tera: web::Data<Tera>, app_state: web::Data<AppState>, token: CsrfToken ) -> impl Responder {
    // --- MODIFIED BLOCK ---
    let contributor_path_prefix = app_state.contributor_prefix.read().unwrap_or_else(|poisoned| {
        log::error!("RwLock for contributor_prefix was poisoned on login page! Recovering lock.");
        poisoned.into_inner()
    });
    // --- END MODIFICATION ---

    if session.get::<String>("username").unwrap().is_some() {
        let dashboard_url = format!("/management/{}/dashboard", *contributor_path_prefix);
        return HttpResponse::Found().append_header(("location", dashboard_url)).finish();
    }
    let mut ctx = Context::new();
    ctx.insert("contributor_path_prefix", &*contributor_path_prefix);
    ctx.insert("csrf_token", token.get());
    if let Some(error) = session.get::<String>("error").unwrap() {
        ctx.insert("error", &error);
        session.remove("error");
    }
    match tera.render("contributor/login.html", &ctx) {
        Ok(rendered) => HttpResponse::Ok().content_type("text/html; charset=utf-8").body(rendered),
        Err(_) => HttpResponse::InternalServerError().body("Template error"),
    }
}

async fn handle_contributor_login( session: Session, pool: web::Data<crate::DbPool>, form: Csrf<web::Form<LoginForm>>, app_state: web::Data<AppState> ) -> impl Responder {
    // --- MODIFIED BLOCK ---
    let contributor_path_prefix = app_state.contributor_prefix.read().unwrap_or_else(|poisoned| {
        log::error!("RwLock for contributor_prefix was poisoned during login! Recovering lock.");
        poisoned.into_inner()
    });
    // --- END MODIFICATION ---

    let login_url = format!("/management/{}/login", *contributor_path_prefix);
    let dashboard_url = format!("/management/{}/dashboard", *contributor_path_prefix);
    let login_data = form.into_inner();
    if let Some((user, role)) = public_helpers::verify_contributor_credentials(&pool, &login_data.username, &login_data.password) {
        if role == "admin" {
            session.insert("error", "Administrators must use the admin login page.").unwrap();
            return HttpResponse::Found().append_header(("location", login_url)).finish();
        }
        session.insert("username", user.clone()).unwrap();
        session.insert("role", role).unwrap();
        session.remove("error");
        
        if let Ok(conn) = pool.get() {
            users_db_operations::update_last_login_time(&conn, &user).ok();
        }

        HttpResponse::Found().append_header(("location", dashboard_url)).finish()
    } else {
        session.insert("error", "Invalid credentials or account suspended.").unwrap();
        HttpResponse::Found().append_header(("location", login_url)).finish()
    }
}

async fn handle_contributor_logout( session: Session, app_state: web::Data<AppState> ) -> impl Responder {
    // --- MODIFIED BLOCK ---
    let contributor_path_prefix = app_state.contributor_prefix.read().unwrap_or_else(|poisoned| {
        log::error!("RwLock for contributor_prefix was poisoned during logout! Recovering lock.");
        poisoned.into_inner()
    });
    // --- END MODIFICATION ---

    let login_url = format!("/management/{}/login", *contributor_path_prefix);
    session.clear();
    HttpResponse::Found().append_header(("location", login_url)).finish()
}


// --- Page Rendering Handlers ---
async fn show_dashboard( auth_user: AuthenticatedContributor, tera: web::Data<Tera>, pool: web::Data<crate::DbPool>, config: web::Data<Config>, app_state: web::Data<AppState>, token: CsrfToken ) -> impl Responder {
    let mut ctx = Context::new();
    let user_details = match contributor_helpers::get_contributor_details(&pool, &auth_user.username) {
        Some(user) => user,
        None => return HttpResponse::InternalServerError().body("Authenticated user not found."),
    };
    let base_url = format!("http://{}:{}", config.web.host, config.web.port);
    ctx.insert("base_url", &base_url);
    ctx.insert("user", &user_details);

    // --- MODIFIED BLOCK ---
    let contributor_path_prefix = app_state.contributor_prefix.read().unwrap_or_else(|poisoned| {
        log::error!("RwLock for contributor_prefix was poisoned on dashboard! Recovering lock.");
        poisoned.into_inner()
    });
    // --- END MODIFICATION ---

    ctx.insert("contributor_path_prefix", &*contributor_path_prefix);
    ctx.insert("csrf_token", token.get());
    match tera.render("contributor/dashboard.html", &ctx) {
        Ok(rendered) => HttpResponse::Ok().content_type("text/html; charset=utf-8").body(rendered),
        Err(err) => {
            log::error!("Template rendering error: {}", err);
            HttpResponse::InternalServerError().body("Error rendering dashboard.")
        }
    }
}

// NEW: Renders the approval page (template to be created later)
async fn show_approve_page( auth_user: AuthenticatedContributor, tera: web::Data<Tera>, pool: web::Data<crate::DbPool>, app_state: web::Data<AppState>, token: CsrfToken ) -> impl Responder {
    let user_details = match get_current_user(&auth_user, &pool) {
        Ok(user) => user,
        Err(resp) => return resp,
    };
    if !user_details.can_approve_posts {
        return HttpResponse::Forbidden().body("You do not have permission to access this page.");
    }
    let mut ctx = Context::new();
    ctx.insert("user", &user_details);

    // --- MODIFIED BLOCK ---
    let contributor_path_prefix = app_state.contributor_prefix.read().unwrap_or_else(|poisoned| {
        log::error!("RwLock for contributor_prefix was poisoned on approve page! Recovering lock.");
        poisoned.into_inner()
    });
    // --- END MODIFICATION ---

    ctx.insert("contributor_path_prefix", &*contributor_path_prefix);
    ctx.insert("csrf_token", token.get());
    match tera.render("contributor/approve.html", &ctx) {
        Ok(rendered) => HttpResponse::Ok().content_type("text/html; charset=utf-8").body(rendered),
        Err(err) => {
            log::error!("Template rendering error for approve page: {}", err);
            HttpResponse::InternalServerError().body("Error rendering approval page.")
        }
    }
}


// --- Core Action Handlers ---
async fn submit_post_action( auth_user: AuthenticatedContributor, db: web::Data<Database>, pool: web::Data<crate::DbPool>, form: web::Bytes ) -> Result<HttpResponse, Error> {
    let contributor = match get_current_user(&auth_user, &pool) {
        Ok(c) => c,
        Err(resp) => return Ok(resp),
    };
    
    let parsed = match crate::helper::form_helpers::parse_form(&form) {
        Ok(p) => p,
        Err(response) => return Ok(response), // Return the 400 Bad Request
    };

    let title = parsed.get("title").map_or("", |s| s.trim());
    let summary = parsed.get("summary").map_or("", |s| s.trim());
    let content = parsed.get("content").map_or("", |s| s.trim());
    let tags = parsed.get("tags").map_or("", |s| s.trim());
    let search_keywords = parsed.get("search_keywords").map_or("", |s| s.trim());
    let cover_image = parsed.get("cover_image").map(|s| s.trim()).filter(|s| !s.is_empty());
    let has_call_to_action = match parsed.get("has_call_to_action").map(|s| s.as_str()) {
        Some("true") => Some(true), Some("false") => Some(false), _ => None,
    };
    if title.is_empty() || summary.is_empty() || content.is_empty() {
        return Ok(HttpResponse::BadRequest().json(json!({ "success": false, "error": "Title, Summary, and Content are required." })));
    }
    match contributor_helpers::submit_post_for_approval(&db, &pool, &contributor, title, summary, content, tags, search_keywords, cover_image, has_call_to_action) {
        Ok(post_id) => Ok(HttpResponse::Ok().json(json!({
            "success": true,
            "message": format!("Successfully submitted for approval. Your Post ID is: {}", post_id),
            "post_id": post_id
        }))),
        Err(e) => Ok(HttpResponse::InternalServerError().json(json!({ "success": false, "error": format!("Failed to submit post: {}", e) }))),
    }
}
async fn upload_media_action( auth_user: AuthenticatedContributor, pool: web::Data<crate::DbPool>, config: web::Data<Config>, payload: Multipart ) -> Result<HttpResponse, Error> {
    let contributor = match get_current_user(&auth_user, &pool) {
        Ok(c) => c,
        Err(resp) => return Ok(resp),
    };
    match contributor_helpers::save_media_attachment(config, pool.clone(), contributor.id, payload).await {
        Ok((display_path, file_id)) => Ok(HttpResponse::Ok().json(json!({ "success": true, "url": display_path, "id": file_id }))),
        Err(e) => Ok(HttpResponse::BadRequest().json(json!({ "success": false, "error": e.to_string() }))),
    }
}

async fn delete_post_action(
    auth_user: AuthenticatedContributor,
    db: web::Data<Database>,
    pool: web::Data<crate::DbPool>,
    form: web::Bytes,
) -> impl Responder {
    let parsed = match crate::helper::form_helpers::parse_form(&form) {
        Ok(p) => p,
        Err(response) => return response, // Return the 400 Bad Request
    };
    let post_id = parsed.get("post_id").cloned().unwrap_or_default();

    let contributor = match get_current_user(&auth_user, &pool) {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    if !contributor_helpers::can_contributor_perform_action(&pool, &contributor, &post_id, PostAction::Delete) {
        return HttpResponse::Forbidden().json(json!({ "success": false, "error": "You do not have permission to delete this post." }));
    }

    match contributor_helpers::delete_post(&db, &pool, &post_id) {
        Ok(_) => HttpResponse::Ok().json(json!({ "success": true, "message": "Post deleted successfully." })),
        Err(e) => {
            log::error!("Failed to delete post {}: {}", post_id, e);
            HttpResponse::InternalServerError().json(json!({ "success": false, "error": format!("Failed to delete post: {}", e) }))
        }
    }
}


async fn delete_media_action(
    auth_user: AuthenticatedContributor,
    pool: web::Data<crate::DbPool>,
    config: web::Data<Config>,
    form: web::Bytes,
) -> impl Responder {
    let contributor = match get_current_user(&auth_user, &pool) {
        Ok(c) => c,
        Err(resp) => return resp,
    };
    
    let parsed = match crate::helper::form_helpers::parse_form(&form) {
        Ok(p) => p,
        Err(response) => return response, // Return the 400 Bad Request
    };
    let media_id = parsed.get("media_id").cloned().unwrap_or_default();
    if media_id.is_empty() {
        return HttpResponse::BadRequest().json(json!({"success": false, "error": "Invalid media ID for deletion."}));
    }

    match contributor_helpers::delete_media(&config, &pool, &contributor, &media_id).await {
        Ok(_) => HttpResponse::Ok().json(json!({ "success": true, "message": "Media deleted successfully." })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "success": false, "error": format!("Failed to delete media: {}", e) })),
    }
}

// --- API Handlers ---
async fn get_my_media_action( auth_user: AuthenticatedContributor, pool: web::Data<crate::DbPool>, config: web::Data<Config> ) -> impl Responder {
    let user = match get_current_user(&auth_user, &pool) { Ok(u) => u, Err(resp) => return resp };
    match contributor_helpers::get_user_media(&config, &pool, user.id) {
        Ok(media_files) => HttpResponse::Ok().json(ApiResponse { success: true, data: Some(media_files), error: None }),
        Err(e) => HttpResponse::InternalServerError().json(ApiResponse { success: false, data: None::<Vec<MediaAttachment>>, error: Some(e.to_string()) }),
    }
}

async fn get_my_posts_action( auth_user: AuthenticatedContributor, db: web::Data<Database>, pool: web::Data<crate::DbPool>, query: web::Query<PaginationQuery> ) -> impl Responder {
    let user = match get_current_user(&auth_user, &pool) { Ok(u) => u, Err(resp) => return resp };
    let page = query.page.unwrap_or(1).max(1); // <-- FIX APPLIED
    let limit = query.limit.unwrap_or(10);
    let offset = (page - 1) * limit;
    match contributor_helpers::fetch_posts_for_user(&db, &pool, user.id, limit, offset) {
        Ok(posts) => HttpResponse::Ok().json(ApiResponse { success: true, data: Some(posts), error: None }),
        Err(e) => {
            log::error!("Failed to fetch posts for user {}: {}", user.id, e);
            HttpResponse::InternalServerError().json(ApiResponse { success: false, data: None::<Vec<PostSummary>>, error: Some("Failed to retrieve posts.".to_string()) })
        }
    }
}

async fn search_media_action( config: web::Data<Config>, pool: web::Data<crate::DbPool>, query: web::Query<SearchQuery> ) -> impl Responder {
    let search_term = query.q.trim();
    let page = query.page.unwrap_or(1).max(1); // <-- FIX APPLIED
    let limit = query.limit.unwrap_or(15);
    let offset = (page - 1) * limit;
    if !search_term.is_empty() {
        let results = contributor_helpers::search_all_media_by_tag(&config, &pool, search_term, limit, offset);
        HttpResponse::Ok().json(ApiResponse { success: true, data: Some(results), error: None })
    } else {
        HttpResponse::BadRequest().json(ApiResponse { success: false, data: None::<Vec<MediaAttachment>>, error: Some("Search query cannot be empty.".to_string()) })
    }
}

async fn check_similar_posts_action( db: web::Data<Database>, payload: web::Json<SimilarCheckPayload> ) -> impl Responder {
    match contributor_helpers::check_similar_posts( &db, &payload.title, &payload.tags, &payload.check_type, None ) {
        Ok(posts) => HttpResponse::Ok().json(ApiResponse { success: true, data: Some(posts), error: None }),
        Err(e) => {
            log::error!("Failed to check for similar posts: {}", e);
            HttpResponse::InternalServerError().json(ApiResponse { success: false, data: None::<Vec<PostSummary>>, error: Some("Failed to perform check.".to_string()) })
        }
    }
}

async fn update_full_post_action(
    auth_user: AuthenticatedContributor,
    path_params: web::Path<(String, String)>,
    db: web::Data<Database>,
    pool: web::Data<crate::DbPool>,
    payload: web::Json<FullPostUpdateRequest>,
) -> impl Responder {
    let post_id = path_params.into_inner().1;
    let contributor = match get_current_user(&auth_user, &pool) { Ok(c) => c, Err(resp) => return resp };

    if !contributor_helpers::can_contributor_perform_action(&pool, &contributor, &post_id, PostAction::Edit) {
        return HttpResponse::Forbidden().json(json!({ "success": false, "error": "You do not have permission to edit this post." }));
    }

    match contributor_helpers::re_submit_for_approval( &db, &pool, &contributor, &post_id, &payload.title, &payload.summary, &payload.content, &payload.tags, &payload.search_keywords, payload.cover_image.as_deref(), payload.has_call_to_action, ) {
        Ok(_) => HttpResponse::Ok().json(json!({ "success": true, "message": "Post has been re-submitted for approval." })),
        Err(e) => {
            log::error!("Failed to perform full update for post {}: {}", post_id, e);
            HttpResponse::InternalServerError().json(json!({ "success": false, "error": format!("Database error during update: {}", e) }))
        }
    }
}

async fn get_available_tags_action( db: web::Data<Database> ) -> impl Responder {
    match contributor_helpers::get_all_available_tags(&db) {
        Ok(tags) => HttpResponse::Ok().json(ApiResponse { success: true, data: Some(tags), error: None }),
        Err(e) => {
            log::error!("Failed to fetch available tags: {}", e);
            HttpResponse::InternalServerError().json(ApiResponse { success: false, data: None::<Vec<String>>, error: Some("Failed to retrieve tags.".to_string()) })
        }
    }
}

async fn search_posts_action( db: web::Data<Database>, query: web::Query<PostSearchQuery> ) -> impl Responder {
    let search_term = query.q.trim();
    let search_type = query.search_type.as_str();
    let page = query.page.unwrap_or(1).max(1); // <-- FIX APPLIED
    let limit = query.limit.unwrap_or(10);
    let offset = (page - 1) * limit;
    if search_term.is_empty() {
        return HttpResponse::BadRequest().json(ApiResponse { success: false, data: None::<Vec<PostSummary>>, error: Some("Search query cannot be empty.".to_string()) });
    }
    match contributor_helpers::search_posts(&db, search_type, search_term, limit, offset) {
        Ok(posts) => HttpResponse::Ok().json(ApiResponse { success: true, data: Some(posts), error: None }),
        Err(e) => {
            log::error!("Failed to search posts: {}", e);
            HttpResponse::InternalServerError().json(ApiResponse { success: false, data: None::<Vec<PostSummary>>, error: Some("Failed to perform search.".to_string()) })
        }
    }
}

// --- NEW API HANDLERS for Approval Workflow ---

async fn get_pending_posts_api( auth_user: AuthenticatedContributor, db: web::Data<Database>, pool: web::Data<crate::DbPool>, query: web::Query<PaginationQuery> ) -> impl Responder {
    let user = match get_current_user(&auth_user, &pool) { Ok(u) => u, Err(resp) => return resp };
    if !user.can_approve_posts {
        return HttpResponse::Forbidden().json(ApiResponse { success: false, data: None::<()>, error: Some("Permission denied.".to_string()) });
    }
    let page = query.page.unwrap_or(1).max(1); // <-- FIX APPLIED
    let limit = query.limit.unwrap_or(10);
    let offset = (page - 1) * limit;

    match contributor_helpers::fetch_pending_posts_with_owners(&db, &pool, limit, offset).await {
        Ok(posts) => HttpResponse::Ok().json(ApiResponse { success: true, data: Some(posts), error: None }),
        Err(e) => {
            log::error!("Failed to fetch pending posts for approval: {}", e);
            HttpResponse::InternalServerError().json(ApiResponse { success: false, data: None::<()>, error: Some("Failed to retrieve pending posts.".to_string()) })
        }
    }
}

async fn get_pending_post_details_api( auth_user: AuthenticatedContributor, pool: web::Data<crate::DbPool>, db: web::Data<Database>, path: web::Path<(String, String)>) -> impl Responder {
    let user = match get_current_user(&auth_user, &pool) { Ok(u) => u, Err(resp) => return resp };
    if !user.can_approve_posts {
        return HttpResponse::Forbidden().json(ApiResponse { success: false, data: None::<()>, error: Some("Permission denied.".to_string()) });
    }
    let post_id = path.into_inner().1;
    match contributor_helpers::get_pending_post_details(&db, &post_id) {
        Some(post) => HttpResponse::Ok().json(ApiResponse { success: true, data: Some(post), error: None }),
        None => HttpResponse::NotFound().json(ApiResponse { success: false, data: None::<()>, error: Some("Pending post not found.".to_string()) }),
    }
}

async fn approve_post_api( auth_user: AuthenticatedContributor, db: web::Data<Database>, pool: web::Data<crate::DbPool>, path: web::Path<(String, String)>, payload: web::Json<ApproveRequest> ) -> impl Responder {
    let user = match get_current_user(&auth_user, &pool) { Ok(u) => u, Err(resp) => return resp };
    if !user.can_approve_posts {
        return HttpResponse::Forbidden().json(json!({"success": false, "error": "Permission denied."}));
    }
    if payload.confirmation.to_lowercase() != "yes" {
        return HttpResponse::BadRequest().json(json!({"success": false, "error": "Confirmation text does not match."}));
    }
    let post_id = path.into_inner().1;
    match contributor_helpers::approve_post(&db, &pool, &post_id) {
        Ok(_) => HttpResponse::Ok().json(json!({"success": true, "message": "Post approved and published successfully."})),
        Err(e) => {
            log::error!("Failed to approve post {}: {}", post_id, e);
            HttpResponse::InternalServerError().json(json!({"success": false, "error": format!("Failed to approve post: {}", e)}))
        }
    }
}

async fn delete_pending_post_api(
    auth_user: AuthenticatedContributor,
    db: web::Data<Database>,
    conn: web::Data<crate::DbPool>,
    path: web::Path<(String, String)>,
) -> impl Responder {
    let user = match get_current_user(&auth_user, &conn) { Ok(u) => u, Err(resp) => return resp };
    let post_id = path.into_inner().1;

    // UPDATED: Use the PostAction enum variant
    if !contributor_helpers::can_contributor_perform_pending_action(&conn, &user, &post_id, PostAction::Delete) {
        return HttpResponse::Forbidden().json(json!({ "success": false, "error": "You do not have permission to delete this pending post." }));
    }

    match contributor_helpers::delete_pending_post(&db, &conn, &post_id) {
        Ok(_) => HttpResponse::Ok().json(json!({"success": true, "message": "Pending post deleted successfully."})),
        Err(e) => {
            log::error!("Failed to delete pending post {}: {}", post_id, e);
            HttpResponse::InternalServerError().json(json!({"success": false, "error": format!("Failed to delete pending post: {}", e)}))
        }
    }
}

async fn get_my_pending_posts_api( auth_user: AuthenticatedContributor, db: web::Data<Database>, pool: web::Data<crate::DbPool>, query: web::Query<PaginationQuery> ) -> impl Responder {
    let user = match get_current_user(&auth_user, &pool) { Ok(u) => u, Err(resp) => return resp };
    let page = query.page.unwrap_or(1).max(1); // <-- FIX APPLIED
    let limit = query.limit.unwrap_or(10);
    let offset = (page - 1) * limit;
    match contributor_helpers::fetch_own_pending_posts(&db, &pool, user.id, limit, offset) {
        Ok(posts) => HttpResponse::Ok().json(ApiResponse { success: true, data: Some(posts), error: None }),
        Err(e) => {
            log::error!("Failed to fetch own pending posts for user {}: {}", user.id, e);
            HttpResponse::InternalServerError().json(ApiResponse { success: false, data: None::<()>, error: Some("Failed to retrieve your pending posts.".to_string()) })
        }
    }
}

async fn delete_my_pending_post_api(
    auth_user: AuthenticatedContributor,
    db: web::Data<Database>,
    conn: web::Data<crate::DbPool>,
    path: web::Path<(String, String)>,
) -> impl Responder {
    let user = match get_current_user(&auth_user, &conn) { Ok(u) => u, Err(resp) => return resp };
    let post_id = path.into_inner().1;

    // UPDATED: Use the PostAction enum variant
    if !contributor_helpers::can_contributor_perform_pending_action(&conn, &user, &post_id, PostAction::Delete) {
        return HttpResponse::Forbidden().json(json!({ "success": false, "error": "You can only delete your own pending posts." }));
    }

    match contributor_helpers::delete_pending_post(&db, &conn, &post_id) {
        Ok(_) => HttpResponse::Ok().json(json!({"success": true, "message": "Your pending submission has been deleted."})),
        Err(e) => {
            log::error!("Failed to delete own pending post {}: {}", post_id, e);
            HttpResponse::InternalServerError().json(json!({"success": false, "error": format!("Failed to delete submission: {}", e)}))
        }
    }
}

// --- NEW APIs FOR EDITING ---

/// NEW: API handler for a contributor to get the full details of their OWN PENDING post.
async fn get_my_pending_post_details_api(auth_user: AuthenticatedContributor, pool: web::Data<crate::DbPool>, db: web::Data<Database>, path: web::Path<(String, String)>) -> impl Responder {
    let user = match get_current_user(&auth_user, &pool) { Ok(u) => u, Err(resp) => return resp };
    let post_id = path.into_inner().1;
    match contributor_helpers::get_own_pending_post_details(&db, &pool, &user, &post_id) {
        Some(post) => HttpResponse::Ok().json(ApiResponse { success: true, data: Some(post), error: None }),
        None => HttpResponse::Forbidden().json(ApiResponse { success: false, data: None::<()>, error: Some("Post not found or permission denied.".to_string()) }),
    }
}

/// NEW: API handler for a contributor to get the full details of their OWN PUBLISHED post.
async fn get_post_details_api(auth_user: AuthenticatedContributor, pool: web::Data<crate::DbPool>, db: web::Data<Database>, path: web::Path<(String, String)>) -> impl Responder {
    let user = match get_current_user(&auth_user, &pool) { Ok(u) => u, Err(resp) => return resp };
    let post_id = path.into_inner().1;
    match contributor_helpers::get_own_post_details(&db, &pool, &user, &post_id) {
        Some(post) => HttpResponse::Ok().json(ApiResponse { success: true, data: Some(post), error: None }),
        None => HttpResponse::Forbidden().json(ApiResponse { success: false, data: None::<()>, error: Some("Post not found or permission denied.".to_string()) }),
    }
}


async fn update_my_pending_post_api(
    auth_user: AuthenticatedContributor,
    path_params: web::Path<(String, String)>,
    db: web::Data<Database>,
    conn: web::Data<crate::DbPool>,
    payload: web::Json<FullPostUpdateRequest>,
) -> impl Responder {
    let post_id = path_params.into_inner().1;
    let contributor = match get_current_user(&auth_user, &conn) { Ok(c) => c, Err(resp) => return resp };
    
    // UPDATED: Use the PostAction enum variant
    if !contributor_helpers::can_contributor_perform_pending_action(&conn, &contributor, &post_id, PostAction::Edit) {
        return HttpResponse::Forbidden().json(json!({ "success": false, "error": "You do not have permission to edit this pending post." }));
    }

    match contributor_helpers::update_pending_post(&db, &post_id, &payload.title, &payload.summary, &payload.content, &payload.tags, &payload.search_keywords, payload.cover_image.as_deref(), payload.has_call_to_action) {
        Ok(_) => HttpResponse::Ok().json(json!({ "success": true, "message": "Pending post updated successfully." })),
        Err(e) => {
            log::error!("Failed to perform full update for pending post {}: {}", post_id, e);
            HttpResponse::InternalServerError().json(json!({ "success": false, "error": format!("Database error during update: {}", e) }))
        }
    }
}