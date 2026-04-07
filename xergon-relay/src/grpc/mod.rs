//! gRPC-like transport layer.
//!
//! Provides binary protobuf-encoded inference and embedding endpoints over HTTP,
//! using gRPC wire framing (length-prefixed messages) without requiring a full
//! tonic runtime. This is useful for high-performance provider-to-relay communication
//! where the JSON serialization overhead of the REST API is undesirable.

pub mod proto;
pub mod service;

pub use proto::GrpcStatusCode;

use axum::Router;
use crate::AppState;

/// Build and return the gRPC transport router.
pub fn build_grpc_router() -> Router<AppState> {
    service::build_grpc_router()
}
