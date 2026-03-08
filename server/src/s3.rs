use crate::config::AppConfig;
use s3::creds::Credentials;
use s3::Bucket;
use s3::Region;
use uuid::Uuid;

/// Initialize an S3 bucket handle from app config.
/// Returns None if S3 is not configured.
pub fn get_bucket(config: &AppConfig) -> Option<Box<Bucket>> {
    let bucket_name = config.s3_bucket.as_ref()?;
    let region_str = config.s3_region.as_ref()?;
    let access_key = config.s3_access_key.as_ref()?;
    let secret_key = config.s3_secret_key.as_ref()?;

    let region = if let Some(endpoint) = &config.s3_endpoint {
        Region::Custom {
            region: region_str.clone(),
            endpoint: endpoint.clone(),
        }
    } else {
        region_str.parse::<Region>().ok()?
    };

    let credentials = Credentials::new(
        Some(access_key.as_str()),
        Some(secret_key.as_str()),
        None, // security_token
        None, // session_token
        None, // profile
    )
    .ok()?;

    let bucket = Bucket::new(bucket_name, region, credentials).ok()?;

    // RustFS / MinIO require path-style addressing (bucket in path, not subdomain)
    if config.s3_endpoint.is_some() {
        Some(bucket.with_path_style())
    } else {
        Some(bucket)
    }
}

/// Upload bytes to S3, returning the public URL.
/// The key is auto-generated: `bio-images/{uuid}.{ext}`
pub async fn upload_image(
    bucket: &Bucket,
    data: &[u8],
    content_type: &str,
    extension: &str,
) -> anyhow::Result<String> {
    let key = format!("bio-images/{}.{}", Uuid::new_v4(), extension);

    bucket
        .put_object_with_content_type(&key, data, content_type)
        .await?;

    // Construct the public URL
    let url = match &bucket.region {
        Region::Custom { endpoint, .. } => {
            format!("{}/{}/{}", endpoint, bucket.name(), key)
        }
        region => {
            format!(
                "https://{}.s3.{}.amazonaws.com/{}",
                bucket.name(),
                region,
                key
            )
        }
    };

    Ok(url)
}
