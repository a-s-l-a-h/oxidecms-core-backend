
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

## 🎥 Live Demo

Here are a few short videos showcasing the core functionalities of the OxideCMS backend.

### 1. Admin Dashboard & User Management

This video demonstrates the main administrator dashboard, including how to create new users (contributors), manage their permissions, and configure site-wide settings.

https://github.com/user-attachments/assets/e5a684e0-f38c-4b6c-854f-9496b88d5ff5

### 2. Content Creation & Editing

See the contributor's dashboard in action. This covers writing a new post using a Markdown editor, uploading media, checking for similar existing posts, and submitting it for review.

https://github.com/user-attachments/assets/5e98c741-8fa8-45b8-bfe2-32b46a4075d1

### 3. Content Approval Workflow

This video shows the approval queue where admins (or contributors with special permissions) can review pending submissions, view the content, and either approve it for publishing or reject it.

https://github.com/user-attachments/assets/c6698dae-6e2e-472b-97c6-a354db4ac5de

## 🏗️ Project Structure

```
oxidecms-core-backend/
├── .env                  <-- YOUR main configuration file (DO NOT COMMIT)
├── appbase_backend/      <-- The main Rust project crate
│   ├── Cargo.toml
│   ├── src/
│   ├── config/
│   ├── templates/
│   └── ssr_static/
├── db/                   <-- Runtime location for database files
├── media/                <-- Runtime location for user-uploaded media files
└── README.md
```

## 🚀 Getting Started: Step-by-Step Setup

### Prerequisites

*   **Rust Toolchain:** Install the latest stable version from [rustup.rs](https://rustup.rs/).

### Step 1: Clone & Create Directories

```bash
git clone <your-repository-url>
cd oxidecms-core-backend

# Create the required data directories
mkdir db
mkdir media
```

### Step 2: Configure the `.env` File

This is the most critical step. Create a file named `.env` in the project root (`oxidecms-core-backend/.env`).

**Copy the template below** into your new file and **carefully edit the values**.

```env
# .env Configuration File

# [CORS] - Cross-Origin Resource Sharing
# For production, specify your frontend's domain. Example: ALLOWED_ORIGINS=https://your-frontend.com
ALLOWED_ORIGINS=*

# [LOGGING]
# Options: error, warn, info, debug, trace
LOG_LEVEL=info

# -----------------------------------------------------------------------------
# [SECURITY & URLs]
#
# The secret path for the main administrator login.
# With the example "secret-admin-area", the full login URL will be:
# http://127.0.0.1:8000/management/secret-admin-area/login
ADMIN_URL_PREFIX="secret-admin-area"
#
# Whitelist of IPs allowed to access the admin login page.
# For production, list specific trusted IPs. Example: ADMIN_LOGIN_ACCEPT_IP="127.0.0.1,88.88.88.88"
ADMIN_LOGIN_ACCEPT_IP="*"
# -----------------------------------------------------------------------------

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

### Step 3: Initialize the Databases

Use the built-in setup CLI.

💡 **Important:** Always provide the **absolute (full) path** to your `.env` file with the `--env-file` flag.

```bash
# On Linux/macOS:
cargo run --bin setup_cli -- --env-file /home/user/projects/oxidecms-core-backend/.env db setup

# On Windows (use Command Prompt or PowerShell):
cargo run --bin setup_cli -- --env-file C:/Users/YourUser/path/to/oxidecms-core-backend/.env db setup
```

### Step 4: Create Your First Admin User

Use the CLI to create your primary administrator account.

```bash
# On Linux/macOS (replace with your details):
cargo run --bin setup_cli -- --env-file /home/user/projects/oxidecms-core-backend/.env admin create --username admin --password 'your_super_secret_password'

# On Windows (replace with your details):
cargo run --bin setup_cli -- --env-file C:/Users/YourUser/path/to/oxidecms-core-backend/.env admin create --username admin --password "your_super_secret_password"
```

## 🛠️ Building the Application

### For Development

A standard build is sufficient for local development and testing.

```bash
cargo build
```

### For Production (Release Build)

For deployment, always create an optimized release build for maximum performance.

```bash
cargo build --release
```

The compiled binaries (`appbase_server` and `setup_cli`) will be located in the `appbase_backend/target/release/` directory.

## ▶️ Running the Server

### Development Server

This command compiles and runs the server, watching for code changes.

```bash
# On Linux/macOS:
cargo run --bin appbase_server -- --env-file /home/user/projects/oxidecms-core-backend/.env

# On Windows:
cargo run --bin appbase_server -- --env-file C:/Users/YourUser/path/to/oxidecms-core-backend/.env
```

**After starting the server, here is how to access the management panels:**

The server will typically start on `http://127.0.0.1:8000`.

*   **Administrator Login:**
    The admin URL is constructed as: `server_address/management/ADMIN_URL_PREFIX/login`.
    Based on the example `.env` file (`ADMIN_URL_PREFIX="secret-admin-area"`), your admin login page is:
    ```
    http://127.0.0.1:8000/management/secret-admin-area/login
    ```

*   **Contributor Login:**
    The contributor URL is based on a prefix set in the database (which defaults to `contributors`). The URL structure is: `server_address/management/{contributor-prefix}/login`.
    By default, the contributor login page is:
    ```
    http://127.0.0.1:8000/management/contributors/login
    ```

## ⚙️ Command-Line Interface (CLI) Usage

The `setup_cli` binary is used for database setup and admin management.

**Base Command Structure:**
Remember to always use the **absolute path** for `--env-file`.

```bash
# Generic Pattern
cargo run --bin setup_cli -- --env-file /path/to/your/.env <COMMAND> <SUBCOMMAND> [OPTIONS]
```

---

### **Database Setup (`db setup`)**

Initializes database files and tables if they don't exist.

```bash
# Linux/macOS Example
cargo run --bin setup_cli -- --env-file /home/user/projects/oxidecms-core-backend/.env db setup
```

---

### **Admin User Management**

*   **`admin create`**: Creates a new administrator.
    ```bash
    # Linux/macOS Example
    cargo run --bin setup_cli -- --env-file /path/to/.env admin create --username new-admin --password 'a-very-secure-password123!'
    ```

*   **`admin list`**: Lists all existing admin usernames.
    ```bash
    # Linux/macOS Example
    cargo run --bin setup_cli -- --env-file /path/to/.env admin list
    ```

*   **`admin change-password`**: Changes an admin's password.
    ```bash
    # Linux/macOS Example
    cargo run --bin setup_cli -- --env-file /path/to/.env admin change-password --username new-admin --new-password 'a-much-stronger-password#$%'
    ```

*   **`admin change-username`**: Changes an admin's username.
    ```bash
    # Linux/macOS Example
    cargo run --bin setup_cli -- --env-file /path/to/.env admin change-username --old-username new-admin --new-username super-admin
    ```

