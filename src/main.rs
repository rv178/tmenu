use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use freedesktop_entry_parser::parse_entry;
use std::process::{exit, Command, Stdio};
use std::{error::Error, fs, io};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use unicode_width::UnicodeWidthStr;

struct Tmenu {
    input: String,
    app_list: Vec<AppItem>,
    index: usize,
}

#[derive(Debug, Clone)]
struct AppItem {
    name: String,
    desc: String,
    cmd: String,
}

impl Tmenu {
    fn default() -> Tmenu {
        Tmenu {
            input: String::new(),
            app_list: Vec::new(),
            index: 0,
        }
    }
    fn next(&mut self) {
        self.index = (self.index + 1) % self.app_list.len();
    }
    fn previous(&mut self) {
        if self.index > 0 {
            self.index -= 1;
        } else {
            self.index = self.app_list.len() - 1;
        }
    }
    fn chain_hook(&mut self) {
        let original_hook = std::panic::take_hook();

        std::panic::set_hook(Box::new(move |panic| {
            reset_terminal().unwrap();
            original_hook(panic);
        }));
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and run it
    let app = Tmenu::default();
    let res = run_app(&mut terminal, app);

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
}

fn reset_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    crossterm::execute!(io::stdout(), LeaveAlternateScreen)?;

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: Tmenu) -> io::Result<()> {
    app.chain_hook();

    for file in fs::read_dir("/usr/share/applications").unwrap() {
        let file_name = file.unwrap().path().display().to_string();
        if file_name.ends_with(".desktop") {
            let entry = parse_entry(file_name)?;

            let name = entry
                .section("Desktop Entry")
                .attr("Name")
                .expect("Name doesn't exist.");
            let nodsp = entry.section("Desktop Entry").attr("NoDisplay");

            match nodsp {
                None | Some("false") => {
                    if let Some(cmd) = entry.section("Desktop Entry").attr("Exec") {
                        if let Some(generic_name) =
                            entry.section("Desktop Entry").attr("GenericName")
                        {
                            app.app_list.push(AppItem {
                                name: name.to_string(),
                                desc: generic_name.to_string(),
                                cmd: cmd.to_string(),
                            });
                        } else {
                            app.app_list.push(AppItem {
                                name: name.to_string(),
                                desc: "".to_string(),
                                cmd: cmd.to_string(),
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    loop {
        terminal.draw(|f| ui(f, &app))?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Enter => {
                    match Command::new("sh")
                        .arg("-c")
                        .arg(app.app_list[app.index].cmd.to_string())
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .output()
                    {
                        Ok(_) => {}
                        Err(e) => {
                            println!("Failed to execute command. Error: `{}`", e);
                        }
                    }

                    reset_terminal().unwrap();
                    exit(0);
                }
                KeyCode::Up => {
                    app.previous();
                }
                KeyCode::Down => {
                    app.next();
                }
                KeyCode::Char(c) => {
                    app.input.push(c);
                }
                KeyCode::Backspace => {
                    app.input.pop();
                }
                KeyCode::Esc => {
                    return Ok(());
                }
                _ => {}
            }
        }
    }
}

fn ui<B: Backend>(f: &mut Frame<B>, app: &Tmenu) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints(
            [
                Constraint::Length(1),
                Constraint::Length(3),
                Constraint::Min(1),
            ]
            .as_ref(),
        )
        .split(f.size());

    let (msg, style) = (
        vec![
            Span::raw("Press "),
            Span::styled(
                "Up/Down key",
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::Blue),
            ),
            Span::raw(" to navigate, "),
            Span::styled(
                "Esc",
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::Blue),
            ),
            Span::raw(" to exit. "),
        ],
        Style::default().add_modifier(Modifier::BOLD),
    );
    let mut text = Text::from(Spans::from(msg));
    text.patch_style(style);
    let help_message = Paragraph::new(text);
    f.render_widget(help_message, chunks[0]);

    let input = Paragraph::new(app.input.as_ref())
        .block(Block::default().borders(Borders::ALL).title("Search"));
    f.render_widget(input, chunks[1]);
    f.set_cursor(chunks[1].x + app.input.width() as u16 + 1, chunks[1].y + 1);

    let app_list: Vec<ListItem> = app
        .app_list
        .iter()
        .enumerate()
        .map(|(_i, m)| {
            let mut display_str = String::new();
            if m.desc == "" {
                display_str.push_str(&format!("{}", m.name));
            } else {
                display_str.push_str(&format!("{} [{}]", m.name, m.desc));
            }
            let content = vec![Spans::from(Span::raw(display_str))];
            ListItem::new(content)
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(app.index));

    let list = List::new(app_list)
        .block(Block::default().borders(Borders::ALL).title("App List"))
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::Blue)
                .fg(Color::Black),
        )
        .highlight_symbol("> ");
    f.render_stateful_widget(list, chunks[2], &mut state);
}
