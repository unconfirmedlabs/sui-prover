// Copyright (c) Unconfirmed Labs, LLC
// SPDX-License-Identifier: Apache-2.0

//! Defines the [ProverFlavor] — a lightweight [MoveFlavor] for the Sui Prover.
//!
//! This provides Sui system dependencies (MoveStdlib, Sui, SuiSystem, DeepBook) plus
//! the SuiProver package, without pulling in the full Sui SDK.

use std::{collections::BTreeMap, path::PathBuf, sync::LazyLock};

use indexmap::IndexMap;
use move_package_alt::{
    MoveFlavor,
    schema::{
        EnvironmentID, EnvironmentName, GitSha, LockfileDependencyInfo, LockfileGitDepInfo,
        OriginalID, PackageName, ParsedManifest, ReplacementDependency, SystemDepName,
    },
};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const SYSTEM_SUI_GIT_REPO: &str = "https://github.com/asymptotic-code/sui.git";
const SYSTEM_PROVER_GIT_REPO: &str = "https://github.com/asymptotic-code/sui-prover.git";
const SYSTEM_GIT_REV: &str = "321cf9102594b6ad3338b77735e1b55af92ab0ee";
const PROVER_GIT_REV: &str = "df637634586abc26e54f5f1b46bd965029407c17";

/// Well-known Sui system addresses (0x0 through 0xf, plus 0xdee9 for DeepBook).
const SYSTEM_ADDRESSES: &[u64] = &[0x0, 0x1, 0x2, 0x3, 0x5, 0x6, 0x7, 0x8, 0x9, 0xdee9];

// ---------------------------------------------------------------------------
// System package definitions
// ---------------------------------------------------------------------------

struct SystemPackage {
    /// The system dep name (lowercase, used in lockfile)
    system_name: &'static str,
    /// The legacy package name (capitalized, used in old Move.toml)
    legacy_name: &'static str,
    /// Subdir within the Sui repo
    repo_path: &'static str,
    /// Local dir name when using SUI_PROVER_FRAMEWORK_PATH
    local_dir: &'static str,
}

static SYSTEM_PACKAGES: &[SystemPackage] = &[
    SystemPackage {
        system_name: "std",
        legacy_name: "MoveStdlib",
        repo_path: "crates/sui-framework/packages/move-stdlib",
        local_dir: "move-stdlib",
    },
    SystemPackage {
        system_name: "sui",
        legacy_name: "Sui",
        repo_path: "crates/sui-framework/packages/sui-framework",
        local_dir: "sui-framework",
    },
    SystemPackage {
        system_name: "sui_system",
        legacy_name: "SuiSystem",
        repo_path: "crates/sui-framework/packages/sui-system",
        local_dir: "sui-system",
    },
];

/// Mapping from legacy (capitalized) names to system dep names.
static LEGACY_TO_SYSTEM: LazyLock<BTreeMap<String, String>> = LazyLock::new(|| {
    SYSTEM_PACKAGES
        .iter()
        .map(|p| (p.legacy_name.to_string(), p.system_name.to_string()))
        .collect()
});

// ---------------------------------------------------------------------------
// ProverFlavor
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct ProverFlavor;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ProverPublishedMetadata {}

impl MoveFlavor for ProverFlavor {
    fn name() -> String {
        "sui".to_string()
    }

    type PublishedMetadata = ProverPublishedMetadata;
    type PackageMetadata = ();
    type AddressInfo = ();

    fn default_environments() -> IndexMap<EnvironmentName, EnvironmentID> {
        IndexMap::from([
            ("testnet".to_string(), "4c78adac".to_string()),
            ("mainnet".to_string(), "35834a8a".to_string()),
        ])
    }

    fn system_deps(_environment: &EnvironmentID) -> BTreeMap<SystemDepName, LockfileDependencyInfo> {
        // System deps (MoveStdlib, Sui, SuiSystem) always come from git.
        // Only prover-specific packages use SUI_PROVER_FRAMEWORK_PATH.
        SYSTEM_PACKAGES
            .iter()
            .map(|pkg| {
                (
                    pkg.system_name.to_string(),
                    LockfileDependencyInfo::Git(LockfileGitDepInfo {
                        repo: SYSTEM_SUI_GIT_REPO.to_string(),
                        path: PathBuf::from(pkg.repo_path),
                        rev: GitSha::try_from(SYSTEM_GIT_REV.to_string())
                            .expect("valid git rev"),
                    }),
                )
            })
            .collect()
    }

    fn implicit_dependencies(
        _environment: &EnvironmentID,
    ) -> BTreeMap<PackageName, ReplacementDependency> {
        let mut deps = BTreeMap::new();

        // Implicit system deps: std and sui
        deps.insert(
            PackageName::new("sui").expect("valid identifier"),
            ReplacementDependency::override_system_dep("sui"),
        );
        deps.insert(
            PackageName::new("std").expect("valid identifier"),
            ReplacementDependency::override_system_dep("std"),
        );

        // SuiProver implicit dependency
        let local_framework_path = std::env::var("SUI_PROVER_FRAMEWORK_PATH").ok();

        if let Some(base_path) = &local_framework_path {
            for dir_name in ["sui-prover", "SuiProver", "prover"] {
                let local_path = PathBuf::from(base_path).join(dir_name);
                if local_path.exists() && local_path.join("Move.toml").exists() {
                    deps.insert(
                        PackageName::new("SuiProver").expect("valid identifier"),
                        ReplacementDependency {
                            dependency: Some(
                                move_package_alt::schema::DefaultDependency {
                                    dependency_info: move_package_alt::schema::ManifestDependencyInfo::Local(
                                        move_package_alt::schema::LocalDepInfo {
                                            local: local_path,
                                        },
                                    ),
                                    is_override: true,
                                    rename_from: None,
                                    modes: None,
                                },
                            ),
                            addresses: None,
                            use_environment: None,
                        },
                    );
                    return deps;
                }
            }
            // Local path specified but SuiProver not found — don't add git fallback
            return deps;
        }

        // Default: use git-based SuiProver.
        // The SuiProver package depends on SuiSpecs (local sibling), which requires
        // the full repo checkout. Git deps with subdirs handle this correctly.
        deps.insert(
            PackageName::new("SuiProver").expect("valid identifier"),
            ReplacementDependency {
                dependency: Some(move_package_alt::schema::DefaultDependency {
                    dependency_info: move_package_alt::schema::ManifestDependencyInfo::Git(
                        move_package_alt::schema::ManifestGitDependency {
                            repo: SYSTEM_PROVER_GIT_REPO.to_string(),
                            subdir: PathBuf::from("packages/sui-prover"),
                            rev: Some(PROVER_GIT_REV.to_string()),
                        },
                    ),
                    is_override: true,
                    rename_from: None,
                    modes: None,
                }),
                addresses: None,
                use_environment: None,
            },
        );

        // Also inject SuiSpecs as it's a transitive dep of SuiProver.
        // The package is named "SuiSpecs" in its manifest but the new system reads
        // the legacy [addresses] section key "specs" as the identifier for legacy packages.
        deps.insert(
            PackageName::new("SuiSpecs").expect("valid identifier"),
            ReplacementDependency {
                dependency: Some(move_package_alt::schema::DefaultDependency {
                    dependency_info: move_package_alt::schema::ManifestDependencyInfo::Git(
                        move_package_alt::schema::ManifestGitDependency {
                            repo: SYSTEM_PROVER_GIT_REPO.to_string(),
                            subdir: PathBuf::from("packages/sui-specs"),
                            rev: Some(PROVER_GIT_REV.to_string()),
                        },
                    ),
                    is_override: true,
                    rename_from: None,
                    modes: None,
                }),
                addresses: None,
                use_environment: None,
            },
        );

        // Also inject Prover (the core prover package).
        // Same legacy [addresses] key issue — "prover" vs "Prover".
        deps.insert(
            PackageName::new("Prover").expect("valid identifier"),
            ReplacementDependency {
                dependency: Some(move_package_alt::schema::DefaultDependency {
                    dependency_info: move_package_alt::schema::ManifestDependencyInfo::Git(
                        move_package_alt::schema::ManifestGitDependency {
                            repo: SYSTEM_PROVER_GIT_REPO.to_string(),
                            subdir: PathBuf::from("packages/prover"),
                            rev: Some(PROVER_GIT_REV.to_string()),
                        },
                    ),
                    is_override: true,
                    rename_from: None,
                    modes: None,
                }),
                addresses: None,
                use_environment: None,
            },
        );

        deps
    }

    fn validate_manifest(manifest: &ParsedManifest) -> Result<(), String> {
        // Warn if legacy system names are used in a modern manifest
        if manifest.legacy_data.is_none() {
            let dep_names: Vec<String> = manifest
                .dependencies
                .keys()
                .map(|n| n.get_ref().to_string())
                .collect();

            for name in &dep_names {
                if LEGACY_TO_SYSTEM.contains_key(name) {
                    return Err(format!(
                        "Dependency `{name}` is a legacy system name. \
                         Use the lowercase system dep name instead."
                    ));
                }
            }
        }
        Ok(())
    }

    fn is_system_address(address: &OriginalID) -> bool {
        let addr_bytes = address.0.into_bytes();
        // Check if all but the last 2 bytes are zero, and the value is in SYSTEM_ADDRESSES
        let val = u64::from_be_bytes(addr_bytes[24..32].try_into().unwrap());
        let prefix_zero = addr_bytes[..24].iter().all(|&b| b == 0);
        prefix_zero && SYSTEM_ADDRESSES.contains(&val)
    }
}
