use crate::{
    auth::AdminUser,
    db_users,
    models::User,
    password,
    AppState,
};
use askama::Template;
use axum::{
    extract::{Form, Path, State},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::{
    cookie::{Cookie, SameSite},
    CookieJar,
};
use serde::Deserialize;
use std::sync::Arc;

// ── Template ──────────────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "users.html")]
struct UsersTemplate {
    users: Vec<User>,
    flash_success: Option<String>,
    flash_error: Option<String>,
    is_admin: bool,
    app_title: String,
}

#[derive(Template)]
#[template(path = "edit_user.html")]
struct EditUserTemplate {
    user: User,
    is_self: bool,
    flash_success: Option<String>,
    flash_error: Option<String>,
    is_admin: bool,
    app_title: String,
}

// ── Form types ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct RoleForm {
    role: String,
}

#[derive(Deserialize)]
pub struct CreateUserForm {
    email: String,
    display_name: String,
    password: String,
    role: Option<String>,
    is_approved: Option<String>,
    force_password_change: Option<String>,
}

#[derive(Deserialize)]
pub struct EditUserForm {
    email: String,
    display_name: String,
    role: Option<String>,
    is_approved: Option<String>,
    force_password_change: Option<String>,
    new_password: Option<String>,
    new_password_confirm: Option<String>,
}

// ── Handlers ──────────────────────────────────────────────────────────────

/// GET /admin/users
pub async fn list_users(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> Response {
    let flash_success = jar.get("flash_success").map(|c| c.value().to_owned());
    let flash_error = jar.get("flash_error").map(|c| c.value().to_owned());

    let clear_success = Cookie::build(("flash_success", ""))
        .path("/")
        .max_age(time::Duration::seconds(0))
        .build();
    let clear_error = Cookie::build(("flash_error", ""))
        .path("/")
        .max_age(time::Duration::seconds(0))
        .build();

    let users = match db_users::get_all_users(&state.db).await {
        Ok(u) => u,
        Err(e) => {
            tracing::error!("Failed to load users: {:?}", e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load users",
            )
                .into_response();
        }
    };

    let tmpl = UsersTemplate {
        users,
        flash_success,
        flash_error,
        is_admin: true,
        app_title: state.config.app_title.clone(),
    };

    (jar.remove(clear_success).remove(clear_error), tmpl).into_response()
}

/// POST /admin/users — Admin creates a new user
pub async fn create_user(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Form(form): Form<CreateUserForm>,
) -> Response {
    let email = form.email.trim().to_lowercase();
    let display_name = form.display_name.trim().to_string();

    // Validation
    if email.is_empty() || !email.contains('@') {
        return set_flash_and_redirect(
            jar,
            None,
            Some("Please enter a valid email address."),
            "/admin/users",
        );
    }
    if display_name.is_empty() {
        return set_flash_and_redirect(
            jar,
            None,
            Some("Display name is required."),
            "/admin/users",
        );
    }
    if form.password.len() < 8 {
        return set_flash_and_redirect(
            jar,
            None,
            Some("Password must be at least 8 characters."),
            "/admin/users",
        );
    }

    // Check if email already exists
    match db_users::get_user_by_email(&state.db, &email).await {
        Ok(Some(_)) => {
            return set_flash_and_redirect(
                jar,
                None,
                Some("An account with that email already exists."),
                "/admin/users",
            );
        }
        Err(e) => {
            tracing::error!("DB error checking email: {:?}", e);
            return set_flash_and_redirect(
                jar,
                None,
                Some("Internal error. Please try again."),
                "/admin/users",
            );
        }
        Ok(None) => {}
    }

    // Hash password
    let pass = form.password.clone();
    let hash = match tokio::task::spawn_blocking(move || password::hash_password(&pass)).await {
        Ok(Ok(h)) => h,
        _ => {
            return set_flash_and_redirect(
                jar,
                None,
                Some("Internal error hashing password."),
                "/admin/users",
            );
        }
    };

    let role = match form.role.as_deref() {
        Some("admin") => "admin",
        _ => "user",
    };
    let is_approved = form.is_approved.as_deref() == Some("on");
    let force_password_change = form.force_password_change.as_deref() == Some("on");

    match db_users::create_user(
        &state.db,
        &email,
        &display_name,
        &hash,
        role,
        is_approved,
        force_password_change,
    )
    .await
    {
        Ok(_user) => set_flash_and_redirect(
            jar,
            Some(&format!("User '{}' created.", email)),
            None,
            "/admin/users",
        ),
        Err(e) => {
            tracing::error!("Failed to create user: {:?}", e);
            let msg = if e.to_string().contains("UNIQUE") {
                "An account with that email already exists."
            } else {
                "Failed to create user."
            };
            set_flash_and_redirect(jar, None, Some(msg), "/admin/users")
        }
    }
}

/// POST /admin/users/:id/approve
pub async fn approve_user(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(id): Path<i64>,
) -> Response {
    match db_users::approve_user(&state.db, id).await {
        Ok(true) => set_flash_and_redirect(jar, Some("User approved."), None, "/admin/users"),
        Ok(false) => set_flash_and_redirect(jar, None, Some("User not found."), "/admin/users"),
        Err(e) => {
            tracing::error!("Failed to approve user {}: {:?}", id, e);
            set_flash_and_redirect(jar, None, Some("Failed to approve user."), "/admin/users")
        }
    }
}

/// POST /admin/users/:id/role
pub async fn change_role(
    admin: AdminUser,
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(id): Path<i64>,
    Form(form): Form<RoleForm>,
) -> Response {
    // Prevent admin from demoting themselves
    if id == admin.user_id {
        return set_flash_and_redirect(
            jar,
            None,
            Some("You cannot change your own role."),
            "/admin/users",
        );
    }

    let role = match form.role.as_str() {
        "admin" | "user" => &form.role,
        _ => {
            return set_flash_and_redirect(jar, None, Some("Invalid role."), "/admin/users");
        }
    };

    match db_users::set_user_role(&state.db, id, role).await {
        Ok(true) => set_flash_and_redirect(
            jar,
            Some(&format!("User role changed to '{}'.", role)),
            None,
            "/admin/users",
        ),
        Ok(false) => set_flash_and_redirect(jar, None, Some("User not found."), "/admin/users"),
        Err(e) => {
            tracing::error!("Failed to change role for user {}: {:?}", id, e);
            set_flash_and_redirect(jar, None, Some("Failed to change role."), "/admin/users")
        }
    }
}

/// POST /admin/users/:id/delete
pub async fn delete_user(
    admin: AdminUser,
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(id): Path<i64>,
) -> Response {
    // Prevent admin from deleting themselves
    if id == admin.user_id {
        return set_flash_and_redirect(
            jar,
            None,
            Some("You cannot delete your own account."),
            "/admin/users",
        );
    }

    match db_users::delete_user(&state.db, id).await {
        Ok(true) => set_flash_and_redirect(jar, Some("User deleted."), None, "/admin/users"),
        Ok(false) => set_flash_and_redirect(jar, None, Some("User not found."), "/admin/users"),
        Err(e) => {
            tracing::error!("Failed to delete user {}: {:?}", id, e);
            set_flash_and_redirect(jar, None, Some("Failed to delete user."), "/admin/users")
        }
    }
}

// ── Edit user (admin) ────────────────────────────────────────────────────

/// GET /admin/users/:id/edit
pub async fn edit_user_page(
    admin: AdminUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    jar: CookieJar,
) -> Response {
    let flash_success = jar.get("flash_success").map(|c| c.value().to_owned());
    let flash_error = jar.get("flash_error").map(|c| c.value().to_owned());

    let clear_success = Cookie::build(("flash_success", ""))
        .path("/")
        .max_age(time::Duration::seconds(0))
        .build();
    let clear_error = Cookie::build(("flash_error", ""))
        .path("/")
        .max_age(time::Duration::seconds(0))
        .build();

    let user = match db_users::get_user_by_id(&state.db, id).await {
        Ok(Some(u)) => u,
        Ok(None) => return set_flash_and_redirect(jar, None, Some("User not found."), "/admin/users"),
        Err(e) => {
            tracing::error!("Failed to load user {}: {:?}", id, e);
            return set_flash_and_redirect(jar, None, Some("Failed to load user."), "/admin/users");
        }
    };

    let tmpl = EditUserTemplate {
        is_self: user.id == admin.user_id,
        user,
        flash_success,
        flash_error,
        is_admin: true,
        app_title: state.config.app_title.clone(),
    };

    (jar.remove(clear_success).remove(clear_error), tmpl).into_response()
}

/// POST /admin/users/:id/edit
pub async fn edit_user(
    admin: AdminUser,
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(id): Path<i64>,
    Form(form): Form<EditUserForm>,
) -> Response {
    let redirect_to = format!("/admin/users/{}/edit", id);
    let email = form.email.trim().to_lowercase();
    let display_name = form.display_name.trim().to_string();

    // Validate
    if email.is_empty() || !email.contains('@') {
        return set_flash_and_redirect(jar, None, Some("Please enter a valid email address."), &redirect_to);
    }
    if display_name.is_empty() {
        return set_flash_and_redirect(jar, None, Some("Display name is required."), &redirect_to);
    }

    // Load current user to check for email change
    let current_user = match db_users::get_user_by_id(&state.db, id).await {
        Ok(Some(u)) => u,
        Ok(None) => return set_flash_and_redirect(jar, None, Some("User not found."), "/admin/users"),
        Err(e) => {
            tracing::error!("Failed to load user {}: {:?}", id, e);
            return set_flash_and_redirect(jar, None, Some("Internal error."), &redirect_to);
        }
    };

    // Check for duplicate email if changed
    if email != current_user.email {
        match db_users::get_user_by_email(&state.db, &email).await {
            Ok(Some(_)) => {
                return set_flash_and_redirect(jar, None, Some("That email is already in use by another account."), &redirect_to);
            }
            Err(e) => {
                tracing::error!("DB error checking email: {:?}", e);
                return set_flash_and_redirect(jar, None, Some("Internal error."), &redirect_to);
            }
            Ok(None) => {}
        }
    }

    // Determine role — prevent admin from changing their own role
    let role = if id == admin.user_id {
        current_user.role.clone()
    } else {
        match form.role.as_deref() {
            Some("admin") => "admin".to_string(),
            _ => "user".to_string(),
        }
    };

    let is_approved = form.is_approved.as_deref() == Some("on");
    let force_password_change = form.force_password_change.as_deref() == Some("on");

    // Update all fields
    if let Err(e) = db_users::update_user_full(
        &state.db, id, &email, &display_name, &role, is_approved, force_password_change,
    ).await {
        tracing::error!("Failed to update user {}: {:?}", id, e);
        let msg = if e.to_string().contains("UNIQUE") {
            "That email is already in use by another account."
        } else {
            "Failed to update user."
        };
        return set_flash_and_redirect(jar, None, Some(msg), &redirect_to);
    }

    // Handle optional password reset
    let new_password = form.new_password.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if let Some(new_pass) = new_password {
        if new_pass.len() < 8 {
            return set_flash_and_redirect(jar, None, Some("New password must be at least 8 characters."), &redirect_to);
        }
        let confirm = form.new_password_confirm.as_deref().unwrap_or("");
        if new_pass != confirm {
            return set_flash_and_redirect(jar, None, Some("New passwords do not match."), &redirect_to);
        }

        let pass = new_pass.to_owned();
        let hash = match tokio::task::spawn_blocking(move || password::hash_password(&pass)).await {
            Ok(Ok(h)) => h,
            _ => return set_flash_and_redirect(jar, None, Some("Internal error hashing password."), &redirect_to),
        };
        if let Err(e) = db_users::update_user_password(&state.db, id, &hash).await {
            tracing::error!("Failed to reset password for user {}: {:?}", id, e);
            return set_flash_and_redirect(jar, None, Some("Failed to reset password."), &redirect_to);
        }
    }

    set_flash_and_redirect(jar, Some("User updated."), None, &redirect_to)
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn set_flash_and_redirect(
    jar: CookieJar,
    success: Option<&str>,
    error: Option<&str>,
    destination: &str,
) -> Response {
    let mut jar = jar;

    if let Some(msg) = success {
        let c = Cookie::build(("flash_success", msg.to_owned()))
            .path("/")
            .http_only(true)
            .same_site(SameSite::Lax)
            .max_age(time::Duration::seconds(30))
            .build();
        jar = jar.add(c);
    }

    if let Some(msg) = error {
        let c = Cookie::build(("flash_error", msg.to_owned()))
            .path("/")
            .http_only(true)
            .same_site(SameSite::Lax)
            .max_age(time::Duration::seconds(30))
            .build();
        jar = jar.add(c);
    }

    (jar, Redirect::to(destination)).into_response()
}
