mod api;
mod app;
mod banner;
mod config;
mod create_form;
mod hours_form;
mod i18n;
mod inputs;
mod lang_select;
mod models;
mod odoo;
mod routes;
#[cfg(test)]
mod testutil;
mod theme;
mod ui;
mod ui_strings;
mod wizard;

use std::io;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use ratatui::crossterm::execute;

use app::{App, Response};
use config::Config;
use i18n::Lang;
use lang_select::LangSelect;
use wizard::{Outcome, Wizard};

/// Cuánto se ve el banner si no se toca ninguna tecla.
const SPLASH: Duration = Duration::from_millis(1500);

const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

enum Screen {
    /// Banner de inicio; mientras tanto la pantalla siguiente ya está cargando.
    Splash {
        since: Instant,
        next: Box<Screen>,
    },
    /// Primer arranque: elegir idioma antes de seguir con la pantalla `next`.
    Lang {
        select: LangSelect,
        next: Box<Screen>,
    },
    Wizard(Box<Wizard>),
    Home(Box<Home>),
}

/// Pantalla principal: el estado y el canal por donde vuelven los GET.
struct Home {
    app: App,
    client: api::Client,
    tx: Sender<Response>,
    rx: Receiver<Response>,
}

impl Home {
    fn new(config: &Config) -> Result<Self, api::ApiError> {
        let (tx, rx) = mpsc::channel();
        let mut home = Self {
            app: App::new(config, app::week_start_today()),
            client: api::Client::new(config)?,
            tx,
            rx,
        };
        let requests = home.app.start();
        home.dispatch(requests);
        Ok(home)
    }

    /// Cada pedido va en su thread; las respuestas viejas las descarta `App`.
    fn dispatch(&self, requests: Vec<app::Request>) {
        for request in requests {
            let (client, tx) = (self.client.clone(), self.tx.clone());
            thread::spawn(move || {
                let _ = tx.send(app::execute(&client, request));
            });
        }
    }
}

type Validation = Receiver<(Config, Result<(), api::ApiError>)>;

fn main() {
    if std::env::args_os()
        .nth(1)
        .is_some_and(|a| a == "--version" || a == "-V")
    {
        println!("lazyodoo {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let Some(path) = config::default_path() else {
        eprintln!("Could not find the system configuration directory.");
        std::process::exit(1);
    };
    let saved_lang = config::load_lang(&path);
    i18n::set(saved_lang.unwrap_or_default());
    let screen = match config::load(&path) {
        Ok(Some(config)) => match Home::new(&config) {
            Ok(home) => Screen::Home(Box::new(home)),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
        Ok(None) => Screen::Wizard(Box::new(Wizard::new())),
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(1);
        }
    };
    let screen = if saved_lang.is_none() {
        Screen::Lang {
            select: LangSelect::new(),
            next: Box::new(screen),
        }
    } else {
        screen
    };
    let screen = Screen::Splash {
        since: Instant::now(),
        next: Box::new(screen),
    };
    let mut terminal = ratatui::init();
    let _ = execute!(io::stdout(), EnableBracketedPaste);
    let result = run(&mut terminal, screen, &path);
    let _ = execute!(io::stdout(), DisableBracketedPaste);
    ratatui::restore();
    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn run(terminal: &mut DefaultTerminal, mut screen: Screen, path: &Path) -> io::Result<()> {
    let mut validation: Option<Validation> = None;
    let mut tick = 0usize;
    loop {
        let spinner = SPINNER[tick % SPINNER.len()];
        terminal.draw(|frame| match &screen {
            Screen::Splash { .. } => banner::render(frame),
            Screen::Lang { select, .. } => select.render(frame),
            Screen::Wizard(wizard) => wizard.render(frame, spinner),
            Screen::Home(home) => ui::render(frame, &home.app, spinner),
        })?;

        if let Some(rx) = &validation {
            match rx.try_recv() {
                Ok((config, Ok(()))) => {
                    validation = None;
                    // recién con la validación OK se guarda y se entra al menú
                    let saved = config::save(path, &config)
                        .and_then(|()| Home::new(&config).map_err(|e| e.to_string()));
                    match saved {
                        Ok(home) => screen = Screen::Home(Box::new(home)),
                        Err(message) => set_wizard_error(&mut screen, message),
                    }
                }
                Ok((_, Err(e))) => {
                    validation = None;
                    set_wizard_error(&mut screen, e.to_string());
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    validation = None;
                    set_wizard_error(
                        &mut screen,
                        i18n::tr("The check ended unexpectedly.").into(),
                    );
                }
            }
        }

        if matches!(&screen, Screen::Splash { since, .. } if since.elapsed() >= SPLASH) {
            screen = skip_splash(screen);
        }

        if let Screen::Home(home) = &mut screen {
            while let Ok(response) = home.rx.try_recv() {
                let requests = home.app.on_response(response);
                home.dispatch(requests);
            }
        }

        if !event::poll(Duration::from_millis(100))? {
            tick += 1;
            continue;
        }
        match (event::read()?, &mut screen) {
            (Event::Key(key), Screen::Splash { .. }) if key.kind == KeyEventKind::Press => {
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    return Ok(());
                }
                screen = skip_splash(screen);
            }
            (Event::Key(key), Screen::Lang { select, .. }) if key.kind == KeyEventKind::Press => {
                match select.handle_key(key) {
                    lang_select::Outcome::Quit => return Ok(()),
                    lang_select::Outcome::Chosen(lang) => {
                        let message = choose_lang(path, lang);
                        screen = skip_lang(screen);
                        if let Some(message) = message {
                            notify(&mut screen, message);
                        }
                    }
                    lang_select::Outcome::None => {}
                }
            }
            (Event::Key(key), Screen::Wizard(wizard)) if key.kind == KeyEventKind::Press => {
                match wizard.handle_key(key) {
                    Outcome::Quit => return Ok(()),
                    Outcome::Submit(config) => validation = Some(spawn_validation(config)),
                    Outcome::None => {}
                }
            }
            (Event::Paste(text), Screen::Wizard(wizard)) => wizard.handle_paste(&text),
            (Event::Key(key), Screen::Home(home)) if key.kind == KeyEventKind::Press => {
                let step = home.app.on_key(key);
                if step.quit {
                    return Ok(());
                }
                home.dispatch(step.requests);
                if step.cycle_lang {
                    let lang = i18n::current().next();
                    if let Some(message) = choose_lang(path, lang) {
                        home.app.set_notice(message);
                    }
                }
            }
            (Event::Paste(text), Screen::Home(home)) => home.app.on_paste(&text),
            _ => {}
        }
    }
}

fn skip_splash(screen: Screen) -> Screen {
    match screen {
        Screen::Splash { next, .. } => *next,
        other => other,
    }
}

fn skip_lang(screen: Screen) -> Screen {
    match screen {
        Screen::Lang { next, .. } => *next,
        other => other,
    }
}

/// Aplica el idioma y lo guarda. Si no se pudo guardar, devuelve el aviso (la sesión
/// sigue en ese idioma igual).
fn choose_lang(path: &Path, lang: Lang) -> Option<String> {
    i18n::set(lang);
    config::save_lang(path, lang).err()
}

fn notify(screen: &mut Screen, message: String) {
    match screen {
        Screen::Wizard(wizard) => wizard.set_error(message),
        Screen::Home(home) => home.app.set_notice(message),
        _ => {}
    }
}

fn set_wizard_error(screen: &mut Screen, message: String) {
    if let Screen::Wizard(wizard) = screen {
        wizard.set_error(message);
    }
}

fn spawn_validation(config: Config) -> Validation {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = api::Client::new(&config).and_then(|client| client.validate());
        let _ = tx.send((config, result));
    });
    rx
}
