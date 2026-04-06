use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::enums::UserRole;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub role: UserRole,
    pub exp: usize,
}

/// HLS 播放专用 Token 的 Claims
/// 仅包含视频 ID、清晰度和过期时间，不包含用户身份信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HlsClaims {
    /// video_id
    pub vid: Uuid,
    /// 清晰度：1080p / 720p / 480p / 360p
    pub res: String,
    /// 过期时间戳（秒，UNIX 时间）
    pub exp: usize,
}

fn jwt_secret() -> Result<String, String> {
    std::env::var("JWT_SECRET").map_err(|_| "JWT_SECRET 环境变量未设置".to_string())
}

fn jwt_expiration_seconds() -> Result<usize, String> {
    let raw = std::env::var("JWT_EXPIRATION").map_err(|_| "JWT_EXPIRATION 环境变量未设置".to_string())?;
    raw.parse::<usize>()
        .map_err(|_| "JWT_EXPIRATION 不是有效数字".to_string())
}

pub fn encode_token(user_id: Uuid, role: &UserRole) -> Result<(String, usize), String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "系统时间异常".to_string())?
        .as_secs() as usize;
    let exp = now + jwt_expiration_seconds()?;
    let claims = Claims {
        sub: user_id,
        role: role.clone(),
        exp,
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret()?.as_bytes()),
    )
    .map_err(|e| e.to_string())?;
    Ok((token, exp))
}

pub fn decode_token(token: &str) -> Result<Claims, String> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(jwt_secret()?.as_bytes()),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|_| "无效或已过期的 token".to_string())
}

/// 为指定视频和清晰度生成一个短效 HLS 播放 Token（URL 签名）。
/// `ttl_secs` 控制 token 有效期（秒），建议 5~15 分钟。
pub fn encode_hls_token(
    video_id: Uuid,
    resolution: &str,
    ttl_secs: usize,
) -> Result<(String, usize), String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "系统时间异常".to_string())?
        .as_secs() as usize;
    let exp = now + ttl_secs;
    let claims = HlsClaims {
        vid: video_id,
        res: resolution.to_string(),
        exp,
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret()?.as_bytes()),
    )
    .map_err(|e| e.to_string())?;
    Ok((token, exp))
}

/// 解析并验证 HLS 播放 Token。
pub fn decode_hls_token(token: &str) -> Result<HlsClaims, String> {
    decode::<HlsClaims>(
        token,
        &DecodingKey::from_secret(jwt_secret()?.as_bytes()),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|_| "无效或已过期的播放链接".to_string())
}
