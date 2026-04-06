use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 游标分页查询参数（用户、专业等通用）
#[derive(Debug, Clone, Deserialize)]
pub struct PageQuery {
    /// 每页条数，默认为 20，最大 100
    #[serde(default = "default_page_size")]
    pub page_size: i64,
    /// 上一页最后一条记录的创建时间（RFC3339）
    pub cursor_created_at: Option<DateTime<Utc>>,
    /// 上一页最后一条记录的 id（与 cursor_created_at 一起使用）
    pub cursor_id: Option<Uuid>,
}

fn default_page_size() -> i64 {
    20
}

impl PageQuery {
    pub fn page_size(&self) -> i64 {
        self.page_size.clamp(1, 100)
    }
}

/// 游标分页响应（用户、专业等通用）
#[derive(Debug, Serialize)]
pub struct PagedList<T: Serialize> {
    /// 每页条数
    pub page_size: i64,
    /// 是否还有下一页
    pub has_more: bool,
    /// 下一页游标：最后一条记录的 created_at
    pub next_cursor_created_at: Option<DateTime<Utc>>,
    /// 下一页游标：最后一条记录的 id
    pub next_cursor_id: Option<Uuid>,
    /// 当前页数据
    pub items: Vec<T>,
}
