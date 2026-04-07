//! File upload management for the Xergon relay.
//!
//! OpenAI-compatible file endpoints:
//! - POST   /v1/files              -- Upload a file
//! - GET    /v1/files              -- List uploaded files
//! - GET    /v1/files/{file_id}    -- Get file info
//! - DELETE /v1/files/{file_id}    -- Delete a file
//! - GET    /v1/files/{file_id}/content -- Download file content

use axum::{
    body::Body,
    extract::{Multipart, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use dashmap::DashMap;
use serde::Serialize;
use std::path::{Path as StdPath, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{info, warn, debug};

use crate::proxy::{ProxyError, AppState};

// ---------------------------------------------------------------------------
// File metadata (in-memory store)
// ---------------------------------------------------------------------------

/// Metadata for an uploaded file.
#[derive(Debug, Clone, Serialize)]
pub struct FileMetadata {
    pub id: String,
    pub object: String,
    pub bytes: u64,
    pub created_at: i64,
    pub filename: String,
    pub purpose: String,
    pub status: String,
}

/// Thread-safe in-memory file metadata store.
pub struct FileStore {
    /// Map from file_id to metadata.
    files: DashMap<String, FileMetadata>,
    /// Map from file_id to disk path.
    paths: DashMap<String, PathBuf>,
    /// Configurable upload directory.
    upload_dir: PathBuf,
    /// Maximum file size in bytes (default: 100MB).
    max_file_size: u64,
}

impl FileStore {
    /// Create a new file store with the given upload directory and size limit.
    pub fn new(upload_dir: PathBuf, max_file_size: u64) -> Self {
        Self {
            files: DashMap::new(),
            paths: DashMap::new(),
            upload_dir,
            max_file_size,
        }
    }

    /// Ensure the upload directory exists.
    pub async fn ensure_dir(&self) -> Result<(), std::io::Error> {
        fs::create_dir_all(&self.upload_dir).await
    }

    /// Store a file on disk and track its metadata.
    pub async fn store(
        &self,
        filename: &str,
        data: &[u8],
        purpose: &str,
    ) -> Result<FileMetadata, String> {
        // Check size limit
        if data.len() as u64 > self.max_file_size {
            return Err(format!(
                "File size {} exceeds limit of {} bytes",
                data.len(),
                self.max_file_size
            ));
        }

        // Generate file ID
        let file_id = format!("file-{}", uuid::Uuid::new_v4());
        let disk_filename = format!("{}_{}", file_id, filename);
        let disk_path = self.upload_dir.join(&disk_filename);

        // Write file to disk
        let mut file = fs::File::create(&disk_path)
            .await
            .map_err(|e| format!("Failed to create file: {}", e))?;
        file.write_all(data)
            .await
            .map_err(|e| format!("Failed to write file: {}", e))?;

        let metadata = FileMetadata {
            id: file_id.clone(),
            object: "file".to_string(),
            bytes: data.len() as u64,
            created_at: Utc::now().timestamp(),
            filename: filename.to_string(),
            purpose: purpose.to_string(),
            status: "processed".to_string(),
        };

        self.paths.insert(file_id.clone(), disk_path);
        self.files.insert(file_id.clone(), metadata.clone());

        info!(
            file_id = %file_id,
            filename = %filename,
            bytes = data.len(),
            purpose = %purpose,
            "File uploaded"
        );

        Ok(metadata)
    }

    /// List all file metadata.
    pub fn list(&self) -> Vec<FileMetadata> {
        self.files.iter().map(|r| r.value().clone()).collect()
    }

    /// Get metadata for a specific file.
    pub fn get(&self, file_id: &str) -> Option<FileMetadata> {
        self.files.get(file_id).map(|r| r.value().clone())
    }

    /// Get the disk path for a specific file.
    fn get_path(&self, file_id: &str) -> Option<PathBuf> {
        self.paths.get(file_id).map(|r| r.value().clone())
    }

    /// Delete a file (from disk and metadata).
    pub async fn delete(&self, file_id: &str) -> bool {
        let removed_meta = self.files.remove(file_id);
        let removed_path = self.paths.remove(file_id);

        if let Some((_, path)) = removed_path {
            // Best-effort delete from disk
            if let Err(e) = fs::remove_file(&path).await {
                warn!(path = %path.display(), error = %e, "Failed to delete file from disk");
            }
        }

        removed_meta.is_some()
    }

    /// Read file content from disk.
    pub async fn read_content(&self, file_id: &str) -> Option<Vec<u8>> {
        let path = self.get_path(file_id)?;
        fs::read(&path).await.ok()
    }

    /// Return the number of stored files.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Return true if no files are stored.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Helper: extract the public key from request headers (same logic as rate_limit).
fn extract_public_key(headers: &HeaderMap) -> String {
    headers
        .get("x-xergon-public-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "anonymous".to_string())
}

/// POST /v1/files -- Upload a file.
pub async fn upload_file_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Response, ProxyError> {
    let _public_key = extract_public_key(&headers);

    let mut file_data: Option<Vec<u8>> = None;
    let mut filename = "untitled".to_string();
    let mut purpose = "fine-tune".to_string();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ProxyError::Validation(format!("Multipart error: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                filename = field.file_name().unwrap_or("untitled").to_string();
                file_data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| ProxyError::Validation(format!("Failed to read file: {}", e)))?
                        .to_vec(),
                );
            }
            "purpose" => {
                purpose = field
                    .text()
                    .await
                    .unwrap_or_else(|_| "fine-tune".to_string());
            }
            _ => {}
        }
    }

    let data = file_data.ok_or_else(|| {
        ProxyError::Validation("Missing required field: file".to_string())
    })?;

    // Validate purpose
    match purpose.as_str() {
        "fine-tune" | "assistants" | "batch" => {}
        _ => {
            return Err(ProxyError::Validation(
                "purpose must be one of: fine-tune, assistants, batch".to_string(),
            ));
        }
    }

    let file_store = &state.file_store;
    file_store
        .ensure_dir()
        .await
        .map_err(|e| ProxyError::Validation(format!("Upload directory error: {}", e)))?;

    let metadata = file_store
        .store(&filename, &data, &purpose)
        .await
        .map_err(|e| ProxyError::Validation(e))?;

    let body = serde_json::to_string(&metadata).unwrap();
    let response = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(body))
        .unwrap();

    Ok(response)
}

/// GET /v1/files -- List all uploaded files.
pub async fn list_files_handler(
    State(state): State<AppState>,
) -> Response {
    let files = state.file_store.list();
    let body = serde_json::json!({
        "object": "list",
        "data": files,
    });
    let response = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
    response
}

/// GET /v1/files/:file_id -- Get file metadata.
pub async fn get_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    match state.file_store.get(&file_id) {
        Some(metadata) => {
            let body = serde_json::to_string(&metadata).unwrap();
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap()
        }
        None => error_response(StatusCode::NOT_FOUND, "File not found"),
    }
}

/// DELETE /v1/files/:file_id -- Delete a file.
pub async fn delete_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    if state.file_store.delete(&file_id).await {
        let body = serde_json::json!({
            "id": file_id,
            "object": "file",
            "deleted": true,
        });
        Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap()
    } else {
        error_response(StatusCode::NOT_FOUND, "File not found")
    }
}

/// GET /v1/files/:file_id/content -- Download file content.
pub async fn get_file_content_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    match state.file_store.read_content(&file_id).await {
        Some(data) => {
            // Determine content type from metadata
            let content_type = state
                .file_store
                .get(&file_id)
                .and_then(|m| guess_content_type(&m.filename))
                .unwrap_or("application/octet-stream");

            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", content_type)
                .body(Body::from(data))
                .unwrap()
        }
        None => error_response(StatusCode::NOT_FOUND, "File not found"),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn error_response(status: StatusCode, message: &str) -> Response {
    let body = serde_json::json!({
        "error": {
            "message": message,
            "type": "invalid_request_error",
        }
    });
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap()
}

/// Simple content-type guess from file extension.
fn guess_content_type(filename: &str) -> Option<&'static str> {
    let ext = StdPath::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match ext.to_lowercase().as_str() {
        "json" => Some("application/json"),
        "jsonl" => Some("application/jsonl"),
        "csv" => Some("text/csv"),
        "txt" => Some("text/plain"),
        "pdf" => Some("application/pdf"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "mp3" => Some("audio/mpeg"),
        "wav" => Some("audio/wav"),
        "mp4" => Some("video/mp4"),
        "tsv" => Some("text/tab-separated-values"),
        _ => Some("application/octet-stream"),
    }
}
