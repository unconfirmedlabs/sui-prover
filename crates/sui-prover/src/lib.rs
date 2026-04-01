pub mod build_model;
pub mod llm_explain;
pub mod prompts;
pub mod prove;
pub mod prover_flavor;
pub mod remote_config;

// Legacy modules kept for backwards compatibility during migration.
// TODO: Remove once move-package-alt migration is fully validated.
pub mod legacy_builder;
pub mod system_dependencies;
