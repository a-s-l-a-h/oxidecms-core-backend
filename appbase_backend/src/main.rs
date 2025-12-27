use actix_cors::Cors;
use actix_web::{web, App, HttpServer, middleware::{Logger, DefaultHeaders}, cookie::Key, HttpResponse, Responder};
use actix_session::{SessionMiddleware, storage::CookieSessionStore, SessionExt};
use actix_csrf::CsrfMiddleware;
use tera::Tera;
use appbase_backend::{
    config::Config,
    routes,
    helper::admin_helpers,
    middleware::{admin_guard, contributor_guard, ip_guard, ContributorPrefixValidation},
    AppState
};
use redb::Database;
use r2d2_sqlite::SqliteConnectionManager; // NEW
use r2d2::Pool; // NEW
use std::fs;
use std::sync::{Arc, RwLock};
use clap::Parser;
use std::path::PathBuf;
use rand::prelude::StdRng;
use hex;
use std::convert::TryFrom;



/// A simple handler for the root URL.
async fn root_handler() -> impl Responder {
    HttpResponse::Ok().content_type("text/plain").body("OK")
}

#[derive(Parser, Debug)]
#[command(name = "appbase_server", author, version, about = "Starts the AppBase web server.")]
struct Cli {
    /// Path to the .env configuration file.
    #[arg(long, required = true, value_name = "FILE")]
    env_file: PathBuf,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let cli = Cli::parse();
    
    // Load configuration first
    let config = Config::from_env(&cli.env_file)
        .expect("FATAL: Failed to load or parse configuration.");

    // Initialize logger using the value from config
    env_logger::init_from_env(env_logger::Env::new().default_filter_or(&config.log_level));

    let tera = Tera::new("templates/**/*.html").expect("Tera initialization failed");

    fs::create_dir_all(&config.database_path)
        .expect("Failed to create database directory");

    let redb_db_data = web::Data::new(Database::open(&config.posts_db_path())
        .expect("FATAL: posts.db not found. Run 'cargo run --bin setup_cli -- --env-file <path> db setup'"));

    // --- NEW: Create a thread-safe connection pool for SQLite ---
    let manager = SqliteConnectionManager::file(config.users_db_path());
    let pool = Pool::builder()
        .build(manager)
        .expect("FATAL: Failed to create Rusqlite connection pool.");

    let initial_contributor_prefix = {
        let conn = pool.get().expect("Failed to get DB connection for initial setup.");
        admin_helpers::get_settings(&conn).contributor_path_prefix
    };

    let app_state = web::Data::new(AppState {
        contributor_prefix: Arc::new(RwLock::new(initial_contributor_prefix)),
    });

    // --- MODIFICATION: Load the session key from the config ---
    let session_key_bytes = hex::decode(&config.session_secret_key)
        .expect("FATAL: SESSION_SECRET_KEY in .env is not a valid hex string.");
    let session_key = Key::try_from(session_key_bytes.as_slice())
        .expect("FATAL: The decoded SESSION_SECRET_KEY is not long enough (minimum 64 bytes required).");
    // --- END MODIFICATION ---

    let server_address = format!("{}:{}", config.web.host, config.web.port);
    println!("🚀 Server starting at http://{}", server_address);

    HttpServer::new(move || {
        let session_mw = SessionMiddleware::builder(CookieSessionStore::default(), session_key.clone())
            .cookie_secure(config.use_secure_cookies) // Use configurable value
            .cookie_http_only(true)
            .cookie_same_site(actix_web::cookie::SameSite::Lax)
            .build();

        // --- NEW: DYNAMIC CORS SETUP ---
        let cors = {
            let allowed_origins_str = &config.allowed_origins;
            if allowed_origins_str.trim() == "*" {
                Cors::default()
                    .allow_any_origin()
                    .allowed_methods(vec!["GET", "POST", "PUT", "DELETE"])
                    .allowed_headers(vec![actix_web::http::header::AUTHORIZATION, actix_web::http::header::ACCEPT, actix_web::http::header::CONTENT_TYPE])
                    .supports_credentials()
                    .max_age(3600)
            } else {
                let mut cors = Cors::default();
                let origins: Vec<&str> = allowed_origins_str.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
                for origin in origins {
                    cors = cors.allowed_origin(origin);
                }
                cors.allowed_methods(vec!["GET", "POST", "PUT", "DELETE"])
                    .allowed_headers(vec![actix_web::http::header::AUTHORIZATION, actix_web::http::header::ACCEPT, actix_web::http::header::CONTENT_TYPE])
                    .supports_credentials()
                    .max_age(3600)
            }
        };

        // Clone for use in the closure. This now comes from the config struct.
        let admin_url_prefix_clone = config.admin_url_prefix.clone();

        App::new()
            .wrap(cors) // APPLY THE CORS MIDDLEWARE
            .wrap(Logger::default())
            .wrap(
                DefaultHeaders::new()
                    .add(("X-Content-Type-Options", "nosniff"))
                    .add(("X-Frame-Options", "DENY"))
                    .add(("X-XSS-Protection", "1; mode=block"))
            )
            .app_data(web::Data::new(config.clone()))
            .app_data(web::Data::new(tera.clone()))
            .app_data(redb_db_data.clone())
            .app_data(web::Data::new(pool.clone())) // Share the connection pool
            .app_data(app_state.clone())

            .configure(routes::public::config_api)
            .service(actix_files::Files::new("/media", &config.media_path))
            .service(actix_files::Files::new("/ssr_static", "./ssr_static"))

            .route("/", web::get().to(root_handler))

            // This scope applies session management to all protected routes below.
            .service(
                web::scope("")
                    .wrap(session_mw)
                    // NEW: Add a /management prefix scope for all protected routes
                    .service(
                        web::scope("/management")
                            .service(
                                web::scope(&admin_url_prefix_clone)
                                    .wrap(
                                        CsrfMiddleware::<StdRng>::new()
                                            // Rule for the login page - UPDATED PATH
                                            .set_cookie(
                                                actix_web::http::Method::GET,
                                                format!("/management/{}/login", admin_url_prefix_clone)
                                            )
                                            // Rule to exempt the dashboard page from validation - UPDATED PATH
                                            .set_cookie(
                                                actix_web::http::Method::GET,
                                                format!("/management/{}/dashboard", admin_url_prefix_clone)
                                            )
                                            
                                            .set_cookie(
                                                actix_web::http::Method::GET,
                                                format!("/management/{}/advanced-db-manager", admin_url_prefix_clone)
                                            )
                                    )
                                    .guard(actix_web::guard::fn_guard(ip_guard))
                                    .configure(routes::admin::config_login)
                                    .service(
                                        web::scope("")
                                            .guard(actix_web::guard::fn_guard(|ctx| admin_guard(&ctx.get_session())))
                                            .configure(routes::admin::config_dashboard)
                                    )
                            )
                            .service(
                                web::scope("/{prefix}")
                                    .wrap(
                                        CsrfMiddleware::<StdRng>::new()
                                            // FIXED: Use the full path PATTERN for dynamic routes - UPDATED PATHS
                                            .set_cookie(actix_web::http::Method::GET, "/management/{prefix}/login")
                                            .set_cookie(actix_web::http::Method::GET, "/management/{prefix}/dashboard")
                                            .set_cookie(actix_web::http::Method::GET, "/management/{prefix}/edit_post/{post_id}")
                                            .set_cookie(actix_web::http::Method::GET, "/management/{prefix}/approve") 
                                    )
                                    .wrap(ContributorPrefixValidation)
                                    .configure(routes::contributor::config_login)
                                    .service(
                                        web::scope("")
                                            .guard(actix_web::guard::fn_guard(|ctx| contributor_guard(&ctx.get_session())))
                                            .configure(routes::contributor::config_dashboard)
                                    )
                            )
                    )
            )
    })
    .bind(server_address)?
    .run()
    .await
}