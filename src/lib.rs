//! Shared document workflow used by the CLI and native Windows desktop.
pub mod benchmark;
pub mod export;
pub mod extraction;
pub mod inference;
pub mod model;
pub mod storage;
pub mod update;
pub const DEFAULT_MODEL: &str = "unsloth/Qwen3.5-4B-GGUF:UD-Q4_K_XL";
pub const SCHEMA_VERSION: u32 = 1;
