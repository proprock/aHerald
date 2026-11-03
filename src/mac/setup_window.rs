//! The setup window (`--setup-window`): the macOS counterpart of the Windows installer's
//! selection pages and the Linux console setup. It runs as the user, shows the choices of
//! `setup_selection::Selection` and delegates every change to `setup::run_with_options`; it
//! never touches the app bundle. Main thread only.
use crate::{
    agent::Agent,
    setup::{self, selected_harnesses, settings_state},
    setup_selection::Selection,
};
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, ProtocolObject},
    sel,
};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSBackingStoreType,
    NSButton, NSControlStateValueOff, NSControlStateValueOn, NSTextField, NSWindow,
    NSWindowStyleMask,
};
use objc2_foundation::{NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};
use std::path::PathBuf;

const WIDTH: f64 = 480.0;
const HEIGHT: f64 = 560.0;
const MARGIN: f64 = 20.0;
const ROW: f64 = 24.0;

/// Order of the checkboxes in `Controls::boxes`.
#[derive(Clone, Copy)]
enum Box {
    Codex,
    Claude,
    CodexUsage,
    ClaudeUsage,
    CodexContext,
    CodexIdle,
    ClaudeContext,
    ClaudeIdle,
    Launch,
}

const BOXES: [(Box, &str); 9] = [
    (Box::Codex, "Codex hooks"),
    (Box::Claude, "Claude Code hooks"),
    (Box::CodexUsage, "Codex usage monitoring"),
    (Box::ClaudeUsage, "Claude usage monitoring"),
    (Box::CodexContext, "Codex context warning"),
    (Box::CodexIdle, "Codex idle warning"),
    (Box::ClaudeContext, "Claude context warning"),
    (Box::ClaudeIdle, "Claude idle warning"),
    (Box::Launch, "Launch aHerald after applying"),
];

struct Controls {
    exe: PathBuf,
    boxes: Vec<Retained<NSButton>>,
    result: Retained<NSTextField>,
}

impl Controls {
    fn checked(&self, which: Box) -> bool {
        self.boxes[which as usize].state() == NSControlStateValueOn
    }

    fn selection(&self) -> Selection {
        Selection {
            codex: self.checked(Box::Codex),
            claude: self.checked(Box::Claude),
            codex_usage: self.checked(Box::CodexUsage),
            claude_usage: self.checked(Box::ClaudeUsage),
            codex_context: self.checked(Box::CodexContext),
            claude_context: self.checked(Box::ClaudeContext),
            codex_idle: self.checked(Box::CodexIdle),
            claude_idle: self.checked(Box::ClaudeIdle),
        }
    }

    /// Runs the backend; a dry run only reports what would change.
    fn run(&self, apply: bool) {
        let selection = self.selection();
        let outcome = setup::run_with_options(
            &self.exe,
            &selection.harnesses(),
            selection.remove(),
            apply,
            &selection.options(),
        );
        let text = match outcome {
            Ok(report) => {
                if apply && self.checked(Box::Launch) && !selection.remove() {
                    launch(&self.exe);
                }
                report
            }
            Err(message) => format!("Nothing was changed: {message}"),
        };
        self.result.setStringValue(&NSString::from_str(&text));
    }
}

/// Starts the resident without waiting for it; there is no autostart entry.
fn launch(exe: &std::path::Path) {
    let _ = std::process::Command::new(exe)
        .arg("--resident")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

define_class!(
    /// Receives the buttons' actions and closes the application with its window.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "AHeraldSetupController"]
    #[ivars = Controls]
    struct Controller;

    unsafe impl NSObjectProtocol for Controller {}

    unsafe impl NSApplicationDelegate for Controller {
        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        fn should_terminate_after_last_window_closed(&self, _app: &NSApplication) -> bool {
            true
        }
    }

    impl Controller {
        #[unsafe(method(preview:))]
        fn preview(&self, _sender: &AnyObject) {
            self.ivars().run(false);
        }

        #[unsafe(method(apply:))]
        fn apply(&self, _sender: &AnyObject) {
            self.ivars().run(true);
        }
    }
);

fn frame(x: f64, top: f64, width: f64, height: f64) -> NSRect {
    // AppKit's origin is the bottom left; layout is written from the top.
    NSRect::new(
        NSPoint::new(x, HEIGHT - top - height),
        NSSize::new(width, height),
    )
}

/// Opens the setup window and returns when it is closed.
pub fn run() -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("the setup window must run on the main thread")?;
    let exe = std::env::current_exe().map_err(|_| "cannot locate aHerald executable")?;
    // A receipt from another installation is reported, not fatal: Apply replaces its bindings.
    let (previous, note) = match selected_harnesses(&exe) {
        Ok(previous) => (previous, String::new()),
        Err(message) => (Vec::<Agent>::new(), format!("\n{message}.")),
    };
    let settings = settings_state(&exe, None)?;
    let defaults = Selection::initial(&previous, &settings);

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

    // SAFETY: a freshly allocated window initialised once with a valid rectangle and style.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            mtm.alloc(),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(WIDTH, HEIGHT)),
            NSWindowStyleMask::Titled
                | NSWindowStyleMask::Closable
                | NSWindowStyleMask::Miniaturizable,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // The application owns the window until it quits.
    // SAFETY: the window is retained by `window`; AppKit must not release it on close.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(&NSString::from_str("aHerald Setup"));
    let content = window.contentView().ok_or("the setup window has no view")?;
    let inner = WIDTH - 2.0 * MARGIN;

    let label = |text: &str, top: f64, height: f64| {
        let field = NSTextField::wrappingLabelWithString(&NSString::from_str(text), mtm);
        field.setFrame(frame(MARGIN, top, inner, height));
        content.addSubview(&field);
        field
    };
    label(
        &format!(
            "Choose the agents that aHerald adds notification hooks to. Nothing changes until you press Apply; each agent still asks you to trust the hooks.\nApp: {}",
            exe.display()
        ),
        MARGIN,
        64.0,
    );

    // Created before the controller so it can own them; targets are set afterwards.
    let state = |on: bool| {
        if on {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        }
    };
    let initial = [
        defaults.codex,
        defaults.claude,
        defaults.codex_usage,
        defaults.claude_usage,
        defaults.codex_context,
        defaults.codex_idle,
        defaults.claude_context,
        defaults.claude_idle,
        true,
    ];
    // Row tops: hooks, usage and warnings groups, each with a heading between.
    let tops = [
        100.0, 126.0, 188.0, 214.0, 276.0, 302.0, 328.0, 354.0, 392.0,
    ];
    let boxes: Vec<Retained<NSButton>> = BOXES
        .iter()
        .zip(initial)
        .zip(tops)
        .map(|(((_, title), on), top)| {
            // SAFETY: no target and no action: the checkbox only keeps its own state.
            let button = unsafe {
                NSButton::checkboxWithTitle_target_action(
                    &NSString::from_str(title),
                    None,
                    None,
                    mtm,
                )
            };
            button.setFrame(frame(MARGIN, top, inner, ROW));
            button.setState(state(on));
            content.addSubview(&button);
            button
        })
        .collect();
    label(
        "Usage monitoring is optional. Claude auto tries your login token, then the last statusLine report.",
        150.0,
        34.0,
    );
    label(
        "Session warnings apply to the selected agents. Existing thresholds are kept.",
        240.0,
        34.0,
    );
    // The note about a receipt from another installation, when there is one.
    let result = NSTextField::wrappingLabelWithString(
        &NSString::from_str(&format!("Press Preview to see the changes.{note}")),
        mtm,
    );
    result.setFrame(frame(MARGIN, 470.0, inner, 80.0));
    content.addSubview(&result);

    let controller = mtm
        .alloc::<Controller>()
        .set_ivars(Controls { exe, boxes, result });
    // SAFETY: `init` of the superclass on a freshly allocated object with its ivars set.
    let controller: Retained<Controller> = unsafe { msg_send![super(controller), init] };

    for (title, action, x) in [
        ("Preview", sel!(preview:), MARGIN),
        ("Apply", sel!(apply:), 130.0),
    ] {
        // SAFETY: both actions are implemented by `Controller`, which outlives the window.
        let button = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str(title),
                Some(&controller),
                Some(action),
                mtm,
            )
        };
        button.setFrame(frame(x, 428.0, 100.0, 32.0));
        content.addSubview(&button);
    }

    app.setDelegate(Some(ProtocolObject::from_ref(&*controller)));
    window.center();
    window.makeKeyAndOrderFront(None);
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
    app.run();
    Ok(())
}
