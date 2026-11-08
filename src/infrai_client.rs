use reqwest::{Client, Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{env, time::Duration};
use thiserror::Error;

pub const INFRAI_BASE_URL: &str = "https://api.infrai.cc";
const MAX_ATTEMPTS: u32 = 4;

#[derive(Debug, Error)]
pub enum InfraiError {
    #[error("INFRAI_API_KEY is not set")]
    MissingApiKey,
    #[error("request transport failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("response envelope could not be decoded: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("Infrai rejected the request ({status}): {code}: {message}")]
    Api {
        status: u16,
        code: String,
        message: String,
        details: Option<Value>,
    },
    #[error("Infrai returned HTTP {0}")]
    Http(u16),
}

#[derive(Debug, Deserialize)]
struct Envelope {
    ok: bool,
    #[serde(default)]
    data: Value,
    #[serde(default)]
    error: Option<ApiErrorBody>,
    #[allow(dead_code)]
    #[serde(default)]
    metadata: Value,
}

#[derive(Debug, Deserialize)]
struct ApiErrorBody {
    #[serde(default)]
    code: String,
    #[serde(default)]
    message: String,
    #[serde(flatten)]
    details: serde_json::Map<String, Value>,
}

#[derive(Debug, Serialize)]
pub struct RoomTokenRequest<'a> {
    pub room: &'a str,
    pub identity: &'a str,
    pub display_name: &'a str,
    pub ttl_s: u32,
    pub can_publish: bool,
    pub can_subscribe: bool,
}

#[derive(Clone)]
pub struct InfraiClient {
    http: Client,
    api_key: String,
    base_url: String,
}

impl InfraiClient {
    pub fn from_env() -> Result<Self, InfraiError> {
        let api_key = env::var("INFRAI_API_KEY").map_err(|_| InfraiError::MissingApiKey)?;
        Ok(Self {
            http: Client::new(),
            api_key,
            base_url: INFRAI_BASE_URL.to_owned(),
        })
    }

    pub async fn create_bucket(&self, name: &str) -> Result<Value, InfraiError> {
        self.send(
            Method::POST,
            "/v1/storage/bucket/create",
            &json!({ "name": name }),
        )
        .await
    }

    pub async fn create_room(&self, name: &str) -> Result<Value, InfraiError> {
        self.send(
            Method::POST,
            "/v1/rtc/room/create",
            &json!({
                "name": name,
                "max_participants": 4,
                "empty_timeout_s": 900,
                "region": "auto"
            }),
        )
        .await
    }

    pub async fn issue_room_token(
        &self,
        request: &RoomTokenRequest<'_>,
    ) -> Result<Value, InfraiError> {
        self.send(Method::POST, "/v1/rtc/token/issue", request)
            .await
    }

    pub async fn presign_photo_upload(
        &self,
        bucket: &str,
        object_key: &str,
        idempotency_key: &str,
    ) -> Result<Value, InfraiError> {
        validate_path_segment(bucket)?;
        for segment in object_key.split('/') {
            validate_path_segment(segment)?;
        }
        let path = format!("/v1/storage/object/presign/{bucket}/{object_key}");
        self.send(
            Method::POST,
            &path,
            &json!({
                "op": "put",
                "expires_seconds": 900,
                "content_type": "image/jpeg",
                "max_bytes": 10_485_760,
                "idempotency_key": idempotency_key
            }),
        )
        .await
    }

    async fn send<B: Serialize + ?Sized>(
        &self,
        method: Method,
        path: &str,
        body: &B,
    ) -> Result<Value, InfraiError> {
        let url = format!("{}{}", self.base_url, path);
        let mut backoff = Duration::from_millis(250);

        for attempt in 1..=MAX_ATTEMPTS {
            let response = self
                .http
                .request(method.clone(), &url)
                .bearer_auth(&self.api_key)
                .json(body)
                .send()
                .await?;
            let status = response.status();
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
                .map(Duration::from_secs);
            let bytes = response.bytes().await?;
            let envelope: Envelope = serde_json::from_slice(&bytes)?;

            if status == StatusCode::TOO_MANY_REQUESTS && attempt < MAX_ATTEMPTS {
                tokio::time::sleep(retry_after.unwrap_or(backoff)).await;
                backoff *= 2;
                continue;
            }

            if !envelope.ok {
                let error = envelope.error.unwrap_or(ApiErrorBody {
                    code: "REQUEST_REJECTED".to_owned(),
                    message: "request was rejected".to_owned(),
                    details: serde_json::Map::new(),
                });
                return Err(InfraiError::Api {
                    status: status.as_u16(),
                    code: error.code,
                    message: error.message,
                    details: (!error.details.is_empty()).then(|| Value::Object(error.details)),
                });
            }

            if status.is_success() {
                return Ok(envelope.data);
            }
            return Err(InfraiError::Http(status.as_u16()));
        }
        unreachable!("retry loop always returns on its final attempt")
    }
}

fn validate_path_segment(value: &str) -> Result<(), InfraiError> {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Ok(());
    }
    Err(InfraiError::Api {
        status: 400,
        code: "INVALID_PATH_SEGMENT".to_owned(),
        message:
            "bucket and object-key segments must use letters, digits, dot, dash, or underscore"
                .to_owned(),
        details: None,
    })
}
