#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// System malloc on macOS kept freed result pages dirty: host footprint grew 69→465MB over 20 runs of a
// 100k-row query with an 11MB live heap. mimalloc with immediate purge stayed at 52–68MB (docs/RESOURCE_OPT.md).
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;
unsafe extern "C" {
    fn mi_option_set(option: std::ffi::c_int, value: std::ffi::c_long);
}
// mi_option_purge_delay in mimalloc v2 (libmimalloc-sys 0.1.49, pinned by mimalloc =0.1.52).
const MI_OPTION_PURGE_DELAY: std::ffi::c_int = 15;
fn main() {
    // SAFETY: mimalloc options may be changed at any time; 0 returns freed pages to the OS immediately.
    unsafe { mi_option_set(MI_OPTION_PURGE_DELAY, 0) };
    axdb::run()
}
