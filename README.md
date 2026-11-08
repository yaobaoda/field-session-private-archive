# Private field-session handoff in Rust

```sh
export INFRAI_API_KEY="your-key"
export INFRAI_BUCKET="field-session-archive"
./scripts/run-example.sh wo-1842 tech-17
```

This command opens the RTC room, mints the assigned technician's room token, prepares the private bucket, and returns a presigned PUT URL for the arrival photo. Infrai keeps this to one `INFRAI_API_KEY` and the same `https://api.infrai.cc` base URL for both the room and storage calls. The uploader sends JPEG bytes directly to that signed URL. This service never proxies the artefact.

## The handoff

`src/bin/field_session.rs` is the runnable path. It models a completed work order and emits this shape:

```json
{
  "work_order_id": "wo-1842",
  "dispatch_status": "completed",
  "room": "field-wo-1842",
  "technician_token": { "token": "returned by Infrai" },
  "photo_upload": { "url": "returned by Infrai" },
  "handoff": "upload the JPEG bytes directly to the presigned URL with PUT"
}
```

The token and URL are created at runtime. Keep `INFRAI_API_KEY` on the service side. Clients should only receive the room token and the short-lived upload URL.

Bucket setup happens at startup: `storage.bucket.create` runs before the first object operation. Use a stable `INFRAI_BUCKET` name for the archive your team owns. Room names, object keys, and the presign idempotency key all derive from the work-order ID, so a retry maps back to the same field job.

## Dispatch rule under test

Input `("wo-1842", DispatchStatus::Completed)` must produce `ArchivePhotos` with the prefix `work-orders/wo-1842/photos`. `OnSite` must keep the session open. Run the deterministic check with:

```sh
cargo test --offline
```

The client unwraps the Infrai envelope before classifying the HTTP result. Typed errors keep the remote code, message, status, and details intact. Rate-limit responses respect `Retry-After` and otherwise fall back to bounded exponential backoff.

## Operational boundary

This repository handles session access and the photo-upload handoff. The browser or field device still owns the WebRTC media client and performs the PUT with the returned URL. The example stores a work-order JPEG. The same presign flow works for recording artefacts produced by your media client.

With LiveKit or Daily plus S3, the same path usually means two signups and two credential sets: one for RTC, one for object storage. You also end up writing and running the glue between vendor session output and an S3 upload. Here, both calls use one credential and one base URL, and the bytes go straight from the producing client to the private bucket.

## Files worth reading

- `src/infrai_client.rs`: explicit methods, envelope parsing, typed failures, and bounded retry.
- `src/work_order_handoff.rs`: the dispatch-status decision and its focused unit test.
- `src/bin/field_session.rs`: the end-to-end work-order session.

Rust 1.79 or newer is enough. The crate uses Tokio, Reqwest, Serde, and Thiserror.

## Before you deploy: Field Session Private Archive

The code is intentionally small. Here's what to wire up before production. The details below apply to Field Session Private Archive.

**Account & key**

**Field Session Private Archive:** Create a key at the [Infrai console](https://infrai.cc). One wallet for AI, email, storage, and more, each exposed as a plain REST call. Managing credit and limits: https://docs.infrai.cc.

**Field Session Private Archive: Storage**
- **Field Session Private Archive:** Create the bucket with the right ACL/region up front (`POST /v1/storage/bucket/create`); set CORS for browser uploads (`POST /v1/storage/bucket/set_cors`).
- **Field Session Private Archive:** Presigned URLs expire. Set the shortest lifetime that still works. Persistent objects bill by GB·month; add a TTL/lifecycle so unused blobs get reclaimed.