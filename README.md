

# OxideCMS Core Backend

OxideCMS Core Backend is a robust, secure, and high-performance backend server for a content management system, written entirely in Rust. It leverages the Actix Web framework for asynchronous request handling and provides a clear separation of concerns between public content delivery and secure content management.

This guide provides all the necessary steps for a developer to set up, build, configure, and run the application in both development and production environments.

## ✨ Features

*   **Role-Based Access Control (RBAC):** Separate roles for **Admins** (full control) and **Contributors** (content management).
*   **Secure Management Areas:** Unique, secret login URLs for Admins and Contributors with CSRF protection and optional IP whitelisting for admins.
*   **Content Approval Workflow:** A full pending/approval queue for new and updated posts ensures content quality.
*   **Advanced Database Manager:** A powerful admin UI to directly view, edit, and manage raw database records safely.
*   **Secure Media Management:** Configurable file upload policies (size, MIME type) with contributor-owned media assets.
*   **High-Performance API:** Built on `redb`, a fast embedded database, with indexed queries for delivering content headlessly.
*   **CLI-Driven Setup:** A dedicated command-line interface for easy database initialization and admin user management.
*   **Security-First Design:** Includes input sanitization to prevent XSS, secure session management, and security-focused HTTP headers.

## 🏗️ Project Structure

The project is structured to keep the core application code separate from configuration, data, and user-generated content.

```
oxidecms-core-backend/
├── .env                  <-- YOUR main configuration file (DO NOT COMMIT)
├── .gitignore            <-- Git ignore rules
├── appbase_backend/      <-- The main Rust project crate
│   ├── Cargo.toml
│   ├── src/              <-- All Rust source code
│   ├── config/           <-- Base configuration files (e.g., default.toml)
│   ├── templates/        <-- Tera HTML templates for admin/contributor UI
│   └── ssr_static/       <-- Static assets (CSS, JS) for the templates
├── db/                   <-- Runtime location for database files
│   ├── contributors/
│   │   └── contributors.db
│   └── posts/
│       └── posts.db
├── media/                <-- Runtime location for user-uploaded media files
│   └── attachments/
└── README.md             <-- This file
```

## 🚀 Getting Started: Step-by-Step Setup

Follow these instructions precisely to get a running instance of the application.

### Prerequisites

*   **Rust Toolchain:** Ensure you have the latest stable version of Rust installed. You can get it from [rustup.rs](https://rustup.rs/).

### Step 1: Clone the Repository

```bash
git clone <your-repository-url>
cd oxidecms-core-backend
```

### Step 2: Create Data Directories

The application requires dedicated directories for databases and media files. Create them in the project root. These directories are intentionally git-ignored.

```bash
# From the oxidecms-core-backend root directory
mkdir db
mkdir media
```

### Step 3: Configure the Environment (`.env` file)

This is the most critical step. Create a file named `.env` in the root of the `oxidecms-core-backend` directory.

**Copy the template below** into your new `.env` file and **carefully edit the values**, especially the paths and the session key.

```env
# .env Configuration File

# [CORS] - Cross-Origin Resource Sharing
# For development, '*' is fine. For production, specify your frontend's domain.
# Example: ALLOWED_ORIGINS=https://your-frontend.com,https://www.your-frontend.com
ALLOWED_ORIGINS=*

# [LOGGING]
# Options: error, warn, info, debug, trace
LOG_LEVEL=info

# [SECURITY & URLs]
# The secret path for the main administrator login.
# Access URL will be: http://localhost:8000/management/secret-admin-area
ADMIN_URL_PREFIX="secret-admin-area"

# Whitelist of IPs allowed to access the admin login page.
# For production, list specific trusted IPs. Example: ADMIN_LOGIN_ACCEPT_IP="127.0.0.1,88.88.88.88"
ADMIN_LOGIN_ACCEPT_IP="*"

# -----------------------------------------------------------------------------
# [SESSION SECRET] - CRITICAL
# This MUST be a 64-byte secret key, represented as 128 hexadecimal characters.
# Generate a new, secure key for your application by running the following command
# in your terminal and pasting the output here.
#
# Command to generate key:
# openssl rand -hex 64
#
# DO NOT use the placeholder value.
SESSION_SECRET_KEY="" # <-- PASTE YOUR 128-CHARACTER HEX KEY HERE
# -----------------------------------------------------------------------------

# [PATHS]
# ⚠️ IMPORTANT: These MUST be ABSOLUTE paths. The application will fail to
# start if you use relative paths (e.g., ./db).

# --- Replace with the FULL path to your project's directories ---

# Example for Windows:
DATABASE_PATH="C:/Users/YourUser/path/to/oxidecms-core-backend/db"
MEDIA_PATH="C:/Users/YourUser/path/to/oxidecms-core-backend/media"

# Example for Linux/macOS:
# DATABASE_PATH="/home/user/projects/oxidecms-core-backend/db"
# MEDIA_PATH="/home/user/projects/oxidecms-core-backend/media"

# [COOKIES]
# Set to "true" in production when using HTTPS.
# Set to "false" for local development over HTTP.
USE_SECURE_COOKIES="false"
```

### Step 4: Initialize the Databases

Use the built-in setup CLI to create the necessary database files and schemas. The command reads the `DATABASE_PATH` from your `.env` file.

```bash
# This command sets up both the contributors.db (SQLite) and posts.db (Redb) databases.
cargo run --bin setup_cli -- --env-file ./.env db setup
```

### Step 5: Create Your First Admin User

Use the CLI to create the primary administrator account.

```bash
# Replace 'admin' and 'your_super_secret_password' with your desired credentials.
cargo run --bin setup_cli -- --env-file ./.env admin create --username admin --password 'your_super_secret_password'
```

## 🛠️ Building the Application

You can build the project for either development or production.

### For Development

A standard build is sufficient for local development and testing.

```bash
cargo build
```

### For Production (Release Build)

For deployment, always create an optimized release build. This will be significantly faster.

```bash
cargo build --release
```

The compiled binaries (`appbase_server` and `setup_cli`) will be located in `appbase_backend/target/release/`.

## ▶️ Running the Server

### Development Server

This command compiles and runs the server. It will automatically re-compile if you make changes to the source code.

```bash
# The --env-file flag tells the server where to find your configuration.
cargo run --bin appbase_server -- --env-file ./.env
```

The server will start at the address configured in `config/default.toml` (typically `http://127.0.0.1:8000`).

### Production Server

In production, run the optimized binary that you built with `cargo build --release`.

**Important:** For security, your production `.env` file should ideally be located *outside* the project directory (e.g., in `/etc/oxidecms/.env`).

```bash
# Example assuming the .env file is at a secure, absolute location.
# This executes the pre-compiled, optimized binary.
./appbase_backend/target/release/appbase_server --env-file /etc/oxidecms/.env
```

## ⚙️ Command-Line Interface (CLI) Usage

The `setup_cli` binary is a powerful tool for managing your instance without needing the server to be running.

**Base Command Structure:**

All CLI commands follow this pattern. The `--env-file` flag is always required.

```bash
cargo run --bin setup_cli -- --env-file ./.env <COMMAND> <SUBCOMMAND> [OPTIONS]
```

---

### **Database Setup**

Initializes database files and tables.

*   **Command:** `db setup`
*   **Description:** Creates `contributors.db` and `posts.db` with the correct schemas if they do not already exist.
*   **Example:**
    ```bash
    cargo run --bin setup_cli -- --env-file ./.env db setup
    ```

---

### **Admin User Management**

*   **Command:** `admin create`
*   **Description:** Creates a new user with the 'admin' role.
*   **Options:**
    *   `--username <USERNAME>` (Required)
    *   `--password <PASSWORD>` (Required)
*   **Example:**
    ```bash
    cargo run --bin setup_cli -- --env-file ./.env admin create --username new-admin --password 'a-very-secure-password123!'
    ```

*   **Command:** `admin list`
*   **Description:** Lists all existing admin usernames.
*   **Example:**
    ```bash
    cargo run --bin setup_cli -- --env-file ./.env admin list
    ```

*   **Command:** `admin change-password`
*   **Description:** Changes the password for an existing admin user.
*   **Options:**
    *   `--username <USERNAME>` (Required)
    *   `--new-password <PASSWORD>` (Required)
*   **Example:**
    ```bash
    cargo run --bin setup_cli -- --env-file ./.env admin change-password --username new-admin --new-password 'a-much-stronger-password#$%'
    ```

*   **Command:** `admin change-username`
*   **Description:** Changes the username for an existing admin user.
*   **Options:**
    *   `--old-username <USERNAME>` (Required)
    *   `--new-username <USERNAME>` (Required)
*   **Example:**
    ```bash
    cargo run --bin setup_cli -- --env-file ./.env admin change-username --old-username new-admin --new-username super-admin
    ```
