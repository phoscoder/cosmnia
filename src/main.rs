// Cosmnia — a COSMIC panel applet that keeps the computer awake.
//
// Click the cup to fill it: while it is full, Cosmnia holds an idle inhibitor
// (org.freedesktop.ScreenSaver, honoured by cosmic-idle, so the screen never
// blanks or locks) and a logind sleep/idle block inhibitor (so the machine
// never suspends). Click again to empty the cup and let it sleep.

use cosmic::app::{Core, Task};
use cosmic::iced::window::Id;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, icon};
use cosmic::{Element, theme};
use std::sync::Arc;

const APP_ID: &str = "io.github.phoscoder.Cosmnia";
const APP_NAME: &str = "Cosmnia";
const REASON: &str = "Cosmnia is keeping the computer awake";

const CUP_EMPTY: &[u8] = include_bytes!("../res/icons/cup-empty.svg");
const CUP_FULL: &[u8] = include_bytes!("../res/icons/cup-full.svg");

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
        (Applet { core, state: State::Asleep, error: None }, Task::none())
    }

    fn update(&mut self, message: Message) -> Task<Message> {
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

        let tip = match (&self.state, &self.error) {
            (State::Awake(_), _) => format!("{APP_NAME} — staying awake (click to allow sleep)"),
            (State::Busy, _) => format!("{APP_NAME} — …"),
            (State::Asleep, Some(e)) => format!("{APP_NAME} — failed: {e}"),
            (State::Asleep, None) => format!("{APP_NAME} — sleep allowed (click to stay awake)"),
        };
        applet
            .applet_tooltip::<Message>(button, tip, false, Message::Surface, None)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Message> {
        "".into()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}
