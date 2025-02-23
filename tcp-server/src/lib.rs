#![cfg(target_family = "unix")]
pub mod core;
pub mod service;
pub mod thread_pool;
pub mod proto {
    include!(concat!(env!("GENERATED_PROTO_DIR"), "/file_transfer.rs"));

    pub mod prelude {
        pub use super::{FileQuery, FileResponse, TransferAck, FileChunk};
        pub use super::file_response::{Response, FileMetadata, ErrorDetails};
        pub use super::file_response::file_metadata::Status;
        pub use super::file_response::error_details::Kind;
        pub use super::transfer_ack::AckStatus;
    }
}
