use clap::{Parser, Subcommand};
use appbase_backend::config::Config;
use appbase_backend::setup::db_setup;
use rusqlite::{params, Connection};
use bcrypt::{hash, DEFAULT_COST};
use redb::Database;
use std::fs;
use std::path::PathBuf; // Import PathBuf

#[derive(Parser, Debug)]
#[command(name = "setup_cli", author, version, about = "A CLI for initial application setup.", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    // --- MODIFIED: This is now a mandatory global argument ---
    /// Path to the .env configuration file.
    #[arg(long, required = true, value_name = "FILE")]
    env_file: PathBuf,
}

// ... (No other changes in the structs)
#[derive(Subcommand, Debug)]
enum Commands {
    Db {
        #[command(subcommand)]
        action: DbAction,
    },
    Admin {
        #[command(subcommand)]
        action: AdminAction,
    },
}

#[derive(Subcommand, Debug)]
enum DbAction {
    Setup {
        db_type: Option<String>,
    }
}

#[derive(Subcommand, Debug)]
enum AdminAction {
    Create {
        #[arg(long)]
        username: String,
        #[arg(long)]
        password: String,
    },
    List,
    ChangePassword {
        #[arg(long)]
        username: String,
        #[arg(long)]
        new_password: String,
    },
    ChangeUsername {
        #[arg(long)]
        old_username: String,
        #[arg(long)]
        new_username: String,
    },
}

fn main() {
    let cli = Cli::parse();
    
    // --- MODIFIED: Pass the required path directly ---
    let config = Config::from_env(&cli.env_file)
        .expect("FATAL: Failed to load or parse configuration.");

    match &cli.command {
        // ... (rest of the file is unchanged and correct)
        Commands::Db { action } => match action {
            DbAction::Setup { db_type } => {
                match db_type.as_deref() {
                    Some("contributors") => setup_contributors_database(&config),
                    Some("posts") => setup_posts_database(&config),
                    Some(other) => eprintln!("❌ Error: Unknown database type '{}'. Use 'contributors' or 'posts'.", other),
                    None => {
                        setup_contributors_database(&config);
                        setup_posts_database(&config);
                    }
                }
            }
        },
        Commands::Admin { action } => match action {
            AdminAction::Create { username, password } => {
                create_admin_user(&config, username, password);
            }
            AdminAction::List => {
                list_admin_users(&config);
            }
            AdminAction::ChangePassword { username, new_password } => {
                change_admin_password(&config, username, new_password);
            }
            AdminAction::ChangeUsername { old_username, new_username } => {
                change_admin_username(&config, old_username, new_username);
            }
        },
    }
}

// ... (rest of the functions are unchanged and correct)
fn setup_contributors_database(config: &Config) {
    let db_path = config.users_db_path();
    if db_path.exists() {
        println!("ℹ️ Contributors database already exists at '{}'. Skipping creation.", db_path.display());
        return;
    }
    println!("\nSetting up contributors database at '{}'...", db_path.display());

    if let Some(parent_dir) = db_path.parent() {
        fs::create_dir_all(parent_dir).expect("Could not create database directory.");
    }

    let mut conn = Connection::open(&db_path).expect("Could not create contributors database file.");
    match db_setup::setup_contributors_db(&mut conn) {
        Ok(_) => println!("✅ Contributors database setup completed successfully."),
        Err(e) => eprintln!("❌ Error setting up contributors database: {}", e),
    }
}

fn setup_posts_database(config: &Config) {
    let db_path = config.posts_db_path();
     if db_path.exists() {
        println!("ℹ️ Posts database already exists at '{}'. Skipping creation.", db_path.display());
        return;
    }
    println!("\nSetting up posts database at '{}'...", db_path.display());

    if let Some(parent_dir) = db_path.parent() {
        fs::create_dir_all(parent_dir).expect("Could not create database directory.");
    }

    let db = Database::create(&db_path).expect("Failed to create posts database file.");
    match db_setup::setup_posts_db(&db) {
        Ok(_) => println!("✅ Posts database setup completed successfully."),
        Err(e) => eprintln!("❌ Error setting up posts database: {}", e),
    }
}

fn create_admin_user(config: &Config, username: &str, password: &str) {
    let db_path = config.users_db_path();
    if !db_path.exists() {
        eprintln!("❌ Error: Contributors database not found at '{}'. Please run `setup_cli db setup` first.", db_path.display());
        return;
    }
    let conn = Connection::open(&db_path).expect("Could not open contributors database.");
    let hashed_password = hash(password, DEFAULT_COST).expect("Failed to hash password");

    match conn.execute(
        "INSERT INTO users (username, password_hash, role, can_edit_and_delete_own_posts, can_edit_any_post, can_delete_any_post) VALUES (?1, ?2, 'admin', 1, 1, 1)",
        params![username, hashed_password],
    ) {
        Ok(_) => println!("✅ Admin user '{}' created successfully.", username),
        Err(e) => eprintln!("❌ Error creating admin user: {}. It might be because the username already exists.", e),
    }
}

fn list_admin_users(config: &Config) {
    let conn = match Connection::open(&config.users_db_path()) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("❌ Error: Contributors database not found. Please run `setup_cli db setup` first.");
            return;
        }
    };
    let mut stmt = match conn.prepare("SELECT username FROM users WHERE role = 'admin' ORDER BY username") {
        Ok(s) => s,
        Err(e) => {
            eprintln!("❌ Error preparing database query: {}", e);
            return;
        }
    };
    let user_iter = stmt.query_map([], |row| row.get(0));

    println!("Listing Admin Users:");
    match user_iter {
        Ok(users) => {
            for user in users {
                println!("- {}", user.unwrap_or_else(|_| "Invalid username".to_string()));
            }
        }
        Err(e) => eprintln!("❌ Error fetching admins: {}", e),
    }
}

fn change_admin_password(config: &Config, username: &str, new_password: &str) {
    let conn = match Connection::open(&config.users_db_path()) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("❌ Error: Contributors database not found.");
            return;
        }
    };
    let hashed_password = hash(new_password, DEFAULT_COST).expect("Failed to hash new password");
    match conn.execute(
        "UPDATE users SET password_hash = ?1 WHERE username = ?2 AND role = 'admin'",
        params![hashed_password, username],
    ) {
        Ok(0) => eprintln!("❌ Error: No admin user named '{}' found.", username),
        Ok(_) => println!("✅ Password for admin user '{}' changed successfully.", username),
        Err(e) => eprintln!("❌ Error updating password: {}", e),
    }
}

fn change_admin_username(config: &Config, old_username: &str, new_username: &str) {
    let conn = match Connection::open(&config.users_db_path()) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("❌ Error: Contributors database not found.");
            return;
        }
    };
    match conn.execute(
        "UPDATE users SET username = ?1 WHERE username = ?2 AND role = 'admin'",
        params![new_username, old_username],
    ) {
        Ok(0) => eprintln!("❌ Error: No admin user named '{}' found.", old_username),
        Ok(_) => println!("✅ Admin username changed from '{}' to '{}'.", old_username, new_username),
        Err(e) => eprintln!("❌ Error changing username: {}. The new username might already be taken.", e),
    }
}