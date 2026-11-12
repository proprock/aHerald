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
    NSBorderType, NSButton, NSColor, NSControlStateValueOff, NSControlStateValueOn, NSFont,
    NSTextField, NSTextView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};
use std::path::PathBuf;

const WIDTH: f64 = 520.0;
const HEIGHT: f64 = 740.0;
/// Space above the first row, so the content does not crowd the title bar.
const SHIFT: f64 = 16.0;
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
    ClaudeStatusline,
    Launch,
    OpenCode,
    Pi,
    Goose,
}

/// Title, column (0 Codex, 1 Claude Code; `None` spans both) and top of each checkbox.
const BOXES: [(Box, &str, Option<usize>, f64); 10] = [
    (Box::Codex, "Hooks", Some(0), 112.0),
    (Box::Claude, "Hooks", Some(1), 112.0),
    (Box::CodexUsage, "Usage monitoring", Some(0), 140.0),
    (Box::ClaudeUsage, "Usage monitoring", Some(1), 140.0),
    (Box::CodexContext, "Context warning", Some(0), 168.0),
    (Box::CodexIdle, "Idle warning", Some(0), 196.0),
    (Box::ClaudeContext, "Context warning", Some(1), 168.0),
    (Box::ClaudeIdle, "Idle warning", Some(1), 196.0),
    (
        Box::ClaudeStatusline,
        "Status line (name, quota, context)",
        Some(1),
        224.0,
    ),
    (
        Box::Launch,
        "Start the resident after applying",
        None,
        440.0,
    ),
];

/// The bridge-file checkboxes: one column each of three, under the two agents.
const BRIDGES: [(Box, &str); 3] = [
    (Box::OpenCode, "OpenCode"),
    (Box::Pi, "Pi"),
    (Box::Goose, "Goose"),
];
const BRIDGES_TOP: f64 = 398.0;

struct Controls {
    exe: PathBuf,
    boxes: Vec<Retained<NSButton>>,
    result: Retained<NSTextView>,
    /// Closes the window; enabled once an Apply succeeded, so the report stays readable first.
    done: Retained<NSButton>,
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
            claude_statusline: self.checked(Box::ClaudeStatusline),
            opencode: self.checked(Box::OpenCode),
            pi: self.checked(Box::Pi),
            goose: self.checked(Box::Goose),
        }
    }

    /// Shows `selection` in the boxes that hold it; the resident and bridge boxes are the user's.
    fn show(&self, selection: &Selection) {
        for (which, on) in [
            (Box::Codex, selection.codex),
            (Box::Claude, selection.claude),
            (Box::CodexContext, selection.codex_context),
            (Box::CodexIdle, selection.codex_idle),
            (Box::ClaudeContext, selection.claude_context),
            (Box::ClaudeIdle, selection.claude_idle),
            (Box::ClaudeStatusline, selection.claude_statusline),
        ] {
            self.boxes[which as usize].setState(if on {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
        }
    }

    /// A box was clicked: switching something on switches on what it needs, switching a parent
    /// off switches off what needed it, and a choice that needs a resident asks for one.
    fn toggled(&self, sender: &AnyObject) {
        let Some(index) = self
            .boxes
            .iter()
            .position(|button| std::ptr::eq(Retained::as_ptr(button).cast::<AnyObject>(), sender))
        else {
            return;
        };
        let selection = self.selection();
        let turned_on = self.boxes[index].state() == NSControlStateValueOn;
        let selection = if turned_on {
            selection.with_dependencies()
        } else {
            selection.without_dependents()
        };
        self.show(&selection);
        if turned_on && selection.needs_resident() {
            self.boxes[Box::Launch as usize].setState(NSControlStateValueOn);
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
                if apply {
                    self.done.setEnabled(true);
                }
                if apply && self.checked(Box::Launch) && !selection.remove() {
                    launch(&self.exe);
                }
                report
            }
            Err(message) => format!("Nothing was changed: {message}"),
        };
        self.result.setString(&NSString::from_str(&text));
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

        #[unsafe(method(toggled:))]
        fn toggled(&self, sender: &AnyObject) {
            self.ivars().toggled(sender);
        }

        #[unsafe(method(done:))]
        fn done(&self, _sender: &AnyObject) {
            NSApplication::sharedApplication(self.mtm()).terminate(None);
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
        NSPoint::new(x, HEIGHT - top - SHIFT - height),
        NSSize::new(width, height),
    )
}

/// Opens the setup window and returns when it is closed.
pub fn run() -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("the setup window must run on the main thread")?;
    let exe = std::env::current_exe().map_err(|_| "cannot locate aHerald executable")?;
    // A receipt from another installation is reported, not fatal: Apply replaces its bindings.
    let (previous, note_text) = match selected_harnesses(&exe) {
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

    let label = |text: &str, x: f64, top: f64, width: f64, height: f64| {
        let field = NSTextField::wrappingLabelWithString(&NSString::from_str(text), mtm);
        field.setFrame(frame(x, top, width, height));
        content.addSubview(&field);
        field
    };
    let column_width = inner / 2.0;
    let column_x = |index: usize| MARGIN + column_width * index as f64;
    label(
        &format!(
            "Choose the agents that aHerald adds notification hooks to. Nothing changes until you press Apply; each agent still asks you to trust the hooks.\nApp: {}",
            exe.display()
        ),
        MARGIN,
        16.0 - SHIFT, // stays where it was; everything below moves down
        inner,
        64.0,
    );
    for (index, name) in ["Codex", "Claude Code"].into_iter().enumerate() {
        let heading = label(name, column_x(index), 88.0, column_width, 20.0);
        heading.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
    }

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
        defaults.claude_statusline,
        true,
    ];
    let checkbox = |title: &str, x: f64, width: f64, top: f64, on: bool| {
        // SAFETY: no target and no action yet; the controller is set as target below for the
        // boxes that have dependencies.
        let button = unsafe {
            NSButton::checkboxWithTitle_target_action(&NSString::from_str(title), None, None, mtm)
        };
        button.setFrame(frame(x, top, width, ROW));
        button.setState(state(on));
        content.addSubview(&button);
        button
    };
    let mut boxes: Vec<Retained<NSButton>> = BOXES
        .iter()
        .zip(initial)
        .map(|(&(_, title, column, top), on)| {
            let (x, width) = match column {
                Some(index) => (column_x(index), column_width),
                None => (MARGIN, inner),
            };
            checkbox(title, x, width, top, on)
        })
        .collect();
    // After the main boxes, in the order of `Box`: OpenCode, Pi, Goose.
    let bridge_width = inner / 3.0;
    boxes.extend(
        BRIDGES
            .iter()
            .zip([defaults.opencode, defaults.pi, defaults.goose])
            .enumerate()
            .map(|(index, (&(_, title), on))| {
                let x = MARGIN + bridge_width * index as f64;
                checkbox(title, x, bridge_width, BRIDGES_TOP, on)
            }),
    );
    let bridges_heading = label(
        "Other harnesses (a small bridge file each; Crush is set up by hand)",
        MARGIN,
        BRIDGES_TOP - 24.0,
        inner,
        20.0,
    );
    bridges_heading.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
    let note = label(
        "Usage, warnings and the status line report to a resident, so Start the resident is ticked for them. Choosing a warning also ticks the hooks it needs; the Claude context warning also ticks the status line. Claude auto prefers fresh statusLine quota. The status line wraps your statusLine command (kept and still shown; restored when turned off or removed). Thresholds are kept.",
        MARGIN,
        256.0,
        inner,
        60.0,
    );
    note.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    note.setTextColor(Some(&NSColor::secondaryLabelColor()));
    let format_note = label(
        "Status line text: set [claude.statusline] format in config.toml (menu: Edit configuration), for example {model.display_name} {context_window} {rate_limits.five_hour}, then Apply again. See docs/configuration.md.",
        MARGIN,
        322.0,
        inner,
        44.0,
    );
    format_note.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    format_note.setTextColor(Some(&NSColor::secondaryLabelColor()));

    let heading = label("Details", MARGIN, 520.0, inner, 18.0);
    heading.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
    // A framed, selectable, scrolling box so it reads as the output area; the note about a
    // receipt from another installation, when there is one, is the first thing shown in it.
    let scroll = NSTextView::scrollableTextView(mtm);
    let result = scroll
        .documentView()
        .and_then(|view| view.downcast::<NSTextView>().ok())
        .ok_or("the details box has no text view")?;
    result.setEditable(false);
    result.setSelectable(true);
    result.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    result.setString(&NSString::from_str(&format!(
        "Preview shows what Apply would change; Apply makes the changes. Done unlocks after a successful Apply.{note_text}"
    )));
    scroll.setBorderType(NSBorderType::BezelBorder);
    scroll.setFrame(frame(MARGIN, 540.0, inner, 158.0));
    content.addSubview(&scroll);

    // SAFETY: no target yet; the controller is set below, once it exists.
    let done = unsafe {
        NSButton::buttonWithTitle_target_action(&NSString::from_str("Done"), None, None, mtm)
    };
    done.setFrame(frame(WIDTH - MARGIN - 100.0, 476.0, 100.0, 32.0));
    done.setEnabled(false);
    content.addSubview(&done);
    let controller = mtm.alloc::<Controller>().set_ivars(Controls {
        exe,
        boxes,
        result,
        done,
    });
    // SAFETY: `init` of the superclass on a freshly allocated object with its ivars set.
    let controller: Retained<Controller> = unsafe { msg_send![super(controller), init] };

    // The boxes with dependencies tell the controller when they are clicked; the resident and
    // bridge boxes depend on nothing.
    for which in [
        Box::Codex,
        Box::Claude,
        Box::CodexUsage,
        Box::ClaudeUsage,
        Box::CodexContext,
        Box::CodexIdle,
        Box::ClaudeContext,
        Box::ClaudeIdle,
        Box::ClaudeStatusline,
    ] {
        let button = &controller.ivars().boxes[which as usize];
        // SAFETY: `toggled:` is implemented by `Controller`, which outlives the window.
        unsafe {
            button.setTarget(Some(&controller));
            button.setAction(Some(sel!(toggled:)));
        }
    }
    // SAFETY: `done:` is implemented by `Controller`, which outlives the window.
    unsafe {
        let done = &controller.ivars().done;
        done.setTarget(Some(&controller));
        done.setAction(Some(sel!(done:)));
    }
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
        button.setFrame(frame(x, 476.0, 100.0, 32.0));
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
