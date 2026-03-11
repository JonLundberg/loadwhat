#[cfg(windows)]
#[path = "harness/mod.rs"]
mod harness;

#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/dynamic_loader_snaps_contract.rs"]
mod dynamic_loader_snaps_contract;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/dynamic_loadlibrary_fullpath.rs"]
mod dynamic_loadlibrary_fullpath;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/dynamic_loadlibrary_name.rs"]
mod dynamic_loadlibrary_name;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/dynamic_missing_direct.rs"]
mod dynamic_missing_direct;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/dynamic_multiple_candidates.rs"]
mod dynamic_multiple_candidates;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/dynamic_nested_loadlibrary.rs"]
mod dynamic_nested_loadlibrary;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/imports_transitive_missing.rs"]
mod imports_transitive_missing;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/path_search_order.rs"]
mod path_search_order;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/run_cli_contract.rs"]
mod run_cli_contract;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/run_output_modes.rs"]
mod run_output_modes;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/static_missing_direct.rs"]
mod static_missing_direct;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/static_missing_transitive.rs"]
mod static_missing_transitive;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/static_wrong_pick.rs"]
mod static_wrong_pick;
