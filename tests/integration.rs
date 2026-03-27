#[cfg(windows)]
#[path = "harness/mod.rs"]
mod harness;

#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/cli_validation_edge_cases.rs"]
mod cli_validation_edge_cases;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/dynamic_all_later_loaded.rs"]
mod dynamic_all_later_loaded;
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
#[path = "integration/imports_bad_image.rs"]
mod imports_bad_image;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/imports_edge_cases.rs"]
mod imports_edge_cases;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/imports_on_dll.rs"]
mod imports_on_dll;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/imports_stability.rs"]
mod imports_stability;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/imports_transitive_missing.rs"]
mod imports_transitive_missing;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/loader_snaps_note_contract.rs"]
mod loader_snaps_note_contract;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/malformed_pe_handling.rs"]
mod malformed_pe_handling;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/path_search_order.rs"]
mod path_search_order;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/post_init_crash.rs"]
mod post_init_crash;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/run_cli_contract.rs"]
mod run_cli_contract;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/run_output_modes.rs"]
mod run_output_modes;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/run_success_edge_cases.rs"]
mod run_success_edge_cases;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/run_timeout_behavior.rs"]
mod run_timeout_behavior;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/run_unreadable_debug_string.rs"]
mod run_unreadable_debug_string;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/runtime_real_world_scenarios.rs"]
mod runtime_real_world_scenarios;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/search_dedup_app_equals_cwd.rs"]
mod search_dedup_app_equals_cwd;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/search_model_confidence.rs"]
mod search_model_confidence;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/search_path_edge_cases.rs"]
mod search_path_edge_cases;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/shared_bad_image_dedup.rs"]
mod shared_bad_image_dedup;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/shared_dependency_graph.rs"]
mod shared_dependency_graph;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/static_bad_image_direct.rs"]
mod static_bad_image_direct;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/static_bad_image_transitive.rs"]
mod static_bad_image_transitive;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/static_circular_dependency.rs"]
mod static_circular_dependency;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/static_deep_chain.rs"]
mod static_deep_chain;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/static_missing_direct.rs"]
mod static_missing_direct;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/static_missing_transitive.rs"]
mod static_missing_transitive;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/static_multiple_missing_at_same_depth.rs"]
mod static_multiple_missing_at_same_depth;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/static_wrong_pick.rs"]
mod static_wrong_pick;
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/verbose_static_and_dynamic.rs"]
mod verbose_static_and_dynamic;
