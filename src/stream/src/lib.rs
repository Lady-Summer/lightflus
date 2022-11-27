pub mod actor;
pub mod dataflow;
pub mod err;
pub mod state;
pub mod v8_runtime;

pub(crate) static MOD_TEST_START: std::sync::Once = std::sync::Once::new();
pub(crate) const DEFAULT_CHANNEL_SIZE: usize = 1000;

pub(crate) type EventReceiver<Input> = tokio::sync::mpsc::Receiver<Input>;
pub(crate) type EventSender<Input> = tokio::sync::mpsc::Sender<Input>;

pub(crate) const DETAULT_WATERMARK: std::time::Duration = std::time::Duration::from_millis(100);

pub fn initialize_v8() {
    // v8::V8::set_flags_from_string(
    //     "--no_freeze_flags_after_init --expose_gc --harmony-import-assertions --harmony-shadow-realm --allow_natives_syntax --turbo_fast_api_calls",
    //   );
    v8::V8::initialize_platform(v8::new_default_platform(10, false).make_shared());
    v8::V8::initialize();
}
