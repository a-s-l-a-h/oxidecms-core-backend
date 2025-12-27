
use crate::helper::{admin_helpers, public_helpers};
use crate::middleware::AuthenticatedContributor;
use crate::models::Notification;
use crate::config::Config;
use crate::AppState;
use crate::routes::advanced_db_manager;
use actix_session::Session;
use actix_web::{web, HttpResponse, Responder};
use redb::Database;
//use rusqlite::Connection;
use tera::{Context, Tera};
//use url::form_urlencoded;
use actix_csrf::extractor::{Csrf, CsrfGuarded, CsrfToken};
use serde::Deserialize;
use crate::models::db_operations::users_db_operations;

#[derive(Deserialize)]
struct LoginForm {
    csrf_token: CsrfToken,
    username: String,
    password: String,
}

impl CsrfGuarded for LoginForm {
    fn csrf_token(&self) -> &CsrfToken {
        &self.csrf_token
    }
}


// ... (keep config_login, config_dashboard, and set_notification functions)
pub fn config_login(cfg: &mut web::ServiceConfig) {
    cfg.route("/login", web::get().to(show_admin_login_form))
        .route("/login", web::post().to(handle_admin_login))
        .route("/logout", web::post().to(handle_admin_logout));
}

pub fn config_dashboard(cfg: &mut web::ServiceConfig) {
    cfg.route("/dashboard", web::get().to(show_admin_dashboard))
        .route("/create_user", web::post().to(create_user_action))
        .route("/update_user", web::post().to(update_user_action))
        .route("/delete_user", web::post().to(delete_user_action))
        .route("/update_settings", web::post().to(update_settings_action))
        .route("/add_tag", web::post().to(add_tag_action))
        .route("/delete_tag", web::post().to(delete_tag_action))
        .configure(advanced_db_manager::config_advanced_db_manager);
}

fn set_notification(session: &Session, message: &str, r#type: &str) {
    session.insert("notification", &Notification { message: message.to_string(), r#type: r#type.to_string() }).unwrap();
}


async fn update_settings_action(
    session: Session,
    pool: web::Data<crate::DbPool>,
    form: web::Bytes,
    app_state: web::Data<AppState>,
    config: web::Data<Config>,
) -> impl Responder {
    let dashboard_url = format!("/management/{}/dashboard", &config.admin_url_prefix);

    let parsed = match crate::helper::form_helpers::parse_form(&form) {
        Ok(p) => p,
        Err(response) => return response, // Return the 400 Bad Request
    };

    let prefix = parsed.get("contributor_path_prefix").map(|s| s.trim()).unwrap_or("");
    let max_size = parsed.get("max_file_upload_size_mb").map(|s| s.trim()).unwrap_or("10");
    let mime_types = parsed.get("allowed_mime_types").map(|s| s.trim()).unwrap_or("");

    let is_prefix_valid = !prefix.is_empty() && prefix.chars().all(|c| c.is_alphanumeric() || c == '-');
    let is_max_size_valid = max_size.parse::<u64>().is_ok();

    if is_prefix_valid && is_max_size_valid {
        let update_prefix_res = admin_helpers::update_setting(&pool, "contributor_path_prefix", prefix);
        let update_size_res = admin_helpers::update_setting(&pool, "max_file_upload_size_mb", max_size);
        let update_mimes_res = admin_helpers::update_setting(&pool, "allowed_mime_types", mime_types);
        
        match (update_prefix_res, update_size_res, update_mimes_res) {
            (Ok(_), Ok(_), Ok(_)) => {
                // --- MODIFIED BLOCK: Safely handle potential RwLock poisoning ---
                let mut state_prefix = app_state.contributor_prefix.write().unwrap_or_else(|poisoned| {
                    log::error!("RwLock for contributor_prefix was poisoned during settings update! Recovering lock.");
                    poisoned.into_inner()
                });
                // --- END MODIFICATION ---
                *state_prefix = prefix.to_string();
                set_notification(&session, "Settings updated successfully.", "success");
            },
            _ => {
                log::error!("Failed to update one or more settings.");
                set_notification(&session, "Failed to update settings in database.", "error");
            }
        }
    } else {
        if !is_prefix_valid {
            set_notification(&session, "Invalid prefix. Use only letters, numbers, and hyphens.", "error");
        } else {
            set_notification(&session, "Invalid max file size. It must be a whole number.", "error");
        }
    }
    HttpResponse::Found().append_header(("location", dashboard_url)).finish()
}

async fn show_admin_login_form(
    session: Session,
    tera: web::Data<Tera>,
    token: CsrfToken,
    config: web::Data<Config>,
) -> impl Responder {
    let admin_url_prefix = &config.admin_url_prefix;
    if session.get::<String>("role").unwrap_or(None) == Some("admin".to_string()) {
        let dashboard_url = format!("/management/{}/dashboard", admin_url_prefix);
        return HttpResponse::Found().append_header(("location", dashboard_url)).finish();
    }

    let mut ctx = Context::new();
    ctx.insert("admin_url_prefix", admin_url_prefix);
    ctx.insert("csrf_token", token.get());

    if let Some(error) = session.get::<String>("error").unwrap() {
        ctx.insert("error", &error);
        session.remove("error");
    }

    match tera.render("admin/login.html", &ctx) {
        Ok(rendered) => HttpResponse::Ok().content_type("text/html; charset=utf-8").body(rendered),
        Err(_) => HttpResponse::InternalServerError().body("Template error"),
    }
}

async fn handle_admin_login(
    session: Session,
    pool: web::Data<crate::DbPool>, // UPDATED: Changed conn to pool
    form: Csrf<web::Form<LoginForm>>,
    config: web::Data<Config>,
) -> impl Responder {
    let admin_url_prefix = &config.admin_url_prefix;
    let login_url = format!("/management/{}/login", admin_url_prefix);
    let dashboard_url = format!("/management/{}/dashboard", admin_url_prefix);

    let login_data = form.into_inner();

    // UPDATED: Pass the pool to the helper function
    if let Some((_user, role)) = public_helpers::verify_contributor_credentials(&pool, &login_data.username, &login_data.password) {
        if role == "admin" {
            session.insert("username", login_data.username.clone()).unwrap();
            session.insert("role", role).unwrap();
            session.remove("error");
            HttpResponse::Found().append_header(("location", dashboard_url)).finish()
        } else {
            session.insert("error", "Access denied. Only administrators may log in here.").unwrap();
            HttpResponse::Found().append_header(("location", login_url)).finish()
        }
    } else {
        session.insert("error", "Invalid credentials or account suspended.").unwrap();
        HttpResponse::Found().append_header(("location", login_url)).finish()
    }
}

async fn handle_admin_logout(
    session: Session,
    config: web::Data<Config>,
) -> impl Responder {
    let admin_url_prefix = &config.admin_url_prefix;
    let login_url = format!("/management/{}/login", admin_url_prefix);
    session.clear();
    HttpResponse::Found().append_header(("location", login_url)).finish()
}

async fn show_admin_dashboard(
    auth_user: AuthenticatedContributor,
    session: Session,
    tera: web::Data<Tera>,
    pool: web::Data<crate::DbPool>, // UPDATED: Changed conn to pool
    db: web::Data<Database>,
    token: CsrfToken,
    config: web::Data<Config>,
) -> impl Responder {
    let mut ctx = Context::new();
    let admin_url_prefix = &config.admin_url_prefix;
    ctx.insert("admin_url_prefix", admin_url_prefix);
    ctx.insert("user", &auth_user);
    ctx.insert("csrf_token", token.get());

    if let Ok(Some(notification)) = session.get::<Notification>("notification") {
        ctx.insert("notification", &notification);
        session.remove("notification");
    }

    // UPDATED: Get a connection from the pool to pass to get_settings
    let settings = match pool.get() {
        Ok(conn) => admin_helpers::get_settings(&conn),
        Err(e) => {
            log::error!("Could not get DB connection from pool for settings: {}", e);
            // Return a default/empty settings object on failure
            admin_helpers::Settings {
                contributor_path_prefix: "error-loading".to_string(),
                max_file_upload_size_mb: "0".to_string(),
                allowed_mime_types: "".to_string(),
            }
        }
    };
    ctx.insert("settings", &settings);

    // UPDATED: Pass the pool to the helper function
    match admin_helpers::fetch_all_contributors(&pool) {
        Ok(all_users) => ctx.insert("contributors", &all_users),
        Err(e) => {
            log::error!("Failed to fetch contributors for admin dashboard: {}", e);
            ctx.insert("contributors", &Vec::<String>::new());
        }
    }

    match admin_helpers::get_all_tags(&db) {
        Ok(mut tags) => {
            tags.sort_unstable();
            ctx.insert("available_tags", &tags);
        },
        Err(e) => {
            log::error!("Failed to fetch available tags: {}", e);
            ctx.insert("available_tags", &Vec::<String>::new());
        }
    }

    match tera.render("admin/dashboard.html", &ctx) {
        Ok(rendered) => HttpResponse::Ok().content_type("text/html; charset=utf-8").body(rendered),
        Err(err) => {
            log::error!("Template rendering error: {}", err);
            HttpResponse::InternalServerError().body("Error rendering admin dashboard.")
        }
    }
}

async fn create_user_action(
    session: Session,
    pool: web::Data<crate::DbPool>,
    form: web::Bytes,
    config: web::Data<Config>,
) -> impl Responder {
    let dashboard_url = format!("/management/{}/dashboard", &config.admin_url_prefix);

    let parsed = match crate::helper::form_helpers::parse_form(&form) {
        Ok(p) => p,
        Err(response) => return response, // Return the 400 Bad Request
    };

    let username = parsed.get("username").map_or("".to_string(), |s| s.trim().to_string());
    let password = parsed.get("password").cloned().unwrap_or_default();
    let role = parsed.get("role").cloned().unwrap_or_default();

    if username.is_empty() || password.is_empty() || (role != "admin" && role != "contributor") {
        set_notification(&session, "Invalid input. All fields required.", "error");
    } else {
        match admin_helpers::create_new_contributor(&pool, &username, &password, &role) {
            Ok(_) => set_notification(&session, &format!("User '{}' created successfully.", username), "success"),
            Err(e) => {
                log::error!("Failed to create user '{}': {}", username, e);
                set_notification(&session, "Username already exists.", "error");
            }
        };
    }
    HttpResponse::Found().append_header(("location", dashboard_url)).finish()
}


async fn update_user_action(
    session: Session,
    pool: web::Data<crate::DbPool>,
    form: web::Bytes,
    config: web::Data<Config>,
) -> impl Responder {
    let dashboard_url = format!("/management/{}/dashboard", &config.admin_url_prefix);

    let parsed = match crate::helper::form_helpers::parse_form(&form) {
        Ok(p) => p,
        Err(response) => return response, // Return the 400 Bad Request
    };

    let user_id = parsed.get("user_id").and_then(|id| id.parse::<i32>().ok()).unwrap_or(0);
    let username = parsed.get("username").map_or("", |s| s.trim());
    let password = parsed.get("password").map(|s| s.as_str());
    let is_active = parsed.contains_key("is_active");
    let can_delete_own = parsed.contains_key("can_edit_and_delete_own_posts");
    let can_edit_any = parsed.contains_key("can_edit_any_post");
    let can_delete_any = parsed.contains_key("can_delete_any_post");
    let can_approve_posts = parsed.contains_key("can_approve_posts");

    if user_id == 0 || username.is_empty() {
        set_notification(&session, "Invalid user data provided.", "error");
    } else {
        match admin_helpers::update_contributor(&pool, user_id, username, password, is_active, can_delete_own, can_edit_any, can_delete_any, can_approve_posts) {
            Ok(_) => set_notification(&session, &format!("User '{}' updated successfully.", username), "success"),
            Err(e) => {
                log::error!("Failed to update user_id {}: {}", user_id, e);
                set_notification(&session, "Failed to update user. Username may already be taken.", "error");
            }
        };
    }
    HttpResponse::Found().append_header(("location", dashboard_url)).finish()
}


async fn delete_user_action(
    session: Session,
    auth_user: AuthenticatedContributor, // We need the current user's details
    pool: web::Data<crate::DbPool>,
    form: web::Bytes,
    config: web::Data<Config>,
) -> impl Responder {
    let admin_url_prefix = &config.admin_url_prefix;
    let dashboard_url = format!("/management/{}/dashboard", admin_url_prefix);
    let login_url = format!("/management/{}/login", admin_url_prefix); // Redirect here after self-deletion

    let parsed = match crate::helper::form_helpers::parse_form(&form) {
        Ok(p) => p,
        Err(response) => return response,
    };
    let user_id_to_delete = parsed.get("user_id").and_then(|id| id.parse::<i32>().ok()).unwrap_or(0);

    if user_id_to_delete == 0 {
         set_notification(&session, "Invalid user ID provided.", "error");
         return HttpResponse::Found().append_header(("location", dashboard_url)).finish();
    }

    // Get the current admin's ID from the database. A connection from the pool is required.
    let conn = match pool.get() {
        Ok(c) => c,
        Err(e) => {
            log::error!("Database pool error on user delete action: {}", e);
            set_notification(&session, "A database connection error occurred.", "error");
            return HttpResponse::Found().append_header(("location", dashboard_url)).finish();
        }
    };
    
    let current_admin_id = match users_db_operations::read_user_by_username(&conn, &auth_user.username) {
        Some(admin) => admin.id,
        None => {
            // This is a critical state inconsistency. The authenticated user does not exist.
            // Purge the session and force a logout immediately.
            session.purge();
            return HttpResponse::Found().append_header(("location", login_url)).finish();
        }
    };

    // Check if the admin is deleting their own account.
    if current_admin_id == user_id_to_delete {
        // Attempt to delete the user from the database first.
        match admin_helpers::delete_contributor(&pool, user_id_to_delete) {
            Ok(_) => {
                // SUCCESS: The user is deleted. Now, destroy the session completely.
                session.purge();
                // Redirect to the login page because the session is now invalid and they are logged out.
                return HttpResponse::Found().append_header(("location", login_url)).finish();
            }
            Err(e) => {
                // The deletion failed for some reason. Log it and report an error.
                log::error!("Failed to self-delete user_id {}: {}", user_id_to_delete, e);
                set_notification(&session, "Could not delete your account due to a database error.", "error");
                return HttpResponse::Found().append_header(("location", dashboard_url)).finish();
            }
        }
    }

    // If the code reaches here, it means the admin is deleting a DIFFERENT user.
    // The existing logic for this case is correct.
    match admin_helpers::delete_contributor(&pool, user_id_to_delete) {
        Ok(0) => set_notification(&session, "User not found or could not be deleted.", "error"),
        Ok(_) => set_notification(&session, "User deleted successfully.", "success"),
        Err(e) => {
            log::error!("Failed to delete user_id {}: {}", user_id_to_delete, e);
            set_notification(&session, "Failed to delete user due to a database error.", "error");
        }
    }
    HttpResponse::Found().append_header(("location", dashboard_url)).finish()
}



async fn add_tag_action(
    session: Session,
    db: web::Data<Database>,
    form: web::Bytes,
    config: web::Data<Config>,
) -> impl Responder {
    let dashboard_url = format!("/management/{}/dashboard", &config.admin_url_prefix);

    let parsed = match crate::helper::form_helpers::parse_form(&form) {
        Ok(p) => p,
        Err(response) => return response, // Return the 400 Bad Request
    };

    if let Some(tag) = parsed.get("tag_name") {
        if !tag.trim().is_empty() {
            match admin_helpers::add_tag(&db, tag) {
                Ok(_) => set_notification(&session, &format!("Tag '{}' added successfully.", tag), "success"),
                Err(e) => {
                    log::error!("Failed to add tag '{}': {}", tag, e);
                    set_notification(&session, "Failed to add tag.", "error");
                }
            }
        } else {
            set_notification(&session, "Tag name cannot be empty.", "error");
        }
    }
    HttpResponse::Found().append_header(("location", dashboard_url)).finish()
}


async fn delete_tag_action(
    session: Session,
    db: web::Data<Database>,
    form: web::Bytes,
    config: web::Data<Config>,
) -> impl Responder {
    let dashboard_url = format!("/management/{}/dashboard", &config.admin_url_prefix);

    let parsed = match crate::helper::form_helpers::parse_form(&form) {
        Ok(p) => p,
        Err(response) => return response, // Return the 400 Bad Request
    };

    if let Some(tag) = parsed.get("tag_name") {
        match admin_helpers::delete_tag(&db, tag) {
            Ok(_) => set_notification(&session, &format!("Tag '{}' deleted successfully.", tag), "success"),
            Err(e) => {
                log::error!("Failed to delete tag '{}': {}", tag, e);
                set_notification(&session, "Failed to delete tag.", "error");
            }
        }
    }
    HttpResponse::Found().append_header(("location", dashboard_url)).finish()
}