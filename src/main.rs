extern crate termion;
extern crate tui;
extern crate git2;
extern crate colored;

use std::{io, env, thread, time, vec};
use std::sync::mpsc;
use termion::event;
use termion::input::TermRead;

use tui::Terminal;
use tui::backend::TermionBackend;
use tui::widgets::{Block, Paragraph, Widget};
use tui::layout::{Direction, Group, Rect, Size};
use tui::style::{Color, Style};
type TermBackend = TermionBackend;
type Term = Terminal<TermBackend>;

use colored::*;

use git2::{Repository, BranchType, Status};
use git2::{
    STATUS_IGNORED,
    STATUS_INDEX_TYPECHANGE, STATUS_INDEX_NEW, STATUS_INDEX_MODIFIED, STATUS_INDEX_DELETED, STATUS_INDEX_RENAMED,
    STATUS_WT_TYPECHANGE, STATUS_WT_NEW, STATUS_WT_MODIFIED, STATUS_WT_DELETED, STATUS_WT_RENAMED
};

enum Event {
    Input(event::Key),
    Tick
}

struct HeadInfo {
    ref_name: String,
    hash: String,
    message: String
}

struct StatusEntry {
    path: String,
    status: Status
}

struct App {
    terminal: Term,
    repo: Option<Repository>,
    head: Option<HeadInfo>,
    branches: vec::Vec<String>,
    untracked: vec::Vec<String>,
    unstaged: vec::Vec<StatusEntry>,
    staged: vec::Vec<StatusEntry>,
    term_size: Rect,
    rx: mpsc::Receiver<Event>,
    refresh: bool
}

fn main() {
    let mut args = env::args();
    let mut app = App::new(args.nth(1));
    
    app.run();
}

fn init_events() -> mpsc::Receiver<Event> {
    let (tx, rx) = mpsc::channel();
    let input_tx = tx.clone();

    thread::spawn(move || {
        let stdin = io::stdin();
        for c in stdin.keys() {
            let evt = c.unwrap();
            input_tx.send(Event::Input(evt)).unwrap();
        }
    });

    thread::spawn(move || {
        let tx = tx.clone();
        loop {
            tx.send(Event::Tick).unwrap();
            thread::sleep(time::Duration::from_millis(200));
        }
    });

    rx
}

impl App {
    fn new(path: Option<String>) -> App {
        let rx = init_events();
        let terminal = Terminal::new(TermBackend::new().unwrap()).unwrap();
        let size = terminal.size().unwrap();

        let mut app = App {
            terminal: terminal,
            repo: Option::None,
            term_size: size,
            head: Option::None,
            branches: Vec::new(),
            untracked: Vec::new(),
            unstaged: Vec::new(),
            staged: Vec::new(),
            rx: rx,
            refresh: true
        };

        if path.is_some() {
            app.open(path.unwrap())
        }

        app
    }

    fn open(&mut self, path: String) {
        self.repo = match Repository::open(path) {
            Ok(repo) => Option::from(repo),
            Err(_) => Option::None
        }
    }

    fn update_size(&mut self) {
        let size = self.terminal.size().unwrap();
        if size != self.term_size {
            self.terminal.resize(size).unwrap();
            self.term_size = size;
        }
    }

    fn run(&mut self) {
        self.terminal.clear().unwrap();
        self.terminal.hide_cursor().unwrap();

        loop {
            self.update_size();

            let evt = self.rx.recv().unwrap();
            match evt {
                Event::Input(input) => match input {
                    event::Key::Char('q') => {
                        break;
                    }
                    event::Key::Char('r') => {
                        self.refresh = true;
                    }
                    _ => {}
                }
                Event::Tick => {
                }
            }

            self.draw();
        }

        self.terminal.show_cursor().unwrap();
    }

    fn draw(&mut self) {
        let t = &mut self.terminal;
        Block::default()
            .style(Style::default().bg(Color::White))
            .render(t, &self.term_size);

        let mut output = String::new();
        output.push_str("rngit\n");
        if self.repo.is_some() && self.refresh {
            let repo = self.repo.take();
            let repo = repo.unwrap();
            {
                let head = repo.head().unwrap();
                let head_commit = repo.find_commit(head.target().unwrap());
                let head_short = head.shorthand();
                self.head.take();
                let head_info = self.head.get_or_insert(HeadInfo {
                    hash: String::default(),
                    message: String::default(),
                    ref_name: String::default()
                });
                if head_short.is_some() {
                    head_info.ref_name = head_short.unwrap().to_string();
                }
                if head_commit.is_ok() {
                    let head_commit = head_commit.unwrap();
                    head_info.message = head_commit.message().unwrap().to_string();
                    head_info.hash = format!("{}", head_commit.id());
                }
            }
            {
                self.branches.clear();

                let branches = repo.branches(Option::from(BranchType::Local));
                match branches {
                    Ok(branches) => {
                        for branch in branches {
                            if branch.is_ok() {
                                let name = branch.unwrap().0;
                                let name = name.name();
                                self.branches.push(name.unwrap().unwrap().to_string());
                            }
                        }
                    },
                    Err(e) => {
                        output.push_str(e.message());
                    }
                }
            }
            {
                self.staged.clear();
                self.unstaged.clear();
                self.untracked.clear();

                let statuses = repo.statuses(Option::None);
                match statuses {
                    Ok(statuses) => {
                        for status in statuses.iter() {
                            let stat = status.status();
                            if stat.contains(STATUS_IGNORED) {
                                continue;
                            }
                            let path = status.path().unwrap();
                            if stat.intersects(STATUS_INDEX_TYPECHANGE | STATUS_INDEX_NEW | STATUS_INDEX_MODIFIED | STATUS_INDEX_DELETED | STATUS_INDEX_RENAMED) {
                                self.staged.push(StatusEntry {
                                    status: stat,
                                    path: path.to_string()
                                });
                            } else if stat.intersects(STATUS_WT_TYPECHANGE | STATUS_WT_MODIFIED | STATUS_WT_DELETED | STATUS_WT_RENAMED) {
                                self.unstaged.push(StatusEntry {
                                    status: stat,
                                    path: path.to_string()
                                });
                            } else if stat.intersects(STATUS_WT_NEW) {
                                self.untracked.push(path.to_string());
                            }
                        }
                    },
                    Err(e) => {
                        output.push_str(e.message());
                    }
                }
            }
            self.repo.get_or_insert(repo);
            self.refresh = false;
        }
        if self.head.is_some() {
            output.push_str("Head: ");
            let head = self.head.take().unwrap();
            output.push_str(&head.ref_name.bright_blue());
            output.push(' ');
            output.push_str(&head.message);
            output.push('\n');
            self.head.get_or_insert(head);
        }
        if !self.branches.is_empty() {
            output.push_str("Branches:\n");
            for branch in &self.branches {
                output.push('\t');
                output.push_str(&branch);
                output.push('\n');
            }
        }
        if !self.untracked.is_empty() {
            output.push_str("\nUntracked changes:\n");
            for status in &self.untracked {
                output.push_str(status);
                output.push('\n');
            }
        }
        if !self.unstaged.is_empty() {
            output.push_str("\nUnstaged changes:\n");
            for status in &self.unstaged {
                if status.status.intersects(STATUS_WT_MODIFIED) {
                    output.push_str("modified: ");
                } else if status.status.intersects(STATUS_WT_DELETED) {
                    output.push_str("deleted:  ");
                } else if status.status.intersects(STATUS_WT_RENAMED) {
                    output.push_str("renamed:  ");
                } else if status.status.intersects(STATUS_WT_TYPECHANGE) {
                    output.push_str("typechange:");
                }
                output.push_str(&status.path);
                output.push('\n');
            }
        }
        if !self.staged.is_empty() {
            output.push_str("\nStaged changes:\n");
            for status in &self.staged {
                if status.status.intersects(STATUS_INDEX_MODIFIED) {
                    output.push_str("modified: ");
                } else if status.status.intersects(STATUS_INDEX_DELETED) {
                    output.push_str("deleted:  ");
                } else if status.status.intersects(STATUS_INDEX_RENAMED) {
                    output.push_str("renamed:  ");
                } else if status.status.intersects(STATUS_INDEX_TYPECHANGE) {
                    output.push_str("typechange:");
                }
                output.push_str(&status.path);
                output.push('\n');
            }
        }

        Group::default()
            .direction(Direction::Vertical)
            .sizes(&[Size::Percent(100)])
            .render(t, &self.term_size, |t, chunks| {
                Group::default()
                    .direction(Direction::Horizontal)
                    .sizes(&[Size::Percent(100)])
                    .render(t, &chunks[0], |t, chunks| {
                        Paragraph::default()
                            .text(
                                output.as_str(),
                            )
                            .render(t, &chunks[0]);
                    });
            });

        t.draw().unwrap();
    }
}