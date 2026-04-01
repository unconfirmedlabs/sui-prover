// Copyright (c) Unconfirmed Labs, LLC
// Copyright (c) Asymptotic
// SPDX-License-Identifier: Apache-2.0

//! Builds a [GlobalEnv] from a Move package using `move-package-alt` for
//! dependency resolution (MVR-compatible).

use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::Result;
use itertools::Itertools;
use move_compiler::{
    editions::{Edition, Flavor},
    shared::{PackageConfig, PackagePaths},
    Flags,
    diagnostics::warning_filters::WarningFiltersBuilder,
    shared::known_attributes::ModeAttribute,
};
use move_core_types::identifier::Identifier;
use move_model::model::GlobalEnv;
use move_model::run_model_builder_with_options_and_compilation_flags;
use move_package::source_package::layout::SourcePackageLayout;
use move_package_alt::{NamedAddress, PackageLoader, RootPackage};
use move_package_alt::schema::Environment;
use move_package_alt_compilation::source_discovery::get_sources;
use move_package_alt_compilation::build_config::BuildConfig as AltBuildConfig;
use move_symbol_pool::Symbol;

use crate::prove::BuildConfig;
use crate::prover_flavor::ProverFlavor;

/// Build a [GlobalEnv] from the Move package at the given path.
///
/// Uses `move-package-alt` for dependency resolution, which supports MVR
/// (`r.mvr = "@namespace/package"`) out of the box.
pub fn build_model(
    path: Option<&Path>,
    build_config: Option<BuildConfig>,
) -> Result<GlobalEnv> {
    let rerooted_path = reroot_path(path)?;
    let _build_config = build_config.unwrap_or_default();

    // Determine the environment. Default to testnet.
    let env = detect_environment(&rerooted_path);

    // Load the package graph using move-package-alt.
    // This resolves all dependencies including MVR.
    let loader = PackageLoader::new(&rerooted_path, env);
    let root_pkg: RootPackage<ProverFlavor> = loader.load_sync()?;

    // Build the GlobalEnv from the resolved package graph.
    build_global_env(&root_pkg)
}

/// Test-only version that skips filesystem locking.
#[doc(hidden)]
pub fn build_model_unlocked(
    path: &Path,
    _build_config: Option<BuildConfig>,
) -> Result<GlobalEnv> {
    let rerooted_path = reroot_path(Some(path))?;
    let env = detect_environment(&rerooted_path);

    let loader = PackageLoader::new(&rerooted_path, env);
    let root_pkg: RootPackage<ProverFlavor> = loader.load_sync()?;

    build_global_env(&root_pkg)
}

/// Extract [PackagePaths] from each package in the resolved graph and feed them
/// to the Move model builder to produce a [GlobalEnv].
fn build_global_env(root_pkg: &RootPackage<ProverFlavor>) -> Result<GlobalEnv> {
    let packages: Vec<move_package_alt::PackageInfo<'_, ProverFlavor>> = root_pkg.sorted_packages();

    // Build a compilation config for source discovery
    let alt_config = AltBuildConfig {
        test_mode: false,
        default_flavor: Some(Flavor::Sui),
        default_edition: Some(Edition::E2024_BETA),
        modes: vec![ModeAttribute::VERIFY_ONLY.into()],
        silence_warnings: true,
        set_unpublished_deps_to_zero: true,
        ..AltBuildConfig::default()
    };

    let mut targets: Vec<PackagePaths> = Vec::new();
    let mut deps: Vec<PackagePaths> = Vec::new();

    for pkg in packages {
        let pkg_addrs: std::collections::BTreeMap<Identifier, NamedAddress> = pkg.named_addresses()?;
        let named_addresses = alt_config.addresses_for_config(pkg_addrs);

        let config = PackageConfig {
            is_dependency: !pkg.is_root(),
            edition: pkg
                .edition()
                .or(alt_config.default_edition)
                .unwrap_or(Edition::LEGACY),
            flavor: Flavor::from_str(pkg.flavor().unwrap_or("sui"))?,
            warning_filter: WarningFiltersBuilder::new_for_source(),
        };

        let safe_name = Symbol::from(pkg.id().clone());

        let package_paths = PackagePaths {
            name: Some((safe_name, config)),
            paths: get_sources(pkg.path(), &alt_config)?,
            named_address_map: named_addresses.inner,
        };

        // debug removed
        if pkg.is_root() {
            targets.push(package_paths);
        } else {
            deps.push(package_paths);
        }
    }

    // Build the GlobalEnv using the legacy model builder.
    // This is the same function that the old pipeline used — we just feed it
    // PackagePaths directly instead of going through ResolvedGraph.
    let flags = Flags::empty()
        .set_silence_warnings(true)
        .set_modes(vec![ModeAttribute::VERIFY_ONLY.into()]);

    run_model_builder_with_options_and_compilation_flags(targets, deps, flags, None)
}

/// Detect the environment from the Move.lock or Move.toml.
/// Falls back to testnet if no environment is specified.
fn detect_environment(path: &Path) -> Environment {
    // Try to read environments from the manifest
    if let Ok(envs) = RootPackage::<ProverFlavor>::environments(path) {
        let envs: indexmap::IndexMap<String, String> = envs;
        if let Some((name, id)) = envs.into_iter().next() {
            return Environment::new(name, id);
        }
    }
    // Default to testnet
    Environment::new("testnet".to_string(), "4c78adac".to_string())
}

fn reroot_path(path: Option<&Path>) -> Result<PathBuf> {
    let path = path
        .map(Path::canonicalize)
        .unwrap_or_else(|| PathBuf::from(".").canonicalize())?;
    let rooted_path = SourcePackageLayout::try_find_root(&path)?;
    std::env::set_current_dir(rooted_path)?;
    Ok(PathBuf::from("."))
}

#[allow(dead_code)]
pub fn build_model_with_target(
    path: Option<&Path>,
) -> Result<(GlobalEnv, PathBuf, move_stackless_bytecode::function_target_pipeline::FunctionTargetsHolder)> {
    use move_stackless_bytecode::{
        function_target_pipeline::{FunctionHolderTarget, FunctionTargetsHolder},
        package_targets::PackageTargets,
    };

    let rerooted_path = reroot_path(path)?;
    let model = build_model(Some(&rerooted_path), None)?;

    if model.has_errors() {
        use codespan_reporting::diagnostic::Severity;
        use termcolor::Buffer;
        let mut error_writer = Buffer::no_color();
        model.report_diag(&mut error_writer, Severity::Error);
        let diagnostic_output = String::from_utf8_lossy(&error_writer.into_inner()).to_string();
        return Err(anyhow::anyhow!(
            "Move Model compiled with errors.\n{}",
            diagnostic_output
        ));
    }

    let package_targets = PackageTargets::new(&model, Default::default(), true, None);
    let mut targets = FunctionTargetsHolder::new(
        Default::default(),
        &package_targets,
        FunctionHolderTarget::All,
    );

    for module in model.get_modules() {
        for func_env in module.get_functions() {
            targets.add_target(&func_env);
        }
    }
    targets.resolve_loop_invariants(&model);

    Ok((model, rerooted_path, targets))
}

#[allow(dead_code)]
pub fn get_all_funs_in_topological_order<'env>(
    env: &'env GlobalEnv,
    targets: &'env move_stackless_bytecode::function_target_pipeline::FunctionTargetsHolder,
    only_targeted: bool,
) -> Vec<move_model::model::FunctionEnv<'env>> {
    use move_stackless_bytecode::function_target_pipeline::FunctionTargetPipeline;

    let mut results = vec![];
    let graph = FunctionTargetPipeline::build_call_graph(env, targets);
    let sccs = petgraph::algo::kosaraju_scc(&graph);
    sccs.iter()
        .map(|scc| scc.iter().map(|node_idx| graph[*node_idx]).collect_vec())
        .for_each(|scc| {
            for qid in &scc {
                let fenv = env.get_function(*qid);
                if !only_targeted || fenv.module_env.is_target() {
                    results.push(fenv);
                }
            }
        });

    results
}
