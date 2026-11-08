use field_session_archive::{
    infrai_client::{InfraiClient, InfraiError, RoomTokenRequest},
    work_order_handoff::{decide_follow_up, DispatchStatus, FollowUp},
};
use serde_json::{json, Value};
use std::{env, process::ExitCode};
use thiserror::Error;

#[derive(Debug, Error)]
enum ServiceError {
    #[error(transparent)]
    Infrai(#[from] InfraiError),
    #[error("missing response field: {0}")]
    MissingField(&'static str),
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(output) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&output).expect("JSON output")
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("field session failed: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<Value, ServiceError> {
    let work_order_id = env::args().nth(1).unwrap_or_else(|| "wo-1842".to_owned());
    let technician_id = env::args().nth(2).unwrap_or_else(|| "tech-17".to_owned());
    let bucket = env::var("INFRAI_BUCKET").unwrap_or_else(|_| "field-session-archive".to_owned());
    let room = format!("field-{work_order_id}");
    let client = InfraiClient::from_env()?;

    client.create_bucket(&bucket).await?;
    client.create_room(&room).await?;
    let token = client
        .issue_room_token(&RoomTokenRequest {
            room: &room,
            identity: &technician_id,
            display_name: "Assigned technician",
            ttl_s: 3600,
            can_publish: true,
            can_subscribe: true,
        })
        .await?;

    let handoff = decide_follow_up(&work_order_id, DispatchStatus::Completed);
    let upload = match handoff {
        FollowUp::ArchivePhotos { object_prefix } => {
            let object_key = format!("{object_prefix}/arrival.jpg");
            let idempotency_key = format!("{work_order_id}-arrival-photo");
            client
                .presign_photo_upload(&bucket, &object_key, &idempotency_key)
                .await?
        }
        FollowUp::KeepSessionOpen => return Err(ServiceError::MissingField("archive handoff")),
    };

    Ok(json!({
        "work_order_id": work_order_id,
        "dispatch_status": "completed",
        "room": room,
        "technician_token": token,
        "photo_upload": upload,
        "handoff": "upload the JPEG bytes directly to the presigned URL with PUT"
    }))
}
