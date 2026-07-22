//! Memory unification module.
//!
//! ## Phase 1 (complete)
//!   - `provider_registry` — llm-providers.json fetch/cache/fallback.
//!   - `shared_root` — canonical path `~/.agent-memory/`.
//!   - `materializer` — `.md` → `memory-<slug>` skill unit.
//!   - `sync` — direct-write deployer to every installed agent.
//!
//! ## Unified write and migration layer
//!   - `adapters::symlink` — file-link adapter with safe copy fallback.
//!   - `bridge` — universal read/write contract distributed to every agent.
//!   - `sources` — native memory discovery, deduplication, backup and reconcile.
//!   - `store` — canonical-memory writes used by CLI and GUI.

pub mod adapters;
pub mod bridge;
pub mod materializer;
pub mod provider_registry;
pub mod shared_root;
pub mod sources;
pub mod store;
pub mod sync;

pub use adapters::MemoryAdapter;
pub use provider_registry::{
    load, ranked, LoadResult, LoadSource, Provider, ProviderRegistry, DEFAULT_PROVIDERS_URL,
};
