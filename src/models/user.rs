use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::enums::UserRole;

/// 用户表对应的完整结构体（含密码哈希，仅供服务端内部使用）
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub password_hash: String,
    pub role: UserRole,
    pub real_name: Option<String>,
    pub avatar_url: Option<String>,
    pub is_active: bool,
    /// 所属专业（学生）；教师/管理员通常为 None
    pub major_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 对外安全暴露的用户信息（去掉密码哈希）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserProfile {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub role: UserRole,
    pub real_name: Option<String>,
    pub avatar_url: Option<String>,
    pub is_active: bool,
    /// 所属专业（学生）；教师/管理员通常为 None
    pub major_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 公开注册接口的请求体（角色固定为 Student，不对外暴露 role 字段）
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: Option<String>,
    pub password: String,
    pub real_name: Option<String>,
}

/// 管理员创建用户时的请求体（可指定任意角色）
#[derive(Debug, Deserialize)]
pub struct CreateUser {
    pub username: String,
    pub email: Option<String>,
    pub password: String,
    pub role: UserRole,
    pub real_name: Option<String>,
}

/// 更新用户信息时的输入结构体
#[derive(Debug, Deserialize)]
pub struct UpdateUser {
    pub email: Option<String>,
    pub real_name: Option<String>,
    pub avatar_url: Option<String>,
    pub is_active: Option<bool>,
    /// 学生可自选所属专业；管理员可设置任意用户；教师/管理员角色留 None
    pub major_id: Option<Uuid>,
    /// 仅管理员可修改角色
    pub role: Option<UserRole>,
}

// 分页类型见 models::pagination::{PageQuery, PagedList}
