

use crate::config::Config;
use crate::helper::advanced_db_manager_helpers as dbm_helpers;
use crate::middleware::AuthenticatedContributor;
use crate::models::advanced_db_manager_models::{CleanTableRequest, DeleteRowRequest, DbSelection, UpdateCellRequest};
use crate::DbPool;
use actix_csrf::extractor::CsrfToken;
use actix_web::{get, post, web, HttpResponse, Responder};
use redb::Database;
use serde::Deserialize;
use tera::{Context, Tera};
use actix_session::Session;
use crate::models::db_operations::users_db_operations;

#[derive(Deserialize)]
pub struct TableDataQuery {
    db: String,
    table: String,
    page: Option<u32>,
    size: Option<u32>,
    search_id: Option<String>,
}

#[derive(Deserialize)]
pub struct DependencyQuery {
    db: String,
    table: String,
    id: String,
}

#[get("/advanced-db-manager")]
async fn show_db_manager_page(
    tera: web::Data<Tera>,
    token: CsrfToken,
    config: web::Data<Config>,
    _user: AuthenticatedContributor,
) -> impl Responder {
    let mut ctx = Context::new();
    ctx.insert("csrf_token", token.get());
    ctx.insert("admin_url_prefix", &config.admin_url_prefix);
    match tera.render("admin/advanced_db_manager.html", &ctx) {
        Ok(rendered) => HttpResponse::Ok().content_type("text/html").body(rendered),
        Err(e) => {
            log::error!("Template rendering error: {}", e);
            HttpResponse::InternalServerError().body("Template error")
        }
    }
}

#[get("/advanced-db-manager/structure")]
async fn get_db_structure(_user: AuthenticatedContributor) -> impl Responder {
    let structure = dbm_helpers::get_db_structure();
    HttpResponse::Ok().json(structure)
}

#[get("/advanced-db-manager/dependencies")]
async fn get_dependencies(
    posts_db: web::Data<Database>,
    pool: web::Data<DbPool>,
    query: web::Query<DependencyQuery>,
    _user: AuthenticatedContributor,
) -> impl Responder {
    let db_selection = match query.db.as_str() {
        "postsdb" => DbSelection::PostsDb,
        "contributordb" => DbSelection::ContributorDb,
        _ => return HttpResponse::BadRequest().json("Invalid 'db' parameter"),
    };

    match dbm_helpers::get_row_dependencies(posts_db, pool, db_selection, query.table.clone(), query.id.clone()).await {
        Ok(deps) => HttpResponse::Ok().json(deps),
        Err(e) => {
            log::error!("Failed to get dependencies: {:?}", e);
            HttpResponse::InternalServerError().json(e.to_string())
        }
    }
}

#[get("/advanced-db-manager/data")]
async fn get_table_data(
    posts_db: web::Data<Database>,
    pool: web::Data<DbPool>,
    query: web::Query<TableDataQuery>,
    _user: AuthenticatedContributor,
) -> impl Responder {
    let page = query.page.unwrap_or(1);
    let size = query.size.unwrap_or(20);

    let db_selection = match query.db.as_str() {
        "postsdb" => DbSelection::PostsDb,
        "contributordb" => DbSelection::ContributorDb,
        _ => return HttpResponse::BadRequest().json("Invalid 'db' parameter"),
    };
    
    let search_id = query.search_id.clone().filter(|s| !s.trim().is_empty());

    match dbm_helpers::get_paginated_table_data(posts_db, pool, db_selection, query.table.clone(), page, size, search_id).await {
        Ok(response) => HttpResponse::Ok().json(response),
        Err(e) => {
            log::error!("Failed to get table data: {:?}", e);
            HttpResponse::InternalServerError().json(e.to_string())
        }
    }
}

#[post("/advanced-db-manager/delete-row")]
async fn delete_row(
    posts_db: web::Data<Database>,
    pool: web::Data<DbPool>,
    req_body: web::Json<DeleteRowRequest>,
    user: AuthenticatedContributor, // We need the current user to check their ID
    session: Session,                // We need the session to purge it on self-deletion
) -> impl Responder {
    let DeleteRowRequest { 
        db_selection, 
        table_name, 
        row_id, 
        dependents 
    } = req_body.into_inner();

    // Special security handling for the 'users' table
    if let (DbSelection::ContributorDb, "users") = (db_selection, table_name.as_str()) {
        
        let conn = match pool.get() {
            Ok(c) => c,
            Err(e) => {
                log::error!("DB Manager: Could not get pool connection: {}", e);
                return HttpResponse::InternalServerError().json(serde_json::json!({"status": "error", "message": "Database connection error."}));
            }
        };

        // Get the current admin's ID
        let current_admin_id = match users_db_operations::read_user_by_username(&conn, &user.username) {
            Some(admin) => admin.id,
            None => {
                // This is a critical error state. Force logout.
                session.purge();
                return HttpResponse::Unauthorized().json(serde_json::json!({"status": "error", "message": "Authenticated user not found in database. Session terminated."}));
            }
        };
        
        // Parse the user ID being deleted from the row_id string
        if let Ok(user_id_to_delete) = row_id.parse::<i32>() {
            // Check if the admin is deleting themselves
            if current_admin_id == user_id_to_delete {
                // Proceed with the deletion...
                match dbm_helpers::delete_table_rows(posts_db, pool, db_selection, table_name, row_id, dependents).await {
                    Ok(_) => {
                        // ... and then immediately purge the session.
                        session.purge();
                        // Return a success response. The front-end's next API call will fail because
                        // the session is gone, effectively logging them out.
                        return HttpResponse::Ok().json(serde_json::json!({"status": "success", "message": "Self-deleted. Session terminated."}));
                    },
                    Err(e) => {
                        log::error!("DB Manager: Failed to self-delete user_id {}: {:?}", user_id_to_delete, e);
                        return HttpResponse::InternalServerError().json(serde_json::json!({"status": "error", "message": "Failed to delete user due to a database error."}));
                    }
                }
            }
        }
    }

    // Normal deletion logic for any other table or any other user. This part remains unchanged.
    match dbm_helpers::delete_table_rows(
        posts_db,
        pool,
        db_selection,
        table_name,
        row_id,
        dependents,
    ).await {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({"status": "success"})),
        Err(e) => {
            log::error!("DB Manager: Failed to delete row(s): {:?}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({"status": "error", "message": e.to_string()}))
        }
    }
}

#[post("/advanced-db-manager/clean-table")]
async fn clean_table(
    posts_db: web::Data<Database>,
    pool: web::Data<DbPool>,
    req_body: web::Json<CleanTableRequest>,
    user: AuthenticatedContributor,
) -> impl Responder {
     match dbm_helpers::clean_table_with_auth(
        posts_db,
        pool,
        user.username,
        req_body.admin_password.clone(),
        req_body.db_selection,
        req_body.table_name.clone(),
        req_body.clean_dependents,
    ).await {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({"status": "success", "message": "Table cleaned successfully."})),
        Err(dbm_helpers::HelperError::InvalidCredentials) => HttpResponse::Forbidden().json(serde_json::json!({"status": "error", "message": "Invalid admin password."})),
        Err(e) => {
            log::error!("Failed to clean table: {:?}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({"status": "error", "message": e.to_string()}))
        }
    }
}

#[post("/advanced-db-manager/update-cell")]
async fn update_cell(
    posts_db: web::Data<Database>,
    pool: web::Data<DbPool>,
    req_body: web::Json<UpdateCellRequest>,
    _user: AuthenticatedContributor,
) -> impl Responder {
    match dbm_helpers::update_table_cell(
        posts_db,
        pool,
        req_body.db_selection,
        req_body.table_name.clone(),
        req_body.row_id.clone(),
        req_body.column_name.clone(),
        req_body.value.clone(),
    ).await {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({"status": "success"})),
        Err(e) => {
            log::error!("Failed to update cell: {:?}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({"status": "error", "message": e.to_string()}))
        }
    }
}

pub fn config_advanced_db_manager(cfg: &mut web::ServiceConfig) {
    cfg.service(show_db_manager_page)
       .service(get_db_structure)
       .service(get_dependencies)
       .service(get_table_data)
       .service(delete_row)
       .service(clean_table)
       .service(update_cell);
}