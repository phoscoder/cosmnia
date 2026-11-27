// Cosmnia — a COSMIC panel applet that keeps the computer awake.
//
// Click the cup to fill it: while it is full, Cosmnia holds an idle inhibitor
// (org.freedesktop.ScreenSaver, honoured by cosmic-idle, so the screen never
// blanks or locks) and a logind sleep/idle block inhibitor (so the machine
// never suspends). Click again to empty the cup and let it sleep.

use cosmic::app::{Core, Task};
use cosmic::iced::window::Id;
use cosmic::applet::cosmic_panel_config::PanelAnchor;
use cosmic::cctk::sctk::reexports::protocols::xdg::shell::client::xdg_positioner::{Anchor, Gravity};
use cosmic::iced::runtime::platform_specific::wayland::popup::{SctkPopupSettings, SctkPositioner};
use cosmic::iced::{Alignment, Length, Limits, Point, Rectangle, Size};
use cosmic::widget::wayland::tooltip::widget::Tooltip;
use cosmic::widget::{self, autosize, icon};
use cosmic::{Element, theme};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

const APP_ID: &str = "io.github.phoscoder.Cosmnia";
const APP_NAME: &str = "Cosmnia";
const REASON: &str = "Cosmnia is keeping the computer awake";

const CUP_EMPTY: &[u8] = include_bytes!("../res/icons/cup-empty.svg");
const CUP_FULL: &[u8] = include_bytes!("../res/icons/cup-full.svg");

/// Text shown by the hover tooltip. libcosmic's `applet_tooltip` freezes its
/// text when the tooltip opens; ours reads this on every redraw instead, so it
/// changes the moment you click.
static TOOLTIP_TEXT: Mutex<String> = Mutex::new(String::new());
static TOOLTIP_WINDOW: LazyLock<Id> = LazyLock::new(Id::unique);
static TOOLTIP_AUTOSIZE: LazyLock<widget::Id> = LazyLock::new(|| widget::Id::new("cosmnia-tooltip"));

fn main() -> cosmic::iced::Result {
    cosmic::applet::run::<Applet>(())
}

/// Held inhibitors. Dropping the logind fd releases the sleep block; the
/// screensaver cookie is released explicitly (and by cosmic-idle if our
/// session connection goes away).
#[derive(Debug)]
struct Inhibitor {
    session: Option<zbus::Connection>,
    cookie: Option<u32>,
    _sleep_fd: Option<zbus::zvariant::OwnedFd>,
}

impl Inhibitor {
    async fn acquire() -> Result<Self, String> {
        let mut errors = Vec::new();

        let (session, cookie) = match screensaver_inhibit().await {
            Ok((c, k)) => (Some(c), Some(k)),
            Err(e) => {
                errors.push(format!("screensaver: {e}"));
                (None, None)
            }
        };

        let sleep_fd = match logind_inhibit().await {
            Ok(fd) => Some(fd),
            Err(e) => {
                errors.push(format!("logind: {e}"));
                None
            }
        };

        if cookie.is_none() && sleep_fd.is_none() {
            return Err(errors.join("; "));
        }
        for e in &errors {
            eprintln!("cosmnia: partial inhibit, {e}");
        }
        Ok(Self { session, cookie, _sleep_fd: sleep_fd })
    }

    async fn release(self) {
        if let (Some(conn), Some(cookie)) = (&self.session, self.cookie) {
            let _ = conn
                .call_method(
                    Some("org.freedesktop.ScreenSaver"),
                    "/org/freedesktop/ScreenSaver",
                    Some("org.freedesktop.ScreenSaver"),
                    "UnInhibit",
                    &(cookie,),
                )
                .await;
        }
        // _sleep_fd and the connection drop here.
    }
}

async fn screensaver_inhibit() -> zbus::Result<(zbus::Connection, u32)> {
    let conn = zbus::Connection::session().await?;
    let reply = conn
        .call_method(
            Some("org.freedesktop.ScreenSaver"),
            "/org/freedesktop/ScreenSaver",
            Some("org.freedesktop.ScreenSaver"),
            "Inhibit",
            &(APP_NAME, REASON),
        )
        .await?;
    let cookie: u32 = reply.body().deserialize()?;
    Ok((conn, cookie))
}

async fn logind_inhibit() -> zbus::Result<zbus::zvariant::OwnedFd> {
    let conn = zbus::Connection::system().await?;
    let reply = conn
        .call_method(
            Some("org.freedesktop.login1"),
            "/org/freedesktop/login1",
            Some("org.freedesktop.login1.Manager"),
            "Inhibit",
            &("sleep:idle", APP_NAME, REASON, "block"),
        )
        .await?;
    let fd: zbus::zvariant::OwnedFd = reply.body().deserialize()?;
    Ok(fd)
}

#[derive(Clone, Debug)]
enum Message {
    Toggle,
    Acquired(Result<Arc<Inhibitor>, String>),
    Released,
    Surface(cosmic::surface::Action<Message>),
}

#[derive(Debug)]
enum State {
    Asleep,
    Busy,
    Awake(Arc<Inhibitor>),
}

struct Applet {
    core: Core,
    state: State,
    error: Option<String>,
}

impl cosmic::Application for Applet {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _: ()) -> (Self, Task<Message>) {
        let applet = Applet { core, state: State::Asleep, error: None };
        applet.sync_tooltip();
        (applet, Task::none())
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);
        self.sync_tooltip();
        task
    }

    fn view(&self) -> Element<'_, Message> {
        let applet = &self.core.applet;
        let horizontal = applet.is_horizontal();
        let (size, _) = applet.suggested_size(true);
        let (major, minor) = applet.suggested_padding(true);
        let (hpad, vpad) = if horizontal { (major, minor) } else { (minor, major) };

        let awake = matches!(self.state, State::Awake(_));
        let svg = if awake { CUP_FULL } else { CUP_EMPTY };
        let cup = widget::icon(icon::from_svg_bytes(svg).symbolic(true)).size(size);

        let button = widget::button::custom(
            widget::layer_container(widget::Row::new().align_y(Alignment::Center).push(cup))
                .center_y(Length::Fixed(f32::from(size + 2 * vpad))),
        )
        .padding([0, hpad])
        .class(theme::Button::AppletIcon)
        .on_press(Message::Toggle);

        live_tooltip(applet.anchor, button)
    }

    fn view_window(&self, _id: Id) -> Element<'_, Message> {
        "".into()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}

impl Applet {
    fn tooltip_text(&self) -> String {
        match (&self.state, &self.error) {
            (State::Awake(_), _) => format!("{APP_NAME} — staying awake (click to allow sleep)"),
            (State::Busy, _) => format!("{APP_NAME} — …"),
            (State::Asleep, Some(e)) => format!("{APP_NAME} — failed: {e}"),
            (State::Asleep, None) => format!("{APP_NAME} — sleep allowed (click to stay awake)"),
        }
    }

    fn sync_tooltip(&self) {
        *TOOLTIP_TEXT.lock().unwrap() = self.tooltip_text();
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Toggle => match std::mem::replace(&mut self.state, State::Busy) {
                State::Asleep => {
                    return cosmic::task::future(async {
                        Message::Acquired(Inhibitor::acquire().await.map(Arc::new))
                    });
                }
                State::Awake(inh) => {
                    return cosmic::task::future(async move {
                        if let Ok(inh) = Arc::try_unwrap(inh) {
                            inh.release().await;
                        }
                        Message::Released
                    });
                }
                State::Busy => {}
            },
            Message::Acquired(Ok(inh)) => {
                self.error = None;
                self.state = State::Awake(inh);
            }
            Message::Acquired(Err(e)) => {
                eprintln!("cosmnia: could not inhibit sleep: {e}");
                self.error = Some(e);
                self.state = State::Asleep;
            }
            Message::Released => self.state = State::Asleep,
            Message::Surface(a) => return cosmic::task::message(cosmic::Action::Surface(a)),
        }
        Task::none()
    }
}

/// Same popup placement as libcosmic's `applet_tooltip`, but with a view that
/// reads [`TOOLTIP_TEXT`] live.
fn live_tooltip<'a>(
    anchor: PanelAnchor,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let window_id = *TOOLTIP_WINDOW;
    let (popup_anchor, gravity) = match anchor {
        PanelAnchor::Left => (Anchor::Right, Gravity::Right),
        PanelAnchor::Right => (Anchor::Left, Gravity::Left),
        PanelAnchor::Top => (Anchor::Bottom, Gravity::Bottom),
        PanelAnchor::Bottom => (Anchor::Top, Gravity::Top),
    };
    Tooltip::<'a, Message, Message>::new(
        content,
        Some(move |bounds: Rectangle| SctkPopupSettings {
            parent: Id::RESERVED,
            id: window_id,
            grab: false,
            input_zone: Some(vec![Rectangle::new(Point::new(-1000., -1000.), Size::default())]),
            positioner: SctkPositioner {
                size: None,
                size_limits: Limits::NONE.min_width(1.).min_height(1.),
                anchor_rect: Rectangle {
                    x: bounds.x.round() as i32,
                    y: bounds.y.round() as i32,
                    width: bounds.width.round() as i32,
                    height: bounds.height.round() as i32,
                },
                anchor: popup_anchor,
                gravity,
                constraint_adjustment: 15,
                offset: (0, 0),
                reactive: true,
            },
            parent_size: None,
            close_with_children: true,
        }),
        || {
            let text = TOOLTIP_TEXT.lock().unwrap().clone();
            Element::from(autosize::autosize(
                widget::layer_container(widget::text(text))
                    .layer(cosmic::cosmic_theme::Layer::Background)
                    .padding(4.),
                TOOLTIP_AUTOSIZE.clone(),
            ))
        },
        Message::Surface(cosmic::surface::Action::DestroyPopup(window_id)),
        Message::Surface,
    )
    .delay(Duration::from_millis(100))
    .into()
}
