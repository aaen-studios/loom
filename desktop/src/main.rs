// A window, not a console program: no console opens beside it.
#![windows_subsystem = "windows"]

//! loom for Windows and macOS: a GPUI window over the Calx core, which it links.
//! The app draws screens and reaches the system; everything else is the core's
//! (PLAN.md, "Architecture").

mod app;
mod autoscroll;
mod core;
mod emoji;
#[cfg(test)]
mod key_select_tests;
#[cfg(target_os = "macos")]
mod mac;
mod meter;
mod model;
mod motion;
mod panel;
mod panel_test;
mod perf;
mod place;
mod prefs;
mod rail;
mod shell;
mod signin;
mod single;
mod smooth;
mod sound;
#[cfg(any(target_os = "macos", test))]
mod swap;
mod thread;
#[path = "../../design/generated/tokens.rs"]
#[allow(dead_code)]
mod tokens;
mod voice;
mod wake;
mod zoom;

use std::borrow::Cow;

use gpui_kit::component::{Theme, ThemeMode, TitleBar};
use gpui_kit::*;

use crate::app::{Loom, Palette};

const FONTS: [&[u8]; 5] = [
    include_bytes!("../assets/fonts/Geist-Regular.ttf"),
    include_bytes!("../assets/fonts/Geist-Medium.ttf"),
    include_bytes!("../assets/fonts/Geist-SemiBold.ttf"),
    include_bytes!("../assets/fonts/GeistMono-Regular.ttf"),
    include_bytes!("../assets/fonts/GeistMono-Medium.ttf"),
];

/// GPUI Kit's widgets in loom's colours, so inputs and Markdown match the rest.
pub fn theme(cx: &mut App, dark: bool) {
    Theme::change(
        if dark {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        },
        None,
        cx,
    );
    let p = Palette::new(dark);
    let t = Theme::global_mut(cx);
    t.font_family = prefs::ui_font().into();
    t.mono_font_family = prefs::mono_font().into();
    t.font_size = px(tokens::size::BODY);
    t.mono_font_size = px(tokens::size::SMALL);
    t.radius = px(tokens::radius::INPUT);
    t.radius_lg = px(tokens::radius::CARD);
    t.shadow = false;
    t.focus_ring = false;
    let c = &mut t.colors;
    c.background = p.bg;
    c.foreground = p.ink;
    c.muted = p.raised;
    c.muted_foreground = p.ink_3;
    c.border = p.line_strong;
    c.input = p.line_strong;
    c.ring = p.line_strong;
    c.accent = p.hover;
    c.accent_foreground = p.ink;
    c.popover = p.raised;
    c.popover_foreground = p.ink;
    c.primary = p.button;
    c.primary_foreground = p.button_ink;
    c.secondary = p.raised_2;
    c.secondary_foreground = p.ink;
    c.caret = p.ink;
    c.link = p.ink;
    c.scrollbar = gpui_kit::transparent_black();
    c.scrollbar_thumb = p.line_strong;
    let mut sel = p.ink;
    sel.a = 0.18;
    c.selection = sel;
}

/// This build's version: the release's, when CI builds one, and Cargo's otherwise.
pub const VERSION: &str = match option_env!("LOOM_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

fn main() {
    // Opened from the Finder or the Dock, a Mac app has only the system's own PATH.
    #[cfg(target_os = "macos")]
    mac::take_login_path();
    perf::start();
    // Started by an agent of the instance in this app, for loom's own tools.
    if std::env::args().nth(1).as_deref() == Some("mcp") {
        std::process::exit(core::mcp());
    }
    // The panel break-out test, which CI runs.
    if std::env::args().nth(1).as_deref() == Some("--panel-test") {
        let out = std::env::args()
            .nth(2)
            .unwrap_or_else(|| String::from("panel-test.json"));
        panel_test::run(out.into());
        return;
    }
    // A Mac update: this new loom puts itself in the old one's place (swap.rs).
    #[cfg(target_os = "macos")]
    if std::env::args().nth(1).as_deref() == Some("--swap") {
        std::process::exit(swap::run());
    }
    // One loom to a data folder: started again, it shows the loom running and ends.
    let background = std::env::args().any(|a| a == "--background");
    let data = app::data_dir();
    // A loom:// link that adds this device waits for whichever loom has the folder.
    if let Some(link) = single::link_in_args() {
        single::keep_link(std::path::Path::new(&data), &link);
    }
    match single::claim(std::path::Path::new(&data), !background, single::WAIT) {
        single::Start::First(owner) => single::keep(owner),
        single::Start::Running => return,
    }
    app::start_first_core();
    let application = gpui_kit::application().with_assets(AppAssets);
    // Opened again from the Dock or Finder while it runs: the window comes back.
    #[cfg(target_os = "macos")]
    application.on_reopen(|_| single::reopened());
    // A loom:// link opened while it runs, or that opened it (Info.plist names the scheme).
    #[cfg(target_os = "macos")]
    {
        let folder = data.clone();
        application.on_open_urls(move |urls| {
            if let Some(link) = urls
                .iter()
                .find(|u| u.starts_with("loom://add?") || u.starts_with("loom://link?"))
            {
                single::opened_link(std::path::Path::new(&folder), link);
            }
        });
    }
    application.run(|cx| {
        gpui_kit::init(cx);
        app::bind_page_keys(cx);
        app::bind_composer_keys(cx);
        // A loom started while this one stops runs as soon as this one has let go.
        cx.on_app_quit(|_| {
            single::let_go();
            async {}
        })
        .detach();
        shell::hear_notifications(cx);
        #[cfg(target_os = "macos")]
        menus::set(cx);
        let fonts: Vec<Cow<'static, [u8]>> = FONTS.iter().map(|f| Cow::Borrowed(*f)).collect();
        if let Err(e) = cx.text_system().add_fonts(fonts) {
            eprintln!("loom: the fonts didn't load: {e}");
        }
        let kept = prefs::Prefs::load(&app::data_dir());
        prefs::use_fonts(&kept);
        let dark = kept.dark(!matches!(
            cx.window_appearance(),
            WindowAppearance::Light | WindowAppearance::VibrantLight
        ));
        theme(cx, dark);
        let bounds = Bounds::centered(None, size(px(1120.), px(760.)), cx);
        let (window, _) = gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                // As narrow as the rail beside the chat at its narrowest: below the
                // whole sidebar's room, the sidebar is a rail (rail.rs).
                window_min_size: Some(size(px(tokens::layout::RAIL + app::CHAT_LEAST), px(480.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("loom".into()),
                    // A Mac's traffic lights, in the middle of the title bar's height
                    // (app.rs, LIGHTS). Windows has none.
                    traffic_light_position: Some(point(px(app::LIGHTS.0), px(app::LIGHTS.1))),
                    ..TitleBar::title_bar_options()
                }),
                app_owns_titlebar_drag: true,
                // A build for measuring opens hidden, and shows itself off screen
                // behind what the person is doing (app.rs): GPUI centers a window
                // whose place is off every screen. On a Mac, loom started at login
                // opens hidden too, in the menu bar.
                focus: !cfg!(feature = "perf") && !hidden_at_start(),
                show: !cfg!(feature = "perf") && !hidden_at_start(),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| Loom::new(window, cx)),
        )
        .expect("failed to open the window");
        // The window is put in its place as it's made (app.rs), when GPUI can't yet
        // hear that it moved or resized, so it would go on laying out for the size it
        // had: a build for measuring, made hidden at Windows' default size, then drew
        // its chat and composer off the bottom of the window until it was next resized.
        // Now it reads its size again.
        window
            .update(cx, |_, window, cx| window.bounds_changed(cx))
            .ok();
        if !cfg!(feature = "perf") && !hidden_at_start() {
            cx.activate(true);
        }
    });
}

/// Whether the window starts hidden: on a Mac, loom started at login or by an update
/// in the background, which goes straight to the menu bar. Windows hides its window
/// once it's made (app.rs).
fn hidden_at_start() -> bool {
    cfg!(target_os = "macos") && std::env::args().any(|a| a == "--background")
}

/// A Mac's menus, and their keys: loom's (hide, quit), Edit for the composer and
/// fields, and Window. ⌘W closes the window as its red button does, into the menu bar.
#[cfg(target_os = "macos")]
mod menus {
    use super::shell;
    use gpui_kit::component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
    use gpui_kit::{App, KeyBinding, Menu, MenuItem, OsAction, Window};

    gpui_kit::actions!(
        loom,
        [Quit, Hide, HideOthers, ShowAll, Close, Minimize, Zoom]
    );

    fn on_window(cx: &mut App, f: fn(&mut Window)) {
        if let Some(w) = cx.active_window() {
            let _ = w.update(cx, |_, window, _| f(window));
        }
    }

    pub fn set(cx: &mut App) {
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_action(|_: &Hide, cx| cx.hide());
        cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
        cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
        cx.on_action(|_: &Close, cx| {
            if !shell::keeps_in_tray() {
                cx.quit();
                return;
            }
            on_window(cx, |w| shell::show_window(shell::hwnd(w), false));
        });
        cx.on_action(|_: &Minimize, cx| on_window(cx, |w| w.minimize_window()));
        cx.on_action(|_: &Zoom, cx| on_window(cx, |w| w.zoom_window()));
        cx.bind_keys([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("cmd-h", Hide, None),
            KeyBinding::new("alt-cmd-h", HideOthers, None),
            KeyBinding::new("cmd-w", Close, None),
            KeyBinding::new("cmd-m", Minimize, None),
        ]);
        cx.set_menus([
            Menu::new("loom").items([
                MenuItem::action("Hide loom", Hide),
                MenuItem::action("Hide Others", HideOthers),
                MenuItem::action("Show All", ShowAll),
                MenuItem::separator(),
                MenuItem::action("Quit loom", Quit),
            ]),
            Menu::new("Edit").items([
                MenuItem::os_action("Undo", Undo, OsAction::Undo),
                MenuItem::os_action("Redo", Redo, OsAction::Redo),
                MenuItem::separator(),
                MenuItem::os_action("Cut", Cut, OsAction::Cut),
                MenuItem::os_action("Copy", Copy, OsAction::Copy),
                MenuItem::os_action("Paste", Paste, OsAction::Paste),
                MenuItem::action("Paste as Plain Text", crate::app::PastePlain),
                MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
            ]),
            Menu::new("Window").items([
                MenuItem::action("Minimize", Minimize),
                MenuItem::action("Zoom", Zoom),
                MenuItem::separator(),
                MenuItem::action("Close", Close),
            ]),
        ]);
    }
}

// The icons loom uses beyond GPUI Kit's own, from the same Lucide set.
gpui_kit::assets::icon_assets!(
    ExtraIcons,
    [
        Paperclip,
        FileText,
        Settings,
        ArrowLeft,
        ArrowRight,
        Dices,
        Pencil,
        Activity,
        Brain,
        RotateCcw,
        Trash,
        Hand,
        ShieldCheck,
        Zap,
        Check,
        CircleUserRound,
        Clock,
        CalendarDays,
        BookOpen,
        Bot,
        Plug,
        Spool,
        Puzzle,
        Smartphone,
        Users,
        Monitor,
        CircleFadingArrowUp,
        Truck,
        Power,
        LogOut,
        Globe,
        X,
        ListTodo,
        Pause,
        Play,
        Pin,
        PinOff,
        CornerDownRight,
        Reply,
        TextQuote,
        MessageCircleQuestionMark,
        GitBranch,
        AppWindow,
        Sheet,
        SquareTerminal,
        SlidersHorizontal,
        Minus,
        Square,
        Copy,
        Laptop,
        Server,
        Download,
        Package,
        KeyRound,
        Import,
        Table2,
        Folder,
        ExternalLink,
        Bell,
        BellOff,
        Palette,
        MessageSquare,
        ArrowRightLeft,
        Lightbulb,
        ArrowUpRight,
        Lock,
        Image,
        ClipboardPaste,
        ClipboardPen,
        FileClock,
        FolderInput,
        FolderOutput,
        FolderPlus,
        // The emoji picker's smiley, its tabs and Settings → Emoji's Add.
        FaceSlightlySmiling,
        Sticker,
        Leaf,
        Apple,
        Plane,
        Volleyball,
        Flag,
        ImagePlus,
        // Acta beside the chat: Boards in the + menu and its board picker, and Make a card.
        Layers,
        LayersPlus,
        // Settings › Voice: a service's tile; and live mode's button and its Text and Mute.
        Cloud,
        AudioLines,
        TextAlignStart,
        MicOff
    ]
);

/// loom's mark, the knot, beside the wordmark (design/logo/README.md).
const KNOT: &str = "logo/knot.svg";

/// GPUI Kit's assets, loom's extra icons, its mark, and the SVGs that come while it runs.
struct AppAssets;

/// SVGs that come while loom runs, by asset path, such as each brain's icon from the ACP
/// Registry, which the instance fetches.
static RUNTIME_SVGS: std::sync::LazyLock<
    std::sync::RwLock<std::collections::HashMap<String, Vec<u8>>>,
> = std::sync::LazyLock::new(Default::default);

/// Keeps `svg` as the asset at `path`, unless it's there already, and gives the path.
pub fn runtime_svg(path: &str, svg: &str) -> SharedString {
    if let Ok(mut m) = RUNTIME_SVGS.write() {
        m.entry(path.to_string())
            .or_insert_with(|| svg.as_bytes().to_vec());
    }
    SharedString::from(path.to_string())
}

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == KNOT {
            return Ok(Some(Cow::Borrowed(include_bytes!(
                "../../design/logo/knot.svg"
            ))));
        }
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        if let Some(bytes) = RUNTIME_SVGS.read().ok().and_then(|m| m.get(path).cloned()) {
            return Ok(Some(Cow::Owned(bytes)));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        if KNOT.starts_with(path) {
            paths.push(KNOT.into());
        }
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}
