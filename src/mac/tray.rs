//! The status item: a two-bar quota icon and the menu of `menu::items`. Main thread only.
use super::{
    geometry::Rect,
    menu::{Command, Item},
};
use crate::{config::TrayStyle, tray_usage::Levels};
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained, sel,
};
use objc2_app_kit::{
    NSControlStateValueOn, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem,
    NSVariableStatusItemLength,
};
use objc2_foundation::{NSObject, NSString};
use std::cell::RefCell;

/// Height of the icon in points; it is drawn at twice that for Retina.
const ICON_POINTS: f64 = 18.0;

pub struct Chosen {
    commands: RefCell<Vec<Command>>,
}

define_class!(
    /// Receives the menu's actions and remembers which commands were chosen.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "AHeraldMenuTarget"]
    #[ivars = Chosen]
    struct MenuTarget;

    impl MenuTarget {
        #[unsafe(method(chosen:))]
        fn chosen(&self, sender: &NSMenuItem) {
            if let Some(command) = Command::from_tag(sender.tag()) {
                self.ivars().commands.borrow_mut().push(command);
            }
        }
    }
);

impl MenuTarget {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc::<Self>().set_ivars(Chosen {
            commands: RefCell::new(Vec::new()),
        });
        // SAFETY: `init` of the superclass on a freshly allocated object with its ivars set.
        unsafe { msg_send![super(this), init] }
    }
}

pub struct Tray {
    item: Retained<NSStatusItem>,
    target: Retained<MenuTarget>,
    mtm: MainThreadMarker,
    shown: Option<(Levels, TrayStyle)>,
}

impl Tray {
    pub fn new(mtm: MainThreadMarker) -> Self {
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        Self {
            item,
            target: MenuTarget::new(mtm),
            mtm,
            shown: None,
        }
    }

    /// Replaces the menu; AppKit shows it when the status item is clicked.
    pub fn set_menu(&self, items: &[Item]) {
        let menu = NSMenu::new(self.mtm);
        // The checks and enabled state are ours, not the responder chain's.
        menu.setAutoenablesItems(false);
        for item in items {
            if item.separator_before {
                menu.addItem(&NSMenuItem::separatorItem(self.mtm));
            }
            // SAFETY: `chosen:` is implemented by `MenuTarget`, which this tray keeps alive.
            let entry = unsafe {
                menu.addItemWithTitle_action_keyEquivalent(
                    &NSString::from_str(&item.label),
                    Some(sel!(chosen:)),
                    &NSString::from_str(""),
                )
            };
            // SAFETY: the target outlives the menu: both are owned by this struct.
            unsafe { entry.setTarget(Some(&self.target)) };
            entry.setTag(item.command.tag());
            if item.checked {
                entry.setState(NSControlStateValueOn);
            }
        }
        self.item.setMenu(Some(&menu));
    }

    /// Draws the quota bars as the icon; unchanged levels cost nothing.
    pub fn set_levels(&mut self, levels: Levels, style: TrayStyle) -> Result<(), String> {
        if self.shown.as_ref() == Some(&(levels.clone(), style)) {
            return Ok(());
        }
        let size = (ICON_POINTS * 2.0) as u32;
        let pixels = match style {
            TrayStyle::Color => levels.render(size),
            TrayStyle::Bw => levels.render_mono(size),
        }
        .map_err(|e| e.to_owned())?;
        let image = super::image::from_bgra_as(
            &pixels,
            size as i32,
            size as i32,
            (ICON_POINTS, ICON_POINTS),
            style == TrayStyle::Bw,
        )?;
        let button = self
            .item
            .button(self.mtm)
            .ok_or("the status item has no button")?;
        button.setImage(Some(&image));
        self.shown = Some((levels, style));
        Ok(())
    }

    pub fn set_tooltip(&self, text: &str) {
        if let Some(button) = self.item.button(self.mtm) {
            button.setToolTip(Some(&NSString::from_str(text)));
        }
    }

    /// Commands chosen from the menu since the last call, oldest first.
    pub fn take_commands(&self) -> Vec<Command> {
        std::mem::take(&mut *self.target.ivars().commands.borrow_mut())
    }

    /// The status item's frame in global screen points (bottom-left origin), once it is on
    /// screen; the usage card hangs under it.
    pub fn frame(&self) -> Option<Rect> {
        let window = self.item.button(self.mtm)?.window()?;
        let frame = window.frame();
        Some(Rect {
            x: frame.origin.x,
            y: frame.origin.y,
            width: frame.size.width,
            height: frame.size.height,
        })
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        NSStatusBar::systemStatusBar().removeStatusItem(&self.item);
    }
}
