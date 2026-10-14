//! Terminal output: colors, tables and progress bars.

use indicatif::{ProgressBar, ProgressStyle};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static COLOR: AtomicBool = AtomicBool::new(false);

pub fn set_color(on: bool) {
    COLOR.store(on, Ordering::Relaxed);
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Paint {
    #[default]
    Plain,
    Bold,
    Dim,
    Red,
    Green,
    Yellow,
    Cyan,
    Heading,
}

pub fn paint(text: impl AsRef<str>, paint: Paint) -> String {
    let text = text.as_ref();
    let code = match paint {
        Paint::Plain => return text.to_string(),
        Paint::Bold => "1",
        Paint::Dim => "2",
        Paint::Red => "31",
        Paint::Green => "32",
        Paint::Yellow => "33",
        Paint::Cyan => "36",
        Paint::Heading => "1;36",
    };
    if COLOR.load(Ordering::Relaxed) {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub fn ok_mark() -> String {
    paint("✓", Paint::Green)
}

pub fn fail_mark() -> String {
    paint("✗", Paint::Red)
}

/// A table cell: pieces of text, each with its own paint.
#[derive(Default, Clone)]
pub struct Cell(Vec<(String, Paint)>);

impl Cell {
    pub fn new(text: impl Into<String>, paint: Paint) -> Cell {
        Cell(vec![(text.into(), paint)])
    }

    pub fn plain(text: impl Into<String>) -> Cell {
        Cell::new(text, Paint::Plain)
    }

    pub fn then(mut self, text: impl Into<String>, paint: Paint) -> Cell {
        self.0.push((text.into(), paint));
        self
    }

    fn width(&self) -> usize {
        self.0.iter().map(|(t, _)| t.chars().count()).sum()
    }

    fn render(&self) -> String {
        self.0.iter().map(|(t, p)| paint(t, *p)).collect()
    }
}

/// Prints rows under a header, indented: the first column left-aligned,
/// the others right-aligned.
pub fn print_table(header: Vec<Cell>, rows: Vec<Vec<Cell>>) {
    let columns = header.len();
    let mut widths = vec![0; columns];
    for row in std::iter::once(&header).chain(&rows) {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.width());
        }
    }
    for row in std::iter::once(&header).chain(&rows) {
        let mut line = String::from("  ");
        for (i, cell) in row.iter().enumerate() {
            let pad = " ".repeat(widths[i] - cell.width());
            if i == 0 {
                line += &format!("{}{pad}", cell.render());
            } else {
                line += &format!("   {pad}{}", cell.render());
            }
        }
        println!("{}", line.trim_end());
    }
}

/// A ratio against the baseline: green if faster, red if slower.
pub fn ratio(r: f64) -> (String, Paint) {
    let paint = if r < 0.95 {
        Paint::Green
    } else if r > 1.05 {
        Paint::Red
    } else {
        Paint::Plain
    };
    (format!("{r:.2}×"), paint)
}

pub fn ms(ms: f64) -> String {
    if ms >= 10_000.0 {
        format!("{:.1} s", ms / 1000.0)
    } else if ms >= 1000.0 {
        format!("{:.2} s", ms / 1000.0)
    } else if ms >= 10.0 {
        format!("{ms:.0} ms")
    } else {
        format!("{ms:.1} ms")
    }
}

pub fn bytes(n: u64) -> String {
    if n >= 10 << 20 {
        format!("{:.1} MB", n as f64 / (1 << 20) as f64)
    } else {
        format!("{} KB", n.div_ceil(1024))
    }
}

pub fn duration(d: Duration) -> String {
    ms(d.as_secs_f64() * 1000.0)
}

/// A progress bar over `len` steps. Hidden when stderr is not a terminal.
pub fn bar(len: usize, message: String) -> ProgressBar {
    let bar = ProgressBar::new(len as u64);
    bar.set_style(
        ProgressStyle::with_template("  {spinner:.cyan} {msg:24} [{bar:30.cyan/blue}] {pos}/{len}")
            .unwrap()
            .progress_chars("=> "),
    );
    bar.set_message(message);
    bar.enable_steady_tick(Duration::from_millis(100));
    bar
}

pub fn spinner(message: String) -> ProgressBar {
    let bar = ProgressBar::new_spinner();
    bar.set_style(ProgressStyle::with_template("  {spinner:.cyan} {msg}").unwrap());
    bar.set_message(message);
    bar.enable_steady_tick(Duration::from_millis(100));
    bar
}
