use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::Response,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    models::enums::UserRole,
    repositories::user as user_repo,
    utils::jwt::decode_token,
};

#[derive(Debug, Clone)]
pub struct AuthContext {
    pub user_id: Uuid,
    pub role: UserRole,
}

#[derive(Debug, Clone)]
pub struct AllowedRoles(pub Vec<UserRole>);

impl AllowedRoles {
    pub fn new(roles: impl IntoIterator<Item = UserRole>) -> Self {
        Self(roles.into_iter().collect())
    }

    pub fn contains(&self, role: &UserRole) -> bool {
        self.0.contains(role)
    }
}

pub async fn auth_middleware(
    State(pool): State<PgPool>,
    mut req: Request,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    let token = extract_bearer_token(req.headers())?;
    let claims = decode_token(token).map_err(|e| (StatusCode::UNAUTHORIZED, e))?;

    let active = user_repo::is_user_active(&pool, claims.sub)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if !active {
        return Err((
            StatusCode::UNAUTHORIZED,
            "账号已被禁用".to_string(),
        ));
    }

    req.extensions_mut().insert(AuthContext {
        user_id: claims.sub,
        role: claims.role,
    });

    Ok(next.run(req).await)
}

pub async fn require_roles_middleware(
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    let allowed_roles = req.extensions().get::<AllowedRoles>().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "路由未声明允许角色集合".to_string(),
        )
    })?;

    let auth_ctx = req
        .extensions()
        .get::<AuthContext>()
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "未认证用户".to_string()))?;

    tracing::debug!(
        user_id = %auth_ctx.user_id,
        role = ?auth_ctx.role,
        "执行角色鉴权检查"
    );

    if !allowed_roles.contains(&auth_ctx.role) {
        return Err((StatusCode::FORBIDDEN, "无权限访问该资源".to_string()));
    }

    Ok(next.run(req).await)
}

/// 从请求头解析可选的 Bearer Token（用于公开接口上「带 Token 则按身份放行」的场景）
pub fn try_optional_auth_context(headers: &HeaderMap) -> Option<AuthContext> {
    let auth_header = headers.get(AUTHORIZATION)?.to_str().ok()?;
    let token = auth_header.strip_prefix("Bearer ")?.trim();
    if token.is_empty() {
        return None;
    }
    let claims = decode_token(token).ok()?;
    Some(AuthContext {
        user_id: claims.sub,
        role: claims.role,
    })
}

/// 与 [`try_optional_auth_context`] 相同，但会查库确认账号仍为启用状态（禁用账号的 JWT 视为未登录）
pub async fn try_optional_auth_context_active(
    pool: &PgPool,
    headers: &HeaderMap,
) -> Result<Option<AuthContext>, sqlx::Error> {
    let Some(ctx) = try_optional_auth_context(headers) else {
        return Ok(None);
    };
    if user_repo::is_user_active(pool, ctx.user_id).await? {
        Ok(Some(ctx))
    } else {
        Ok(None)
    }
}

fn extract_bearer_token(headers: &HeaderMap) -> Result<&str, (StatusCode, String)> {
    let auth_header = headers.get(AUTHORIZATION).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            "缺少 Authorization 头".to_string(),
        )
    })?;

    let auth_str = auth_header.to_str().map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            "Authorization 头格式无效".to_string(),
        )
    })?;

    let token = auth_str.strip_prefix("Bearer ").ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            "Authorization 必须是 Bearer Token".to_string(),
        )
    })?;

    if token.is_empty() {
        return Err((StatusCode::UNAUTHORIZED, "Token 不能为空".to_string()));
    }

    Ok(token)
}
