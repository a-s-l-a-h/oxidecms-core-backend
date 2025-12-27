

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents a generic, paginated response for the frontend.
#[derive(Serialize)]
pub struct PaginatedResponse {
    pub data: Vec<HashMap<String, String>>,
    pub last_page: u32,
}

/// Represents the database and table selected by the admin.
#[derive(Deserialize, Debug, Clone, Copy)]
pub enum DbSelection {
    PostsDb,
    ContributorDb,
}

// NEW: Defines a specific dependent to also be deleted.
#[derive(Deserialize)]
pub struct DependentToDelete {
    pub table_name: String,
    pub row_id: String,
}

/// Payload for a request to delete a row.
#[derive(Deserialize)]
pub struct DeleteRowRequest {
    pub db_selection: DbSelection,
    pub table_name: String,
    pub row_id: String,
    pub dependents: Vec<DependentToDelete>, // MODIFIED: Now a list of specific dependents.
}

/// Payload for a request to clean a table.
#[derive(Deserialize)]
pub struct CleanTableRequest {
    pub db_selection: DbSelection,
    pub table_name: String,
    pub admin_password: String,
    pub clean_dependents: bool,
}

/// Payload for a request to update a single cell's value.
#[derive(Deserialize)]
pub struct UpdateCellRequest {
    pub db_selection: DbSelection,
    pub table_name: String,
    pub row_id: String,
    pub column_name: String,
    pub value: String,
}


// --- STRUCTS FOR DYNAMIC FRONTEND & DEPENDENCY CHECK ---

#[derive(Serialize)]
pub struct TableInfo {
    pub name: String,
    pub cleanable: bool,
    pub dependencies: Vec<String>, // MODIFIED: Now a list of potential dependencies
}

#[derive(Serialize)]
pub struct DbInfo {
    pub id: String,
    pub name: String,
    pub tables: Vec<TableInfo>,
}

#[derive(Serialize)]
pub struct DbStructureResponse {
    pub databases: Vec<DbInfo>,
    #[serde(rename = "editableCells")]
    pub editable_cells: HashMap<String, Vec<String>>,
}

// NEW: Represents a found dependent row for the frontend modal.
#[derive(Serialize)]
pub struct FoundDependency {
    pub table_name: String,
    pub row_id: String,
    pub preview: String, // A short preview of the data
}