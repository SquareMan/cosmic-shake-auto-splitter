use crate::State;

static mut STATE: Option<State> = None;

/// LiveSplit Auto Splitting Runtime expects a function named `update` to be exported in the wasm module.
#[no_mangle]
pub extern "C" fn update() {
    // SAFETY: Wasm is single threaded, so we don't have to worry about the `STATE` static
    // being accessed from multiple threads.
    unsafe { STATE.get_or_insert_with(|| State::new()).update() };
}
