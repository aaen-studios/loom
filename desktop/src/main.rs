// A window, not a console program: no console opens beside it.
#![windows_subsystem = "windows"]

//! loom for Windows and macOS: a GPUI window over the Calx core, which it links.
//! The app draws screens and reaches the system; everything else is the core's
//! (PLAN.md, "Architecture").

mod app;
mod core;
mod model;
mod panel;
mod panel_test;
mod shell;
mod smooth;
mod thread;
#[path = "../../design/generated/tokens.rs"]
#[allow(dead_code)]
mod tokens;

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
fn theme(cx: &mut App, dark: bool) {
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
    t.font_family = tokens::font::UI.into();
    t.mono_font_family = tokens::font::MONO.into();
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
    shell::after_update();
    gpui_kit::application().with_assets(AppAssets).run(|cx| {
        gpui_kit::init(cx);
        let fonts: Vec<Cow<'static, [u8]>> = FONTS.iter().map(|f| Cow::Borrowed(*f)).collect();
        if let Err(e) = cx.text_system().add_fonts(fonts) {
            eprintln!("loom: the fonts didn't load: {e}");
        }
        let dark = match std::env::var("LOOM_THEME").as_deref() {
            Ok("dark") => true,
            Ok("light") => false,
            _ => !matches!(
                cx.window_appearance(),
                WindowAppearance::Light | WindowAppearance::VibrantLight
            ),
        };
        theme(cx, dark);
        let bounds = Bounds::centered(None, size(px(1120.), px(760.)), cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(720.), px(480.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("loom".into()),
                    ..TitleBar::title_bar_options()
                }),
                app_owns_titlebar_drag: true,
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| Loom::new(window, cx)),
        )
        .expect("failed to open the window");
        cx.activate(true);
    });
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
        BookOpen,
        Bot,
        Plug,
        Puzzle,
        Smartphone,
        Users,
        Monitor,
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
        MessageCircleQuestionMark,
        GitBranch,
        AppWindow,
        Sheet,
        SquareTerminal,
        Minus,
        Square,
        Copy,
        Laptop,
        Server,
        Download,
        KeyRound,
        Import,
        Table2,
        Folder,
        ExternalLink,
        Bell
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
