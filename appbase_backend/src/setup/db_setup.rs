use redb::{Database, TableDefinition, CommitError, StorageError, TableError, TransactionError};
use rusqlite::{Connection, Result as RusqliteResult, Transaction};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SetupError {
    #[error("Rusqlite error: {0}")]
    Rusqlite(#[from] rusqlite::Error),
    #[error("Redb storage error: {0}")]
    RedbStorage(#[from] StorageError),
    #[error("Redb transaction error: {0}")]
    RedbTransaction(#[from] TransactionError),
    #[error("Redb table error: {0}")]
    RedbTable(#[from] TableError),
    #[error("Redb commit error: {0}")]
    RedbCommit(#[from] CommitError),
}

pub fn setup_contributors_db(conn: &mut Connection) -> Result<(), SetupError> {
    let tx = conn.transaction()?;
    println!("- Creating 'users' table...");
    tx.execute(
        "CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            role TEXT NOT NULL CHECK(role IN ('admin', 'contributor')),
            is_active INTEGER NOT NULL DEFAULT 1,
            can_edit_and_delete_own_posts INTEGER NOT NULL DEFAULT 0,
            can_edit_any_post INTEGER NOT NULL DEFAULT 0,
            can_delete_any_post INTEGER NOT NULL DEFAULT 0,
            can_approve_posts INTEGER NOT NULL DEFAULT 0, -- <-- NEW FIELD
            last_login_time TEXT
        )",
        [],
    )?;

    println!("- Creating 'post_ownership' table...");
    tx.execute(
        "CREATE TABLE IF NOT EXISTS post_ownership (
            post_id TEXT PRIMARY KEY,
            user_id INTEGER NOT NULL,
            edit_log TEXT,
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
        )",
        [],
    )?;

    // --- NEW TABLE for pending post ownership ---
    println!("- Creating 'pending_post_ownership' table...");
    tx.execute(
        "CREATE TABLE IF NOT EXISTS pending_post_ownership (
            post_id TEXT PRIMARY KEY,
            user_id INTEGER NOT NULL,
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
        )",
        [],
    )?;
    // --- END NEW TABLE ---

    println!("- Creating 'settings' table...");
    tx.execute(
        "CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
        [],
    )?;

    println!("- Creating 'media_attachments' table...");
    tx.execute(
        "CREATE TABLE IF NOT EXISTS media_attachments (
            id TEXT PRIMARY KEY,
            user_id INTEGER NOT NULL,
            tags TEXT,
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
        )",
        [],
    )?;

    seed_initial_settings(&tx)?;

    tx.commit()?;
    Ok(())
}

fn seed_initial_settings(tx: &Transaction) -> RusqliteResult<()> {
    println!("- Seeding initial settings...");
    let default_prefix = "contributors";
    tx.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('contributor_path_prefix', ?1)",
        [&default_prefix],
    )?;
    println!("  > Default contributor path prefix set to: {}", default_prefix);

    let default_max_size = "10";
    tx.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('max_file_upload_size_mb', ?1)",
        [&default_max_size],
    )?;
    println!("  > Default max file upload size set to: {} MB", default_max_size);

    // Secure Default: Start with an empty list. Admin must explicitly add types.
    let default_mime_types = "";
    tx.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('allowed_mime_types', ?1)",
        [&default_mime_types],
    )?;
    println!("  > Default allowed MIME types set to: (empty - admin must configure)");

    Ok(())
}


pub fn setup_posts_db(db: &Database) -> Result<(), SetupError> {
    let write_txn = db.begin_write()?;
    {
        // --- Existing tables for published posts ---
        const POSTS: TableDefinition<&[u8; 16], &str> = TableDefinition::new("posts");
        const METADATA: TableDefinition<&[u8; 16], &str> = TableDefinition::new("metadata");
        const TAG_INDEX: TableDefinition<(&str, i64, &[u8; 16]), ()> = TableDefinition::new("tag_index");
        const AVAILABLE_TAGS: TableDefinition<&str, ()> = TableDefinition::new("available_tags");
        const SEARCH_APPEAR_KEYWORD_INDEX: TableDefinition<(&str, i64, &[u8; 16]), ()> = TableDefinition::new("search_appear_keyword_index");
        // NEW TABLE
        const CHRONOLOGICAL_INDEX: TableDefinition<(i64, &[u8; 16]), ()> = TableDefinition::new("chronological_index");


        // --- NEW TABLES for pending posts ---
        const PENDING_POSTS: TableDefinition<&[u8; 16], &str> = TableDefinition::new("pending_posts");
        const PENDING_METADATA: TableDefinition<&[u8; 16], &str> = TableDefinition::new("pending_metadata");

        println!("- Creating 'posts' table in Redb...");
        write_txn.open_table(POSTS)?;

        println!("- Creating 'metadata' table in Redb...");
        write_txn.open_table(METADATA)?;

        println!("- Creating 'tag_index' table in Redb...");
        write_txn.open_table(TAG_INDEX)?;

        println!("- Creating 'available_tags' table in Redb...");
        write_txn.open_table(AVAILABLE_TAGS)?;

        println!("- Creating 'search_appear_keyword_index' table in Redb...");
        write_txn.open_table(SEARCH_APPEAR_KEYWORD_INDEX)?;

        println!("- Creating 'chronological_index' table in Redb...");
        write_txn.open_table(CHRONOLOGICAL_INDEX)?;
        
        // --- NEW ---
        println!("- Creating 'pending_posts' table in Redb...");
        write_txn.open_table(PENDING_POSTS)?;
        
        println!("- Creating 'pending_metadata' table in Redb...");
        write_txn.open_table(PENDING_METADATA)?;
        // --- END NEW ---

    }
    write_txn.commit()?;
    Ok(())
}