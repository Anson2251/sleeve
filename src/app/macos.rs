use relm4::{ComponentSender, gtk};

#[cfg(target_os = "macos")]
use relm4::gtk::prelude::{Cast, NativeExt, WidgetExt};

use super::{AppModel, AppMsg};

#[cfg(target_os = "macos")]
use {
    relm4::gtk::gdk,
    std::{sync::OnceLock, time::Duration},
};

#[cfg(target_os = "macos")]
static MACOS_MENU_CALLBACK: OnceLock<Box<dyn Fn(AppMsg) + Send + Sync>> = OnceLock::new();

#[cfg(target_os = "macos")]
static MACOS_MENU_TARGET: OnceLock<objc2::rc::Retained<SleeveMenuHandler>> = OnceLock::new();

#[cfg(target_os = "macos")]
objc2::define_class!(
    #[unsafe(super(objc2::runtime::NSObject))]
    #[name = "SleeveMenuHandler"]
    struct SleeveMenuHandler;

    impl SleeveMenuHandler {
        #[unsafe(method(handleMenuAction:))]
        fn handle_menu_action(&self, sender: &objc2::runtime::NSObject) {
            use objc2::msg_send;

            let tag: isize = unsafe { msg_send![sender, tag] };
            let message = match tag {
                1 => AppMsg::ShowAbout,
                2 => AppMsg::ChooseDirectory,
                3 => AppMsg::Undo,
                4 => AppMsg::Redo,
                5 => AppMsg::ToggleSidebar,
                6 => AppMsg::ToggleInspector,
                7 => AppMsg::RequestClose,
                _ => return,
            };
            if let Some(callback) = MACOS_MENU_CALLBACK.get() {
                callback(message);
            }
        }
    }
);

#[cfg(target_os = "macos")]
impl SleeveMenuHandler {
    objc2::extern_methods!(
        #[unsafe(method(new))]
        fn new() -> objc2::rc::Retained<Self>;
    );
}

#[cfg(target_os = "macos")]
pub(super) fn configure_macos_menubar(_: &gtk::Window, sender: ComponentSender<AppModel>) {
    use objc2::{MainThreadMarker, sel};
    use objc2_app_kit::{NSApp, NSEventModifierFlags, NSMenu, NSMenuItem};

    let menu_sender = sender.clone();
    let (tx, rx) = std::sync::mpsc::channel::<AppMsg>();
    let _ = MACOS_MENU_CALLBACK.set(Box::new(move |message| {
        let _ = tx.send(message);
    }));
    glib::timeout_add_local(Duration::from_millis(50), move || {
        while let Ok(message) = rx.try_recv() {
            menu_sender.input(message);
        }
        glib::ControlFlow::Continue
    });

    let _ = MACOS_MENU_TARGET.set(SleeveMenuHandler::new());
    let target = MACOS_MENU_TARGET
        .get()
        .expect("macOS menu target should be initialized");
    let mtm = unsafe { MainThreadMarker::new_unchecked() };

    unsafe {
        let main_menu = NSMenu::init(mtm.alloc::<NSMenu>());
        let app_menu_item = NSMenuItem::init(mtm.alloc::<NSMenuItem>());
        let app_menu = NSMenu::init(mtm.alloc::<NSMenu>());
        app_menu_item.setSubmenu(Some(&app_menu));
        main_menu.addItem(&app_menu_item);

        add_macos_callback_item(&app_menu, mtm, target, &crate::t!("menu.about"), 1, None);
        app_menu.addItem(&NSMenuItem::separatorItem(mtm));
        add_macos_responder_item(
            &app_menu,
            mtm,
            &crate::t!("menu.hide"),
            sel!(hide:),
            "h",
            NSEventModifierFlags::Command,
        );
        add_macos_responder_item(
            &app_menu,
            mtm,
            &crate::t!("menu.hide_others"),
            sel!(hideOtherApplications:),
            "h",
            NSEventModifierFlags::Command | NSEventModifierFlags::Option,
        );
        add_macos_responder_item(
            &app_menu,
            mtm,
            &crate::t!("menu.show_all"),
            sel!(unhideAllApplications:),
            "",
            NSEventModifierFlags::empty(),
        );
        app_menu.addItem(&NSMenuItem::separatorItem(mtm));
        add_macos_callback_item(
            &app_menu,
            mtm,
            target,
            &crate::t!("menu.quit"),
            7,
            Some(("q", NSEventModifierFlags::Command)),
        );

        let file_menu = add_macos_submenu(&main_menu, mtm, &crate::t!("menu.file"));
        add_macos_callback_item(
            &file_menu,
            mtm,
            target,
            &crate::t!("menu.open_folder"),
            2,
            Some(("o", NSEventModifierFlags::Command)),
        );

        let edit_menu = add_macos_submenu(&main_menu, mtm, &crate::t!("menu.edit"));
        add_macos_callback_item(
            &edit_menu,
            mtm,
            target,
            &crate::t!("menu.undo"),
            3,
            Some(("z", NSEventModifierFlags::Command)),
        );
        add_macos_callback_item(
            &edit_menu,
            mtm,
            target,
            &crate::t!("menu.redo"),
            4,
            Some((
                "z",
                NSEventModifierFlags::Command | NSEventModifierFlags::Shift,
            )),
        );
        edit_menu.addItem(&NSMenuItem::separatorItem(mtm));
        add_macos_responder_item(
            &edit_menu,
            mtm,
            &crate::t!("menu.cut"),
            sel!(cut:),
            "x",
            NSEventModifierFlags::Command,
        );
        add_macos_responder_item(
            &edit_menu,
            mtm,
            &crate::t!("menu.copy"),
            sel!(copy:),
            "c",
            NSEventModifierFlags::Command,
        );
        add_macos_responder_item(
            &edit_menu,
            mtm,
            &crate::t!("menu.paste"),
            sel!(paste:),
            "v",
            NSEventModifierFlags::Command,
        );
        edit_menu.addItem(&NSMenuItem::separatorItem(mtm));
        add_macos_responder_item(
            &edit_menu,
            mtm,
            &crate::t!("menu.select_all"),
            sel!(selectAll:),
            "a",
            NSEventModifierFlags::Command,
        );

        let view_menu = add_macos_submenu(&main_menu, mtm, &crate::t!("menu.view"));
        add_macos_callback_item(
            &view_menu,
            mtm,
            target,
            &crate::t!("menu.toggle_files"),
            5,
            None,
        );
        add_macos_callback_item(
            &view_menu,
            mtm,
            target,
            &crate::t!("menu.toggle_inspector"),
            6,
            None,
        );

        NSApp(mtm).setMainMenu(Some(&main_menu));
    }
}

#[cfg(target_os = "macos")]
unsafe fn add_macos_submenu(
    main_menu: &objc2_app_kit::NSMenu,
    mtm: objc2::MainThreadMarker,
    title: &str,
) -> objc2::rc::Retained<objc2_app_kit::NSMenu> {
    use objc2_app_kit::{NSMenu, NSMenuItem};
    use objc2_foundation::NSString;

    let item = NSMenuItem::init(mtm.alloc::<NSMenuItem>());
    let menu = NSMenu::init(mtm.alloc::<NSMenu>());
    menu.setTitle(&NSString::from_str(title));
    item.setSubmenu(Some(&menu));
    main_menu.addItem(&item);
    menu
}

#[cfg(target_os = "macos")]
unsafe fn add_macos_callback_item(
    menu: &objc2_app_kit::NSMenu,
    mtm: objc2::MainThreadMarker,
    target: &SleeveMenuHandler,
    title: &str,
    tag: isize,
    shortcut: Option<(&str, objc2_app_kit::NSEventModifierFlags)>,
) {
    use objc2::sel;
    use objc2_app_kit::NSMenuItem;
    use objc2_foundation::NSString;

    let item = NSMenuItem::init(mtm.alloc::<NSMenuItem>());
    item.setTitle(&NSString::from_str(title));
    unsafe {
        item.setAction(Some(sel!(handleMenuAction:)));
        item.setTarget(Some(target));
    }
    item.setTag(tag);
    if let Some((key, modifiers)) = shortcut {
        item.setKeyEquivalent(&NSString::from_str(key));
        item.setKeyEquivalentModifierMask(modifiers);
    }
    menu.addItem(&item);
}

#[cfg(target_os = "macos")]
unsafe fn add_macos_responder_item(
    menu: &objc2_app_kit::NSMenu,
    mtm: objc2::MainThreadMarker,
    title: &str,
    action: objc2::runtime::Sel,
    key: &str,
    modifiers: objc2_app_kit::NSEventModifierFlags,
) {
    use objc2_app_kit::NSMenuItem;
    use objc2_foundation::NSString;

    let item = NSMenuItem::init(mtm.alloc::<NSMenuItem>());
    item.setTitle(&NSString::from_str(title));
    unsafe {
        item.setAction(Some(action));
        item.setTarget(None);
    }
    item.setKeyEquivalent(&NSString::from_str(key));
    item.setKeyEquivalentModifierMask(modifiers);
    menu.addItem(&item);
}

#[cfg(not(target_os = "macos"))]
pub(super) fn configure_macos_menubar(_: &gtk::Window, _: ComponentSender<AppModel>) {}

#[cfg(target_os = "macos")]
pub(super) fn configure_macos_window(window: &gtk::Window) {
    use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior};

    window.connect_realize(|window| {
        let Some(surface) = window.surface() else {
            return;
        };
        let Some(macos_surface) = surface.downcast_ref::<gdk4_macos::MacosSurface>() else {
            return;
        };
        let native_window = macos_surface.native();
        let ns_window = unsafe { &*(native_window as *const NSWindow) };
        ns_window.setCollectionBehavior(NSWindowCollectionBehavior::FullScreenNone);
    });
}

#[cfg(target_os = "macos")]
pub(super) fn configure_macos_window_style() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(
        "window, .background, .titlebar, headerbar, .window-frame { border-radius: 0px; }",
    );
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

#[cfg(not(target_os = "macos"))]
pub(super) fn configure_macos_window(_: &gtk::Window) {}

#[cfg(not(target_os = "macos"))]
pub(super) fn configure_macos_window_style() {}
