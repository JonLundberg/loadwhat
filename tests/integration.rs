#[path = "harness/mod.rs"]
mod harness;

#[path = "integration/static_missing_direct.rs"]
mod static_missing_direct;
#[path = "integration/static_missing_transitive.rs"]
mod static_missing_transitive;
#[path = "integration/static_wrong_pick.rs"]
mod static_wrong_pick;
#[path = "integration/dynamic_loadlibrary_name.rs"]
mod dynamic_loadlibrary_name;
#[path = "integration/dynamic_loadlibrary_fullpath.rs"]
mod dynamic_loadlibrary_fullpath;
#[path = "integration/dynamic_missing_direct.rs"]
mod dynamic_missing_direct;
