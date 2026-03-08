use crate::models::User;
use sqlx::SqlitePool;

const USER_COLUMNS: &str =
    "id, email, display_name, password_hash, role, is_approved, created_at, updated_at, force_password_change";

/// Find a user by email (for login).
pub async fn get_user_by_email(pool: &SqlitePool, email: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {USER_COLUMNS} FROM users WHERE email = ?1"
    ))
    .bind(email)
    .fetch_optional(pool)
    .await
}

/// Find a user by ID.
pub async fn get_user_by_id(pool: &SqlitePool, id: i64) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {USER_COLUMNS} FROM users WHERE id = ?1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Create a new user. Returns the created row.
pub async fn create_user(
    pool: &SqlitePool,
    email: &str,
    display_name: &str,
    password_hash: &str,
    role: &str,
    is_approved: bool,
    force_password_change: bool,
) -> Result<User, sqlx::Error> {
    let id = sqlx::query(
        "INSERT INTO users (email, display_name, password_hash, role, is_approved, force_password_change)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(email)
    .bind(display_name)
    .bind(password_hash)
    .bind(role)
    .bind(is_approved)
    .bind(force_password_change)
    .execute(pool)
    .await?
    .last_insert_rowid();

    get_user_by_id(pool, id)
        .await
        .map(|opt| opt.expect("just-inserted user must exist"))
}

/// Count total users.
pub async fn count_users(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    Ok(count)
}

/// List all users (for admin user management page).
pub async fn get_all_users(pool: &SqlitePool) -> Result<Vec<User>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {USER_COLUMNS} FROM users ORDER BY created_at DESC"
    ))
    .fetch_all(pool)
    .await
}

/// Approve a user (admin action).
pub async fn approve_user(pool: &SqlitePool, user_id: i64) -> Result<bool, sqlx::Error> {
    let affected = sqlx::query(
        "UPDATE users SET is_approved = 1, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?1",
    )
    .bind(user_id)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(affected > 0)
}

/// Change a user's role (admin action).
pub async fn set_user_role(pool: &SqlitePool, user_id: i64, role: &str) -> Result<bool, sqlx::Error> {
    let affected = sqlx::query(
        "UPDATE users SET role = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?2",
    )
    .bind(role)
    .bind(user_id)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(affected > 0)
}

/// Delete a user (admin action). Links/pages become unowned (user_id = NULL via ON DELETE SET NULL).
pub async fn delete_user(pool: &SqlitePool, user_id: i64) -> Result<bool, sqlx::Error> {
    let affected = sqlx::query("DELETE FROM users WHERE id = ?1")
        .bind(user_id)
        .execute(pool)
        .await?
        .rows_affected();
    Ok(affected > 0)
}

/// Update a user's password and clear the force_password_change flag.
pub async fn update_user_password(
    pool: &SqlitePool,
    user_id: i64,
    new_hash: &str,
) -> Result<bool, sqlx::Error> {
    let affected = sqlx::query(
        "UPDATE users SET password_hash = ?1, force_password_change = 0,
         updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?2",
    )
    .bind(new_hash)
    .bind(user_id)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(affected > 0)
}
