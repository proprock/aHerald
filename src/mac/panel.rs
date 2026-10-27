//! The one non-activating, transparent, floating card window. Main thread only.
use super::{
    geometry::{Frame, Rect, Screen},
    surface::Surface,
};
use objc2::{
    AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained,
};
use objc2_app_kit::{
    NSBackingStoreType, NSBitmapFormat, NSBitmapImageRep, NSColor, NSDeviceRGBColorSpace, NSEvent,
    NSImage, NSImageScaling, NSImageView, NSPanel, NSScreen, NSStatusWindowLevel,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use std::cell::RefCell;

pub struct Clicks {
    /// Mouse-down positions in points from the card's top-left corner, oldest first.
    points: RefCell<Vec<(f64, f64)>>,
}

define_class!(
    /// The card's picture, which also reports where it was clicked.
    #[unsafe(super(NSImageView))]
    #[thread_kind = MainThreadOnly]
    #[name = "AHeraldCardView"]
    #[ivars = Clicks]
    struct CardView;

    impl CardView {
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let at = event.locationInWindow();
            let height = self.frame().size.height;
            self.ivars().points.borrow_mut().push((at.x, height - at.y));
        }

        // A click on a card of another application's window still reaches the buttons.
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }
    }
);

impl CardView {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc::<Self>().set_ivars(Clicks {
            points: RefCell::new(Vec::new()),
        });
        // SAFETY: `init` of the superclass on a freshly allocated, initialised-ivar object.
        unsafe { msg_send![super(this), init] }
    }
}

pub struct Panel {
    panel: Retained<NSPanel>,
    view: Retained<CardView>,
    mtm: MainThreadMarker,
}

fn ns_rect(rect: Rect) -> NSRect {
    NSRect::new(
        NSPoint::new(rect.x, rect.y),
        NSSize::new(rect.width, rect.height),
    )
}

fn rect(frame: NSRect) -> Rect {
    Rect {
        x: frame.origin.x,
        y: frame.origin.y,
        width: frame.size.width,
        height: frame.size.height,
    }
}

/// The displays in AppKit order; the first is the primary one.
pub fn screens(mtm: MainThreadMarker) -> Vec<Screen> {
    NSScreen::screens(mtm)
        .iter()
        .map(|screen| Screen {
            frame: rect(screen.frame()),
            visible: rect(screen.visibleFrame()),
            scale: screen.backingScaleFactor(),
        })
        .collect()
}

/// The mouse position in global AppKit coordinates (points, bottom-left origin).
pub fn cursor() -> (f64, f64) {
    let point = NSEvent::mouseLocation();
    (point.x, point.y)
}

impl Panel {
    pub fn new(mtm: MainThreadMarker) -> Result<Self, String> {
        // SAFETY: plain AppKit configuration of a window this struct owns, on the main thread.
        unsafe {
            let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
                NSPanel::alloc(mtm),
                NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(320.0, 150.0)),
                NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
                NSBackingStoreType::Buffered,
                false,
            );
            // The panel is kept alive by this struct, not by closing it.
            panel.setReleasedWhenClosed(false);
            panel.setOpaque(false);
            panel.setBackgroundColor(Some(&NSColor::clearColor()));
            panel.setHasShadow(false);
            panel.setLevel(NSStatusWindowLevel);
            panel.setHidesOnDeactivate(false);
            panel.setCanHide(false);
            panel.setBecomesKeyOnlyIfNeeded(true);
            panel.setIgnoresMouseEvents(false);
            panel.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::FullScreenAuxiliary
                    | NSWindowCollectionBehavior::Stationary
                    | NSWindowCollectionBehavior::IgnoresCycle,
            );
            let view = CardView::new(mtm);
            view.setImageScaling(NSImageScaling::ScaleAxesIndependently);
            panel.setContentView(Some(&view));
            Ok(Self { panel, view, mtm })
        }
    }

    /// Puts `surface` at `frame` with the window opacity `alpha`, without taking focus.
    pub fn show(&self, surface: &Surface, frame: &Frame, alpha: u8) -> Result<(), String> {
        let _ = self.mtm;
        // SAFETY: the representation owns its storage; the copy below stays inside it.
        unsafe {
            let rep = NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bitmapFormat_bytesPerRow_bitsPerPixel(
                NSBitmapImageRep::alloc(),
                std::ptr::null_mut(),
                surface.width as isize,
                surface.height as isize,
                8,
                4,
                true,
                false,
                NSDeviceRGBColorSpace,
                NSBitmapFormat::empty(),
                surface.width as isize * 4,
                32,
            )
            .ok_or("cannot create card bitmap")?;
            let data = rep.bitmapData();
            if data.is_null() {
                return Err("card bitmap has no storage".into());
            }
            let target = std::slice::from_raw_parts_mut(data, surface.pixels().len());
            for (to, from) in target
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(surface.pixels().as_chunks::<4>().0)
            {
                *to = [from[2], from[1], from[0], from[3]];
            }
            // The size in points makes the pixels land one to one on the backing store.
            rep.setSize(NSSize::new(frame.rect.width, frame.rect.height));
            let image = NSImage::initWithSize(
                NSImage::alloc(),
                NSSize::new(frame.rect.width, frame.rect.height),
            );
            image.addRepresentation(&rep);
            self.view.setImage(Some(&image));
            self.panel.setFrame_display(ns_rect(frame.rect), true);
            self.panel.setAlphaValue(f64::from(alpha) / 255.0);
            self.panel.orderFrontRegardless();
        }
        Ok(())
    }

    /// Mouse-down positions since the last call, in points from the card's top-left corner.
    pub fn take_clicks(&self) -> Vec<(f64, f64)> {
        std::mem::take(&mut *self.view.ivars().points.borrow_mut())
    }

    pub fn hide(&self) {
        self.panel.orderOut(None);
    }

    pub fn is_visible(&self) -> bool {
        self.panel.isVisible()
    }
}

impl Drop for Panel {
    fn drop(&mut self) {
        self.panel.orderOut(None);
        self.panel.close();
    }
}
