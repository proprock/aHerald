//! Integration tests that use only the public API of the library, linked as one binary instead of one
//! per file. Tests that start the program, set process-wide state (log, notice sink) or are gated to an
//! OS keep their own file under `tests/`.

mod activation;
mod animation;
mod audio;
mod card;
mod card_border;
mod card_folder;
mod claude_desktop;
mod click_target;
mod codex_approval_observation;
mod codex_context;
mod codex_lifecycle;
mod config_preview;
mod demo;
mod installer_bridges;
mod ipc;
mod perf_artwork;
mod perf_report;
mod permission_delay;
mod placement;
mod presenter;
mod queue;
mod queue_outcome;
mod raw_output;
mod reentry;
mod resource_cache;
mod session_source;
mod session_warning_adapters;
mod statusline_launch;
mod tool_use_id;
mod transient_names;
mod tray_usage;
mod usage;
mod usage_compact;
mod usage_service;
mod usage_shared;
mod usage_ui;
mod warning_controls;
mod warning_messages;
