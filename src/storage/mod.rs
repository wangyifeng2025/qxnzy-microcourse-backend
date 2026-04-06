use aws_sdk_s3::{
    Client,
    config::{BehaviorVersion, Credentials, Region},
    presigning::PresigningConfig,
};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// S3 兼容对象存储客户端封装，通过 `Arc<AppStorage>` 在各层之间共享。
///
/// 支持以下存储后端（均通过 S3 兼容 API 接入）：
/// - **MinIO**（本地开发 / 自托管）
/// - **阿里云 OSS**（S3 兼容模式）
/// - **AWS S3**（标准 S3）
///
/// ## 环境变量优先级
///
/// 新版通用变量（`S3_*`）优先于旧版 MinIO 变量（`MINIO_*`）：
///
/// | 配置项            | 优先读取          | 回退读取           | 内置默认值               |
/// |-------------------|-------------------|--------------------|--------------------------|
/// | 服务端点          | `S3_ENDPOINT`     | `MINIO_ENDPOINT`   | `http://localhost:9000`  |
/// | Access Key        | `S3_ACCESS_KEY`   | `MINIO_ACCESS_KEY` | `admin`                  |
/// | Secret Key        | `S3_SECRET_KEY`   | `MINIO_SECRET_KEY` | `12345678`               |
/// | Bucket 名称       | `S3_BUCKET`       | `MINIO_BUCKET`     | `videos`                 |
/// | Region            | `S3_REGION`       | —                  | `us-east-1`              |
/// | 路径风格寻址      | `S3_FORCE_PATH_STYLE` | —              | `true`                   |
///
/// ### 阿里云 OSS 配置示例
///
/// ```env
/// S3_ENDPOINT=https://oss-cn-hangzhou.aliyuncs.com
/// S3_ACCESS_KEY=<AccessKeyId>
/// S3_SECRET_KEY=<AccessKeySecret>
/// S3_BUCKET=my-bucket
/// S3_REGION=oss-cn-hangzhou
/// S3_FORCE_PATH_STYLE=true
/// ```
///
/// > OSS 同时支持路径风格（`S3_FORCE_PATH_STYLE=true`）和虚拟主机风格（`false`）。
/// > 路径风格更简单，无需额外 DNS 解析，推荐使用。
#[derive(Clone)]
pub struct AppStorage {
    pub client: Client,
    pub bucket: String,
}

impl AppStorage {
    /// 从环境变量初始化存储客户端。
    ///
    /// 同时兼容 `S3_*`（通用）和 `MINIO_*`（旧版）两套环境变量，方便现有部署平滑迁移。
    pub fn from_env() -> Arc<Self> {
        // 按优先级读取：S3_* > MINIO_* > 内置默认值
        let endpoint = std::env::var("S3_ENDPOINT")
            .or_else(|_| std::env::var("MINIO_ENDPOINT"))
            .unwrap_or_else(|_| "http://localhost:9000".to_string());

        let access_key = std::env::var("S3_ACCESS_KEY")
            .or_else(|_| std::env::var("MINIO_ACCESS_KEY"))
            .unwrap_or_else(|_| "admin".to_string());

        let secret_key = std::env::var("S3_SECRET_KEY")
            .or_else(|_| std::env::var("MINIO_SECRET_KEY"))
            .unwrap_or_else(|_| "12345678".to_string());

        let bucket = std::env::var("S3_BUCKET")
            .or_else(|_| std::env::var("MINIO_BUCKET"))
            .unwrap_or_else(|_| "videos".to_string());

        // OSS 区域示例：oss-cn-hangzhou；MinIO/AWS 默认：us-east-1
        let region = std::env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".to_string());

        // MinIO 必须用路径风格；OSS 两种均可，路径风格更稳定。默认 true。
        // 设置 S3_FORCE_PATH_STYLE=false 可切换为虚拟主机风格（仅 OSS / AWS 建议）。
        let force_path_style = std::env::var("S3_FORCE_PATH_STYLE")
            .map(|v| v.to_lowercase() != "false" && v != "0")
            .unwrap_or(true);

        let creds = Credentials::new(&access_key, &secret_key, None, None, "env");
        let config = aws_sdk_s3::config::Builder::new()
            .behavior_version(BehaviorVersion::latest())
            .endpoint_url(&endpoint)
            .credentials_provider(creds)
            .region(Region::new(region))
            .force_path_style(force_path_style)
            .build();

        tracing::info!(
            endpoint = %endpoint,
            bucket = %bucket,
            force_path_style = force_path_style,
            "对象存储客户端已初始化"
        );

        Arc::new(Self {
            client: Client::from_conf(config),
            bucket,
        })
    }

    /// 生成前端直传用的预签名 PUT URL
    pub async fn presigned_put_url(&self, key: &str, expires_secs: u64) -> Result<String, String> {
        let cfg = PresigningConfig::expires_in(Duration::from_secs(expires_secs))
            .map_err(|e| e.to_string())?;
        let req = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(cfg)
            .await
            .map_err(|e| e.to_string())?;
        Ok(req.uri().to_string())
    }

    /// 生成内部下载用的预签名 GET URL（Worker 下载原始文件时使用）
    pub async fn presigned_get_url(&self, key: &str, expires_secs: u64) -> Result<String, String> {
        let cfg = PresigningConfig::expires_in(Duration::from_secs(expires_secs))
            .map_err(|e| e.to_string())?;
        let req = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(cfg)
            .await
            .map_err(|e| e.to_string())?;
        Ok(req.uri().to_string())
    }

    /// 上传字节数组到指定 key
    pub async fn upload_bytes(
        &self,
        key: &str,
        data: Vec<u8>,
        content_type: &str,
    ) -> Result<(), String> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .body(data.into())
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 下载对象到字节向量
    pub async fn download_bytes(&self, key: &str) -> Result<Vec<u8>, String> {
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let bytes = resp
            .body
            .collect()
            .await
            .map_err(|e| e.to_string())?
            .into_bytes();
        Ok(bytes.to_vec())
    }

    /// 检查 key 是否存在
    pub async fn object_exists(&self, key: &str) -> bool {
        self.client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .is_ok()
    }

    /// 删除对象
    pub async fn delete_object(&self, key: &str) -> Result<(), String> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 列出指定前缀下的所有对象 key（自动处理分页）
    pub async fn list_object_keys_with_prefix(&self, prefix: &str) -> Result<Vec<String>, String> {
        let mut keys = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let mut req = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(prefix);

            if let Some(ref token) = continuation_token {
                req = req.continuation_token(token);
            }

            let resp = req.send().await.map_err(|e| e.to_string())?;

            for obj in resp.contents() {
                if let Some(k) = obj.key() {
                    keys.push(k.to_string());
                }
            }

            match resp.next_continuation_token() {
                Some(t) => continuation_token = Some(t.to_string()),
                None => break,
            }
        }

        Ok(keys)
    }

    /// 删除指定前缀下所有对象（用于删除某视频的原始文件目录、HLS 目录、封面目录等）
    pub async fn delete_objects_with_prefix(&self, prefix: &str) -> Result<(), String> {
        let keys = self.list_object_keys_with_prefix(prefix).await?;
        for key in keys {
            self.delete_object(&key).await?;
        }
        Ok(())
    }

    /// 删除与视频关联的全部存储资源：
    /// - `raw/{video_id}/` 直传原始文件
    /// - `hls/{video_id}/` 转码输出的 m3u8 与分片
    /// - `covers/{video_id}/` 封面缩略图
    pub async fn delete_video_assets(&self, video_id: Uuid) -> Result<(), String> {
        let raw_prefix = format!("raw/{video_id}/");
        let hls_prefix = format!("hls/{video_id}/");
        let covers_prefix = format!("covers/{video_id}/");

        self.delete_objects_with_prefix(&raw_prefix).await?;
        self.delete_objects_with_prefix(&hls_prefix).await?;
        self.delete_objects_with_prefix(&covers_prefix).await?;

        Ok(())
    }

    /// 检查存储桶是否可访问，并区分不同失败原因给出具体诊断信息：
    ///
    /// - HTTP 200 → 正常，直接返回 Ok
    /// - HTTP 403 / 401 → 认证失败，立即报错（不尝试创建）
    /// - HTTP 404 → bucket 不存在，尝试创建（适用于 MinIO 自建；OSS 建议提前在控制台创建）
    /// - 网络 / 连接错误 → 报 S3_ENDPOINT 配置或网络问题
    pub async fn ensure_bucket(&self) -> Result<(), String> {
        use aws_sdk_s3::error::SdkError;

        match self.client.head_bucket().bucket(&self.bucket).send().await {
            Ok(_) => {
                tracing::info!(bucket = %self.bucket, "存储桶连接正常，认证通过");
                return Ok(());
            }
            Err(SdkError::ServiceError(ref se)) => {
                let status = se.raw().status().as_u16();
                match status {
                    // 认证 / 权限失败 —— 直接报错，不再尝试创建 bucket
                    401 | 403 => {
                        return Err(format!(
                            "认证失败（HTTP {status}）：\
                             请检查 S3_ACCESS_KEY / S3_SECRET_KEY 是否正确，\
                             以及该 AccessKey 是否拥有操作 bucket '{}' 的权限。\
                             阿里云 OSS 请确认已开启 S3 兼容 API 且子账号已授权 oss:GetBucket / oss:PutObject 等权限。",
                            self.bucket
                        ));
                    }
                    // bucket 不存在 —— 仅 MinIO 等自建存储会走到这里，继续尝试创建
                    404 => {
                        tracing::info!(
                            bucket = %self.bucket,
                            "存储桶不存在（HTTP 404），尝试自动创建..."
                        );
                    }
                    _ => {
                        return Err(format!(
                            "访问存储桶失败（HTTP {status}）：\
                             请检查 S3_ENDPOINT（当前：{}）和网络连通性。原始错误：{se:?}",
                            std::env::var("S3_ENDPOINT")
                                .or_else(|_| std::env::var("MINIO_ENDPOINT"))
                                .unwrap_or_else(|_| "<未设置>".to_string())
                        ));
                    }
                }
            }
            // 非 HTTP 错误：DNS 解析失败、连接超时、TLS 握手失败等
            Err(e) => {
                return Err(format!(
                    "连接对象存储失败（可能是 S3_ENDPOINT 配置有误、网络不通或 TLS 证书问题）：{e}"
                ));
            }
        }

        // 走到这里说明 HEAD 返回了 404，尝试创建 bucket
        self.client
            .create_bucket()
            .bucket(&self.bucket)
            .send()
            .await
            .map_err(|e| match &e {
                SdkError::ServiceError(se) => {
                    let status = se.raw().status().as_u16();
                    if status == 401 || status == 403 {
                        format!(
                            "创建存储桶被拒绝（HTTP {status}）：\
                             AccessKey 无创建 bucket 权限。\
                             请在 OSS 控制台手动创建 bucket '{}'，或为 AccessKey 授予 oss:PutBucket 权限。",
                            self.bucket
                        )
                    } else {
                        format!("创建存储桶失败（HTTP {status}）：{e}")
                    }
                }
                _ => format!("创建存储桶失败：{e}"),
            })?;

        tracing::info!(bucket = %self.bucket, "存储桶已创建");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 诊断测试：cargo test test_oss -- --nocapture
// 从 .env 读取配置，逐步验证 OSS 认证、Bucket 访问、上传/下载/删除
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    /// 构建与 AppStorage::from_env() 完全相同的客户端，但打印所有配置供排查
    fn build_client_from_env() -> (Client, String) {
        let endpoint = std::env::var("S3_ENDPOINT")
            .or_else(|_| std::env::var("MINIO_ENDPOINT"))
            .unwrap_or_else(|_| "http://localhost:9000".to_string());
        let access_key = std::env::var("S3_ACCESS_KEY")
            .or_else(|_| std::env::var("MINIO_ACCESS_KEY"))
            .unwrap_or_else(|_| "admin".to_string());
        let secret_key = std::env::var("S3_SECRET_KEY")
            .or_else(|_| std::env::var("MINIO_SECRET_KEY"))
            .unwrap_or_else(|_| "12345678".to_string());
        let bucket = std::env::var("S3_BUCKET")
            .or_else(|_| std::env::var("MINIO_BUCKET"))
            .unwrap_or_else(|_| "videos".to_string());
        let region = std::env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".to_string());
        let force_path_style = std::env::var("S3_FORCE_PATH_STYLE")
            .map(|v| v.to_lowercase() != "false" && v != "0")
            .unwrap_or(true);

        println!("────────────────────────────────────────");
        println!("  OSS 认证诊断配置");
        println!("────────────────────────────────────────");
        println!("  endpoint         : {endpoint}");
        println!("  access_key       : {access_key}");
        println!(
            "  secret_key       : {}***{}",
            &secret_key[..secret_key.len().min(3)],
            &secret_key[secret_key.len().saturating_sub(3)..]
        );
        println!("  bucket           : {bucket}");
        println!("  region           : {region}");
        println!("  force_path_style : {force_path_style}");
        println!("────────────────────────────────────────\n");

        let creds = Credentials::new(&access_key, &secret_key, None, None, "env");
        let config = aws_sdk_s3::config::Builder::new()
            .behavior_version(BehaviorVersion::latest())
            .endpoint_url(&endpoint)
            .credentials_provider(creds)
            .region(Region::new(region))
            .force_path_style(force_path_style)
            .build();

        (Client::from_conf(config), bucket)
    }

    /// 全量 OSS 认证与操作诊断
    ///
    /// 运行方式：
    /// ```bash
    /// cargo test test_oss -- --nocapture
    /// ```
    #[tokio::test]
    async fn test_oss_auth_and_operations() {
        // 加载 .env（测试环境不自动加载）
        let _ = dotenvy::dotenv();

        let (client, bucket) = build_client_from_env();
        let mut all_passed = true;

        // ── Step 1：list_buckets（只验证 AK/SK，不依赖具体 bucket）──────────
        print!("[Step 1] list_buckets（验证 AK/SK 签名）... ");
        match client.list_buckets().send().await {
            Ok(resp) => {
                let names: Vec<_> = resp.buckets().iter().filter_map(|b| b.name()).collect();
                println!(
                    "✅ 认证通过，账号下共 {} 个 bucket：{:?}",
                    names.len(),
                    names
                );
            }
            Err(e) => {
                println!("❌ 失败：{e}");
                // list_buckets 在路径风格下 OSS 有时不支持，不作为硬性失败
                println!("   → list_buckets 在部分 OSS 路径风格配置下不可用，继续后续步骤");
            }
        }

        // ── Step 2：head_bucket（验证 bucket 存在且有访问权限）──────────────
        print!("[Step 2] head_bucket '{bucket}'（验证 bucket 存在 & 有权限）... ");
        match client.head_bucket().bucket(&bucket).send().await {
            Ok(_) => println!("✅ bucket 存在且认证通过"),
            Err(ref e) => {
                use aws_sdk_s3::error::SdkError;
                let msg = match e {
                    SdkError::ServiceError(se) => {
                        let status = se.raw().status().as_u16();
                        match status {
                            401 | 403 => format!(
                                "❌ HTTP {status} 认证失败 — AK/SK 错误或无 oss:GetBucket 权限"
                            ),
                            404 => format!(
                                "❌ HTTP 404 — bucket '{bucket}' 不存在，请在 OSS 控制台创建"
                            ),
                            _ => format!("❌ HTTP {status} — {e}"),
                        }
                    }
                    SdkError::DispatchFailure(_) => {
                        format!("❌ 网络连接失败 — 请检查 S3_ENDPOINT 是否可达：{e}")
                    }
                    _ => format!("❌ {e}"),
                };
                println!("{msg}");
                all_passed = false;
            }
        }

        // ── Step 3：put_object（验证写入权限）──────────────────────────────
        let test_key = "oss-auth-test/probe.txt";
        let test_body = b"OSS auth probe".to_vec();
        print!("[Step 3] put_object '{test_key}'（验证写权限）... ");
        match client
            .put_object()
            .bucket(&bucket)
            .key(test_key)
            .content_type("text/plain")
            .body(test_body.into())
            .send()
            .await
        {
            Ok(_) => println!("✅ 写入成功"),
            Err(e) => {
                println!("❌ 失败：{e}");
                all_passed = false;
            }
        }

        // ── Step 4：get_object（验证读取权限）──────────────────────────────
        print!("[Step 4] get_object '{test_key}'（验证读权限）... ");
        match client
            .get_object()
            .bucket(&bucket)
            .key(test_key)
            .send()
            .await
        {
            Ok(resp) => {
                let bytes = resp.body.collect().await.unwrap().into_bytes();
                println!("✅ 读取成功，内容：\"{}\"", String::from_utf8_lossy(&bytes));
            }
            Err(e) => {
                println!("❌ 失败：{e}");
                all_passed = false;
            }
        }

        // ── Step 5：presigned_put_url（验证预签名 URL 生成，用于前端直传）──
        print!("[Step 5] presigned PUT URL 生成（用于前端直传上传视频）... ");
        let storage = Arc::new(AppStorage {
            client: client.clone(),
            bucket: bucket.clone(),
        });
        match storage
            .presigned_put_url("raw/test-video/sample.mp4", 3600)
            .await
        {
            Ok(url) => println!(
                "✅ 预签名 URL 生成成功\n          URL 前缀：{}...",
                &url[..url.len().min(80)]
            ),
            Err(e) => {
                println!("❌ 失败：{e}");
                all_passed = false;
            }
        }

        // ── Step 6：presigned_get_url（验证预签名 GET URL，用于视频播放）───
        print!("[Step 6] presigned GET URL 生成（用于播放/下载）... ");
        match storage.presigned_get_url(test_key, 3600).await {
            Ok(url) => println!(
                "✅ 预签名 GET URL 生成成功\n          URL 前缀：{}...",
                &url[..url.len().min(80)]
            ),
            Err(e) => {
                println!("❌ 失败：{e}");
                all_passed = false;
            }
        }

        // ── Step 7：delete_object（清理测试文件）───────────────────────────
        print!("[Step 7] delete_object '{test_key}'（清理测试文件）... ");
        match client
            .delete_object()
            .bucket(&bucket)
            .key(test_key)
            .send()
            .await
        {
            Ok(_) => println!("✅ 删除成功"),
            Err(e) => println!("⚠️  删除失败（不影响功能）：{e}"),
        }

        println!("\n────────────────────────────────────────");
        if all_passed {
            println!("  🎉 全部步骤通过，OSS 认证和读写权限正常！");
        } else {
            println!("  ⚠️  部分步骤失败，请根据上方错误信息排查。");
        }
        println!("────────────────────────────────────────\n");

        assert!(
            all_passed,
            "OSS 认证/操作诊断未全部通过，请查看上方详细输出"
        );
    }
}
