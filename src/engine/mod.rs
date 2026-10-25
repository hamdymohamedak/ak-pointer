//! Recording and playback. The UI starts these on a background thread and can cancel them.

mod play;
mod record;

pub use play::{play_action, PlayStatus};
pub use record::{
    prepare_recording, record_action, stop_recording, AllowedWindow, RecordResult, RecordStatus,
};
